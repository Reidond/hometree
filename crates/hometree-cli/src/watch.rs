use std::path::{Path, PathBuf};

use globset::GlobSet;
use hometree_core::{Config, ManagedSet, SecretsManager};
use notify::EventKind;

pub fn should_handle_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) | EventKind::Any
    )
}

pub fn watch_paths(config: &Config) -> Vec<PathBuf> {
    let mut set = std::collections::BTreeSet::new();
    for root in &config.manage.roots {
        let trimmed = root.trim_start_matches("./");
        if trimmed.is_empty() || has_glob_meta(trimmed) {
            continue;
        }
        let path = trimmed.trim_end_matches("/**").trim_end_matches('/');
        if !path.is_empty() {
            set.insert(path.to_string());
        }
    }
    for extra in &config.manage.extra_files {
        if !extra.is_empty() {
            set.insert(extra.clone());
        }
    }
    set.into_iter().map(PathBuf::from).collect()
}

pub fn root_to_pathspec(root: &str) -> String {
    let trimmed = root.trim_start_matches("./");
    if trimmed.is_empty() {
        return String::new();
    }
    if has_glob_meta(trimmed) {
        if trimmed.starts_with(":(glob)") {
            return trimmed.to_string();
        }
        return format!(":(glob){trimmed}");
    }
    trimmed
        .trim_end_matches("/**")
        .trim_end_matches('/')
        .to_string()
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains('{')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    Ignore,
    SecretPlaintext,
    Managed {
        auto_add: bool,
        is_allowed: bool,
        matches_allowlist: bool,
    },
}

#[derive(Debug, Default)]
pub struct WatchDecisions {
    pub managed_stage: std::collections::BTreeSet<PathBuf>,
    pub secret_plaintext: std::collections::BTreeSet<PathBuf>,
    pub auto_add: std::collections::BTreeSet<PathBuf>,
    pub auto_add_meta: Vec<AutoAddMeta>,
}

#[derive(Debug, Clone)]
pub struct AutoAddMeta {
    pub path: PathBuf,
    pub auto_add: bool,
    pub is_allowed: bool,
    pub matches_allowlist: bool,
}

pub fn decide_watch_action(
    managed: &ManagedSet,
    secrets: &SecretsManager,
    allowlist: &GlobSet,
    auto_add_enabled: bool,
    rel_path: &Path,
) -> WatchAction {
    if secrets.is_ciphertext_path(rel_path) {
        return WatchAction::Ignore;
    }
    if secrets.is_secret_plaintext(rel_path) {
        return WatchAction::SecretPlaintext;
    }
    if !managed.is_managed(rel_path) {
        return WatchAction::Ignore;
    }

    let is_allowed = managed.is_allowed(rel_path);
    let matches_allowlist = allowlist.is_match(rel_path);
    let auto_add = auto_add_enabled && is_allowed && matches_allowlist;
    WatchAction::Managed {
        auto_add,
        is_allowed,
        matches_allowlist,
    }
}

pub fn collect_watch_decisions(
    managed: &ManagedSet,
    secrets: &SecretsManager,
    allowlist: &GlobSet,
    auto_add_enabled: bool,
    rel_paths: impl IntoIterator<Item = PathBuf>,
) -> WatchDecisions {
    let mut decisions = WatchDecisions::default();

    for rel_path in rel_paths {
        match decide_watch_action(
            managed,
            secrets,
            allowlist,
            auto_add_enabled,
            &rel_path,
        ) {
            WatchAction::Ignore => {}
            WatchAction::SecretPlaintext => {
                decisions.secret_plaintext.insert(rel_path);
            }
            WatchAction::Managed {
                auto_add,
                is_allowed,
                matches_allowlist,
            } => {
                decisions.managed_stage.insert(rel_path.clone());
                if auto_add {
                    decisions.auto_add.insert(rel_path.clone());
                }
                decisions.auto_add_meta.push(AutoAddMeta {
                    path: rel_path,
                    auto_add,
                    is_allowed,
                    matches_allowlist,
                });
            }
        }
    }

    decisions
}

pub fn build_allowlist(patterns: &[String]) -> anyhow::Result<GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        builder.add(globset::Glob::new(trimmed)?);
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::{build_allowlist, collect_watch_decisions, decide_watch_action, WatchAction};
    use hometree_core::config::SecretRule;
    use hometree_core::{Config, ManagedSet, Paths, SecretsManager};
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn allowlist_matches_expected_patterns() {
        let list = build_allowlist(&vec![".config/**".to_string(), ".local/bin/*".to_string()])
            .expect("allowlist");
        assert!(list.is_match(Path::new(".config/app/config.toml")));
        assert!(list.is_match(Path::new(".local/bin/script")));
        assert!(!list.is_match(Path::new(".ssh/id_rsa")));
    }

    #[test]
    fn decide_watch_actions_for_secrets() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&xdg).unwrap();

        let paths = Paths::new_with_overrides(Some(&home), Some(&xdg)).expect("paths");
        let mut config = Config::default_with_paths(&paths);
        config.secrets.enabled = true;
        config.secrets.rules.push(SecretRule {
            path: ".config/app/secret.txt".to_string(),
            ciphertext: None,
            mode: None,
        });
        let managed = ManagedSet::from_config(&config).expect("managed");
        let secrets = SecretsManager::from_config(&config.secrets);
        let allowlist = build_allowlist(&vec![".config/**".to_string()]).expect("allowlist");

        assert_eq!(
            decide_watch_action(
                &managed,
                &secrets,
                &allowlist,
                true,
                Path::new(".config/app/secret.txt")
            ),
            WatchAction::SecretPlaintext
        );
        assert_eq!(
            decide_watch_action(
                &managed,
                &secrets,
                &allowlist,
                true,
                Path::new(".config/app/secret.txt.age")
            ),
            WatchAction::Ignore
        );
        match decide_watch_action(
            &managed,
            &secrets,
            &allowlist,
            true,
            Path::new(".config/app/config.toml"),
        ) {
            WatchAction::Managed { auto_add, .. } => {
                assert!(auto_add);
            }
            other => panic!("unexpected action: {other:?}"),
        }
        assert_eq!(
            decide_watch_action(
                &managed,
                &secrets,
                &allowlist,
                true,
                Path::new(".local/share/other.txt")
            ),
            WatchAction::Ignore
        );
    }

    #[test]
    fn watch_decisions_produce_staging_lists() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&xdg).unwrap();

        let paths = Paths::new_with_overrides(Some(&home), Some(&xdg)).expect("paths");
        let mut config = Config::default_with_paths(&paths);
        config.secrets.enabled = true;
        config.secrets.rules.push(SecretRule {
            path: ".config/app/secret.txt".to_string(),
            ciphertext: None,
            mode: None,
        });
        let managed = ManagedSet::from_config(&config).expect("managed");
        let secrets = SecretsManager::from_config(&config.secrets);
        let allowlist = build_allowlist(&vec![".config/**".to_string()]).expect("allowlist");

        let rel_paths = vec![
            PathBuf::from(".config/app/secret.txt"),
            PathBuf::from(".config/app/secret.txt.age"),
            PathBuf::from(".config/app/config.toml"),
            PathBuf::from(".local/share/other.txt"),
        ];

        let decisions = collect_watch_decisions(&managed, &secrets, &allowlist, true, rel_paths);

        assert!(decisions
            .secret_plaintext
            .contains(Path::new(".config/app/secret.txt")));
        assert!(decisions
            .managed_stage
            .contains(Path::new(".config/app/config.toml")));
        assert!(decisions
            .auto_add
            .contains(Path::new(".config/app/config.toml")));
        assert!(!decisions
            .managed_stage
            .contains(Path::new(".config/app/secret.txt.age")));
        assert!(!decisions
            .managed_stage
            .contains(Path::new(".local/share/other.txt")));
    }
}

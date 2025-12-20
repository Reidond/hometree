use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use hometree_core::ManagedSet;

#[derive(Debug, Clone)]
pub struct TrackDecision {
    pub rel_path: PathBuf,
    pub add_to_extra_files: bool,
}

pub fn decide_track(
    input: &Path,
    home_dir: &Path,
    managed: &ManagedSet,
    roots: &[String],
    allow_outside: bool,
    force: bool,
) -> Result<TrackDecision> {
    let abs = if input.is_absolute() {
        input.to_path_buf()
    } else {
        home_dir.join(input)
    };

    let rel = abs
        .strip_prefix(home_dir)
        .map_err(|_| anyhow!("path is outside $HOME: {}", abs.display()))?
        .to_path_buf();

    if managed.is_managed(&rel) {
        return Ok(TrackDecision {
            rel_path: rel,
            add_to_extra_files: false,
        });
    }

    let under_roots = is_under_managed_roots(&rel, roots);
    let allowed = managed.is_allowed(&rel);

    if under_roots {
        if !allowed && !force {
            return Err(anyhow!("path is ignored or denylisted: {}", rel.display()));
        }
        return Ok(TrackDecision {
            rel_path: rel,
            add_to_extra_files: false,
        });
    }

    if !allow_outside {
        return Err(anyhow!(
            "path is outside managed roots; use --allow-outside to track: {}",
            rel.display()
        ));
    }

    if !allowed && !force {
        return Err(anyhow!("path is ignored or denylisted: {}", rel.display()));
    }

    Ok(TrackDecision {
        rel_path: rel,
        add_to_extra_files: true,
    })
}

fn is_under_managed_roots(rel: &Path, roots: &[String]) -> bool {
    let rel_str = rel.to_string_lossy();
    for root in roots {
        let normalized = normalize_root(root);
        let root_prefix = normalized.trim_end_matches("/**").trim_end_matches('/');
        if rel_str == root_prefix || rel_str.starts_with(&format!("{}/", root_prefix)) {
            return true;
        }
    }
    false
}

fn normalize_root(root: &str) -> String {
    let trimmed = root.trim_start_matches("./");
    if trimmed.ends_with("/**") {
        trimmed.to_string()
    } else if trimmed.ends_with('/') {
        format!("{trimmed}**")
    } else {
        format!("{trimmed}/**")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hometree_core::Config;
    use hometree_core::Paths;

    fn config() -> (Config, ManagedSet, PathBuf) {
        let paths = Paths::new().expect("paths");
        let mut cfg = Config::default_with_paths(&paths);
        cfg.manage.roots = vec![".config/".to_string()];
        cfg.manage.extra_files = vec![".zshrc".to_string()];
        cfg.ignore.patterns = vec![".config/ignored/**".to_string(), ".ssh/**".to_string()];
        let managed = ManagedSet::from_config(&cfg).expect("managed");
        (cfg, managed, paths.home_dir().to_path_buf())
    }

    #[test]
    fn track_inside_roots() {
        let (cfg, managed, home) = config();
        let decision = decide_track(
            Path::new(".config/app/config.toml"),
            &home,
            &managed,
            &cfg.manage.roots,
            false,
            false,
        )
        .expect("decision");
        assert_eq!(decision.rel_path, PathBuf::from(".config/app/config.toml"));
        assert!(!decision.add_to_extra_files);
    }

    #[test]
    fn track_outside_requires_allow() {
        let (cfg, managed, home) = config();
        let err = decide_track(
            Path::new(".vimrc"),
            &home,
            &managed,
            &cfg.manage.roots,
            false,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("outside managed roots"));
    }

    #[test]
    fn track_outside_allowed_adds_extra() {
        let (cfg, managed, home) = config();
        let decision = decide_track(
            Path::new(".vimrc"),
            &home,
            &managed,
            &cfg.manage.roots,
            true,
            false,
        )
        .expect("decision");
        assert!(decision.add_to_extra_files);
    }

    #[test]
    fn track_ignored_requires_force() {
        let (cfg, managed, home) = config();
        let err = decide_track(
            Path::new(".config/ignored/file.txt"),
            &home,
            &managed,
            &cfg.manage.roots,
            false,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("ignored"));
    }

    #[test]
    fn track_ignored_with_force() {
        let (cfg, managed, home) = config();
        let decision = decide_track(
            Path::new(".config/ignored/file.txt"),
            &home,
            &managed,
            &cfg.manage.roots,
            false,
            true,
        )
        .expect("decision");
        assert_eq!(decision.rel_path, PathBuf::from(".config/ignored/file.txt"));
    }
}

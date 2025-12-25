use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub repo: RepoConfig,
    pub manage: ManageConfig,
    pub ignore: IgnoreConfig,
    pub watch: WatchConfig,
    pub snapshot: SnapshotConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub git_dir: PathBuf,
    pub work_tree: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManageConfig {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgnoreConfig {
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub enabled: bool,
    pub debounce_ms: u64,
    pub auto_stage_tracked_only: bool,
    pub auto_add_new: bool,
    pub auto_add_allow_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    pub auto_message_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecretsConfig {
    pub enabled: bool,
    pub backend: String,
    pub sidecar_suffix: String,
    pub recipients: Vec<String>,
    pub identity_files: Vec<PathBuf>,
    pub rules: Vec<SecretRule>,
    pub backup_policy: BackupPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRule {
    pub path: String,
    #[serde(default)]
    pub ciphertext: Option<String>,
    #[serde(default)]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupPolicy {
    #[default]
    Encrypt,
    Skip,
    Plaintext,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "age".to_string(),
            sidecar_suffix: ".age".to_string(),
            recipients: Vec::new(),
            identity_files: Vec::new(),
            rules: Vec::new(),
            backup_policy: BackupPolicy::Encrypt,
        }
    }
}

impl Config {
    pub fn default_with_paths(paths_ctx: &Paths) -> Self {
        Self {
            repo: RepoConfig {
                git_dir: paths_ctx.repo_dir(),
                work_tree: paths_ctx.home_dir().to_path_buf(),
            },
            manage: ManageConfig {
                paths: vec![".config/hometree/config.toml".to_string()],
            },
            ignore: IgnoreConfig {
                patterns: vec![
                    ".ssh/**".to_string(),
                    ".gnupg/**".to_string(),
                    ".local/share/keyrings/**".to_string(),
                    ".local/share/kwalletd/**".to_string(),
                    ".pki/**".to_string(),
                    ".mozilla/**".to_string(),
                    ".config/google-chrome/**".to_string(),
                    ".config/chromium/**".to_string(),
                    ".config/BraveSoftware/**".to_string(),
                    "**/*token*".to_string(),
                    "**/*secret*".to_string(),
                ],
            },
            watch: WatchConfig {
                enabled: false,
                debounce_ms: 500,
                auto_stage_tracked_only: true,
                auto_add_new: false,
                auto_add_allow_patterns: Vec::new(),
            },
            snapshot: SnapshotConfig {
                auto_message_template: None,
            },
            secrets: SecretsConfig::default(),
        }
    }

    pub fn write_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&contents)?;
        config.apply_secrets_defaults();
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        // Validate auto_add_allow_patterns count
        let non_empty_patterns = self
            .watch
            .auto_add_allow_patterns
            .iter()
            .filter(|p| !p.trim().is_empty())
            .count();
        if non_empty_patterns > MAX_AUTO_ADD_PATTERNS {
            return Err(crate::error::HometreeError::Config(format!(
                "auto_add_allow_patterns has {} entries; maximum is {}",
                non_empty_patterns, MAX_AUTO_ADD_PATTERNS
            )));
        }
        // Validate auto_add_allow_patterns to prevent overly broad patterns
        for pattern in &self.watch.auto_add_allow_patterns {
            let trimmed = pattern.trim();
            if trimmed.is_empty() {
                continue;
            }
            if is_overly_broad_pattern(trimmed) {
                return Err(crate::error::HometreeError::Config(format!(
                    "auto_add_allow_patterns contains overly broad pattern: '{}'. \
                     Use specific paths to avoid accidentally tracking all files.",
                    trimmed
                )));
            }
        }
        if self.secrets.enabled {
            if self.secrets.backend != "age" {
                return Err(crate::error::HometreeError::Config(format!(
                    "unsupported secrets backend: {}",
                    self.secrets.backend
                )));
            }
            if self.secrets.sidecar_suffix.trim().is_empty() {
                return Err(crate::error::HometreeError::Config(
                    "secrets.sidecar_suffix cannot be empty".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn apply_secrets_defaults(&mut self) {
        if !self.secrets.enabled {
            return;
        }
        if self.secrets.sidecar_suffix.trim().is_empty() {
            self.secrets.sidecar_suffix = ".age".to_string();
        }
        for rule in &self.secrets.rules {
            if rule.path.trim().is_empty() {
                continue;
            }
            if !self.ignore.patterns.contains(&rule.path) {
                self.ignore.patterns.push(rule.path.clone());
            }
        }
    }
}

/// Maximum number of auto_add_allow_patterns permitted.
const MAX_AUTO_ADD_PATTERNS: usize = 50;

fn is_overly_broad_pattern(pattern: &str) -> bool {
    // Reject patterns that would match everything or almost everything
    let dangerous_patterns = ["*", "**", "**/*", "*/**", ".**", ".*/**"];

    if dangerous_patterns.contains(&pattern) {
        return true;
    }

    // Reject patterns with no path separator (would match any file name anywhere)
    // Exception: allow patterns starting with a dot (like .gitignore)
    if !pattern.contains('/') && !pattern.starts_with('.') {
        return true;
    }

    // Reject absolute paths (patterns should be relative to home)
    if pattern.starts_with('/') {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{is_overly_broad_pattern, Config};
    use crate::paths::Paths;

    #[test]
    fn default_config_uses_xdg_paths() {
        let paths = Paths::new().expect("paths resolve");
        let cfg = Config::default_with_paths(&paths);
        assert_eq!(cfg.repo.git_dir, paths.repo_dir());
        assert_eq!(cfg.repo.work_tree, paths.home_dir());
        assert!(!cfg.manage.paths.is_empty());
        assert!(!cfg.ignore.patterns.is_empty());
    }

    #[test]
    fn overly_broad_patterns_rejected() {
        assert!(is_overly_broad_pattern("*"));
        assert!(is_overly_broad_pattern("**"));
        assert!(is_overly_broad_pattern("**/*"));
        assert!(is_overly_broad_pattern("*/**"));
        assert!(is_overly_broad_pattern("*.txt")); // No path separator
        assert!(is_overly_broad_pattern("/etc/passwd")); // Absolute path
        assert!(is_overly_broad_pattern("/home/user/.config/**")); // Absolute path
    }

    #[test]
    fn reasonable_patterns_allowed() {
        assert!(!is_overly_broad_pattern(".config/**"));
        assert!(!is_overly_broad_pattern(".config/*.conf"));
        assert!(!is_overly_broad_pattern(".local/bin/*"));
        assert!(!is_overly_broad_pattern(".gitignore")); // Starts with dot
    }
}

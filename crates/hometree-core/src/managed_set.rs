use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

use crate::Config;

pub struct ManagedSet {
    paths: GlobSet,
    ignore_patterns: GlobSet,
    denylist_patterns: GlobSet,
}

impl ManagedSet {
    pub fn from_config(config: &Config, home_dir: &Path) -> Result<Self, globset::Error> {
        let normalized = normalize_paths(&config.manage.paths, Some(home_dir));
        let ignore_patterns = config.ignore.patterns.clone();
        let denylist_patterns = Vec::new();

        Self::new(normalized, ignore_patterns, denylist_patterns)
    }

    pub fn is_allowed(&self, path: &Path) -> bool {
        let is_ignored = self.ignore_patterns.is_match(path);
        let is_denylisted = self.denylist_patterns.is_match(path);

        !(is_ignored || is_denylisted)
    }

    pub fn new<I, K, L>(
        paths: I,
        ignore_patterns: K,
        denylist_patterns: L,
    ) -> Result<Self, globset::Error>
    where
        I: IntoIterator<Item = String>,
        K: IntoIterator<Item = String>,
        L: IntoIterator<Item = String>,
    {
        let paths = build_globset(paths)?;
        let ignore_patterns = build_globset(ignore_patterns)?;
        let denylist_patterns = build_globset(denylist_patterns)?;

        Ok(Self {
            paths,
            ignore_patterns,
            denylist_patterns,
        })
    }

    pub fn is_managed(&self, path: &Path) -> bool {
        let matches_path = self.paths.is_match(path);
        let is_ignored = self.ignore_patterns.is_match(path);
        let is_denylisted = self.denylist_patterns.is_match(path);

        matches_path && !is_ignored && !is_denylisted
    }
}

fn build_globset<I>(patterns: I) -> Result<GlobSet, globset::Error>
where
    I: IntoIterator<Item = String>,
{
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(&pattern)?);
    }
    builder.build()
}

pub fn normalize_paths(paths: &[String], home_dir: Option<&Path>) -> Vec<String> {
    paths.iter().map(|p| normalize_path(p, home_dir)).collect()
}

pub fn normalize_path(path: &str, home_dir: Option<&Path>) -> String {
    let trimmed = path.trim_start_matches("./");
    if has_glob_meta(trimmed) {
        return trimmed.to_string();
    }
    if trimmed.ends_with("/**") {
        return trimmed.to_string();
    }
    if is_directory_path(trimmed, home_dir) {
        let base = trimmed.trim_end_matches('/');
        return format!("{base}/**");
    }
    trimmed.to_string()
}

pub fn is_directory_path(path: &str, home_dir: Option<&Path>) -> bool {
    if path.ends_with('/') {
        return true;
    }

    if let Some(home) = home_dir {
        let trimmed = path.trim_end_matches('/');
        let full_path = home.join(trimmed);
        if let Ok(metadata) = std::fs::metadata(&full_path) {
            return metadata.is_dir();
        }
    }

    false
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains('{')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_managed_set_creation() {
        let paths = normalize_paths(&["foo/".to_string(), "bar/baz.txt".to_string()], None);
        let managed_set = ManagedSet::new(
            paths,
            vec!["foo/ignore.txt".to_string()],
            vec!["**/*.bak".to_string()],
        )
        .unwrap();

        assert!(managed_set.is_managed(&PathBuf::from("foo/file.txt")));
        assert!(managed_set.is_managed(&PathBuf::from("bar/baz.txt")));
        assert!(!managed_set.is_managed(&PathBuf::from("foo/ignore.txt")));
        assert!(!managed_set.is_managed(&PathBuf::from("any/file.bak")));
        assert!(!managed_set.is_managed(&PathBuf::from("non_managed.txt")));
    }

    #[test]
    fn test_config_ignore_patterns_apply() {
        let paths = normalize_paths(&[".config/".to_string()], None);
        let managed_set = ManagedSet::new(
            paths,
            vec![
                ".config/google-chrome/**".to_string(),
                ".ssh/**".to_string(),
            ],
            Vec::<String>::new(),
        )
        .unwrap();

        assert!(managed_set.is_managed(&PathBuf::from(".config/app/config.toml")));
        assert!(!managed_set.is_managed(&PathBuf::from(".config/google-chrome/Default/History")));
        assert!(!managed_set.is_managed(&PathBuf::from(".ssh/id_rsa")));
    }

    #[test]
    fn test_ignore_overrides_path() {
        let paths = normalize_paths(&["my_project/".to_string()], None);
        let managed_set = ManagedSet::new(
            paths,
            vec!["my_project/ignored_dir/**".to_string()],
            Vec::<String>::new(),
        )
        .unwrap();

        assert!(managed_set.is_managed(&PathBuf::from("my_project/src/main.rs")));
        assert!(!managed_set.is_managed(&PathBuf::from("my_project/ignored_dir/another_file.txt")));
    }

    #[test]
    fn test_denylist_overrides_all() {
        let paths = normalize_paths(
            &[
                "my_project/".to_string(),
                "my_project/important_file.txt".to_string(),
            ],
            None,
        );
        let managed_set =
            ManagedSet::new(paths, Vec::<String>::new(), vec!["**/*.secret".to_string()]).unwrap();

        assert!(managed_set.is_managed(&PathBuf::from("my_project/src/main.rs")));
        assert!(managed_set.is_managed(&PathBuf::from("my_project/important_file.txt")));
        assert!(!managed_set.is_managed(&PathBuf::from("my_project/src/config.secret")));
        assert!(!managed_set.is_managed(&PathBuf::from("my_project/important_file.secret")));
    }

    #[test]
    fn test_file_path() {
        let paths = normalize_paths(&[".zshrc".to_string()], None);
        let managed_set =
            ManagedSet::new(paths, Vec::<String>::new(), Vec::<String>::new()).unwrap();

        assert!(managed_set.is_managed(&PathBuf::from(".zshrc")));
        assert!(!managed_set.is_managed(&PathBuf::from("src/main.rs")));
    }

    #[test]
    fn test_normalize_path_without_filesystem() {
        assert_eq!(normalize_path(".config/", None), ".config/**");
        assert_eq!(normalize_path(".local/bin/", None), ".local/bin/**");
        assert_eq!(normalize_path(".zshrc", None), ".zshrc");
        assert_eq!(normalize_path(".bashrc", None), ".bashrc");
        assert_eq!(
            normalize_path("scripts/deploy.sh", None),
            "scripts/deploy.sh"
        );
        assert_eq!(
            normalize_path(".config/app/config.toml", None),
            ".config/app/config.toml"
        );
        assert_eq!(normalize_path("**/*.txt", None), "**/*.txt");
        assert_eq!(
            normalize_path(".config/ghostty/config", None),
            ".config/ghostty/config"
        );
    }

    #[test]
    fn test_normalize_path_with_filesystem() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();

        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::fs::write(home.join(".config"), "file content").unwrap();

        assert_eq!(normalize_path(".local/bin", Some(home)), ".local/bin/**");
        assert_eq!(normalize_path(".config", Some(home)), ".config");
        assert_eq!(normalize_path(".nonexistent", Some(home)), ".nonexistent");
    }

    #[test]
    fn test_directory_in_paths_matches_contents() {
        let paths = normalize_paths(&[".local/bin/".to_string()], None);
        let managed_set =
            ManagedSet::new(paths, Vec::<String>::new(), Vec::<String>::new()).unwrap();

        assert!(managed_set.is_managed(&PathBuf::from(".local/bin/myscript")));
        assert!(managed_set.is_managed(&PathBuf::from(".local/bin/subdir/tool")));
        assert!(!managed_set.is_managed(&PathBuf::from(".local/share/other")));
    }

    #[test]
    fn test_extensionless_file_not_treated_as_directory() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();

        std::fs::create_dir_all(home.join(".config/ghostty")).unwrap();
        std::fs::write(home.join(".config/ghostty/config"), "file content").unwrap();

        let paths = normalize_paths(&[".config/ghostty/config".to_string()], Some(home));
        let managed_set =
            ManagedSet::new(paths, Vec::<String>::new(), Vec::<String>::new()).unwrap();

        assert!(managed_set.is_managed(&PathBuf::from(".config/ghostty/config")));
        assert!(!managed_set.is_managed(&PathBuf::from(".config/ghostty/config/subfile")));
    }
}

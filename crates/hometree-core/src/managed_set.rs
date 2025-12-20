use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

use crate::Config;

/// Represents a set of rules for managing paths, including roots, extra files,
/// ignore patterns, and denylist patterns.
pub struct ManagedSet {
    roots: GlobSet,
    extra_files: GlobSet,
    ignore_patterns: GlobSet,
    denylist_patterns: GlobSet,
}

impl ManagedSet {
    pub fn from_config(config: &Config) -> Result<Self, globset::Error> {
        let roots = normalize_roots(&config.manage.roots);
        let extra_files = config.manage.extra_files.clone();
        let ignore_patterns = config.ignore.patterns.clone();
        let denylist_patterns = Vec::new();

        Self::new(roots, extra_files, ignore_patterns, denylist_patterns)
    }

    pub fn is_allowed(&self, path: &Path) -> bool {
        let is_ignored = self.ignore_patterns.is_match(path);
        let is_denylisted = self.denylist_patterns.is_match(path);

        !(is_ignored || is_denylisted)
    }

    pub fn new<I, J, K, L>(
        roots: I,
        extra_files: J,
        ignore_patterns: K,
        denylist_patterns: L,
    ) -> Result<Self, globset::Error>
    where
        I: IntoIterator<Item = String>,
        J: IntoIterator<Item = String>,
        K: IntoIterator<Item = String>,
        L: IntoIterator<Item = String>,
    {
        // Normalize roots so that entries like `foo/` or `foo` manage
        // all paths under that directory (`foo/**`). Callers can still
        // pass explicit glob patterns and they will be preserved.
        let roots_vec: Vec<String> = roots.into_iter().collect();
        let roots = build_globset(normalize_roots(&roots_vec))?;
        let extra_files = build_globset(extra_files)?;
        let ignore_patterns = build_globset(ignore_patterns)?;
        let denylist_patterns = build_globset(denylist_patterns)?;

        Ok(Self {
            roots,
            extra_files,
            ignore_patterns,
            denylist_patterns,
        })
    }

    /// Checks if a given path is managed by this `ManagedSet`.
    ///
    /// A path is considered managed if:
    /// 1. It matches any of the `roots` or `extra_files`.
    /// 2. AND it does NOT match any `ignore_patterns`.
    /// 3. AND it does NOT match any `denylist_patterns`.
    ///
    /// Paths should be relative to the HOME directory.
    pub fn is_managed(&self, path: &Path) -> bool {
        let is_root_or_extra = self.roots.is_match(path) || self.extra_files.is_match(path);
        let is_ignored = self.ignore_patterns.is_match(path);
        let is_denylisted = self.denylist_patterns.is_match(path);

        is_root_or_extra && !is_ignored && !is_denylisted
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

fn normalize_roots(roots: &[String]) -> Vec<String> {
    roots
        .iter()
        .map(|root| normalize_root_pattern(root))
        .collect()
}

fn normalize_root_pattern(root: &str) -> String {
    let trimmed = root.trim_start_matches("./");
    if has_glob_meta(trimmed) {
        return trimmed.to_string();
    }
    if trimmed.ends_with("/**") {
        return trimmed.to_string();
    }
    if trimmed.ends_with('/') {
        return format!("{trimmed}**");
    }
    format!("{trimmed}/**")
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains('{')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_managed_set_creation() {
        let managed_set = ManagedSet::new(
            vec!["foo/".to_string()],
            vec!["bar/baz.txt".to_string()],
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
        let managed_set = ManagedSet::new(
            vec![".config/".to_string()],
            Vec::<String>::new(),
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
    fn test_ignore_overrides_root() {
        let managed_set = ManagedSet::new(
            vec!["my_project/".to_string()],
            Vec::<String>::new(),
            vec!["my_project/ignored_dir/**".to_string()],
            Vec::<String>::new(),
        )
        .unwrap();

        assert!(managed_set.is_managed(&PathBuf::from("my_project/src/main.rs")));
        assert!(!managed_set.is_managed(&PathBuf::from("my_project/ignored_dir/another_file.txt")));
    }

    #[test]
    fn test_denylist_overrides_all() {
        let managed_set = ManagedSet::new(
            vec!["my_project/".to_string()],
            vec!["my_project/important_file.txt".to_string()],
            Vec::<String>::new(),
            vec!["**/*.secret".to_string()],
        )
        .unwrap();

        assert!(managed_set.is_managed(&PathBuf::from("my_project/src/main.rs")));
        assert!(managed_set.is_managed(&PathBuf::from("my_project/important_file.txt")));
        assert!(!managed_set.is_managed(&PathBuf::from("my_project/src/config.secret")));
        assert!(!managed_set.is_managed(&PathBuf::from("my_project/important_file.secret")));
    }

    #[test]
    fn test_only_extra_files() {
        let managed_set = ManagedSet::new(
            Vec::<String>::new(),
            vec![".zshrc".to_string()],
            Vec::<String>::new(),
            Vec::<String>::new(),
        )
        .unwrap();

        assert!(managed_set.is_managed(&PathBuf::from(".zshrc")));
        assert!(!managed_set.is_managed(&PathBuf::from("src/main.rs")));
    }
}

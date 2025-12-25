use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use hometree_core::ManagedSet;

#[derive(Debug, Clone)]
pub struct TrackDecision {
    pub rel_path: PathBuf,
    /// Whether the path needs to be added to config.manage.paths
    pub add_to_paths: bool,
}

/// Decide whether a path can be tracked and whether it needs to be added to paths.
///
/// With the unified `paths` config, any path under $HOME can be tracked.
/// If the path is already covered by an existing pattern in `paths`, we don't
/// need to add it again. Otherwise, we add it to `paths`.
pub fn decide_track(
    input: &Path,
    home_dir: &Path,
    managed: &ManagedSet,
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
            add_to_paths: false,
        });
    }

    if !managed.is_allowed(&rel) && !force {
        return Err(anyhow!("path is ignored or denylisted: {}", rel.display()));
    }

    Ok(TrackDecision {
        rel_path: rel,
        add_to_paths: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hometree_core::Config;
    use hometree_core::Paths;

    fn config() -> (ManagedSet, PathBuf) {
        let paths = Paths::new().expect("paths");
        let mut cfg = Config::default_with_paths(&paths);
        cfg.manage.paths = vec![".config/".to_string(), ".zshrc".to_string()];
        cfg.ignore.patterns = vec![".config/ignored/**".to_string(), ".ssh/**".to_string()];
        let managed = ManagedSet::from_config(&cfg).expect("managed");
        (managed, paths.home_dir().to_path_buf())
    }

    #[test]
    fn track_inside_managed_paths() {
        let (managed, home) = config();
        let decision = decide_track(Path::new(".config/app/config.toml"), &home, &managed, false)
            .expect("decision");
        assert_eq!(decision.rel_path, PathBuf::from(".config/app/config.toml"));
        assert!(!decision.add_to_paths);
    }

    #[test]
    fn track_already_managed_file() {
        let (managed, home) = config();
        let decision = decide_track(Path::new(".zshrc"), &home, &managed, false).expect("decision");
        assert_eq!(decision.rel_path, PathBuf::from(".zshrc"));
        assert!(!decision.add_to_paths);
    }

    #[test]
    fn track_new_file_adds_to_paths() {
        let (managed, home) = config();
        let decision = decide_track(Path::new(".vimrc"), &home, &managed, false).expect("decision");
        assert_eq!(decision.rel_path, PathBuf::from(".vimrc"));
        assert!(decision.add_to_paths);
    }

    #[test]
    fn track_ignored_requires_force() {
        let (managed, home) = config();
        let err = decide_track(
            Path::new(".config/ignored/file.txt"),
            &home,
            &managed,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("ignored"));
    }

    #[test]
    fn track_ignored_with_force() {
        let (managed, home) = config();
        let decision = decide_track(Path::new(".config/ignored/file.txt"), &home, &managed, true)
            .expect("decision");
        assert_eq!(decision.rel_path, PathBuf::from(".config/ignored/file.txt"));
        assert!(decision.add_to_paths);
    }

    #[test]
    fn track_denylisted_requires_force() {
        let (managed, home) = config();
        let err = decide_track(Path::new(".ssh/id_rsa"), &home, &managed, false).unwrap_err();
        assert!(err.to_string().contains("ignored"));
    }
}

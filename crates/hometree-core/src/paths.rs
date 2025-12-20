use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::error::{HometreeError, Result};

#[derive(Debug, Clone)]
pub struct Paths {
    home_dir: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    state_dir: PathBuf,
    cache_dir: PathBuf,
    runtime_dir: Option<PathBuf>,
}

impl Paths {
    pub fn new() -> Result<Self> {
        Self::new_with_overrides(None, None)
    }

    pub fn new_with_overrides(home_root: Option<&Path>, xdg_root: Option<&Path>) -> Result<Self> {
        let base = BaseDirs::new();
        let home_dir = match home_root {
            Some(root) => root.to_path_buf(),
            None => base
                .as_ref()
                .ok_or(HometreeError::NoBaseDirs)?
                .home_dir()
                .to_path_buf(),
        };

        let (config_dir, data_dir, state_dir, cache_dir) = if let Some(xdg_root) = xdg_root {
            (
                xdg_root.join("config").join("hometree"),
                xdg_root.join("data").join("hometree"),
                xdg_root.join("state").join("hometree"),
                xdg_root.join("cache").join("hometree"),
            )
        } else if home_root.is_some() {
            (
                home_dir.join(".config").join("hometree"),
                home_dir.join(".local").join("share").join("hometree"),
                home_dir.join(".local").join("state").join("hometree"),
                home_dir.join(".cache").join("hometree"),
            )
        } else {
            let base = base.ok_or(HometreeError::NoBaseDirs)?;
            (
                base.config_dir().join("hometree"),
                base.data_dir().join("hometree"),
                base.state_dir().unwrap_or(base.data_dir()).join("hometree"),
                base.cache_dir().join("hometree"),
            )
        };

        let runtime_dir = std::env::var_os("HOMETREE_RUNTIME_DIR")
            .or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
            .map(PathBuf::from)
            .map(|base| base.join("hometree"));

        Ok(Self {
            home_dir,
            config_dir,
            data_dir,
            state_dir,
            cache_dir,
            runtime_dir,
        })
    }

    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn config_home_dir(&self) -> PathBuf {
        self.config_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.config_dir.clone())
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn runtime_dir(&self) -> Option<&Path> {
        self.runtime_dir.as_deref()
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn repo_dir(&self) -> PathBuf {
        self.data_dir.join("repo.git")
    }
}

#[cfg(test)]
mod tests {
    use super::Paths;
    use tempfile::TempDir;

    #[test]
    fn paths_include_hometree_suffix() {
        let paths = Paths::new().expect("paths resolve");
        assert!(paths.config_dir().ends_with("hometree"));
        assert!(paths.data_dir().ends_with("hometree"));
        assert!(paths.state_dir().ends_with("hometree"));
        assert!(paths.cache_dir().ends_with("hometree"));
    }

    #[test]
    fn paths_support_overrides() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");

        let paths = Paths::new_with_overrides(Some(&home), Some(&xdg)).expect("paths resolve");
        assert_eq!(paths.home_dir(), home);
        assert!(paths.config_dir().starts_with(xdg.join("config")));
        assert!(paths.data_dir().starts_with(xdg.join("data")));
        assert!(paths.state_dir().starts_with(xdg.join("state")));
        assert!(paths.cache_dir().starts_with(xdg.join("cache")));
    }
}

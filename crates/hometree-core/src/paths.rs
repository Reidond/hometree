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
}

impl Paths {
    pub fn new() -> Result<Self> {
        let base = BaseDirs::new().ok_or(HometreeError::NoBaseDirs)?;
        let home_dir = base.home_dir().to_path_buf();
        let config_dir = base.config_dir().join("hometree");
        let data_dir = base.data_dir().join("hometree");
        let state_dir = base.state_dir().unwrap_or(base.data_dir()).join("hometree");
        let cache_dir = base.cache_dir().join("hometree");

        Ok(Self {
            home_dir,
            config_dir,
            data_dir,
            state_dir,
            cache_dir,
        })
    }

    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
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

    #[test]
    fn paths_include_hometree_suffix() {
        let paths = Paths::new().expect("paths resolve");
        assert!(paths.config_dir().ends_with("hometree"));
        assert!(paths.data_dir().ends_with("hometree"));
        assert!(paths.state_dir().ends_with("hometree"));
        assert!(paths.cache_dir().ends_with("hometree"));
    }
}

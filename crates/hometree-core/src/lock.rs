use std::fs::{self, OpenOptions};

use fs2::FileExt;

use crate::error::Result;
use crate::Paths;

pub fn lock_path(paths: &Paths) -> std::path::PathBuf {
    paths.state_dir().join("lock")
}

pub fn acquire_lock(paths: &Paths) -> Result<std::fs::File> {
    fs::create_dir_all(paths.state_dir())?;
    let path = lock_path(paths);
    // Lock file is for locking only; no content needed, so truncate is irrelevant.
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    file.lock_exclusive()?;
    Ok(file)
}

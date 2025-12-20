use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{HometreeError, Result};
use crate::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InhibitMarker {
    pub reason: String,
    pub pid: u32,
    pub expires_at: String,
    pub epoch: u64,
}

impl InhibitMarker {
    pub fn new(reason: impl Into<String>, ttl: Duration) -> Result<Self> {
        let now = SystemTime::now();
        let expires = now + ttl;
        let expires_at = OffsetDateTime::from(expires)
            .format(&Rfc3339)
            .map_err(|err| HometreeError::Config(format!("invalid timestamp: {err}")))?;
        let epoch = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(Self {
            reason: reason.into(),
            pid: std::process::id(),
            expires_at,
            epoch,
        })
    }

    pub fn is_expired(&self, now: SystemTime) -> bool {
        let parsed = OffsetDateTime::parse(&self.expires_at, &Rfc3339);
        let expires_at = match parsed {
            Ok(ts) => ts,
            Err(_) => return true,
        };
        let now_ts = OffsetDateTime::from(now);
        now_ts >= expires_at
    }
}

pub fn inhibit_path(paths: &Paths) -> PathBuf {
    paths.state_dir().join("inhibit.json")
}

pub fn write_inhibit(paths: &Paths, marker: &InhibitMarker) -> Result<()> {
    fs::create_dir_all(paths.state_dir())?;
    let path = inhibit_path(paths);
    let contents = serde_json::to_string_pretty(marker)?;
    fs::write(path, contents)?;
    Ok(())
}

pub fn read_inhibit(paths: &Paths) -> Result<Option<InhibitMarker>> {
    let path = inhibit_path(paths);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)?;
    let marker: InhibitMarker = serde_json::from_str(&contents)?;
    Ok(Some(marker))
}

pub fn clear_inhibit(paths: &Paths) -> Result<()> {
    let path = inhibit_path(paths);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn active_inhibit(paths: &Paths) -> Result<Option<InhibitMarker>> {
    let marker = match read_inhibit(paths)? {
        Some(marker) => marker,
        None => return Ok(None),
    };
    if marker.is_expired(SystemTime::now()) {
        let _ = clear_inhibit(paths);
        return Ok(None);
    }
    Ok(Some(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Paths;
    use tempfile::TempDir;

    #[test]
    fn inhibit_roundtrip() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&xdg).unwrap();
        let paths = Paths::new_with_overrides(Some(&home), Some(&xdg)).expect("paths");

        let marker = InhibitMarker::new("test", Duration::from_secs(60)).expect("marker");
        write_inhibit(&paths, &marker).expect("write");
        let loaded = read_inhibit(&paths).expect("read").expect("marker");
        assert_eq!(loaded.reason, "test");
        assert!(active_inhibit(&paths).expect("active").is_some());
        clear_inhibit(&paths).expect("clear");
        assert!(read_inhibit(&paths).expect("read").is_none());
    }
}

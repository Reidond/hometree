use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationEntry {
    pub timestamp: u64,
    pub rev: String,
    pub message: Option<String>,
    pub host: String,
    pub user: String,
    pub config_hash: Option<String>,
}

pub fn append_generation(state_dir: &Path, entry: &GenerationEntry) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let path = state_dir.join("generations.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn read_generations(state_dir: &Path) -> Result<Vec<GenerationEntry>> {
    let path = state_dir.join("generations.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: GenerationEntry = serde_json::from_str(&line)?;
        entries.push(entry);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{append_generation, read_generations, GenerationEntry};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    #[test]
    fn generations_round_trip() {
        let dir = TempDir::new().expect("tempdir");
        let entry = GenerationEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            rev: "abc123".to_string(),
            message: Some("deploy".to_string()),
            host: "host".to_string(),
            user: "user".to_string(),
            config_hash: None,
        };
        append_generation(dir.path(), &entry).expect("append");
        let entries = read_generations(dir.path()).expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rev, "abc123");
    }
}

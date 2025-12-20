use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use age::{Decryptor, Encryptor};

use crate::config::{BackupPolicy, SecretRule, SecretsConfig};
use crate::error::{HometreeError, Result};

pub trait SecretsBackend {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}

pub struct AgeBackend {
    recipients: Vec<age::x25519::Recipient>,
    identities: Vec<age::x25519::Identity>,
}

impl AgeBackend {
    pub fn from_config(config: &SecretsConfig) -> Result<Self> {
        let mut recipients = Vec::new();
        for recipient in &config.recipients {
            let trimmed = recipient.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parsed = trimmed
                .parse::<age::x25519::Recipient>()
                .map_err(|_| HometreeError::Config(format!("invalid age recipient: {trimmed}")))?;
            recipients.push(parsed);
        }

        let mut identities = Vec::new();
        for path in &config.identity_files {
            let contents = fs::read_to_string(path)?;
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let parsed = trimmed.parse::<age::x25519::Identity>().map_err(|_| {
                    HometreeError::Config(format!("invalid age identity: {path:?}"))
                })?;
                identities.push(parsed);
            }
        }

        Ok(Self {
            recipients,
            identities,
        })
    }

    pub fn ensure_recipients(&self) -> Result<()> {
        if self.recipients.is_empty() {
            return Err(HometreeError::Config(
                "secrets enabled but recipients list is empty".to_string(),
            ));
        }
        Ok(())
    }

    pub fn ensure_identities(&self) -> Result<()> {
        if self.identities.is_empty() {
            return Err(HometreeError::Config(
                "secrets enabled but identity files are missing".to_string(),
            ));
        }
        Ok(())
    }
}

impl SecretsBackend for AgeBackend {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        self.ensure_recipients()?;
        let recipients: Vec<Box<dyn age::Recipient + Send>> = self
            .recipients
            .iter()
            .cloned()
            .map(|r| Box::new(r) as Box<dyn age::Recipient + Send>)
            .collect();
        let encryptor = Encryptor::with_recipients(recipients)
            .ok_or_else(|| HometreeError::Config("no recipients configured".to_string()))?;
        let mut out = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut out)
            .map_err(|e| HometreeError::Config(format!("age encrypt failed: {e}")))?;
        writer
            .write_all(plaintext)
            .map_err(|e| HometreeError::Config(format!("age encrypt failed: {e}")))?;
        writer
            .finish()
            .map_err(|e| HometreeError::Config(format!("age encrypt failed: {e}")))?;
        Ok(out)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        self.ensure_identities()?;
        let decryptor = match Decryptor::new(ciphertext)
            .map_err(|e| HometreeError::Config(format!("age decrypt failed: {e}")))?
        {
            Decryptor::Recipients(decryptor) => decryptor,
            Decryptor::Passphrase(_) => {
                return Err(HometreeError::Config(
                    "passphrase encryption is not supported".to_string(),
                ))
            }
        };
        let mut reader = decryptor
            .decrypt(self.identities.iter().map(|i| i as &dyn age::Identity))
            .map_err(|e| HometreeError::Config(format!("age decrypt failed: {e}")))?;
        let mut out = Vec::new();
        reader.read_to_end(&mut out)?;
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct SecretsManager {
    enabled: bool,
    sidecar_suffix: String,
    rules: Vec<SecretRule>,
    backup_policy: BackupPolicy,
}

impl SecretsManager {
    pub fn from_config(config: &SecretsConfig) -> Self {
        Self {
            enabled: config.enabled,
            sidecar_suffix: config.sidecar_suffix.clone(),
            rules: config.rules.clone(),
            backup_policy: config.backup_policy.clone(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn rules(&self) -> &[SecretRule] {
        &self.rules
    }

    pub fn backup_policy(&self) -> BackupPolicy {
        self.backup_policy.clone()
    }

    pub fn plaintext_path(&self, rule: &SecretRule) -> PathBuf {
        PathBuf::from(&rule.path)
    }

    pub fn ciphertext_path(&self, rule: &SecretRule) -> PathBuf {
        if let Some(ciphertext) = &rule.ciphertext {
            return PathBuf::from(ciphertext);
        }
        add_suffix(Path::new(&rule.path), &self.sidecar_suffix)
    }

    pub fn is_secret_plaintext(&self, path: &Path) -> bool {
        self.rules.iter().any(|rule| path == Path::new(&rule.path))
    }

    pub fn is_ciphertext_rule_path(&self, path: &Path) -> bool {
        self.rules
            .iter()
            .any(|rule| self.ciphertext_path(rule) == path)
    }

    pub fn is_ciphertext_path(&self, path: &Path) -> bool {
        path.to_string_lossy().ends_with(&self.sidecar_suffix)
    }
}

pub fn add_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.to_string_lossy().to_string();
    s.push_str(suffix);
    PathBuf::from(s)
}

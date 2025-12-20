use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::deploy::{collect_current_paths, collect_target_paths};
use crate::error::Result;
use crate::git::GitBackend;
use crate::secrets::{AgeBackend, SecretsBackend, SecretsManager};
use crate::{Config, ManagedSet, Paths};

#[derive(Debug, Clone, Copy)]
pub struct VerifyOptions {
    pub strict: bool,
    pub secrets_mode: SecretsVerifyMode,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretsVerifyMode {
    Skip,
    Presence,
    Decrypt,
}

#[derive(Debug, Serialize, Clone)]
pub struct VerifyReport {
    pub rev: String,
    pub strict: bool,
    pub secrets_mode: SecretsVerifyMode,
    pub missing: Vec<String>,
    pub modified: Vec<String>,
    pub type_mismatch: Vec<String>,
    pub mode_mismatch: Vec<String>,
    pub unexpected: Vec<String>,
    pub secret_missing_plaintext: Vec<String>,
    pub secret_missing_ciphertext: Vec<String>,
    pub secret_mismatch: Vec<String>,
    pub secret_decrypt_error: Vec<String>,
}

impl VerifyReport {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty()
            && self.modified.is_empty()
            && self.type_mismatch.is_empty()
            && self.mode_mismatch.is_empty()
            && self.unexpected.is_empty()
            && self.secret_missing_plaintext.is_empty()
            && self.secret_missing_ciphertext.is_empty()
            && self.secret_mismatch.is_empty()
            && self.secret_decrypt_error.is_empty()
    }
}

pub fn verify(
    config: &Config,
    paths: &Paths,
    git: &impl GitBackend,
    rev: &str,
    options: VerifyOptions,
) -> Result<VerifyReport> {
    let managed = ManagedSet::from_config(config)?;
    let resolved = git.rev_parse(&config.repo.git_dir, &config.repo.work_tree, rev)?;
    let secrets = SecretsManager::from_config(&config.secrets);
    let secrets_ref = if secrets.enabled() {
        Some(&secrets)
    } else {
        None
    };
    let target_entries = collect_target_paths(
        &managed,
        secrets_ref,
        git,
        &config.repo.git_dir,
        &config.repo.work_tree,
        &resolved,
    )?;

    let mut report = VerifyReport {
        rev: resolved,
        strict: options.strict,
        secrets_mode: options.secrets_mode,
        missing: Vec::new(),
        modified: Vec::new(),
        type_mismatch: Vec::new(),
        mode_mismatch: Vec::new(),
        unexpected: Vec::new(),
        secret_missing_plaintext: Vec::new(),
        secret_missing_ciphertext: Vec::new(),
        secret_mismatch: Vec::new(),
        secret_decrypt_error: Vec::new(),
    };

    let rev = report.rev.clone();

    verify_expected(
        &target_entries,
        paths.home_dir(),
        git,
        &config.repo.git_dir,
        &config.repo.work_tree,
        &rev,
        options.strict,
        &mut report,
    )?;

    if options.strict {
        let current_paths = collect_current_paths(
            &managed,
            secrets_ref,
            paths.home_dir(),
            &config.manage.roots,
            &config.manage.extra_files,
        )?;
        for rel in current_paths {
            if !target_entries.contains_key(&rel) {
                report.unexpected.push(rel.to_string_lossy().to_string());
            }
        }
    }

    verify_secrets(
        config,
        paths,
        options.secrets_mode,
        git,
        &config.repo.git_dir,
        &config.repo.work_tree,
        &rev,
        &target_entries,
        &mut report,
    )?;

    Ok(report)
}

fn verify_expected(
    target_entries: &BTreeMap<PathBuf, crate::git::TreeEntry>,
    home_dir: &Path,
    git: &impl GitBackend,
    git_dir: &Path,
    work_tree: &Path,
    rev: &str,
    strict: bool,
    report: &mut VerifyReport,
) -> Result<()> {
    for (rel, entry) in target_entries {
        let rel_str = rel.to_string_lossy().to_string();
        let abs = home_dir.join(rel);
        if !abs.exists() {
            report.missing.push(rel_str);
            continue;
        }

        let meta = fs::symlink_metadata(&abs)?;
        if entry.mode == "120000" {
            if !meta.file_type().is_symlink() {
                report.type_mismatch.push(rel_str);
                continue;
            }
            let expected = git.show_blob(git_dir, work_tree, rev, rel)?;
            let expected_target = String::from_utf8_lossy(&expected).to_string();
            let actual_target = fs::read_link(&abs)?.to_string_lossy().to_string();
            if actual_target != expected_target {
                report.modified.push(rel_str);
            }
            continue;
        }

        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            report.type_mismatch.push(rel_str);
            continue;
        }

        let expected = git.show_blob(git_dir, work_tree, rev, rel)?;
        let actual = fs::read(&abs)?;
        if actual != expected {
            report.modified.push(rel_str.clone());
        }

        if strict {
            let expected_exec = entry.mode == "100755";
            if exec_bit_mismatch(&meta, expected_exec) {
                report.mode_mismatch.push(rel_str);
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn exec_bit_mismatch(meta: &fs::Metadata, expected_exec: bool) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode();
    let actual_exec = mode & 0o111 != 0;
    actual_exec != expected_exec
}

#[cfg(not(unix))]
fn exec_bit_mismatch(_meta: &fs::Metadata, _expected_exec: bool) -> bool {
    false
}

fn verify_secrets(
    config: &Config,
    paths: &Paths,
    mode: SecretsVerifyMode,
    git: &impl GitBackend,
    git_dir: &Path,
    work_tree: &Path,
    rev: &str,
    target_entries: &BTreeMap<PathBuf, crate::git::TreeEntry>,
    report: &mut VerifyReport,
) -> Result<()> {
    if matches!(mode, SecretsVerifyMode::Skip) || !config.secrets.enabled {
        return Ok(());
    }

    let secrets = SecretsManager::from_config(&config.secrets);
    let backend = if matches!(mode, SecretsVerifyMode::Decrypt) {
        Some(AgeBackend::from_config(&config.secrets)?)
    } else {
        None
    };

    for rule in secrets.rules() {
        let plaintext_rel = secrets.plaintext_path(rule);
        let ciphertext_rel = secrets.ciphertext_path(rule);
        let plaintext_abs = paths.home_dir().join(&plaintext_rel);
        let ciphertext_in_repo = target_entries.contains_key(&ciphertext_rel);

        if !plaintext_abs.exists() {
            report
                .secret_missing_plaintext
                .push(plaintext_rel.to_string_lossy().to_string());
        }
        if !ciphertext_in_repo {
            report
                .secret_missing_ciphertext
                .push(ciphertext_rel.to_string_lossy().to_string());
        }

        if !matches!(mode, SecretsVerifyMode::Decrypt) {
            continue;
        }

        if !plaintext_abs.exists() || !ciphertext_in_repo {
            continue;
        }

        let plaintext = fs::read(&plaintext_abs)?;
        let ciphertext = git.show_blob(git_dir, work_tree, rev, &ciphertext_rel)?;
        let decrypted = match backend.as_ref().unwrap().decrypt(&ciphertext) {
            Ok(data) => data,
            Err(_) => {
                report
                    .secret_decrypt_error
                    .push(plaintext_rel.to_string_lossy().to_string());
                continue;
            }
        };
        if decrypted != plaintext {
            report
                .secret_mismatch
                .push(plaintext_rel.to_string_lossy().to_string());
        }
    }

    Ok(())
}

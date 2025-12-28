use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::deploy::{collect_current_paths, collect_target_paths};
use crate::error::Result;
use crate::git::GitBackend;
use crate::secrets::SecretsManager;
use crate::{Config, ManagedSet, Paths};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanEntry {
    pub action: PlanAction,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployPlan {
    pub rev: String,
    pub entries: Vec<PlanEntry>,
}

pub fn plan_deploy(
    config: &Config,
    paths: &Paths,
    git: &impl GitBackend,
    rev: &str,
) -> Result<DeployPlan> {
    let managed = ManagedSet::from_config(config)?;
    let secrets = SecretsManager::from_config(&config.secrets);
    let secrets_ref = if secrets.enabled() {
        Some(&secrets)
    } else {
        None
    };
    let resolved = git.rev_parse(&config.repo.git_dir, &config.repo.work_tree, rev)?;

    let target_entries = collect_target_paths(
        &managed,
        secrets_ref,
        git,
        &config.repo.git_dir,
        &config.repo.work_tree,
        &resolved,
    )?;
    let current_paths = collect_current_paths(
        &managed,
        secrets_ref,
        paths.home_dir(),
        &config.manage.paths,
    )?;

    let mut entries = Vec::new();
    let target_paths: BTreeSet<PathBuf> = target_entries.keys().cloned().collect();

    for rel in target_paths.iter() {
        let abs = paths.home_dir().join(rel);
        if !abs.exists() {
            entries.push(PlanEntry {
                action: PlanAction::Create,
                path: rel.to_string_lossy().to_string(),
            });
            continue;
        }

        let target_blob =
            match git.show_blob(&config.repo.git_dir, &config.repo.work_tree, &resolved, rel) {
                Ok(blob) => blob,
                Err(_) => continue,
            };

        let needs_update = if let Ok(meta) = fs::symlink_metadata(&abs) {
            if meta.file_type().is_symlink() {
                let link_target = fs::read_link(&abs).unwrap_or_default();
                let target_str = String::from_utf8_lossy(&target_blob);
                link_target.to_string_lossy() != target_str.trim_end_matches('\0')
            } else {
                let current = fs::read(&abs).unwrap_or_default();
                current != target_blob
            }
        } else {
            true
        };

        if needs_update {
            entries.push(PlanEntry {
                action: PlanAction::Update,
                path: rel.to_string_lossy().to_string(),
            });
        }
    }

    for rel in current_paths {
        if !target_paths.contains(&rel) {
            entries.push(PlanEntry {
                action: PlanAction::Delete,
                path: rel.to_string_lossy().to_string(),
            });
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(DeployPlan {
        rev: resolved,
        entries,
    })
}

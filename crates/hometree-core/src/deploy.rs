use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use filetime::{self, FileTime};
#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use walkdir::WalkDir;

use crate::error::Result;
use crate::lock::acquire_lock;
use crate::generations::{append_generation, GenerationEntry};
use crate::git::{GitBackend, TreeEntry};
use crate::secrets::{AgeBackend, SecretsBackend, SecretsManager};
use crate::{Config, ManagedSet, Paths};

pub fn deploy(
    config: &Config,
    paths: &Paths,
    git: &impl GitBackend,
    rev: &str,
) -> Result<GenerationEntry> {
    deploy_with_options(config, paths, git, rev, DeployOptions { no_backup: false })
}

#[derive(Debug, Clone, Copy)]
pub struct DeployOptions {
    pub no_backup: bool,
}

pub fn deploy_with_options(
    config: &Config,
    paths: &Paths,
    git: &impl GitBackend,
    rev: &str,
    options: DeployOptions,
) -> Result<GenerationEntry> {
    let _lock = acquire_lock(paths)?;
    let managed = ManagedSet::from_config(config)?;
    let secrets = SecretsManager::from_config(&config.secrets);
    let secrets_ref = if secrets.enabled() { Some(&secrets) } else { None };
    let secrets_backend = if secrets.enabled() {
        Some(AgeBackend::from_config(&config.secrets)?)
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
        &config.manage.roots,
        &config.manage.extra_files,
    )?;

    if !options.no_backup {
        let backup_dir = create_backup_dir(paths)?;
        backup_current(&backup_dir, paths.home_dir(), &current_paths)?;
        backup_secrets(
            &backup_dir,
            paths.home_dir(),
            &secrets,
            secrets_backend.as_ref(),
        )?;
    }

    apply_target(
        paths.home_dir(),
        git,
        &config.repo.git_dir,
        &config.repo.work_tree,
        &resolved,
        &target_entries,
    )?;

    apply_secrets(
        paths.home_dir(),
        &secrets,
        secrets_backend.as_ref(),
        git,
        &config.repo.git_dir,
        &config.repo.work_tree,
        &resolved,
    )?;

    let target_paths: BTreeSet<PathBuf> = target_entries.keys().cloned().collect();
    delete_missing(paths.home_dir(), &current_paths, &target_paths)?;

    let entry = GenerationEntry {
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        rev: resolved,
        message: None,
        host: std::env::var("HOSTNAME").unwrap_or_default(),
        user: std::env::var("USER").unwrap_or_default(),
        config_hash: None,
    };
    append_generation(paths.state_dir(), &entry)?;
    Ok(entry)
}

pub fn rollback(
    config: &Config,
    paths: &Paths,
    git: &impl GitBackend,
    rev: &str,
) -> Result<GenerationEntry> {
    deploy(config, paths, git, rev)
}

fn create_backup_dir(paths: &Paths) -> Result<PathBuf> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dir = paths.state_dir().join("backups").join(ts.to_string());
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn collect_target_paths(
    managed: &ManagedSet,
    secrets: Option<&SecretsManager>,
    git: &impl GitBackend,
    git_dir: &Path,
    work_tree: &Path,
    rev: &str,
) -> Result<BTreeMap<PathBuf, TreeEntry>> {
    let mut map = BTreeMap::new();
    let entries = git.ls_tree_detailed(git_dir, work_tree, rev)?;
    for entry in entries {
        let rel = PathBuf::from(&entry.path);
        let is_secret_cipher = secrets
            .map(|secrets| secrets.is_ciphertext_rule_path(&rel))
            .unwrap_or(false);
        if managed.is_managed(&rel) || is_secret_cipher {
            map.insert(rel, entry);
        }
    }
    Ok(map)
}

pub(crate) fn collect_current_paths(
    managed: &ManagedSet,
    secrets: Option<&SecretsManager>,
    home_dir: &Path,
    roots: &[String],
    extra_files: &[String],
) -> Result<BTreeSet<PathBuf>> {
    let mut set = BTreeSet::new();

    for extra in extra_files {
        let rel = PathBuf::from(extra);
        let abs = home_dir.join(&rel);
        let is_secret_cipher = secrets
            .map(|secrets| secrets.is_ciphertext_rule_path(&rel))
            .unwrap_or(false);
        if abs.exists() && (managed.is_managed(&rel) || is_secret_cipher) {
            set.insert(rel);
        }
    }

    for root in roots {
        let root_path = normalize_root(root);
        if root_path.as_os_str().is_empty() {
            continue;
        }
        let abs_root = home_dir.join(&root_path);
        if !abs_root.exists() {
            continue;
        }
        for entry in WalkDir::new(&abs_root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                continue;
            }
            let rel = match entry.path().strip_prefix(home_dir) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };
            let is_secret_cipher = secrets
                .map(|secrets| secrets.is_ciphertext_rule_path(&rel))
                .unwrap_or(false);
            if managed.is_managed(&rel) || is_secret_cipher {
                set.insert(rel);
            }
        }
    }

    Ok(set)
}

fn normalize_root(root: &str) -> PathBuf {
    let trimmed = root.trim_start_matches("./");
    let trimmed = trimmed.trim_end_matches("/**").trim_end_matches('/');
    if has_glob_meta(trimmed) {
        return PathBuf::new();
    }
    PathBuf::from(trimmed)
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains('{')
}

fn backup_current(backup_dir: &Path, home_dir: &Path, current: &BTreeSet<PathBuf>) -> Result<()> {
    for rel in current {
        let src = home_dir.join(rel);
        let dest = backup_dir.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Ok(meta) = fs::symlink_metadata(&src) {
            if meta.file_type().is_symlink() {
                #[cfg(unix)]
                {
                    let target = fs::read_link(&src)?;
                    let _ = std::os::unix::fs::symlink(&target, &dest);
                }
                #[cfg(not(unix))]
                {
                    let _ = fs::copy(&src, &dest);
                }
                continue;
            }
        }
        let _ = fs::copy(&src, &dest);
    }
    Ok(())
}

fn backup_secrets(
    backup_dir: &Path,
    home_dir: &Path,
    secrets: &SecretsManager,
    backend: Option<&AgeBackend>,
) -> Result<()> {
    if !secrets.enabled() {
        return Ok(());
    }

    for rule in secrets.rules() {
        let plaintext_rel = secrets.plaintext_path(rule);
        let plaintext_abs = home_dir.join(&plaintext_rel);
        if !plaintext_abs.exists() {
            continue;
        }
        match secrets.backup_policy() {
            crate::config::BackupPolicy::Skip => {}
            crate::config::BackupPolicy::Plaintext => {
                let dest = backup_dir.join(&plaintext_rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                let _ = fs::copy(&plaintext_abs, &dest);
            }
            crate::config::BackupPolicy::Encrypt => {
                let backend = backend.ok_or_else(|| {
                    std::io::Error::other("secrets backend missing for backup")
                })?;
                let plaintext = fs::read(&plaintext_abs)?;
                let ciphertext = backend.encrypt(&plaintext)?;
                let ciphertext_rel = secrets.ciphertext_path(rule);
                let dest = backup_dir.join(ciphertext_rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&dest, ciphertext)?;
            }
        }
    }

    Ok(())
}

fn apply_secrets(
    home_dir: &Path,
    secrets: &SecretsManager,
    backend: Option<&AgeBackend>,
    git: &impl GitBackend,
    git_dir: &Path,
    work_tree: &Path,
    rev: &str,
) -> Result<()> {
    if !secrets.enabled() {
        return Ok(());
    }
    let backend = backend.ok_or_else(|| std::io::Error::other("secrets backend missing"))?;

    for rule in secrets.rules() {
        let plaintext_rel = secrets.plaintext_path(rule);
        let ciphertext_rel = secrets.ciphertext_path(rule);
        let ciphertext = git.show_blob(git_dir, work_tree, rev, &ciphertext_rel)?;
        let plaintext = backend.decrypt(&ciphertext)?;
        let dest = home_dir.join(&plaintext_rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Ok(meta) = fs::symlink_metadata(&dest) {
            if meta.is_dir() {
                return Err(
                    std::io::Error::other("refusing to replace directory with secret file").into(),
                );
            }
            if meta.file_type().is_symlink() {
                fs::remove_file(&dest)?;
            }
        }
        let mode = rule.mode.unwrap_or(0o600);
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options.open(&dest)?;
        file.write_all(&plaintext)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dest, fs::Permissions::from_mode(mode))?;
        }
    }

    Ok(())
}

fn validate_symlink_target(home_dir: &Path, symlink_path: &Path, target: &str) -> Result<()> {
    if !symlink_path.starts_with(home_dir) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "symlink path must live under home: {}",
                symlink_path.display()
            ),
        )
        .into());
    }

    let target_path = Path::new(target);
    let base = symlink_path.parent().unwrap_or(home_dir);
    let resolved = normalize_symlink_target(base, target_path);

    if resolved.starts_with(home_dir) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "symlink target escapes home directory: {} -> {}",
                symlink_path.display(),
                target
            ),
        )
        .into())
    }
}

fn normalize_symlink_target(base: &Path, target: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();

    if target.is_absolute() {
        normalized.push(Path::new("/"));
    } else {
        normalized.push(base);
    }

    for component in target.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => {
                normalized.clear();
                normalized.push(Path::new("/"));
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

#[cfg(unix)]
#[derive(Clone, Debug)]
struct PreservedMetadata {
    uid: u32,
    gid: u32,
    atime: FileTime,
    mtime: FileTime,
}

#[cfg(not(unix))]
#[derive(Clone, Debug)]
struct PreservedMetadata;

#[cfg(unix)]
fn capture_metadata(path: &Path) -> Option<PreservedMetadata> {
    fs::metadata(path).ok().map(|meta| PreservedMetadata {
        uid: meta.uid(),
        gid: meta.gid(),
        atime: FileTime::from_unix_time(meta.atime(), meta.atime_nsec() as u32),
        mtime: FileTime::from_unix_time(meta.mtime(), meta.mtime_nsec() as u32),
    })
}

#[cfg(not(unix))]
fn capture_metadata(_path: &Path) -> Option<PreservedMetadata> {
    None
}

#[cfg(unix)]
fn restore_metadata(path: &Path, meta: &PreservedMetadata) {
    let _ = filetime::set_file_times(path, meta.atime, meta.mtime);
    if let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) {
        // Restore owner/group if they differ; this is best-effort and may fail without privileges.
        unsafe {
            let _ = libc::chown(c_path.as_ptr(), meta.uid, meta.gid);
        }
    }
}

#[cfg(not(unix))]
fn restore_metadata(_path: &Path, _meta: &PreservedMetadata) {}

fn apply_target(
    home_dir: &Path,
    git: &impl GitBackend,
    git_dir: &Path,
    work_tree: &Path,
    rev: &str,
    target: &BTreeMap<PathBuf, TreeEntry>,
) -> Result<()> {
    for (rel, entry) in target {
        let dest = home_dir.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        if entry.mode == "120000" {
            let data = git.show_blob(git_dir, work_tree, rev, rel)?;
            let target = String::from_utf8_lossy(&data).to_string();

            // Validate symlink target doesn't escape home directory
            validate_symlink_target(home_dir, &dest, &target)?;

            if let Ok(meta) = fs::symlink_metadata(&dest) {
                if meta.is_dir() {
                    return Err(std::io::Error::other(format!(
                        "refusing to replace directory with symlink: {}",
                        dest.display()
                    ))
                    .into());
                }
                fs::remove_file(&dest)?;
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &dest)?;
            }
            #[cfg(not(unix))]
            {
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&dest)?;
                file.write_all(&data)?;
            }
            continue;
        }

        let preserved_meta = match fs::symlink_metadata(&dest) {
            Ok(meta) => {
                if meta.is_dir() {
                    return Err(std::io::Error::other(format!(
                        "refusing to replace directory with file: {}",
                        dest.display()
                    ))
                    .into());
                }
                if meta.file_type().is_symlink() {
                    fs::remove_file(&dest)?;
                    None
                } else if !meta.file_type().is_file() {
                    return Err(std::io::Error::other(format!(
                        "refusing to replace non-regular file with file: {}",
                        dest.display()
                    ))
                    .into());
                } else {
                    capture_metadata(&dest)
                }
            }
            Err(_) => None,
        };
        let data = git.show_blob(git_dir, work_tree, rev, rel)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&dest)?;
        file.write_all(&data)?;
        drop(file); // Close file before setting metadata

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&dest)?.permissions();
            let mut mode = perms.mode();
            if entry.mode == "100755" {
                mode |= 0o111;
            } else {
                mode &= !0o111;
            }
            perms.set_mode(mode);
            let _ = fs::set_permissions(&dest, perms);
        }

        if let Some(meta) = preserved_meta {
            restore_metadata(&dest, &meta);
        }
    }
    Ok(())
}

fn delete_missing(
    home_dir: &Path,
    current: &BTreeSet<PathBuf>,
    target: &BTreeSet<PathBuf>,
) -> Result<()> {
    for rel in current {
        if target.contains(rel) {
            continue;
        }
        let path = home_dir.join(rel);
        let _ = fs::remove_file(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_target, collect_current_paths, validate_symlink_target};
    use crate::git::{BranchInfo, FileStatus, GitBackend, GitError, TreeEntry};
    use crate::{Config, ManagedSet, Paths};
    use filetime::FileTime;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    #[derive(Default)]
    struct StubGit {
        blobs: BTreeMap<PathBuf, Vec<u8>>,
    }

    type Result<T> = crate::git::GitResult<T>;

    impl GitBackend for StubGit {
        fn init_repo(&self, _git_dir: &Path) -> Result<()> {
            unimplemented!()
        }

        fn status_porcelain(
            &self,
            _git_dir: &Path,
            _work_tree: &Path,
            _paths: &[PathBuf],
            _include_untracked: bool,
        ) -> Result<Vec<FileStatus>> {
            unimplemented!()
        }

        fn branch_info(&self, _git_dir: &Path, _work_tree: &Path) -> Result<BranchInfo> {
            unimplemented!()
        }

        fn is_repository(&self, _git_dir: &Path) -> bool {
            true
        }

        fn add(
            &self,
            _git_dir: &Path,
            _work_tree: &Path,
            _paths: &[PathBuf],
            _mode: crate::git::AddMode,
        ) -> Result<()> {
            unimplemented!()
        }

        fn commit(&self, _git_dir: &Path, _work_tree: &Path, _message: &str) -> Result<String> {
            unimplemented!()
        }

        fn log(&self, _git_dir: &Path, _work_tree: &Path, _limit: Option<usize>) -> Result<String> {
            unimplemented!()
        }

        fn rev_parse(&self, _git_dir: &Path, _work_tree: &Path, _rev: &str) -> Result<String> {
            unimplemented!()
        }

        fn ls_tree(&self, _git_dir: &Path, _work_tree: &Path, _rev: &str) -> Result<Vec<String>> {
            unimplemented!()
        }

        fn ls_tree_detailed(
            &self,
            _git_dir: &Path,
            _work_tree: &Path,
            _rev: &str,
        ) -> Result<Vec<TreeEntry>> {
            unimplemented!()
        }

        fn show_blob(
            &self,
            _git_dir: &Path,
            _work_tree: &Path,
            _rev: &str,
            path: &Path,
        ) -> Result<Vec<u8>> {
            self.blobs
                .get(path)
                .cloned()
                .ok_or_else(|| GitError::CommandFailed(format!("missing blob: {}", path.display())))
        }

        fn config_set(
            &self,
            _git_dir: &Path,
            _work_tree: &Path,
            _key: &str,
            _value: &str,
        ) -> Result<()> {
            unimplemented!()
        }

        fn checkout(&self, _git_dir: &Path, _work_tree: &Path, _rev: &str) -> Result<()> {
            unimplemented!()
        }

        fn get_commit_info(
            &self,
            _git_dir: &Path,
            _work_tree: &Path,
            _rev: &str,
        ) -> Result<String> {
            unimplemented!()
        }
    }

    #[test]
    fn apply_target_preserves_existing_timestamps() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();
        let rel = PathBuf::from(".config/app/config.toml");
        let dest = home.join(&rel);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, "old").unwrap();

        let preserved = FileTime::from_unix_time(1_600_000_000, 0);
        filetime::set_file_times(&dest, preserved, preserved).unwrap();

        let mut blobs = BTreeMap::new();
        blobs.insert(rel.clone(), b"new contents".to_vec());
        let git = StubGit { blobs };

        let mut target = BTreeMap::new();
        target.insert(
            rel.clone(),
            TreeEntry {
                mode: "100644".to_string(),
                path: rel.to_string_lossy().to_string(),
            },
        );

        apply_target(home, &git, Path::new("."), Path::new("."), "HEAD", &target).unwrap();

        let meta = fs::metadata(&dest).unwrap();
        let mtime = FileTime::from_last_modification_time(&meta);
        assert_eq!(mtime, preserved);
    }

    #[test]
    #[cfg(unix)]
    fn apply_target_replaces_symlink_without_dereferencing() {
        let temp_home = TempDir::new().expect("temp");
        let home = temp_home.path();
        let temp_outside = TempDir::new().expect("temp");
        let victim = temp_outside.path().join("victim.txt");
        fs::write(&victim, "victim").unwrap();

        let rel = PathBuf::from(".config/app/config.toml");
        let dest = home.join(&rel);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&victim, &dest).unwrap();

        let mut blobs = BTreeMap::new();
        blobs.insert(rel.clone(), b"new contents".to_vec());
        let git = StubGit { blobs };

        let mut target = BTreeMap::new();
        target.insert(
            rel.clone(),
            TreeEntry {
                mode: "100644".to_string(),
                path: rel.to_string_lossy().to_string(),
            },
        );

        apply_target(home, &git, Path::new("."), Path::new("."), "HEAD", &target).unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "victim");
        let meta = fs::symlink_metadata(&dest).unwrap();
        assert!(!meta.file_type().is_symlink());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "new contents");
    }

    #[test]
    fn collect_current_paths_filters() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();
        fs::create_dir_all(home.join(".config/app")).unwrap();
        fs::write(home.join(".config/app/config.toml"), "ok").unwrap();
        fs::create_dir_all(home.join(".config/ignored")).unwrap();
        fs::write(home.join(".config/ignored/secret.txt"), "no").unwrap();

        let paths = Paths::new().expect("paths");
        let mut config = Config::default_with_paths(&paths);
        config.manage.roots = vec![".config/".to_string()];
        config.manage.extra_files = vec![".zshrc".to_string()];
        config.ignore.patterns = vec![".config/ignored/**".to_string()];
        let managed = ManagedSet::from_config(&config).expect("managed");

        let current = collect_current_paths(
            &managed,
            None,
            home,
            &config.manage.roots,
            &config.manage.extra_files,
        )
        .expect("collect");

        assert!(current.contains(&PathBuf::from(".config/app/config.toml")));
        assert!(!current.contains(&PathBuf::from(".config/ignored/secret.txt")));
    }

    #[test]
    fn validate_symlink_relative_ok() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();
        let link = home.join(".config/link");
        assert!(validate_symlink_target(home, &link, "../target").is_ok());
        assert!(validate_symlink_target(home, &link, "target").is_ok());
        assert!(validate_symlink_target(home, &link, "./.local/data").is_ok());
    }

    #[test]
    fn validate_symlink_relative_escape_fails() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();
        let link = home.join(".config/link");
        // Escaping from .config/ up to /.. should fail
        assert!(validate_symlink_target(home, &link, "../../etc/passwd").is_err());
        assert!(validate_symlink_target(home, &link, "../..").is_err());
    }

    #[test]
    fn validate_symlink_absolute_ok() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();
        let link = home.join(".config/link");
        let target = home.join(".local/share/data").to_string_lossy().to_string();
        assert!(validate_symlink_target(home, &link, &target).is_ok());
    }

    #[test]
    fn validate_symlink_escape_fails() {
        let temp = TempDir::new().expect("temp");
        let home = temp.path();
        let link = home.join(".config/link");
        assert!(validate_symlink_target(home, &link, "/etc/passwd").is_err());
        assert!(validate_symlink_target(home, &link, "/tmp/evil").is_err());
    }
}

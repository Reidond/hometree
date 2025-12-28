use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatus {
    pub path: String,
    pub status: StatusCode,
    pub index_status: char,
    pub worktree_status: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    Unmodified,
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    Untracked,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub oid: String,
    pub head: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

pub type GitResult<T> = Result<T, GitError>;

#[derive(thiserror::Error, Debug)]
pub enum GitError {
    #[error("git command failed: {0}")]
    CommandFailed(String),
    #[error("failed to parse git output: {0}")]
    ParseError(String),
    #[error("not a git repository")]
    NotARepository,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub enum AddMode {
    /// Stage only tracked changes (equivalent to git add -u).
    TrackedOnly,
    /// Stage all provided paths.
    Paths,
}

pub trait GitBackend {
    fn init_repo(&self, git_dir: &Path) -> GitResult<()>;

    fn status_porcelain(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        paths: &[PathBuf],
        include_untracked: bool,
    ) -> GitResult<Vec<FileStatus>>;

    fn branch_info(&self, git_dir: &Path, work_tree: &Path) -> GitResult<BranchInfo>;

    fn is_repository(&self, git_dir: &Path) -> bool;

    fn add(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        paths: &[PathBuf],
        mode: AddMode,
    ) -> GitResult<()>;

    fn commit(&self, git_dir: &Path, work_tree: &Path, message: &str) -> GitResult<String>;

    fn log(&self, git_dir: &Path, work_tree: &Path, limit: Option<usize>) -> GitResult<String>;

    fn log_detailed(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        limit: Option<usize>,
    ) -> GitResult<Vec<LogEntry>>;

    fn rev_parse(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<String>;

    fn ls_tree(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<Vec<String>>;

    fn ls_tree_detailed(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        rev: &str,
    ) -> GitResult<Vec<TreeEntry>>;

    fn show_blob(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        rev: &str,
        path: &Path,
    ) -> GitResult<Vec<u8>>;

    fn config_set(&self, git_dir: &Path, work_tree: &Path, key: &str, value: &str)
        -> GitResult<()>;

    fn checkout(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<()>;

    fn get_commit_info(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<String>;

    fn remote_add(&self, git_dir: &Path, work_tree: &Path, name: &str, url: &str) -> GitResult<()>;

    fn remote_remove(&self, git_dir: &Path, work_tree: &Path, name: &str) -> GitResult<()>;

    fn remote_list(&self, git_dir: &Path, work_tree: &Path) -> GitResult<Vec<RemoteInfo>>;

    fn push(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        remote: &str,
        refspec: Option<&str>,
        set_upstream: bool,
        force: bool,
    ) -> GitResult<String>;

    fn pull(&self, git_dir: &Path, work_tree: &Path, remote: &str) -> GitResult<String>;

    /// Reset the index to match a given revision without touching the work tree.
    /// Equivalent to `git reset <rev>` (mixed reset).
    fn reset(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub hash: String,
    pub date: String,
    pub message: String,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub status: FileChangeStatus,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unknown,
}

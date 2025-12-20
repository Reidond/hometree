mod backend;
mod cli;

pub use backend::{
    AddMode, BranchInfo, FileStatus, GitBackend, GitError, GitResult, StatusCode, TreeEntry,
};
pub use cli::GitCliBackend;

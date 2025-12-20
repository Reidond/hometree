use std::path::{Path, PathBuf};
use std::process::Command;

use super::backend::{
    AddMode, BranchInfo, FileStatus, GitBackend, GitError, GitResult, StatusCode, TreeEntry,
};

#[derive(Debug, Default, Clone)]
pub struct GitCliBackend;

impl GitCliBackend {
    pub fn new() -> Self {
        Self
    }

    fn run_command(&self, git_dir: &Path, work_tree: &Path, args: &[&str]) -> GitResult<String> {
        let output = Command::new("git")
            .args(["--git-dir", git_dir.to_string_lossy().as_ref()])
            .args(["--work-tree", work_tree.to_string_lossy().as_ref()])
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn run_command_bytes(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        args: &[&str],
    ) -> GitResult<Vec<u8>> {
        let output = Command::new("git")
            .args(["--git-dir", git_dir.to_string_lossy().as_ref()])
            .args(["--work-tree", work_tree.to_string_lossy().as_ref()])
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitError::CommandFailed(stderr.to_string()));
        }

        Ok(output.stdout)
    }

    fn run_command_owned(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        args: &[String],
    ) -> GitResult<String> {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_command(git_dir, work_tree, &refs)
    }

    fn parse_porcelain_v2_status(output: &str) -> GitResult<Vec<FileStatus>> {
        let mut statuses = Vec::new();

        for line in output.split('\0') {
            if line.is_empty() {
                continue;
            }

            if let Some(entry) = parse_status_entry(line)? {
                statuses.push(entry);
            }
        }

        Ok(statuses)
    }

    fn parse_porcelain_v2_branch(output: &str) -> GitResult<Option<BranchInfo>> {
        let mut oid = None;
        let mut head = None;
        let mut upstream = None;
        let mut ahead = 0;
        let mut behind = 0;

        for line in output.split('\0') {
            if line.is_empty() {
                continue;
            }

            if line.starts_with("# branch.oid ") {
                oid = Some(line.strip_prefix("# branch.oid ").unwrap().to_string());
            } else if line.starts_with("# branch.head ") {
                head = Some(line.strip_prefix("# branch.head ").unwrap().to_string());
            } else if line.starts_with("# branch.upstream ") {
                upstream = Some(line.strip_prefix("# branch.upstream ").unwrap().to_string());
            } else if line.starts_with("# branch.ab ") {
                let ab = line.strip_prefix("# branch.ab ").unwrap();
                let parts: Vec<&str> = ab.split_whitespace().collect();
                if parts.len() == 2 {
                    ahead = parts[0].trim_start_matches('+').parse().unwrap_or(0);
                    behind = parts[1].trim_start_matches('-').parse().unwrap_or(0);
                }
            }
        }

        if let (Some(oid), Some(head)) = (oid, head) {
            Ok(Some(BranchInfo {
                oid,
                head,
                upstream,
                ahead,
                behind,
            }))
        } else {
            Ok(None)
        }
    }
}

impl GitBackend for GitCliBackend {
    fn init_repo(&self, git_dir: &Path) -> GitResult<()> {
        let status = Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(git_dir)
            .status()?;
        if !status.success() {
            return Err(GitError::CommandFailed("git init --bare failed".into()));
        }
        Ok(())
    }

    fn status_porcelain(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        paths: &[PathBuf],
        include_untracked: bool,
    ) -> GitResult<Vec<FileStatus>> {
        let mut args = vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "-z".to_string(),
        ];
        let untracked = if include_untracked { "all" } else { "no" };
        args.push(format!("--untracked-files={untracked}"));
        if !paths.is_empty() {
            args.push("--".to_string());
            for path in paths {
                args.push(path.to_string_lossy().to_string());
            }
        }
        let output = self.run_command_owned(git_dir, work_tree, &args)?;

        Self::parse_porcelain_v2_status(&output)
    }

    fn branch_info(&self, git_dir: &Path, work_tree: &Path) -> GitResult<BranchInfo> {
        let output = self.run_command(
            git_dir,
            work_tree,
            &["status", "--porcelain=v2", "-z", "--branch"],
        )?;

        Self::parse_porcelain_v2_branch(&output)?
            .ok_or_else(|| GitError::ParseError("no branch info found".to_string()))
    }

    fn is_repository(&self, git_dir: &Path) -> bool {
        Command::new("git")
            .args([
                "--git-dir",
                git_dir.to_string_lossy().as_ref(),
                "rev-parse",
                "--git-dir",
            ])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn add(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        paths: &[PathBuf],
        mode: AddMode,
    ) -> GitResult<()> {
        let mut args = vec!["add".to_string()];
        match mode {
            AddMode::TrackedOnly => {
                args.push("-u".to_string());
                args.push("--".to_string());
            }
            AddMode::Paths => {
                args.push("--".to_string());
            }
        }
        for path in paths {
            args.push(path.to_string_lossy().to_string());
        }
        self.run_command_owned(git_dir, work_tree, &args)?;
        Ok(())
    }

    fn commit(&self, git_dir: &Path, work_tree: &Path, message: &str) -> GitResult<String> {
        self.run_command(git_dir, work_tree, &["commit", "-m", message])
    }

    fn log(&self, git_dir: &Path, work_tree: &Path, limit: Option<usize>) -> GitResult<String> {
        let mut args = vec!["log".to_string(), "--oneline".to_string()];
        if let Some(limit) = limit {
            args.push("-n".to_string());
            args.push(limit.to_string());
        }
        self.run_command_owned(git_dir, work_tree, &args)
    }

    fn rev_parse(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<String> {
        let output = self.run_command(git_dir, work_tree, &["rev-parse", rev])?;
        Ok(output.trim().to_string())
    }

    fn ls_tree(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<Vec<String>> {
        let output = self.run_command(
            git_dir,
            work_tree,
            &["ls-tree", "-r", "--name-only", "-z", rev],
        )?;
        let mut paths = Vec::new();
        for part in output.split('\0') {
            if !part.is_empty() {
                paths.push(part.to_string());
            }
        }
        Ok(paths)
    }

    fn ls_tree_detailed(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        rev: &str,
    ) -> GitResult<Vec<TreeEntry>> {
        let output = self.run_command(git_dir, work_tree, &["ls-tree", "-r", "-z", rev])?;
        let mut entries = Vec::new();
        for part in output.split('\0') {
            if part.is_empty() {
                continue;
            }
            let mut meta = part.splitn(2, '\t');
            let left = meta.next().unwrap_or("");
            let path = meta.next().unwrap_or("");
            let mut fields = left.split_whitespace();
            let mode = fields.next().unwrap_or("");
            if path.is_empty() || mode.is_empty() {
                continue;
            }
            entries.push(TreeEntry {
                mode: mode.to_string(),
                path: path.to_string(),
            });
        }
        Ok(entries)
    }

    fn show_blob(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        rev: &str,
        path: &Path,
    ) -> GitResult<Vec<u8>> {
        let spec = format!("{}:{}", rev, path.to_string_lossy());
        self.run_command_bytes(git_dir, work_tree, &["show", &spec])
    }

    fn config_set(
        &self,
        git_dir: &Path,
        work_tree: &Path,
        key: &str,
        value: &str,
    ) -> GitResult<()> {
        self.run_command(git_dir, work_tree, &["config", key, value])?;
        Ok(())
    }

    fn checkout(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<()> {
        self.run_command(git_dir, work_tree, &["checkout", "-f", rev])?;
        Ok(())
    }

    fn get_commit_info(&self, git_dir: &Path, work_tree: &Path, rev: &str) -> GitResult<String> {
        self.run_command(git_dir, work_tree, &["log", "-1", "--oneline", rev])
    }
}

fn parse_status_code(c: char) -> StatusCode {
    match c {
        '.' => StatusCode::Unmodified,
        'M' => StatusCode::Modified,
        'T' => StatusCode::TypeChanged,
        'A' => StatusCode::Added,
        'D' => StatusCode::Deleted,
        'R' => StatusCode::Renamed,
        'C' => StatusCode::Copied,
        'U' => StatusCode::Unmerged,
        '?' => StatusCode::Untracked,
        '!' => StatusCode::Ignored,
        _ => StatusCode::Unmodified,
    }
}

fn parse_status_entry(line: &str) -> GitResult<Option<FileStatus>> {
    if line.is_empty() || line.starts_with("# ") {
        return Ok(None);
    }

    if line.starts_with("1 ") {
        let mut parts = line.splitn(9, ' ');
        let _record_type = parts.next();
        let xy = parts.next().unwrap_or("..");
        if xy.len() != 2 {
            return Err(GitError::ParseError(format!(
                "invalid XY status code: {xy}"
            )));
        }

        let index_status = xy.chars().next().unwrap();
        let worktree_status = xy.chars().nth(1).unwrap();

        let path = parts.nth(6).unwrap_or("").to_string();
        let status = if worktree_status != '.' {
            parse_status_code(worktree_status)
        } else {
            parse_status_code(index_status)
        };

        return Ok(Some(FileStatus {
            path,
            status,
            index_status,
            worktree_status,
        }));
    }

    if line.starts_with("2 ") {
        let mut parts = line.splitn(10, ' ');
        let _record_type = parts.next();
        let xy = parts.next().unwrap_or("..");
        if xy.len() != 2 {
            return Err(GitError::ParseError(format!(
                "invalid XY status code: {xy}"
            )));
        }

        let index_status = xy.chars().next().unwrap();
        let worktree_status = xy.chars().nth(1).unwrap();

        let path = parts.nth(7).unwrap_or("").to_string();
        let status = if worktree_status != '.' {
            parse_status_code(worktree_status)
        } else {
            parse_status_code(index_status)
        };

        return Ok(Some(FileStatus {
            path,
            status,
            index_status,
            worktree_status,
        }));
    }

    if line.starts_with("? ") {
        let path = line.strip_prefix("? ").unwrap_or("").to_string();
        return Ok(Some(FileStatus {
            path,
            status: StatusCode::Untracked,
            index_status: '?',
            worktree_status: '?',
        }));
    }

    if line.starts_with("! ") {
        let path = line.strip_prefix("! ").unwrap_or("").to_string();
        return Ok(Some(FileStatus {
            path,
            status: StatusCode::Ignored,
            index_status: '!',
            worktree_status: '!',
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_status_code() {
        assert_eq!(parse_status_code('.'), StatusCode::Unmodified);
        assert_eq!(parse_status_code('M'), StatusCode::Modified);
        assert_eq!(parse_status_code('A'), StatusCode::Added);
        assert_eq!(parse_status_code('D'), StatusCode::Deleted);
        assert_eq!(parse_status_code('?'), StatusCode::Untracked);
    }

    #[test]
    fn test_parse_untracked_file() {
        let line = "? src/main.rs";
        let result = parse_status_entry(line).unwrap();

        assert!(result.is_some());
        let status = result.unwrap();
        assert_eq!(status.path, "src/main.rs");
        assert_eq!(status.status, StatusCode::Untracked);
        assert_eq!(status.index_status, '?');
        assert_eq!(status.worktree_status, '?');
    }

    #[test]
    fn test_parse_ignored_file() {
        let line = "! target/debug/main";
        let result = parse_status_entry(line).unwrap();

        assert!(result.is_some());
        let status = result.unwrap();
        assert_eq!(status.path, "target/debug/main");
        assert_eq!(status.status, StatusCode::Ignored);
    }

    #[test]
    fn test_parse_ordinary_changed_entry() {
        let line = "1 .M N... 100644 100644 100644 abc123 def456 src/lib.rs";
        let result = parse_status_entry(line).unwrap();

        assert!(result.is_some());
        let status = result.unwrap();
        assert_eq!(status.path, "src/lib.rs");
        assert_eq!(status.status, StatusCode::Modified);
        assert_eq!(status.index_status, '.');
        assert_eq!(status.worktree_status, 'M');
    }

    #[test]
    fn test_parse_staged_entry() {
        let line = "1 A. N... 000000 100644 100644 000000 abc123 src/new.rs";
        let result = parse_status_entry(line).unwrap();

        assert!(result.is_some());
        let status = result.unwrap();
        assert_eq!(status.path, "src/new.rs");
        assert_eq!(status.status, StatusCode::Added);
        assert_eq!(status.index_status, 'A');
        assert_eq!(status.worktree_status, '.');
    }

    #[test]
    fn test_parse_multiple_statuses() {
        let output =
            "? src/new1.rs\0? src/new2.rs\01 .M N... 100644 100644 100644 abc def src/lib.rs\0";
        let result = GitCliBackend::parse_porcelain_v2_status(output).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, "src/new1.rs");
        assert_eq!(result[1].path, "src/new2.rs");
        assert_eq!(result[2].path, "src/lib.rs");
    }

    #[test]
    fn test_parse_branch_header_lines() {
        let output = "# branch.oid 1234567890abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -3\0";

        let branch_info = GitCliBackend::parse_porcelain_v2_branch(output).unwrap();

        assert!(branch_info.is_some());
        let info = branch_info.unwrap();
        assert_eq!(info.oid, "1234567890abcdef");
        assert_eq!(info.head, "main");
        assert_eq!(info.upstream, Some("origin/main".to_string()));
        assert_eq!(info.ahead, 2);
        assert_eq!(info.behind, 3);
    }

    #[test]
    fn test_parse_type2_rename_entry() {
        let line = "2 R. N... 100644 100644 100644 abc123 def456 R100 src/new.rs";
        let result = parse_status_entry(line).unwrap();
        assert!(result.is_some());
        let status = result.unwrap();
        assert_eq!(status.path, "src/new.rs");
        assert_eq!(status.status, StatusCode::Renamed);
    }
}

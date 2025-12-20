use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use hometree_cli::debounce::Debounce;
use hometree_cli::track::decide_track;
use hometree_core::git::{AddMode, GitBackend, GitCliBackend};
use hometree_core::{deploy, read_generations, rollback, Config, ManagedSet, Paths};
use notify::{EventKind, RecursiveMode, Watcher};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "hometree", version, about = "Manage a versioned home tree")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize hometree state and config
    Init,
    /// Show status of managed files
    Status,
    /// Track paths (adds to managed set when allowed)
    Track {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Allow tracking paths outside managed roots
        #[arg(long)]
        allow_outside: bool,
        /// Force tracking even if ignored/denylisted
        #[arg(long)]
        force: bool,
    },
    /// Stop managing paths without deleting them
    Untrack {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Create a snapshot commit from staged changes
    Snapshot {
        /// Commit message
        #[arg(short = 'm', long = "message", required_unless_present = "auto")]
        message: Option<String>,
        /// Use the auto message template
        #[arg(long)]
        auto: bool,
    },
    /// Show commit history
    Log {
        /// Limit number of commits
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Watch for changes and auto-stage tracked updates
    Watch {
        #[command(subcommand)]
        command: Option<WatchCommand>,
        /// Compatibility alias for `watch foreground`
        #[arg(long)]
        foreground: bool,
    },
    /// Deploy a commit to managed paths
    Deploy {
        /// Target commit, branch, or tag
        #[arg(required = true)]
        target: String,
    },
    /// Roll back to a previous generation
    Rollback {
        /// Specific commit to roll back to
        #[arg(long, conflicts_with = "steps")]
        to: Option<String>,
        /// Number of generations to roll back (default: 1)
        #[arg(long, default_value_t = 1, conflicts_with = "to")]
        steps: usize,
    },
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => run_init(),
        Commands::Status => run_status(),
        Commands::Track {
            paths,
            allow_outside,
            force,
        } => run_track(paths, allow_outside, force),
        Commands::Untrack { paths } => run_untrack(paths),
        Commands::Snapshot { message, auto } => run_snapshot(message, auto),
        Commands::Log { limit } => run_log(limit),
        Commands::Watch {
            command,
            foreground,
        } => run_watch(command, foreground),
        Commands::Deploy { target } => run_deploy(target),
        Commands::Rollback { to, steps } => run_rollback(to, steps),
    }
}

#[derive(Subcommand)]
enum WatchCommand {
    /// Run in the foreground
    Foreground,
    /// Install a systemd user unit
    InstallSystemd,
    /// Start the systemd user unit
    Start,
    /// Stop the systemd user unit
    Stop,
    /// Show systemd user unit status
    Status,
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn run_init() -> Result<()> {
    let paths = Paths::new().context("resolve XDG paths")?;

    std::fs::create_dir_all(paths.config_dir()).context("create config dir")?;
    std::fs::create_dir_all(paths.data_dir()).context("create data dir")?;
    std::fs::create_dir_all(paths.state_dir()).context("create state dir")?;

    let config_path = paths.config_file();
    if !config_path.exists() {
        let cfg = Config::default_with_paths(&paths);
        cfg.write_to(&config_path).context("write default config")?;
        info!(path = %config_path.display(), "wrote config");
    } else {
        info!(path = %config_path.display(), "config exists; leaving unchanged");
    }

    let repo_dir = paths.repo_dir();
    if !repo_dir.exists() {
        init_bare_repo(&repo_dir).context("init bare repo")?;
        info!(path = %repo_dir.display(), "initialized bare repo");
    } else {
        info!(path = %repo_dir.display(), "repo exists; leaving unchanged");
    }

    let git = GitCliBackend::new();
    if git.is_repository(&repo_dir) {
        let _ = git.config_set(
            &repo_dir,
            paths.home_dir(),
            "status.showUntrackedFiles",
            "no",
        );
    }

    println!("hometree initialized.");
    Ok(())
}

fn run_status() -> Result<()> {
    let (_paths, config) = load_config()?;
    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let git = GitCliBackend::new();
    let paths = status_paths(&config);
    let include_untracked = !paths.is_empty();
    let statuses = git
        .status_porcelain(
            &config.repo.git_dir,
            &config.repo.work_tree,
            &paths,
            include_untracked,
        )
        .context("git status")?;

    let mut filtered: Vec<_> = statuses
        .into_iter()
        .filter(|status| managed.is_managed(Path::new(&status.path)))
        .filter(|status| status.status != hometree_core::git::StatusCode::Ignored)
        .collect();
    filtered.sort_by(|a, b| a.path.cmp(&b.path));

    if filtered.is_empty() {
        println!("clean");
        return Ok(());
    }

    for status in filtered {
        println!(
            "{}{} {}",
            status.index_status, status.worktree_status, status.path
        );
    }

    Ok(())
}

fn run_track(paths: Vec<PathBuf>, allow_outside: bool, force: bool) -> Result<()> {
    let (paths_ctx, mut config) = load_config()?;
    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let home_dir = paths_ctx.home_dir();

    let mut to_stage: Vec<PathBuf> = Vec::new();
    let mut extra_files_changed = false;

    for input in paths {
        let decision = decide_track(
            &input,
            home_dir,
            &managed,
            &config.manage.roots,
            allow_outside,
            force,
        )?;

        if decision.add_to_extra_files {
            let rel_str = decision.rel_path.to_string_lossy().to_string();
            if !config.manage.extra_files.contains(&rel_str) {
                config.manage.extra_files.push(rel_str);
                extra_files_changed = true;
            }
        }

        to_stage.push(decision.rel_path);
    }

    if extra_files_changed {
        let config_path = paths_ctx.config_file();
        config
            .write_to(&config_path)
            .with_context(|| format!("write config to {}", config_path.display()))?;
    }

    let git = GitCliBackend::new();
    git.add(
        &config.repo.git_dir,
        &config.repo.work_tree,
        &to_stage,
        AddMode::Paths,
    )
    .context("git add")?;

    println!("tracked {} path(s)", to_stage.len());
    Ok(())
}

fn run_untrack(paths: Vec<PathBuf>) -> Result<()> {
    let (paths_ctx, mut config) = load_config()?;
    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let home_dir = paths_ctx.home_dir();
    let mut changed = false;
    let mut to_unstage: Vec<PathBuf> = Vec::new();

    for input in paths {
        let rel = resolve_rel_path(home_dir, &input)?;
        let rel_str = rel.to_string_lossy().to_string();
        if let Some(pos) = config.manage.extra_files.iter().position(|p| p == &rel_str) {
            config.manage.extra_files.remove(pos);
            changed = true;
        } else if managed.is_managed(&rel) {
            let ignore = ignore_pattern_for(&rel, home_dir);
            if !config.ignore.patterns.contains(&ignore) {
                config.ignore.patterns.push(ignore);
                changed = true;
            }
        } else {
            return Err(anyhow!("path is not managed: {}", rel.display()));
        }

        to_unstage.push(rel);
    }

    if changed {
        let config_path = paths_ctx.config_file();
        config
            .write_to(&config_path)
            .with_context(|| format!("write config to {}", config_path.display()))?;
    }

    if !to_unstage.is_empty() {
        git_rm_cached(&config.repo.git_dir, &config.repo.work_tree, &to_unstage)
            .context("git rm --cached")?;
    }

    println!("untracked {} path(s)", to_unstage.len());
    Ok(())
}

fn run_snapshot(message: Option<String>, auto: bool) -> Result<()> {
    let (_paths, config) = load_config()?;
    let git = GitCliBackend::new();
    let msg = if auto {
        if message.is_some() {
            return Err(anyhow!("cannot use --auto with -m"));
        }
        config
            .snapshot
            .auto_message_template
            .clone()
            .ok_or_else(|| anyhow!("auto message template is not configured"))?
    } else {
        message.ok_or_else(|| anyhow!("message is required"))?
    };

    let output = git
        .commit(&config.repo.git_dir, &config.repo.work_tree, &msg)
        .context("git commit")?;
    println!("{output}");
    Ok(())
}

fn run_log(limit: Option<usize>) -> Result<()> {
    let (_paths, config) = load_config()?;
    let git = GitCliBackend::new();
    let output = git
        .log(&config.repo.git_dir, &config.repo.work_tree, limit)
        .context("git log")?;
    print!("{output}");
    Ok(())
}

fn run_watch(command: Option<WatchCommand>, foreground: bool) -> Result<()> {
    if foreground {
        if command.is_some() {
            return Err(anyhow!("--foreground cannot be combined with a subcommand"));
        }
        return run_watch_foreground();
    }

    match command.unwrap_or(WatchCommand::Foreground) {
        WatchCommand::Foreground => run_watch_foreground(),
        WatchCommand::InstallSystemd => install_systemd_unit(),
        WatchCommand::Start => systemctl_user(&["start", "hometree.service"]),
        WatchCommand::Stop => systemctl_user(&["stop", "hometree.service"]),
        WatchCommand::Status => systemctl_user(&["status", "hometree.service"]),
    }
}

fn run_watch_foreground() -> Result<()> {
    let (paths, config) = load_config()?;
    if !config.watch.enabled {
        return Err(anyhow!("watch is disabled in config"));
    }

    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let watch_paths = watch_paths(&config);
    if watch_paths.is_empty() {
        return Err(anyhow!("no managed roots or extra files configured"));
    }

    let debounce_ms = config.watch.debounce_ms.max(50);
    let mut debouncer = Debounce::new(Duration::from_millis(debounce_ms));
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;

    for path in &watch_paths {
        let abs = paths.home_dir().join(path);
        watcher.watch(&abs, RecursiveMode::Recursive)?;
    }

    let git = GitCliBackend::new();
    let work_tree = &config.repo.work_tree;
    let git_dir = &config.repo.git_dir;
    let allowlist_patterns = &config.watch.auto_add_allow_patterns;
    let allowlist_has_entries = allowlist_patterns.iter().any(|p| !p.trim().is_empty());
    let allowlist = build_allowlist(allowlist_patterns)?;
    let auto_add_enabled = config.watch.auto_add_new && allowlist_has_entries;

    if config.watch.auto_add_new && !allowlist_has_entries {
        info!("auto_add_new enabled but allowlist is empty; skipping auto-add");
    }

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if !should_handle_event(&event.kind) {
                    continue;
                }
                for path in event.paths {
                    if let Ok(rel) = path.strip_prefix(paths.home_dir()) {
                        let rel_path = rel.to_path_buf();
                        let is_allowed = managed.is_allowed(&rel_path);
                        let is_managed = managed.is_managed(&rel_path);
                        let matches_allowlist = allowlist.is_match(&rel_path);
                        let allow_auto =
                            auto_add_enabled && is_allowed && is_managed && matches_allowlist;
                        if auto_add_enabled && !allow_auto {
                            // Log why auto-add was skipped (for troubleshooting)
                            if !is_allowed {
                                debug!(path = %rel_path.display(), "skipped auto-add: path is ignored or denylisted");
                            } else if !is_managed {
                                debug!(path = %rel_path.display(), "skipped auto-add: path is not under managed roots/extra_files");
                            } else if !matches_allowlist {
                                debug!(path = %rel_path.display(), "skipped auto-add: path does not match allowlist");
                            }
                        }
                        if allow_auto {
                            match git.add(
                                git_dir,
                                work_tree,
                                std::slice::from_ref(&rel_path),
                                AddMode::Paths,
                            ) {
                                Ok(_) => info!(path = %rel_path.display(), "auto-added new path"),
                                Err(err) => {
                                    eprintln!("auto-add failed for {}: {}", rel_path.display(), err)
                                }
                            }
                        }
                        if is_managed {
                            debouncer.push(rel_path, Instant::now());
                        }
                    }
                }
            }
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if debouncer.is_due(Instant::now()) && !debouncer.is_empty() {
            let paths_to_stage = debouncer.drain();
            info!("staging {} changed file(s)", paths_to_stage.len());
            let _ = git.add(git_dir, work_tree, &paths_to_stage, AddMode::TrackedOnly);
        }
    }

    Ok(())
}

fn install_systemd_unit() -> Result<()> {
    let paths = Paths::new().context("resolve XDG paths")?;
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.home_dir().join(".config"));
    let unit_dir = config_dir.join("systemd").join("user");
    std::fs::create_dir_all(&unit_dir).context("create systemd user dir")?;

    let unit_path = unit_dir.join("hometree.service");
    let unit = format!(
        "[Unit]\nDescription=hometree watch daemon\n\n[Service]\nExecStart={exe} watch foreground\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n",
        exe = std::env::current_exe()?.display()
    );
    std::fs::write(&unit_path, unit).context("write systemd unit")?;

    println!("installed {}", unit_path.display());
    println!("run: systemctl --user daemon-reload");
    Ok(())
}

fn systemctl_user(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("systemctl --user")?;
    if !status.success() {
        return Err(anyhow!("systemctl failed"));
    }
    Ok(())
}

fn should_handle_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn build_allowlist(patterns: &[String]) -> Result<globset::GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        builder.add(globset::Glob::new(trimmed)?);
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::build_allowlist;
    use std::path::Path;

    #[test]
    fn allowlist_matches_expected_patterns() {
        let list = build_allowlist(&vec![".config/**".to_string(), ".local/bin/*".to_string()])
            .expect("allowlist");
        assert!(list.is_match(Path::new(".config/app/config.toml")));
        assert!(list.is_match(Path::new(".local/bin/script")));
        assert!(!list.is_match(Path::new(".ssh/id_rsa")));
    }
}

fn run_deploy(target: String) -> Result<()> {
    let (paths, config) = load_config()?;
    let git = GitCliBackend::new();
    let entry = deploy(&config, &paths, &git, &target).context("deploy")?;
    println!("deployed {}", entry.rev);
    Ok(())
}

fn run_rollback(to: Option<String>, steps: usize) -> Result<()> {
    if steps == 0 {
        return Err(anyhow!("steps must be >= 1"));
    }
    let (paths, config) = load_config()?;
    let target = if let Some(rev) = to {
        rev
    } else {
        let generations = read_generations(paths.state_dir()).context("read generations")?;
        if generations.is_empty() {
            format!("HEAD~{}", steps)
        } else if generations.len() <= steps {
            return Err(anyhow!("not enough generations to rollback"));
        } else {
            generations[generations.len() - 1 - steps].rev.clone()
        }
    };
    let git = GitCliBackend::new();
    let entry = rollback(&config, &paths, &git, &target).context("rollback")?;
    println!("rolled back to {}", entry.rev);
    Ok(())
}

fn load_config() -> Result<(Paths, Config)> {
    let paths = Paths::new().context("resolve XDG paths")?;
    let config_path = paths.config_file();
    if !config_path.exists() {
        return Err(anyhow!("config not found; run `hometree init`"));
    }
    let cfg = Config::load_from(&config_path).context("load config")?;
    Ok((paths, cfg))
}

fn status_paths(config: &Config) -> Vec<PathBuf> {
    let mut set = BTreeSet::new();
    for root in &config.manage.roots {
        let pathspec = root_to_pathspec(root);
        if !pathspec.is_empty() {
            set.insert(pathspec);
        }
    }
    for extra in &config.manage.extra_files {
        if !extra.is_empty() {
            set.insert(extra.clone());
        }
    }
    set.into_iter().map(PathBuf::from).collect()
}

fn watch_paths(config: &Config) -> Vec<PathBuf> {
    let mut set = BTreeSet::new();
    for root in &config.manage.roots {
        let trimmed = root.trim_start_matches("./");
        if trimmed.is_empty() || has_glob_meta(trimmed) {
            continue;
        }
        let path = trimmed.trim_end_matches("/**").trim_end_matches('/');
        if !path.is_empty() {
            set.insert(path.to_string());
        }
    }
    for extra in &config.manage.extra_files {
        if !extra.is_empty() {
            set.insert(extra.clone());
        }
    }
    set.into_iter().map(PathBuf::from).collect()
}

fn resolve_rel_path(home_dir: &Path, input: &Path) -> Result<PathBuf> {
    let abs = if input.is_absolute() {
        input.to_path_buf()
    } else {
        home_dir.join(input)
    };
    let rel = abs
        .strip_prefix(home_dir)
        .map_err(|_| anyhow!("path is outside $HOME: {}", abs.display()))?
        .to_path_buf();
    Ok(rel)
}

fn ignore_pattern_for(rel: &Path, home_dir: &Path) -> String {
    let abs = home_dir.join(rel);
    let is_dir = abs.is_dir();
    let rel_str = rel.to_string_lossy();
    if is_dir {
        format!("{rel_str}/**")
    } else {
        rel_str.to_string()
    }
}

fn git_rm_cached(git_dir: &Path, work_tree: &Path, paths: &[PathBuf]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(["--git-dir", git_dir.to_string_lossy().as_ref()])
        .args(["--work-tree", work_tree.to_string_lossy().as_ref()])
        .arg("rm")
        .arg("-r")
        .arg("--cached")
        .arg("--ignore-unmatch")
        .arg("--");
    for path in paths {
        cmd.arg(path.to_string_lossy().as_ref());
    }
    let status = cmd.status().context("git rm --cached")?;
    if !status.success() {
        return Err(anyhow!("git rm --cached failed"));
    }
    Ok(())
}

fn root_to_pathspec(root: &str) -> String {
    let trimmed = root.trim_start_matches("./");
    if trimmed.is_empty() {
        return String::new();
    }
    if has_glob_meta(trimmed) {
        if trimmed.starts_with(":(glob)") {
            return trimmed.to_string();
        }
        return format!(":(glob){trimmed}");
    }
    trimmed
        .trim_end_matches("/**")
        .trim_end_matches('/')
        .to_string()
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains('{')
}

fn init_bare_repo(path: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("init")
        .arg("--bare")
        .arg(path)
        .status()
        .context("run git init --bare")?;
    if !status.success() {
        return Err(anyhow!("git init --bare failed"));
    }
    Ok(())
}

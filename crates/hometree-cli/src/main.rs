use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use hometree_cli::track::decide_track;
use hometree_cli::watch::root_to_pathspec;
use hometree_core::git::{AddMode, GitBackend, GitCliBackend};
use hometree_core::secrets::{AgeBackend, SecretsBackend, SecretsManager};
use hometree_core::{
    deploy_with_options, plan_deploy, read_generations, rollback, verify, Config, ManagedSet, Paths,
};
use std::time::Duration;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod daemon;

#[derive(Parser)]
#[command(name = "hometree", version, about = "Manage a versioned home tree")]
struct Cli {
    /// Override the home directory used by hometree
    #[arg(long, env = "HOMETREE_HOME_ROOT")]
    home_root: Option<PathBuf>,
    /// Override the XDG base directory root (config/data/state/cache)
    #[arg(long, env = "HOMETREE_XDG_ROOT")]
    xdg_root: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug)]
struct Overrides {
    home_root: Option<PathBuf>,
    xdg_root: Option<PathBuf>,
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
    /// Run and manage the daemon (alias: watch)
    #[command(alias = "watch")]
    Daemon {
        #[command(subcommand)]
        command: Option<DaemonCommand>,
        /// Compatibility alias for `daemon run --foreground`
        #[arg(long)]
        foreground: bool,
    },
    /// Deploy a commit to managed paths
    Deploy {
        /// Target commit, branch, or tag
        #[arg(required = true)]
        target: String,
        /// Skip secrets decryption and secret backups
        #[arg(long)]
        no_secrets: bool,
        /// Skip backups entirely
        #[arg(long)]
        no_backup: bool,
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
    /// Plan changes without applying them
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    /// Verify that the home tree matches a commit
    Verify {
        /// Commit, branch, or tag to verify (default: HEAD)
        #[arg(long)]
        rev: Option<String>,
        /// Enforce strict checks (permissions and unexpected files)
        #[arg(long)]
        strict: bool,
        /// Secrets verification mode
        #[arg(long, value_enum, default_value = "presence")]
        with_secrets: SecretsVerifyArg,
        /// Emit JSON output
        #[arg(long)]
        json: bool,
        /// Show plaintext secret paths in output
        #[arg(long)]
        show_paths: bool,
    },
    /// Manage secret sidecar files
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },
}

#[derive(Subcommand)]
enum PlanCommand {
    /// Plan a deploy without applying changes
    Deploy {
        /// Target commit, branch, or tag
        #[arg(required = true)]
        target: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SecretsVerifyArg {
    Skip,
    Presence,
    Decrypt,
}

#[derive(Subcommand)]
enum SecretCommand {
    /// Add a secret rule and create the sidecar ciphertext
    Add {
        #[arg(required = true)]
        path: PathBuf,
    },
    /// Re-encrypt secrets (all or selected paths)
    Refresh {
        #[arg(required = false)]
        paths: Vec<PathBuf>,
    },
    /// Show secret sync status
    Status {
        /// Show plaintext paths in output
        #[arg(long)]
        show_paths: bool,
    },
    /// Re-encrypt secrets with current recipients
    Rekey,
}

fn main() -> Result<()> {
    init_tracing();
    let Cli {
        command,
        home_root,
        xdg_root,
    } = Cli::parse();
    let overrides = Overrides {
        home_root,
        xdg_root,
    };
    match command {
        Commands::Init => run_init(&overrides),
        Commands::Status => run_status(&overrides),
        Commands::Track {
            paths,
            allow_outside,
            force,
        } => run_track(&overrides, paths, allow_outside, force),
        Commands::Untrack { paths } => run_untrack(&overrides, paths),
        Commands::Snapshot { message, auto } => run_snapshot(&overrides, message, auto),
        Commands::Log { limit } => run_log(&overrides, limit),
        Commands::Daemon {
            command,
            foreground,
        } => daemon::run_daemon_command(&overrides, command, foreground),
        Commands::Deploy {
            target,
            no_secrets,
            no_backup,
        } => run_deploy(&overrides, target, no_secrets, no_backup),
        Commands::Rollback { to, steps } => run_rollback(&overrides, to, steps),
        Commands::Plan { command } => run_plan(&overrides, command),
        Commands::Verify {
            rev,
            strict,
            with_secrets,
            json,
            show_paths,
        } => run_verify(&overrides, rev, strict, with_secrets, json, show_paths),
        Commands::Secret { command } => run_secret(&overrides, command),
    }
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Run the daemon
    Run {
        /// Run in the foreground (compat)
        #[arg(long)]
        foreground: bool,
    },
    /// Compatibility alias for `run --foreground`
    #[command(alias = "foreground")]
    Foreground,
    /// Install a systemd user unit
    InstallSystemd,
    /// Uninstall the systemd user unit
    UninstallSystemd,
    /// Start the systemd user unit
    Start,
    /// Stop the systemd user unit
    Stop,
    /// Restart the systemd user unit
    Restart,
    /// Show daemon status
    Status,
    /// Reload daemon config
    Reload,
    /// Pause staging (inhibit)
    Pause {
        /// Pause duration in milliseconds
        #[arg(long, default_value_t = 300_000)]
        ttl_ms: u64,
        /// Reason for pausing
        #[arg(long, default_value = "manual")]
        reason: String,
    },
    /// Resume staging
    Resume,
    /// Flush staged changes immediately
    Flush,
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn run_init(overrides: &Overrides) -> Result<()> {
    let paths = load_paths(overrides).context("resolve XDG paths")?;

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

fn run_status(overrides: &Overrides) -> Result<()> {
    let (_paths, config) = load_config(overrides)?;
    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let secrets = SecretsManager::from_config(&config.secrets);
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
        .filter(|status| !secrets.is_secret_plaintext(Path::new(&status.path)))
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

fn run_track(
    overrides: &Overrides,
    paths: Vec<PathBuf>,
    allow_outside: bool,
    force: bool,
) -> Result<()> {
    let (paths_ctx, mut config) = load_config(overrides)?;
    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let home_dir = paths_ctx.home_dir();
    let secrets = SecretsManager::from_config(&config.secrets);

    let mut to_stage: Vec<PathBuf> = Vec::new();
    let mut extra_files_changed = false;

    for input in paths {
        if secrets.enabled() {
            let rel = resolve_rel_path(home_dir, &input)?;
            if secrets.is_secret_plaintext(&rel) {
                return Err(anyhow!(
                    "path is a secret; use `hometree secret add` instead"
                ));
            }
        }
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
    with_lock(&paths_ctx, || {
        git.add(
            &config.repo.git_dir,
            &config.repo.work_tree,
            &to_stage,
            AddMode::Paths,
        )
        .context("git add")
    })?;

    println!("tracked {} path(s)", to_stage.len());
    Ok(())
}

fn run_untrack(overrides: &Overrides, paths: Vec<PathBuf>) -> Result<()> {
    let (paths_ctx, mut config) = load_config(overrides)?;
    let managed = ManagedSet::from_config(&config).context("build managed set")?;
    let secrets = SecretsManager::from_config(&config.secrets);
    let home_dir = paths_ctx.home_dir();
    let mut changed = false;
    let mut to_unstage: Vec<PathBuf> = Vec::new();

    for input in paths {
        let rel = resolve_rel_path(home_dir, &input)?;
        if secrets.enabled() && secrets.is_secret_plaintext(&rel) {
            return Err(anyhow!("path is a secret; use `hometree secret add`"));
        }
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
        with_lock(&paths_ctx, || {
            git_rm_cached(&config.repo.git_dir, &config.repo.work_tree, &to_unstage)
                .context("git rm --cached")
        })?;
    }

    println!("untracked {} path(s)", to_unstage.len());
    Ok(())
}

fn run_snapshot(overrides: &Overrides, message: Option<String>, auto: bool) -> Result<()> {
    let (paths, config) = load_config(overrides)?;
    let _inhibit = daemon::DaemonInhibitGuard::new(&paths, "rollback", Duration::from_secs(300))?;
    let git = GitCliBackend::new();
    guard_snapshot_secrets(&config, &git)?;
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

    let output = with_lock(&paths, || {
        git.commit(&config.repo.git_dir, &config.repo.work_tree, &msg)
            .context("git commit")
    })?;
    println!("{output}");
    Ok(())
}

fn guard_snapshot_secrets(config: &Config, git: &GitCliBackend) -> Result<()> {
    if !config.secrets.enabled || config.secrets.rules.is_empty() {
        return Ok(());
    }
    let secrets = SecretsManager::from_config(&config.secrets);
    let paths: Vec<PathBuf> = secrets
        .rules()
        .iter()
        .map(|rule| PathBuf::from(&rule.path))
        .collect();
    let statuses = git
        .status_porcelain(
            &config.repo.git_dir,
            &config.repo.work_tree,
            &paths,
            true,
        )
        .context("git status")?;
    for status in statuses {
        let rel = Path::new(&status.path);
        if secrets.is_secret_plaintext(rel) {
            let idx = status.index_status;
            if idx != '.' && idx != '?' && idx != '!' {
                return Err(anyhow!(
                    "plaintext secret is staged; refuse snapshot"
                ));
            }
        }
    }
    Ok(())
}

fn run_log(overrides: &Overrides, limit: Option<usize>) -> Result<()> {
    let (_paths, config) = load_config(overrides)?;
    let git = GitCliBackend::new();
    let output = git
        .log(&config.repo.git_dir, &config.repo.work_tree, limit)
        .context("git log")?;
    print!("{output}");
    Ok(())
}

fn run_deploy(
    overrides: &Overrides,
    target: String,
    no_secrets: bool,
    no_backup: bool,
) -> Result<()> {
    let (paths, mut config) = load_config(overrides)?;
    let _inhibit = daemon::DaemonInhibitGuard::new(&paths, "deploy", Duration::from_secs(300))?;
    if no_secrets {
        config.secrets.enabled = false;
    }
    let git = GitCliBackend::new();
    let entry = deploy_with_options(
        &config,
        &paths,
        &git,
        &target,
        hometree_core::DeployOptions { no_backup },
    )
    .context("deploy")?;
    println!("deployed {}", entry.rev);
    Ok(())
}

fn run_rollback(overrides: &Overrides, to: Option<String>, steps: usize) -> Result<()> {
    if steps == 0 {
        return Err(anyhow!("steps must be >= 1"));
    }
    let (paths, config) = load_config(overrides)?;
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

fn run_plan(overrides: &Overrides, command: PlanCommand) -> Result<()> {
    match command {
        PlanCommand::Deploy { target } => run_plan_deploy(overrides, target),
    }
}

fn run_plan_deploy(overrides: &Overrides, target: String) -> Result<()> {
    let (paths, config) = load_config(overrides)?;
    let git = GitCliBackend::new();
    let plan = plan_deploy(&config, &paths, &git, &target).context("plan deploy")?;
    for entry in plan.entries {
        let action = match entry.action {
            hometree_core::PlanAction::Create => "create",
            hometree_core::PlanAction::Update => "update",
            hometree_core::PlanAction::Delete => "delete",
        };
        println!("{action} {}", entry.path);
    }
    Ok(())
}

fn run_verify(
    overrides: &Overrides,
    rev: Option<String>,
    strict: bool,
    with_secrets: SecretsVerifyArg,
    json: bool,
    show_paths: bool,
) -> Result<()> {
    let (paths, config) = load_config(overrides)?;
    let git = GitCliBackend::new();
    let target = rev.as_deref().unwrap_or("HEAD");
    let report = verify(
        &config,
        &paths,
        &git,
        target,
        hometree_core::VerifyOptions {
            strict,
            secrets_mode: match with_secrets {
                SecretsVerifyArg::Skip => hometree_core::verify::SecretsVerifyMode::Skip,
                SecretsVerifyArg::Presence => hometree_core::verify::SecretsVerifyMode::Presence,
                SecretsVerifyArg::Decrypt => hometree_core::verify::SecretsVerifyMode::Decrypt,
            },
        },
    )
    .context("verify")?;

    if json {
        let output_report = if show_paths {
            report.clone()
        } else {
            redact_verify_report(&report)
        };
        let output = serde_json::to_string_pretty(&output_report).context("serialize json")?;
        println!("{output}");
    } else {
        print_verify_report(&report, show_paths);
    }

    if !report.is_clean() {
        std::process::exit(1);
    }

    Ok(())
}

fn print_verify_report(report: &hometree_core::VerifyReport, show_paths: bool) {
    if report.is_clean() {
        println!("clean");
        return;
    }

    for path in &report.missing {
        println!("missing {path}");
    }
    for path in &report.modified {
        println!("modified {path}");
    }
    for path in &report.type_mismatch {
        println!("type-mismatch {path}");
    }
    for path in &report.mode_mismatch {
        println!("mode-mismatch {path}");
    }
    for path in &report.unexpected {
        println!("unexpected {path}");
    }
    if show_paths {
        for path in &report.secret_missing_plaintext {
            println!("secret-missing-plaintext {path}");
        }
        for path in &report.secret_missing_ciphertext {
            println!("secret-missing-ciphertext {path}");
        }
        for path in &report.secret_mismatch {
            println!("secret-mismatch {path}");
        }
        for path in &report.secret_decrypt_error {
            println!("secret-decrypt-error {path}");
        }
    } else {
        for _ in &report.secret_missing_plaintext {
            println!("secret-missing-plaintext <redacted>");
        }
        for _ in &report.secret_missing_ciphertext {
            println!("secret-missing-ciphertext <redacted>");
        }
        for _ in &report.secret_mismatch {
            println!("secret-mismatch <redacted>");
        }
        for _ in &report.secret_decrypt_error {
            println!("secret-decrypt-error <redacted>");
        }
    }
}

fn redact_verify_report(report: &hometree_core::VerifyReport) -> hometree_core::VerifyReport {
    let mut redacted = report.clone();
    let redacted_len = |len: usize| vec!["<redacted>".to_string(); len];
    redacted.secret_missing_plaintext = redacted_len(report.secret_missing_plaintext.len());
    redacted.secret_missing_ciphertext = redacted_len(report.secret_missing_ciphertext.len());
    redacted.secret_mismatch = redacted_len(report.secret_mismatch.len());
    redacted.secret_decrypt_error = redacted_len(report.secret_decrypt_error.len());
    redacted
}

fn run_secret(overrides: &Overrides, command: SecretCommand) -> Result<()> {
    match command {
        SecretCommand::Add { path } => run_secret_add(overrides, path),
        SecretCommand::Refresh { paths } => run_secret_refresh(overrides, paths),
        SecretCommand::Status { show_paths } => run_secret_status(overrides, show_paths),
        SecretCommand::Rekey => run_secret_rekey(overrides),
    }
}

fn run_secret_add(overrides: &Overrides, path: PathBuf) -> Result<()> {
    let (paths, mut config) = load_config(overrides)?;
    config.secrets.enabled = true;
    let rel = resolve_rel_path(paths.home_dir(), &path)?;
    let rel_str = rel.to_string_lossy().to_string();
    if config.secrets.rules.iter().any(|rule| rule.path == rel_str) {
        return Err(anyhow!("secret rule already exists"));
    }
    config
        .secrets
        .rules
        .push(hometree_core::config::SecretRule {
            path: rel_str.clone(),
            ciphertext: None,
            mode: None,
        });
    if !config.ignore.patterns.contains(&rel_str) {
        config.ignore.patterns.push(rel_str.clone());
    }

    let config_path = paths.config_file();
    config
        .write_to(&config_path)
        .with_context(|| format!("write config to {}", config_path.display()))?;

    let secrets = SecretsManager::from_config(&config.secrets);
    let backend = AgeBackend::from_config(&config.secrets)?;
    let plaintext_abs = paths.home_dir().join(&rel);
    let plaintext = std::fs::read(&plaintext_abs)
        .context("read secret plaintext")?;
    let ciphertext = backend.encrypt(&plaintext)?;
    let ciphertext_rel = secrets.ciphertext_path(
        secrets
            .rules()
            .iter()
            .find(|rule| rule.path == rel_str)
            .expect("rule"),
    );
    let ciphertext_abs = paths.home_dir().join(&ciphertext_rel);
    if let Some(parent) = ciphertext_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&ciphertext_abs, ciphertext)?;

    ensure_git_excludes(&paths, &config)?;

    let git = GitCliBackend::new();
    with_lock(&paths, || {
        git.add(
            &config.repo.git_dir,
            &config.repo.work_tree,
            std::slice::from_ref(&ciphertext_rel),
            AddMode::Paths,
        )
        .context("git add")
    })?;

    println!("secret added");
    Ok(())
}

fn run_secret_refresh(overrides: &Overrides, paths: Vec<PathBuf>) -> Result<()> {
    let (paths_ctx, config) = load_config(overrides)?;
    let secrets = SecretsManager::from_config(&config.secrets);
    if !secrets.enabled() {
        return Err(anyhow!("secrets are not enabled"));
    }
    let backend = AgeBackend::from_config(&config.secrets)?;
    let git = GitCliBackend::new();
    let mut to_stage = Vec::new();
    let filter: Option<std::collections::BTreeSet<PathBuf>> = if paths.is_empty() {
        None
    } else {
        let mut set = std::collections::BTreeSet::new();
        for path in paths {
            let rel = resolve_rel_path(paths_ctx.home_dir(), &path)?;
            set.insert(rel);
        }
        Some(set)
    };

    for rule in secrets.rules() {
        let plaintext_rel = secrets.plaintext_path(rule);
        if let Some(filter) = filter.as_ref() {
            if !filter.contains(&plaintext_rel) {
                continue;
            }
        }
        if plaintext_rel.as_os_str().is_empty() {
            continue;
        }
        let plaintext_abs = paths_ctx.home_dir().join(&plaintext_rel);
        let plaintext = std::fs::read(&plaintext_abs)?;
        let ciphertext = backend.encrypt(&plaintext)?;
        let ciphertext_rel = secrets.ciphertext_path(rule);
        let ciphertext_abs = paths_ctx.home_dir().join(&ciphertext_rel);
        if let Some(parent) = ciphertext_abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&ciphertext_abs, ciphertext)?;
        to_stage.push(ciphertext_rel);
    }

    if !to_stage.is_empty() {
        with_lock(&paths_ctx, || {
            git.add(
                &config.repo.git_dir,
                &config.repo.work_tree,
                &to_stage,
                AddMode::Paths,
            )
            .context("git add")
        })?;
    }

    println!("refreshed {} secret(s)", to_stage.len());
    Ok(())
}

fn run_secret_status(overrides: &Overrides, show_paths: bool) -> Result<()> {
    let (paths_ctx, config) = load_config(overrides)?;
    let secrets = SecretsManager::from_config(&config.secrets);
    if !secrets.enabled() {
        println!("secrets disabled");
        return Ok(());
    }
    let backend = AgeBackend::from_config(&config.secrets).ok();

    for rule in secrets.rules() {
        let plaintext_rel = secrets.plaintext_path(rule);
        let ciphertext_rel = secrets.ciphertext_path(rule);
        let plaintext_abs = paths_ctx.home_dir().join(&plaintext_rel);
        let ciphertext_abs = paths_ctx.home_dir().join(&ciphertext_rel);
        let has_plaintext = plaintext_abs.exists();
        let has_ciphertext = ciphertext_abs.exists();
        let status = if !has_plaintext {
            "missing-plaintext"
        } else if !has_ciphertext {
            "missing-ciphertext"
        } else if let Some(backend) = backend.as_ref() {
            let plaintext = std::fs::read(&plaintext_abs)?;
            let ciphertext = std::fs::read(&ciphertext_abs)?;
            match backend.decrypt(&ciphertext) {
                Ok(decrypted) => {
                    if decrypted == plaintext {
                        "in-sync"
                    } else {
                        "drift"
                    }
                }
                Err(_) => "decrypt-error",
            }
        } else {
            "unknown"
        };
        if show_paths {
            println!("{status} {}", plaintext_rel.display());
        } else {
            println!("{status} <redacted>");
        }
    }

    Ok(())
}

fn run_secret_rekey(overrides: &Overrides) -> Result<()> {
    let (paths_ctx, config) = load_config(overrides)?;
    let secrets = SecretsManager::from_config(&config.secrets);
    if !secrets.enabled() {
        return Err(anyhow!("secrets are not enabled"));
    }
    let backend = AgeBackend::from_config(&config.secrets)?;
    let git = GitCliBackend::new();
    let mut to_stage = Vec::new();

    for rule in secrets.rules() {
        let plaintext_rel = secrets.plaintext_path(rule);
        let plaintext_abs = paths_ctx.home_dir().join(&plaintext_rel);
        let plaintext = std::fs::read(&plaintext_abs)?;
        let ciphertext = backend.encrypt(&plaintext)?;
        let ciphertext_rel = secrets.ciphertext_path(rule);
        let ciphertext_abs = paths_ctx.home_dir().join(&ciphertext_rel);
        if let Some(parent) = ciphertext_abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&ciphertext_abs, ciphertext)?;
        to_stage.push(ciphertext_rel);
    }

    if !to_stage.is_empty() {
        with_lock(&paths_ctx, || {
            git.add(
                &config.repo.git_dir,
                &config.repo.work_tree,
                &to_stage,
                AddMode::Paths,
            )
            .context("git add")
        })?;
    }

    println!("rekeyed {} secret(s)", to_stage.len());
    Ok(())
}

fn load_paths(overrides: &Overrides) -> Result<Paths> {
    Paths::new_with_overrides(
        overrides.home_root.as_deref(),
        overrides.xdg_root.as_deref(),
    )
    .context("resolve XDG paths")
}

fn load_config(overrides: &Overrides) -> Result<(Paths, Config)> {
    let paths = load_paths(overrides)?;
    let config_path = paths.config_file();
    if !config_path.exists() {
        return Err(anyhow!("config not found; run `hometree init`"));
    }
    let mut cfg = Config::load_from(&config_path).context("load config")?;
    if overrides.home_root.is_some() {
        cfg.repo.work_tree = paths.home_dir().to_path_buf();
    }
    Ok((paths, cfg))
}

fn with_lock<T>(paths: &Paths, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let _lock = hometree_core::acquire_lock(paths)?;
    f()
}

fn ensure_git_excludes(paths: &Paths, config: &Config) -> Result<()> {
    let excludes_path = paths.config_dir().join("gitignore");
    if let Some(parent) = excludes_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut existing = std::collections::BTreeSet::new();
    if excludes_path.exists() {
        let contents = std::fs::read_to_string(&excludes_path)?;
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            existing.insert(trimmed.to_string());
        }
    }

    // Standard sensitive directories (mirrors default ignore patterns).
    let default_excludes = [
        ".ssh/**",
        ".gnupg/**",
        ".local/share/keyrings/**",
        ".local/share/kwalletd/**",
        ".pki/**",
        ".mozilla/**",
        ".config/google-chrome/**",
        ".config/chromium/**",
        ".config/BraveSoftware/**",
    ];
    for pattern in default_excludes {
        existing.insert(pattern.to_string());
    }
    for rule in &config.secrets.rules {
        if !rule.path.trim().is_empty() {
            existing.insert(rule.path.clone());
        }
    }

    let mut output = String::new();
    output.push_str("# hometree secrets (plaintext)") ;
    output.push('\n');
    for line in existing {
        output.push_str(&line);
        output.push('\n');
    }
    std::fs::write(&excludes_path, output)?;

    let git = GitCliBackend::new();
    git.config_set(
        &config.repo.git_dir,
        &config.repo.work_tree,
        "core.excludesFile",
        excludes_path.to_string_lossy().as_ref(),
    )
    .context("git config core.excludesFile")?;

    Ok(())
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

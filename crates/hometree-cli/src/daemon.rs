use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use hometree_cli::debounce::Debounce;
use hometree_cli::watch::{
    build_allowlist, collect_watch_decisions, should_handle_event, watch_paths, WatchDecisions,
};
use hometree_core::git::{AddMode, GitBackend, GitCliBackend};
use hometree_core::{
    active_inhibit, clear_inhibit, lock_path, write_inhibit, AgeBackend, InhibitMarker, ManagedSet,
    Paths, SecretsBackend, SecretsManager,
};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use signal_hook::consts::signal::SIGHUP;
use signal_hook::iterator::Signals;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::{debug, error, info, warn};

use crate::{load_config, load_paths, DaemonCommand, Overrides};

const SOCKET_FILENAME: &str = "daemon.sock";
const IPC_MAX_LINE: usize = 64 * 1024;
const INHIBIT_TTL_DEFAULT: Duration = Duration::from_secs(300);

pub fn run_daemon_command(
    overrides: &Overrides,
    command: Option<DaemonCommand>,
    foreground: bool,
) -> Result<()> {
    let cmd = command.unwrap_or(DaemonCommand::Run { foreground: false });
    if foreground {
        match cmd {
            DaemonCommand::Run { .. } | DaemonCommand::Foreground => {}
            _ => {
                return Err(anyhow!(
                    "--foreground cannot be combined with this subcommand"
                ))
            }
        }
    }

    match cmd {
        DaemonCommand::Run { foreground: cmd_fg } => {
            run_daemon_foreground(overrides, foreground || cmd_fg)
        }
        DaemonCommand::Foreground => run_daemon_foreground(overrides, true),
        DaemonCommand::InstallSystemd => {
            let paths = load_paths(overrides)?;
            install_systemd_unit(&paths)
        }
        DaemonCommand::UninstallSystemd => {
            let paths = load_paths(overrides)?;
            uninstall_systemd_unit(&paths)
        }
        DaemonCommand::Start => systemctl_user(&["enable", "--now", "hometree.service"]),
        DaemonCommand::Stop => systemctl_user(&["disable", "--now", "hometree.service"]),
        DaemonCommand::Restart => systemctl_user(&["restart", "hometree.service"]),
        DaemonCommand::Status => daemon_status_cmd(overrides),
        DaemonCommand::Reload => daemon_simple_cmd(overrides, "reload", None, None),
        DaemonCommand::Pause { ttl_ms, reason } => daemon_simple_cmd(
            overrides,
            "pause",
            Some(Duration::from_millis(ttl_ms)),
            Some(reason),
        ),
        DaemonCommand::Resume => daemon_simple_cmd(overrides, "resume", None, None),
        DaemonCommand::Flush => daemon_simple_cmd(overrides, "flush", None, None),
    }
}

pub struct DaemonInhibitGuard {
    paths: Paths,
}

impl DaemonInhibitGuard {
    pub fn new(paths: &Paths, reason: &str, ttl: Duration) -> Result<Self> {
        let marker = InhibitMarker::new(reason, ttl)?;
        write_inhibit(paths, &marker)?;
        if let Err(err) = ipc_pause(paths, ttl, reason) {
            debug!("daemon pause failed: {err}");
        }
        Ok(Self {
            paths: paths.clone(),
        })
    }
}

impl Drop for DaemonInhibitGuard {
    fn drop(&mut self) {
        let _ = clear_inhibit(&self.paths);
        if let Err(err) = ipc_resume(&self.paths) {
            debug!("daemon resume failed: {err}");
        }
    }
}

fn daemon_status_cmd(overrides: &Overrides) -> Result<()> {
    let paths = load_paths(overrides)?;
    match ipc_status(&paths) {
        Ok(status) => {
            print_status(&status);
            Ok(())
        }
        Err(err) => {
            eprintln!("daemon not reachable: {err}");
            eprintln!("hint: run `hometree daemon start`");
            let _ = systemctl_user(&["status", "hometree.service"]);
            Ok(())
        }
    }
}

fn daemon_simple_cmd(
    overrides: &Overrides,
    cmd: &str,
    ttl: Option<Duration>,
    reason: Option<String>,
) -> Result<()> {
    let paths = load_paths(overrides)?;
    ipc_command(&paths, cmd, ttl, reason)
}

fn run_daemon_foreground(overrides: &Overrides, _foreground: bool) -> Result<()> {
    let mut ctx = DaemonContext::load(overrides)?;
    let runtime_dir = ensure_runtime_dir(&ctx.paths)?;
    let socket_path = runtime_dir.join(SOCKET_FILENAME);

    if socket_in_use(&socket_path)? {
        return Err(anyhow!("daemon already running"));
    }
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    let shared = Arc::new(Mutex::new(DaemonShared::new(
        ctx.paths.config_file(),
        ctx.watch_roots.clone(),
    )));

    let (control_tx, control_rx) = mpsc::channel();
    start_ipc_server(&socket_path, control_tx.clone(), shared.clone())?;
    install_sighup_handler(control_tx.clone())?;

    let (mut _watcher, mut rx) = setup_watcher(&ctx.paths, &ctx.watch_roots)?;
    let mut debouncer = Debounce::new(ctx.debounce);
    let mut secrets_debouncer = Debounce::new(ctx.debounce);
    let mut auto_add_queue: BTreeSet<PathBuf> = BTreeSet::new();
    let mut backoff = Backoff::new();
    let mut pause_until: Option<Instant> = None;
    let mut pause_reason: Option<String> = None;
    let mut reload_response: Option<mpsc::Sender<Result<()>>> = None;
    let mut reload_requested = false;
    let mut force_flush = false;
    let mut last_inhibit_check = Instant::now();

    loop {
        while let Ok(msg) = control_rx.try_recv() {
            match msg {
                ControlMessage::Pause {
                    ttl,
                    reason,
                    respond,
                } => {
                    pause_until = Some(Instant::now() + ttl);
                    pause_reason = Some(reason.clone());
                    debouncer.clear();
                    secrets_debouncer.clear();
                    auto_add_queue.clear();
                    update_inhibit(&shared, true, Some(reason));
                    respond_if_needed(respond, Ok(()));
                }
                ControlMessage::Resume { respond } => {
                    pause_until = None;
                    pause_reason = None;
                    update_inhibit(&shared, false, None);
                    respond_if_needed(respond, Ok(()));
                }
                ControlMessage::Flush { respond } => {
                    force_flush = true;
                    respond_if_needed(respond, Ok(()));
                }
                ControlMessage::Reload { respond } => {
                    reload_requested = true;
                    reload_response = respond;
                }
                ControlMessage::Shutdown { respond } => {
                    respond_if_needed(respond, Ok(()));
                    let _ = fs::remove_file(&socket_path);
                    return Ok(());
                }
            }
        }

        if pause_until.is_some() && Instant::now() >= pause_until.unwrap() {
            pause_until = None;
            pause_reason = None;
            update_inhibit(&shared, false, None);
        }

        if reload_requested {
            let reload_result = match DaemonContext::load(overrides) {
                Ok(new_ctx) => {
                    ctx = new_ctx;
                    match setup_watcher(&ctx.paths, &ctx.watch_roots) {
                        Ok((new_watcher, new_rx)) => {
                            _watcher = new_watcher;
                            rx = new_rx;
                            update_watch_roots(&shared, ctx.watch_roots.clone());
                            Ok(())
                        }
                        Err(err) => Err(err),
                    }
                }
                Err(err) => Err(err),
            };
            if let Err(ref err) = reload_result {
                record_error(&shared, &ctx.paths, err);
            }
            respond_if_needed(reload_response.take(), reload_result);
            reload_requested = false;
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if should_handle_event(&event.kind) {
                    handle_event(
                        &ctx,
                        &event.paths,
                        &mut debouncer,
                        &mut secrets_debouncer,
                        &mut auto_add_queue,
                        &mut force_flush,
                    );
                }
            }
            Ok(Err(err)) => {
                record_error(&shared, &ctx.paths, &err);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        update_queue_size(
            &shared,
            debouncer.len(),
            secrets_debouncer.len(),
            auto_add_queue.len(),
        );

        let now = Instant::now();
        if now.duration_since(last_inhibit_check) >= Duration::from_secs(1) {
            if let Ok(marker) = active_inhibit(&ctx.paths) {
                if let Some(marker) = marker {
                    update_inhibit(&shared, true, Some(marker.reason.clone()));
                } else if pause_until.is_none() {
                    update_inhibit(&shared, false, None);
                }
            }
            last_inhibit_check = now;
        }

        let flush_due = force_flush
            || (debouncer.is_due(now) && !debouncer.is_empty())
            || (secrets_debouncer.is_due(now) && !secrets_debouncer.is_empty());
        if !flush_due {
            continue;
        }

        if !backoff.ready(now) {
            continue;
        }

        force_flush = false;
        let inhibit_marker = active_inhibit(&ctx.paths)?;
        if pause_until.is_some() || inhibit_marker.is_some() {
            if let Some(marker) = inhibit_marker {
                update_inhibit(&shared, true, Some(marker.reason));
            } else if pause_until.is_some() {
                update_inhibit(&shared, true, pause_reason.clone());
            }
            debouncer.clear();
            secrets_debouncer.clear();
            auto_add_queue.clear();
            continue;
        }

        let flush_result = flush_changes(
            &ctx,
            &mut debouncer,
            &mut secrets_debouncer,
            &mut auto_add_queue,
        );
        match flush_result {
            Ok(()) => {
                backoff.reset();
                update_flush_time(&shared);
            }
            Err(err) => {
                record_error(&shared, &ctx.paths, &err);
                backoff.fail(now);
            }
        }
    }

    Ok(())
}

fn handle_event(
    ctx: &DaemonContext,
    paths: &[PathBuf],
    debouncer: &mut Debounce<PathBuf>,
    secrets_debouncer: &mut Debounce<PathBuf>,
    auto_add_queue: &mut BTreeSet<PathBuf>,
    force_flush: &mut bool,
) {
    for abs in paths {
        let rel = match normalize_rel_path(ctx.paths.home_dir(), abs) {
            Some(rel) => rel,
            None => continue,
        };
        let decisions = collect_watch_decisions(
            &ctx.managed,
            &ctx.secrets,
            &ctx.allowlist,
            ctx.auto_add_enabled,
            std::iter::once(rel),
        );
        log_auto_add_skips(&decisions, ctx.auto_add_enabled);
        for rel_path in decisions.managed_stage {
            debouncer.push(rel_path, Instant::now());
        }
        for rel_path in decisions.secret_plaintext {
            secrets_debouncer.push(rel_path, Instant::now());
        }
        if !decisions.auto_add.is_empty() {
            for rel_path in decisions.auto_add {
                auto_add_queue.insert(rel_path);
            }
            *force_flush = true;
        }
    }
}

fn log_auto_add_skips(decisions: &WatchDecisions, auto_add_enabled: bool) {
    if !auto_add_enabled {
        return;
    }
    for meta in &decisions.auto_add_meta {
        if meta.auto_add {
            continue;
        }
        if !meta.is_allowed {
            debug!(path = %meta.path.display(), "skipped auto-add: path is ignored or denylisted");
        } else if !meta.matches_allowlist {
            debug!(path = %meta.path.display(), "skipped auto-add: path does not match allowlist");
        }
    }
}

fn flush_changes(
    ctx: &DaemonContext,
    debouncer: &mut Debounce<PathBuf>,
    secrets_debouncer: &mut Debounce<PathBuf>,
    auto_add_queue: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let now = Instant::now();
    let managed_paths = debouncer.drain();
    let secret_paths = secrets_debouncer.drain();
    let auto_add_paths: Vec<PathBuf> = auto_add_queue.iter().cloned().collect();
    auto_add_queue.clear();

    if managed_paths.is_empty() && secret_paths.is_empty() && auto_add_paths.is_empty() {
        return Ok(());
    }

    let result = (|| {
        let lock_file = match try_acquire_lock(&ctx.paths)? {
            Some(file) => file,
            None => return Err(anyhow!("lock busy")),
        };

        let git = GitCliBackend::new();
        let git_dir = &ctx.config.repo.git_dir;
        let work_tree = &ctx.config.repo.work_tree;

        if !auto_add_paths.is_empty() {
            git.add(git_dir, work_tree, &auto_add_paths, AddMode::Paths)
                .context("auto-add")?;
        }

        if !secret_paths.is_empty() {
            let backend = ctx
                .secrets_backend
                .as_ref()
                .ok_or_else(|| anyhow!("secrets enabled but backend unavailable"))?;
            let mut ciphertext_paths = Vec::new();
            for rel in &secret_paths {
                let plaintext_abs = ctx.paths.home_dir().join(rel);
                let plaintext = match std::fs::read(&plaintext_abs) {
                    Ok(contents) => contents,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => {
                        warn!("failed to read secret plaintext: {err}");
                        continue;
                    }
                };
                let ciphertext = backend.encrypt(&plaintext)?;
                let rule = match ctx
                    .config
                    .secrets
                    .rules
                    .iter()
                    .find(|rule| rule.path == rel.to_string_lossy())
                {
                    Some(rule) => rule,
                    None => {
                        warn!("secret rule not found for path");
                        continue;
                    }
                };
                let ciphertext_rel = if let Some(ciphertext_path) = &rule.ciphertext {
                    PathBuf::from(ciphertext_path)
                } else {
                    hometree_core::secrets::add_suffix(
                        Path::new(&rule.path),
                        &ctx.config.secrets.sidecar_suffix,
                    )
                };
                let ciphertext_abs = ctx.paths.home_dir().join(&ciphertext_rel);
                if let Some(parent) = ciphertext_abs.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&ciphertext_abs, ciphertext)?;
                ciphertext_paths.push(ciphertext_rel);
            }
            if !ciphertext_paths.is_empty() {
                git.add(git_dir, work_tree, &ciphertext_paths, AddMode::Paths)
                    .context("stage secret sidecars")?;
            }
        }

        if !managed_paths.is_empty() {
            let mode = if ctx.config.watch.auto_stage_tracked_only {
                AddMode::TrackedOnly
            } else {
                AddMode::Paths
            };
            git.add(git_dir, work_tree, &managed_paths, mode)
                .context("stage managed paths")?;
        }

        drop(lock_file);
        Ok(())
    })();

    if result.is_err() {
        requeue(
            debouncer,
            secrets_debouncer,
            auto_add_queue,
            &managed_paths,
            &secret_paths,
            &auto_add_paths,
            now,
        );
    }
    result
}

fn requeue(
    debouncer: &mut Debounce<PathBuf>,
    secrets_debouncer: &mut Debounce<PathBuf>,
    auto_add_queue: &mut BTreeSet<PathBuf>,
    managed: &[PathBuf],
    secrets: &[PathBuf],
    auto_add: &[PathBuf],
    now: Instant,
) {
    for path in managed {
        debouncer.push(path.clone(), now);
    }
    for path in secrets {
        secrets_debouncer.push(path.clone(), now);
    }
    for path in auto_add {
        auto_add_queue.insert(path.clone());
    }
}

fn normalize_rel_path(home_dir: &Path, abs: &Path) -> Option<PathBuf> {
    let rel = abs.strip_prefix(home_dir).ok()?.to_path_buf();
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    Some(rel)
}

fn try_acquire_lock(paths: &Paths) -> Result<Option<std::fs::File>> {
    fs::create_dir_all(paths.state_dir())?;
    let path = lock_path(paths);
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn ensure_runtime_dir(paths: &Paths) -> Result<PathBuf> {
    let runtime = paths
        .runtime_dir()
        .ok_or_else(|| anyhow!("XDG_RUNTIME_DIR is not set"))?
        .to_path_buf();
    fs::create_dir_all(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    Ok(runtime)
}

fn socket_in_use(socket_path: &Path) -> Result<bool> {
    if !socket_path.exists() {
        return Ok(false);
    }
    match UnixStream::connect(socket_path) {
        Ok(mut stream) => {
            let req = IpcRequest::ping();
            if write_request(&mut stream, &req).is_err() {
                return Ok(false);
            }
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

fn start_ipc_server(
    socket_path: &Path,
    control_tx: mpsc::Sender<ControlMessage>,
    shared: Arc<Mutex<DaemonShared>>,
) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(socket_path).context("bind daemon socket")?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(err) = handle_ipc_connection(stream, &control_tx, &shared) {
                        error!("ipc error: {err}");
                    }
                }
                Err(err) => {
                    error!("ipc accept error: {err}");
                }
            }
        }
    });

    Ok(())
}

fn handle_ipc_connection(
    stream: UnixStream,
    control_tx: &mpsc::Sender<ControlMessage>,
    shared: &Arc<Mutex<DaemonShared>>,
) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        return Ok(());
    }
    if line.len() > IPC_MAX_LINE {
        return Err(anyhow!("request too large"));
    }
    let req: IpcRequest = serde_json::from_str(line.trim_end())?;
    let response: IpcResponse<serde_json::Value> = match req.cmd.as_str() {
        "ping" => IpcResponse::ok(serde_json::Value::Null),
        "status" => {
            let status = shared.lock().map(|s| s.status()).unwrap_or_default();
            let value = serde_json::to_value(status)?;
            IpcResponse::ok(value)
        }
        "pause" => {
            let ttl = req
                .ttl_ms
                .map(Duration::from_millis)
                .unwrap_or(INHIBIT_TTL_DEFAULT);
            let reason = req.reason.unwrap_or_else(|| "manual".to_string());
            let (tx, rx) = mpsc::channel();
            let msg = ControlMessage::Pause {
                ttl,
                reason,
                respond: Some(tx),
            };
            let send_res = control_tx.send(msg).map_err(|e| anyhow!(e.to_string()));
            match send_res.and_then(|_| rx.recv().unwrap_or_else(|e| Err(anyhow!(e)))) {
                Ok(_) => IpcResponse::ok(serde_json::Value::Null),
                Err(err) => IpcResponse::err(err.to_string()),
            }
        }
        "resume" => ipc_control_simple(
            control_tx,
            ControlMessage::Resume { respond: None },
            "resume",
        ),
        "flush" => ipc_control_simple(control_tx, ControlMessage::Flush { respond: None }, "flush"),
        "reload" => ipc_control_simple(
            control_tx,
            ControlMessage::Reload { respond: None },
            "reload",
        ),
        "shutdown" => ipc_control_simple(
            control_tx,
            ControlMessage::Shutdown { respond: None },
            "shutdown",
        ),
        other => IpcResponse::err(format!("unknown command: {other}")),
    };
    let mut writer = BufWriter::new(stream);
    let json = serde_json::to_string(&response)?;
    writer.write_all(json.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn ipc_control_simple(
    control_tx: &mpsc::Sender<ControlMessage>,
    msg: ControlMessage,
    name: &str,
) -> IpcResponse<serde_json::Value> {
    let (tx, rx) = mpsc::channel();
    let msg = match msg {
        ControlMessage::Resume { .. } => ControlMessage::Resume { respond: Some(tx) },
        ControlMessage::Flush { .. } => ControlMessage::Flush { respond: Some(tx) },
        ControlMessage::Reload { .. } => ControlMessage::Reload { respond: Some(tx) },
        ControlMessage::Shutdown { .. } => ControlMessage::Shutdown { respond: Some(tx) },
        other => other,
    };
    if let Err(err) = control_tx.send(msg) {
        return IpcResponse::err(format!("{name} send failed: {err}"));
    }
    match rx.recv().unwrap_or_else(|e| Err(anyhow!(e))) {
        Ok(_) => IpcResponse::ok(serde_json::Value::Null),
        Err(err) => IpcResponse::err(err.to_string()),
    }
}

fn install_sighup_handler(control_tx: mpsc::Sender<ControlMessage>) -> Result<()> {
    let mut signals = Signals::new([SIGHUP])?;
    std::thread::spawn(move || {
        for _ in signals.forever() {
            let _ = control_tx.send(ControlMessage::Reload { respond: None });
        }
    });
    Ok(())
}

fn setup_watcher(
    paths: &Paths,
    watch_roots: &[PathBuf],
) -> Result<(
    notify::RecommendedWatcher,
    mpsc::Receiver<notify::Result<notify::Event>>,
)> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    for path in watch_roots {
        let abs = paths.home_dir().join(path);
        watcher.watch(&abs, RecursiveMode::Recursive)?;
    }
    Ok((watcher, rx))
}

fn update_queue_size(
    shared: &Arc<Mutex<DaemonShared>>,
    managed: usize,
    secrets: usize,
    auto_add: usize,
) {
    if let Ok(mut shared) = shared.lock() {
        shared.queue_size = managed + secrets + auto_add;
    }
}

fn update_flush_time(shared: &Arc<Mutex<DaemonShared>>) {
    if let Ok(mut shared) = shared.lock() {
        shared.last_flush_at = Some(SystemTime::now());
        shared.total_flushes = shared.total_flushes.saturating_add(1);
    }
}

fn update_inhibit(shared: &Arc<Mutex<DaemonShared>>, inhibited: bool, reason: Option<String>) {
    if let Ok(mut shared) = shared.lock() {
        shared.inhibited = inhibited;
        shared.inhibit_reason = reason;
    }
}

fn update_watch_roots(shared: &Arc<Mutex<DaemonShared>>, roots: Vec<PathBuf>) {
    if let Ok(mut shared) = shared.lock() {
        shared.watch_roots = roots;
    }
}

fn record_error(shared: &Arc<Mutex<DaemonShared>>, paths: &Paths, err: &dyn std::fmt::Display) {
    if let Ok(mut shared) = shared.lock() {
        let now = SystemTime::now();
        shared.last_error = Some(err.to_string());
        shared.last_error_at = Some(now);
        let _ = write_last_error(paths, &shared.last_error, shared.last_error_at);
    }
}

fn write_last_error(paths: &Paths, message: &Option<String>, ts: Option<SystemTime>) -> Result<()> {
    let Some(message) = message.as_ref() else {
        return Ok(());
    };
    let timestamp = ts.map(format_time).unwrap_or_else(|| "unknown".to_string());
    let payload = LastError {
        message: message.clone(),
        timestamp,
    };
    let dir = paths.state_dir().join("daemon");
    fs::create_dir_all(&dir)?;
    let path = dir.join("last_error.json");
    fs::write(path, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}

fn respond_if_needed(respond: Option<mpsc::Sender<Result<()>>>, result: Result<()>) {
    if let Some(tx) = respond {
        let _ = tx.send(result);
    }
}

fn ipc_command(
    paths: &Paths,
    cmd: &str,
    ttl: Option<Duration>,
    reason: Option<String>,
) -> Result<()> {
    let req = IpcRequest {
        cmd: cmd.to_string(),
        ttl_ms: ttl.map(|d| d.as_millis() as u64),
        reason,
    };
    let resp: IpcResponse<serde_json::Value> = ipc_request(paths, &req)?;
    if resp.ok {
        Ok(())
    } else {
        Err(anyhow!(resp
            .error
            .unwrap_or_else(|| "unknown error".to_string())))
    }
}

fn ipc_pause(paths: &Paths, ttl: Duration, reason: &str) -> Result<()> {
    ipc_command(paths, "pause", Some(ttl), Some(reason.to_string()))
}

fn ipc_resume(paths: &Paths) -> Result<()> {
    ipc_command(paths, "resume", None, None)
}

fn ipc_status(paths: &Paths) -> Result<DaemonStatus> {
    let req = IpcRequest::status();
    let resp: IpcResponse<serde_json::Value> = ipc_request(paths, &req)?;
    if resp.ok {
        let value = resp.result.ok_or_else(|| anyhow!("missing status"))?;
        let status: DaemonStatus = serde_json::from_value(value)?;
        Ok(status)
    } else {
        Err(anyhow!(resp
            .error
            .unwrap_or_else(|| "unknown error".to_string())))
    }
}

fn ipc_request<T: for<'de> Deserialize<'de>, R: Serialize>(
    paths: &Paths,
    req: &R,
) -> Result<IpcResponse<T>> {
    let socket = daemon_socket_path(paths)?;
    let mut stream = UnixStream::connect(socket).context("connect daemon socket")?;
    write_request(&mut stream, req)?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.len() > IPC_MAX_LINE {
        return Err(anyhow!("response too large"));
    }
    let resp: IpcResponse<T> = serde_json::from_str(line.trim_end())?;
    Ok(resp)
}

fn write_request<R: Serialize>(stream: &mut UnixStream, req: &R) -> Result<()> {
    let mut writer = BufWriter::new(stream);
    let json = serde_json::to_string(req)?;
    if json.len() > IPC_MAX_LINE {
        return Err(anyhow!("request too large"));
    }
    writer.write_all(json.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn daemon_socket_path(paths: &Paths) -> Result<PathBuf> {
    let runtime = paths
        .runtime_dir()
        .ok_or_else(|| anyhow!("XDG_RUNTIME_DIR is not set"))?;
    Ok(runtime.join(SOCKET_FILENAME))
}

fn print_status(status: &DaemonStatus) {
    println!("running_since: {}", status.running_since);
    println!("config_path: {}", status.config_path);
    println!("watch_roots:");
    for root in &status.watch_roots {
        println!("  - {root}");
    }
    println!("queue_size: {}", status.queue_size);
    println!("inhibited: {}", status.inhibited);
    if let Some(reason) = &status.inhibit_reason {
        println!("inhibit_reason: {reason}");
    }
    if let Some(last_flush) = &status.last_flush_at {
        println!("last_flush_at: {last_flush}");
    }
    if let Some(last_error) = &status.last_error {
        println!("last_error: {last_error}");
    }
    if let Some(ts) = &status.last_error_at {
        println!("last_error_at: {ts}");
    }
    println!("total_flushes: {}", status.total_flushes);
}

fn install_systemd_unit(paths: &Paths) -> Result<()> {
    let config_dir = paths.config_home_dir();
    let unit_dir = config_dir.join("systemd").join("user");
    fs::create_dir_all(&unit_dir).context("create systemd user dir")?;

    let unit_path = unit_dir.join("hometree.service");
    let unit = format!(
        "[Unit]\nDescription=hometree daemon\n\n[Service]\nType=simple\nExecStart={exe} daemon run\nRestart=on-failure\nRestartSec=2\nRuntimeDirectory=hometree\nRuntimeDirectoryMode=0700\nExecReload=/bin/kill -HUP $MAINPID\n\n[Install]\nWantedBy=default.target\n",
        exe = std::env::current_exe()?.display()
    );
    fs::write(&unit_path, unit).context("write systemd unit")?;

    println!("installed {}", unit_path.display());
    println!("run: systemctl --user daemon-reload");
    Ok(())
}

fn uninstall_systemd_unit(paths: &Paths) -> Result<()> {
    let config_dir = paths.config_home_dir();
    let unit_dir = config_dir.join("systemd").join("user");
    let unit_path = unit_dir.join("hometree.service");
    let _ = systemctl_user(&["disable", "--now", "hometree.service"]);
    if unit_path.exists() {
        fs::remove_file(&unit_path).context("remove systemd unit")?;
    }
    let _ = systemctl_user(&["daemon-reload"]);
    Ok(())
}

fn systemctl_user(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("systemctl --user")?;
    if !status.success() {
        return Err(anyhow!("systemctl failed"));
    }
    Ok(())
}

struct DaemonContext {
    paths: Paths,
    config: hometree_core::Config,
    managed: ManagedSet,
    secrets: SecretsManager,
    secrets_backend: Option<AgeBackend>,
    allowlist: globset::GlobSet,
    auto_add_enabled: bool,
    debounce: Duration,
    watch_roots: Vec<PathBuf>,
}

impl DaemonContext {
    fn load(overrides: &Overrides) -> Result<Self> {
        let (paths, config) = load_config(overrides)?;
        if !config.watch.enabled {
            return Err(anyhow!("watch is disabled in config"));
        }
        let managed = ManagedSet::from_config(&config).context("build managed set")?;
        let watch_roots = watch_paths(&config);
        if watch_roots.is_empty() {
            return Err(anyhow!("no managed roots or extra files configured"));
        }
        let allowlist_patterns = &config.watch.auto_add_allow_patterns;
        let allowlist_has_entries = allowlist_patterns.iter().any(|p| !p.trim().is_empty());
        let allowlist = build_allowlist(allowlist_patterns)?;
        let auto_add_enabled = config.watch.auto_add_new && allowlist_has_entries;
        if config.watch.auto_add_new && !allowlist_has_entries {
            info!("auto_add_new enabled but allowlist is empty; skipping auto-add");
        }
        let secrets = SecretsManager::from_config(&config.secrets);
        let secrets_backend = if secrets.enabled() {
            Some(AgeBackend::from_config(&config.secrets)?)
        } else {
            None
        };
        let debounce_ms = config.watch.debounce_ms.max(50);
        Ok(Self {
            paths,
            config,
            managed,
            secrets,
            secrets_backend,
            allowlist,
            auto_add_enabled,
            debounce: Duration::from_millis(debounce_ms),
            watch_roots,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DaemonStatus {
    running_since: String,
    config_path: String,
    watch_roots: Vec<String>,
    queue_size: usize,
    inhibited: bool,
    inhibit_reason: Option<String>,
    last_flush_at: Option<String>,
    last_error: Option<String>,
    last_error_at: Option<String>,
    total_flushes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct LastError {
    message: String,
    timestamp: String,
}

struct DaemonShared {
    running_since: SystemTime,
    config_path: PathBuf,
    watch_roots: Vec<PathBuf>,
    queue_size: usize,
    inhibited: bool,
    inhibit_reason: Option<String>,
    last_flush_at: Option<SystemTime>,
    last_error: Option<String>,
    last_error_at: Option<SystemTime>,
    total_flushes: u64,
}

impl Default for DaemonShared {
    fn default() -> Self {
        Self {
            running_since: SystemTime::now(),
            config_path: PathBuf::new(),
            watch_roots: Vec::new(),
            queue_size: 0,
            inhibited: false,
            inhibit_reason: None,
            last_flush_at: None,
            last_error: None,
            last_error_at: None,
            total_flushes: 0,
        }
    }
}

impl DaemonShared {
    fn new(config_path: PathBuf, watch_roots: Vec<PathBuf>) -> Self {
        Self {
            running_since: SystemTime::now(),
            config_path,
            watch_roots,
            ..Default::default()
        }
    }

    fn status(&self) -> DaemonStatus {
        DaemonStatus {
            running_since: format_time(self.running_since),
            config_path: self.config_path.display().to_string(),
            watch_roots: self
                .watch_roots
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            queue_size: self.queue_size,
            inhibited: self.inhibited,
            inhibit_reason: self.inhibit_reason.clone(),
            last_flush_at: self.last_flush_at.map(format_time),
            last_error: self.last_error.clone(),
            last_error_at: self.last_error_at.map(format_time),
            total_flushes: self.total_flushes,
        }
    }
}

fn format_time(ts: SystemTime) -> String {
    OffsetDateTime::from(ts)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcRequest {
    cmd: String,
    ttl_ms: Option<u64>,
    reason: Option<String>,
}

impl IpcRequest {
    fn ping() -> Self {
        Self {
            cmd: "ping".to_string(),
            ttl_ms: None,
            reason: None,
        }
    }

    fn status() -> Self {
        Self {
            cmd: "status".to_string(),
            ttl_ms: None,
            reason: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcResponse<T> {
    ok: bool,
    result: Option<T>,
    error: Option<String>,
}

impl<T> IpcResponse<T> {
    fn ok(result: T) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn err(error: String) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

enum ControlMessage {
    Pause {
        ttl: Duration,
        reason: String,
        respond: Option<mpsc::Sender<Result<()>>>,
    },
    Resume {
        respond: Option<mpsc::Sender<Result<()>>>,
    },
    Flush {
        respond: Option<mpsc::Sender<Result<()>>>,
    },
    Reload {
        respond: Option<mpsc::Sender<Result<()>>>,
    },
    Shutdown {
        respond: Option<mpsc::Sender<Result<()>>>,
    },
}

struct Backoff {
    current: Duration,
    max: Duration,
    until: Option<Instant>,
}

impl Backoff {
    fn new() -> Self {
        Self {
            current: Duration::from_millis(200),
            max: Duration::from_secs(10),
            until: None,
        }
    }

    fn ready(&self, now: Instant) -> bool {
        self.until.is_none_or(|u| now >= u)
    }

    fn fail(&mut self, now: Instant) {
        let next = (self.current + self.current).min(self.max);
        self.current = next;
        self.until = Some(now + self.current);
    }

    fn reset(&mut self) {
        self.current = Duration::from_millis(200);
        self.until = None;
    }
}

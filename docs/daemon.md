---
title: Daemon
---

# hometree daemon (watch)

How to run the watcher in the foreground or as a systemd user service. The watcher is event-driven (notify + debounce) and never scans your full home directory.

## Prerequisites
- `watch.enabled = true` in `config.toml`.
- At least one managed root or extra file. Patterns with glob meta are skipped for watching; use concrete directories.
- Optional auto-add: set `watch.auto_add_new = true` **and** provide a non-empty `watch.auto_add_allow_patterns`. Allowlist entries are validated (max 50; broad patterns like `*` or absolute paths are rejected).
- Secrets: if enabled, you must have valid age recipients (encrypt) and identity files (decrypt). Plaintext secrets are never staged.

## Foreground mode
```bash
RUST_LOG=info hometree daemon run
# alias
RUST_LOG=info hometree watch foreground
```
- `hometree daemon --foreground` is a compatibility alias for foreground mode.
- Debounces filesystem events (`watch.debounce_ms`, minimum 50ms). `watch.auto_stage_tracked_only=true` by default, so only managed files are staged.
- Auto-adds new files only when they are (a) under the managed set, (b) allowed by ignore/denylist, and (c) match the allowlist. Skipped auto-adds log a reason at `debug` level.
- Secrets: plaintext edits trigger sidecar regeneration and staging when secrets are enabled.
- Use `--home-root` / `--xdg-root` if you need a temporary HOME/XDG for testing the daemon.

## IPC commands
These commands talk to the running daemon via a Unix socket under `$HOMETREE_RUNTIME_DIR/hometree` (or `$XDG_RUNTIME_DIR/hometree` when not set).

```bash
hometree daemon status
hometree daemon reload
hometree daemon pause --ttl-ms 300000 --reason deploy
hometree daemon resume
hometree daemon flush
```

Notes:
- `pause` writes an inhibit marker and stops staging for the TTL (defaults: `ttl_ms=300000`, `reason=manual`). `resume` clears it.
- `reload` re-reads config and watch roots.
- `flush` immediately stages queued changes.
- IPC commands require a runtime dir; if neither `HOMETREE_RUNTIME_DIR` nor `XDG_RUNTIME_DIR` is set, they will fail.

## Systemd user service
Systemd integration is user-session only.

Install the unit:
```bash
hometree daemon install-systemd
systemctl --user daemon-reload
```

Start/stop/status:
```bash
hometree daemon start
hometree daemon stop
hometree daemon restart
hometree daemon status
hometree daemon uninstall-systemd
```

Details:
- Unit path: `~/.config/systemd/user/hometree.service` (XDG-aware). ExecStart: `hometree daemon run`. Restart policy: `on-failure`.
- Uses the hometree binary found when you run `install-systemd`; re-run the install step after upgrading the binary or changing HOME/XDG overrides.
- Logs: `journalctl --user -u hometree.service` (add `-f` to follow). To change log level, set `RUST_LOG` in the service environment (edit the unit file) and restart.

## Safety notes
- Watch scope is limited to managed roots/extra files; ignored/denylisted paths are not staged. Secrets ciphertext files are also skipped for watch-triggered staging.
- Auto-add is allowlist-only and remains off when the allowlist is empty. Patterns are validated to prevent accidental full-home ingestion.
- Symlink targets are not followed by the watcher. Deploy/rollback continues to enforce symlink target staying under `$HOME`.

## Updating config
- The watcher reads config on start. After editing `config.toml` (e.g., toggling auto-add), restart the foreground process or `systemctl --user restart hometree.service`.

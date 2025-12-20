# hometree

Linux-first CLI for managing a versioned subset of your home directory using a bare git repo.

## Quick start

```bash
hometree init
hometree track ~/.config/myapp/config.toml
hometree untrack ~/.config/myapp/config.toml
hometree snapshot -m "track config"
```

## Watcher

Foreground watcher (stages tracked changes; optionally auto-adds new files):

```bash
hometree watch foreground
# or legacy form
hometree watch --foreground
```

Auto-add new files (optional, allowlist required):

```toml
[watch]
auto_add_new = true
auto_add_allow_patterns = [".config/**", ".local/bin/*"]
```

Auto-add applies to new files under managed roots/extra_files when the allowlist matches and the path is not ignored/denylisted.

### systemd user integration (optional)

Install the unit:

```bash
hometree watch install-systemd
```

Then reload user units:

```bash
systemctl --user daemon-reload
```

Start/stop/status:

```bash
hometree watch start
hometree watch stop
hometree watch status
```

## Safety Features

hometree includes several safety features to protect your system:

### Symlink Security
- Symlink targets are validated to prevent escaping the home directory
- Both absolute and relative symlink paths are checked before deployment
- Prevents malicious symlinks from pointing to sensitive system files
- When deploying a regular file over an existing symlink, hometree replaces the symlink (does not dereference it)

### Pattern Validation
- `auto_add_allow_patterns` are validated to prevent overly broad patterns
- Patterns like `*`, `**`, or `*.txt` (no path separator) are rejected
- Absolute paths (e.g., `/etc/passwd`) are rejected; patterns should be relative to home
- Maximum of 50 patterns allowed in `auto_add_allow_patterns`
- Prevents accidentally tracking all files in your home directory

### Auto-Add Logging
- When auto-add is enabled, each auto-added file is logged with `info!()` level
- Skipped files are logged at `debug!()` level with reason (ignored/denylisted or not matching allowlist)
- Set `RUST_LOG=info` to see auto-add activity; use `RUST_LOG=debug` for troubleshooting

### Metadata Preservation
- On Unix systems, deploy best-effort preserves owner/group/mtime for existing files
- File permissions (executable bit) are preserved from git tree mode
- Symlinks are preserved as-is (not dereferenced)

## Limitations (MVP)
- The systemd unit runs `hometree watch foreground` only.
- Auto-add is allowlisted-only; if `auto_add_allow_patterns` is empty, auto-add is disabled.
- If no generations exist, `rollback` falls back to `HEAD~N`.
- Owner/group preservation requires appropriate privileges; failures are silent.

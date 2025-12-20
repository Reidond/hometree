# hometree

Linux-first CLI for managing a versioned subset of your home directory using a bare git repo.

## Security model (secrets)

Threat model:
- Prevent sensitive data from being stored in git history (especially when pushed to remotes).

Non-goals:
- Full-disk encryption (use OS/disk encryption such as LUKS).
- Defending against a fully compromised machine (malware can read plaintext in $HOME).

## Quick start

```bash
hometree init
hometree track ~/.config/myapp/config.toml
hometree untrack ~/.config/myapp/config.toml
hometree snapshot -m "track config"
```

## Verify

Verify that your home tree matches a commit (default: HEAD):

```bash
hometree verify --strict
```

Secrets-aware verification (presence/decrypt) with redacted paths by default:

```bash
hometree verify --with-secrets=presence
hometree verify --with-secrets=decrypt --show-paths
```

## Plan deploy

Preview the changes a deploy would make without applying them:

```bash
hometree plan deploy HEAD
```

## Deploy (no-backup)

Skip backups for test runs or dry environments:

```bash
hometree deploy HEAD --no-backup
```

## Fake HOME / XDG roots

Use a temporary HOME and XDG roots for testing:

```bash
hometree --home-root /tmp/home --xdg-root /tmp/xdg verify
```

## Sandbox / chroot (optional)

Rootless sandbox or chroot-based validation is not implemented in the MVP.
For stronger isolation, run hometree inside your own sandbox tool (e.g. user
namespaces or container) with HOME/XDG roots pointed at a temp directory.

## Secrets (sidecar .age)

Secrets are opt-in and stored as sidecar ciphertext files (e.g. `secret.txt.age`).
Plaintext secret paths are ignored by git via a hometree-managed excludes file.

Secrets config example:

```toml
[secrets]
enabled = true
backend = "age"
sidecar_suffix = ".age"
recipients = ["age1example..."]
identity_files = ["~/.config/hometree/keys/identity.txt"]
backup_policy = "encrypt" # encrypt | skip | plaintext

[[secrets.rules]]
path = ".config/app/secret.txt"
mode = 0o600
```

```bash
hometree secret add ~/.config/app/secret.txt
hometree secret refresh
hometree secret status
hometree secret status --show-paths
hometree secret rekey
```

## Daemon / Watcher

Foreground daemon (stages tracked changes; optionally auto-adds new files):

```bash
hometree daemon run --foreground
# or alias
hometree watch foreground
```

Auto-add new files (optional, allowlist required):

```toml
[watch]
auto_add_new = true
auto_add_allow_patterns = [".config/**", ".local/bin/*"]
```

Auto-add applies to new files under managed roots/extra_files when the allowlist matches and the path is not ignored.

### systemd user integration (optional)

Install the unit:

```bash
hometree daemon install-systemd
```

Then reload user units:

```bash
systemctl --user daemon-reload
```

Start/stop/status:

```bash
hometree daemon start
hometree daemon stop
hometree daemon status
```

Reload/pause/resume/flush via IPC:

```bash
hometree daemon reload
hometree daemon pause --ttl-ms 300000 --reason deploy
hometree daemon resume
hometree daemon flush
```

Deploy/rollback automatically pause the daemon and set an inhibit marker to avoid staging applied changes.

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
- Skipped files are logged at `debug!()` level with reason (ignored or not matching allowlist)
- Set `RUST_LOG=info` to see auto-add activity; use `RUST_LOG=debug` for troubleshooting

### Metadata Preservation
- On Unix systems, deploy best-effort preserves owner/group/mtime for existing files
- File permissions (executable bit) are preserved from git tree mode
- Symlinks are preserved as-is (not dereferenced)

## Limitations (MVP)
- Systemd integration is Linux/user-session only (no DBus API).
- Auto-add is allowlisted-only; if `auto_add_allow_patterns` is empty, auto-add is disabled.
- If no generations exist, `rollback` falls back to `HEAD~N`.
- Owner/group preservation requires appropriate privileges; failures are silent.

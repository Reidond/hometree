# hometree CLI

Reference for every hometree command, flag, and safety default. Examples are copy/pasteable on Linux shells.

## Paths and defaults
- Config: `~/.config/hometree/config.toml` (XDG-aware). Repo: `~/.local/share/hometree/repo.git`. State/backups: `~/.local/state/hometree/`.
- Default managed roots: `.config/`, `.local/bin/`, `.local/share/systemd/user/`, `.local/share/applications/`. Extra files start empty.
- Default ignore patterns (deny tracking): `.ssh/**`, `.gnupg/**`, `.local/share/keyrings/**`, `.local/share/kwalletd/**`, `.pki/**`, `.mozilla/**`, `.config/google-chrome/**`, `.config/chromium/**`, `.config/BraveSoftware/**`, and any path containing `token` or `secret`.
- Watcher defaults: disabled, `debounce_ms=500`, `auto_stage_tracked_only=true`, `auto_add_new=false`, `auto_add_allow_patterns=[]` (empty disables auto-add). Max 50 allowlist patterns; overly broad patterns (`*`, `**`, no path separator, absolute paths) are rejected.
- Secrets: disabled by default; backend `age`, sidecar suffix `.age`, backup policy `encrypt`. When enabled, recipients are required to encrypt; identity files are required to decrypt/verify.
- Snapshots: no auto message template by default.

## Global flags
- `--home-root <path>` (`HOMETREE_HOME_ROOT`): fake `$HOME` for all operations (tests/sandboxes only).
- `--xdg-root <path>` (`HOMETREE_XDG_ROOT`): override XDG roots; hometree config/data/state/cache live under this root.

## Commands

### init
```
hometree init
```
- Creates config/data/state dirs, writes default `config.toml` if missing, and initializes the bare git repo. Idempotent.
- Sets `status.showUntrackedFiles=no` in the repo to keep `git status` lean.

### status
```
hometree status
```
- Shows porcelain status for managed files only; ignores files outside the managed set and plaintext secrets. Prints `clean` when no changes.

### track
```
hometree track [--allow-outside] [--force] <path>...
```
- Tracks paths relative to `$HOME`. Paths already under managed roots are added directly; outside paths require `--allow-outside` and are appended to `manage.extra_files`.
- Honors ignore/denylist; use `--force` to override. Refuses plaintext secret paths (`use hometree secret add`).
- Stages tracked paths in git. Updates `config.toml` when extra files are added.

### untrack
```
hometree untrack <path>...
```
- Stops managing paths without deleting them. Removes entries from `extra_files` or adds an ignore pattern for in-root paths (directories become `path/**`).
- Refuses plaintext secret paths. Unstages paths from git (`rm --cached`).

### snapshot
```
hometree snapshot -m "message"
hometree snapshot --auto
```
- Commits staged changes. `-m` is required unless `--auto` is set.
- `--auto` uses `snapshot.auto_message_template`; errors if missing.
- Safety: aborts if any plaintext secret is staged.

### log
```
hometree log [--limit N]
```
- Shows git history limited to the managed work tree.

### daemon (alias: watch)
```
hometree daemon            # same as: daemon run
hometree daemon run
hometree daemon --foreground
hometree watch foreground
hometree daemon install-systemd
hometree daemon uninstall-systemd
hometree daemon start|stop|restart|status
hometree daemon reload
hometree daemon pause --ttl-ms 300000 --reason deploy
hometree daemon resume
hometree daemon flush
```
- Requires `watch.enabled = true` and at least one managed root/extra file.
- Event-driven watcher (no full-home scans). Debounces events (`debounce_ms`, minimum 50ms). Stages changes to managed files only.
- Auto-add: enable with `watch.auto_add_new = true` *and* a non-empty `auto_add_allow_patterns` allowlist (max 50, overly broad patterns rejected). Auto-add applies only to managed, allowed paths; skipped reasons are logged at `debug` level.
- Secrets: when enabled, plaintext secret changes trigger sidecar regeneration and staging.
- `install-systemd` writes `~/.config/systemd/user/hometree.service` (ExecStart=`hometree daemon run`, Restart=on-failure).

### deploy
```
hometree deploy <target> [--no-secrets] [--no-backup]
```
- Applies a commit/branch/tag to managed paths. Default: secrets processed and backups taken.
- Backups stored under `~/.local/state/hometree/backups/<timestamp>`; secrets backup obeys `secrets.backup_policy` (default encrypt).
- Guardrails: validates symlink targets stay under `$HOME`; refuses to replace directories with files/symlinks and vice versa; preserves existing owner/group/mtime best-effort.
- `--no-secrets` skips secrets entirely. `--no-backup` skips backups (use only for throwaway runs).

### rollback
```
hometree rollback [--to <rev> | --steps N]
```
- Re-deploys a previous generation (default: last generation, else `HEAD~N`). `--steps` defaults to 1 and must be >=1.
- Uses the same deploy guardrails and performs backups. Errors if there are not enough recorded generations.

### plan deploy
```
hometree plan deploy <target>
```
- Dry-run of `deploy`; prints `create|update|delete <path>` without touching the filesystem.

### verify
```
hometree verify [--rev REV] [--strict] [--with-secrets skip|presence|decrypt] [--json] [--show-paths]
```
- Compares the home tree to a commit (default `HEAD`). Exits 1 on drift.
- `--strict` also reports unexpected files and exec-bit mismatches.
- Secrets modes: `presence` (default) checks plaintext + ciphertext presence, `decrypt` compares decrypted bytes, `skip` ignores secrets.
- Without `--show-paths`, secret paths are redacted (also in `--json` output).

### secret
```
hometree secret add <path>
hometree secret refresh [<path>...]
hometree secret status [--show-paths]
hometree secret rekey
```
- `add`: enables secrets, records a rule, writes ciphertext sidecar (`<path><suffix>` by default), updates ignores/excludes, stages the ciphertext. Requires plaintext to exist and age recipients to be configured.
- `refresh`: re-encrypts sidecars (optionally filtered). Errors if secrets are disabled. Stages updated ciphertexts.
- `status`: reports `in-sync`, `drift`, `missing-plaintext`, `missing-ciphertext`, or `decrypt-error` per rule; redacts paths unless `--show-paths`.
- `rekey`: re-encrypts all secrets with current recipients. Requires secrets enabled and identity files for decryption.

## Examples
```bash
# Initialize and use a temp HOME/XDG root for testing
HOMETREE_HOME_ROOT=/tmp/home HOMETREE_XDG_ROOT=/tmp/xdg hometree init
HOMETREE_HOME_ROOT=/tmp/home hometree track .config/myapp/config.toml

# Stage and commit
hometree status
hometree snapshot -m "track myapp config"

# Plan then deploy a tag
hometree plan deploy v1.2.0
hometree deploy v1.2.0

# Verify with strict + secrets decryption, showing paths
hometree verify --strict --with-secrets=decrypt --show-paths

# Enable secrets and add one (use real age keys)
cat > ~/.config/app/secret.txt <<'EOF'
super-secret
EOF
hometree secret add ~/.config/app/secret.txt
```

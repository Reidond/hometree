---
title: Safety Model
---

# Safety model

- Threat scope: prevent accidental leakage of sensitive home files into git; no protection against a rooted host. Use OS disk encryption for rest.
- Watcher is event-driven over managed roots/extra files; no full-home scans.

## Symlinks and file writes
- Deploy validates symlink targets (absolute and normalized relative) to keep them under `$HOME`; escaping paths (e.g. `/etc/passwd`, `../../..`) are rejected.
- Writing a regular file over an existing symlink removes the symlink without dereferencing the target; directories and non-regular files cause deploy to abort.
- Symlink backups are preserved as symlinks (not dereferenced).
- Secrets deploy refuses to replace a directory with a secret file and removes existing symlinks before writing the plaintext.

## Managed set, ignores, and secrets
- A path is managed only if it matches roots/extra files **and** is not ignored/denylisted. Defaults ignore browsers, SSH/GPG, keyrings, tokens, etc.
- Secret plaintext paths are auto-added to both config ignores and git excludes; status/track/watch all skip staging plaintext secrets.
- Tracking a secret path directly is blocked; use `hometree secret add` instead.
- Snapshot guard: `hometree snapshot` fails if any plaintext secret is staged.

## Auto-add allowlist guardrails
- `auto_add_new` only runs when `auto_add_allow_patterns` has entries; otherwise it logs and does nothing.
- Patterns are validated: max 50 entries; rejects overly broad values like `*`, `**`, `**/*`, `*/**`, `*.txt` (no path separator), or any absolute path.
- Auto-add triggers only for new, managed, non-ignored paths that also match the allowlist; secrets are excluded. Skips log at `debug` with reason.

Example allowlist:
```toml
[watch]
auto_add_new = true
auto_add_allow_patterns = [
  ".config/**",
  ".local/bin/*"
]
```

## Secret-specific safety
- Plaintext never enters git: ignores/excludes are written, and watcher staging ignores plaintext while re-encrypting to sidecars.
- Verify supports redacted secret output; use `--with-secrets=presence|decrypt` and `--show-paths` only when you are comfortable revealing paths.
- Deploy honors `--no-secrets` to skip decrypt/backups when testing; default deploy also backs up secrets according to `backup_policy`.

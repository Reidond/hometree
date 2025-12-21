---
title: Secrets
---

# Secrets

## Threat model
- Goal: keep sensitive data out of git history (especially remotes).
- Non-goals: full-disk encryption or protecting a fully compromised machine (malware can read `$HOME`).

## Storage model
- Secrets are opt-in. Each rule points to a plaintext path and a ciphertext sidecar (default suffix: `.age`).
- Plaintext paths are ignored automatically. When you add secrets via `hometree secret add`, hometree writes `~/.config/hometree/gitignore` and sets `core.excludesFile` so plaintext is never staged.
- Backend: age (X25519 recipients only). Other backends are rejected at config load.

### Config example
```toml
[secrets]
enabled = true
backend = "age"
sidecar_suffix = ".age"
recipients = ["age1example..."]           # required for encrypt
identity_files = ["~/.config/hometree/keys/identity.txt"]
backup_policy = "encrypt"                 # encrypt | skip | plaintext

[[secrets.rules]]
path = ".config/app/secret.txt"           # plaintext (ignored by git)
# ciphertext = ".config/app/secret.enc"   # optional override; defaults to path + suffix
mode = 0o600                               # optional; defaults to 0o600
```

## Lifecycle
- `hometree secret add <path>`: creates a rule, appends the plaintext path to ignores/excludes, encrypts to the sidecar, and stages the ciphertext.
- `hometree secret refresh [paths...]`: re-encrypts selected or all secrets and stages the updated sidecars (use after plaintext edits).
- `hometree secret rekey`: re-encrypts all secrets with current recipients (identity files must decrypt old ciphertexts).
- `hometree secret status [--show-paths]`: reports `missing-plaintext`, `missing-ciphertext`, `in-sync`, `drift`, or `decrypt-error` for each rule.
- Snapshot guard: if a plaintext secret is staged (index status not `.`, `?`, or `!`), `hometree snapshot` refuses to commit.

### Watcher integration
- The watcher is event-driven on managed roots/extra files. Plaintext secret changes are detected and re-encrypted to sidecars, which are staged automatically.
- Secret plaintext paths are never auto-added or staged by watch/status logic.

## Deploy, verify, and backups
- Deploy decrypts sidecars with the age identities and writes plaintext to the destination paths (default mode `0o600` unless overridden per rule). Use `--no-secrets` to skip decrypt/backup handling.
- Verify supports secrets: `--with-secrets=presence` checks plaintext/ciphertext existence; `--with-secrets=decrypt` also decrypts and compares contents (paths redacted unless `--show-paths`).
- Backups during deploy: each run writes to `state/backups/<timestamp>`. For each secret rule:
  - `encrypt` (default): encrypt plaintext and store ciphertext in the backup.
  - `plaintext`: copy plaintext as-is.
  - `skip`: no secret backup entry.

## Quick workflow
```bash
hometree secret add ~/.config/app/secret.txt
# edit plaintext
hometree secret refresh ~/.config/app/secret.txt
hometree secret status --show-paths
hometree verify --with-secrets=decrypt --show-paths
```

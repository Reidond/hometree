---
title: Deploy & Rollback
---

# Deploy and rollback

Deploy applies a specific commit to your managed paths in `$HOME`. Rollback re-applies a previous generation.

## Plan a deploy

Always preview first:

```bash
hometree plan deploy HEAD
```

The plan shows `create`, `update`, and `delete` actions for managed paths.

## Deploy a commit

```bash
hometree deploy <rev>
```

Options:
- `--no-backup` skips backups entirely.
- `--no-secrets` skips secrets decryption and secret backups.

## Rollback

Rollback uses the generations log (created on deploy) to roll back safely:

```bash
hometree rollback --steps 1
```

Or target a specific commit:

```bash
hometree rollback --to <rev>
```

## Backups

During deploy, hometree creates a backup of current managed files unless `--no-backup` is used.

Secrets backup behavior is controlled by `secrets.backup_policy`:
- `encrypt` (default): back up secret plaintexts as ciphertext using age.
- `skip`: no secret backups.
- `plaintext`: copy plaintext as-is to the backup directory.

## Metadata preservation
- File permissions (exec bit) are preserved from the git tree mode.
- On Unix, uid/gid and mtime are preserved best-effort for existing files.
- Symlinks are recreated as symlinks (never dereferenced).

## Safety checks
- Symlink targets are validated to stay under `$HOME`.
- Deploy refuses to replace directories with files or vice versa.
- Secrets never overwrite directories; existing symlinks are removed before writing files.

## Generations log

Each deploy appends an entry to `state_dir/generations.jsonl` with:
- timestamp
- rev
- host/user
- optional commit message and config hash

See `docs/paths.md` for the exact state directory location.

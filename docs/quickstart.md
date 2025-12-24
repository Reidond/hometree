---
title: Quick Start
---

# Quick start

This is the shortest path to a working setup.

## 1) Initialize

```bash
hometree init
```

If you haven't installed hometree yet, see `docs/install.md`.

This creates:
- A config file at `~/.config/hometree/config.toml` (XDG-aware)
- A bare repo at `~/.local/share/hometree/repo.git`

## 2) Track files

```bash
hometree track ~/.config/myapp/config.toml
```

`track` adds paths to the managed set if they are under managed roots or you pass `--allow-outside`.

## 3) Stage and snapshot

```bash
hometree snapshot -m "track config"
```

Snapshots are git commits created from staged changes.

## 4) Verify

```bash
hometree verify --strict
```

Use `--with-secrets` to control secrets verification:

```bash
hometree verify --with-secrets=presence
hometree verify --with-secrets=decrypt --show-paths
```

## 5) Deploy and rollback (optional)

Preview first:

```bash
hometree plan deploy HEAD
```

Deploy a commit:

```bash
hometree deploy HEAD
```

Rollback one generation:

```bash
hometree rollback --steps 1
```

## 6) Connect to GitHub (optional)

Add a remote and push your dotfiles:

```bash
hometree remote add origin git@github.com:YOUR_USER/dotfiles.git
hometree remote push -u origin -b main
```

Check configured remotes:

```bash
hometree remote list
```

For subsequent pushes:

```bash
hometree remote push
```

## 7) Encrypt secrets with age (optional)

hometree uses [age](https://age-encryption.org/) encryption to keep sensitive files out of git history.

Generate an age keypair:

```bash
age-keygen -o ~/.config/hometree/keys/identity.txt
```

Copy the public key from the output (starts with `age1...`) and add it to `~/.config/hometree/config.toml`:

```toml
[secrets]
enabled = true
backend = "age"
recipients = ["age1your-public-key-here"]
identity_files = ["~/.config/hometree/keys/identity.txt"]
```

Add a secret file:

```bash
hometree secret add ~/.config/app/secret.txt
hometree snapshot -m "add encrypted secret"
```

The plaintext stays local; only the `.age` ciphertext is committed.

## Common next steps
- Edit `docs/config.md` to customize managed roots or ignore patterns.
- Enable the daemon if you want event-driven staging: `docs/daemon.md`.
- See full secrets reference: `docs/secrets.md`.

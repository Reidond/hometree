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

## Common next steps
- Edit `docs/config.md` to customize managed roots or ignore patterns.
- Enable the daemon if you want event-driven staging: `docs/daemon.md`.
- Add secrets support: `docs/secrets.md`.

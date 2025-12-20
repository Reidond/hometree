# hometree

Linux-first CLI for managing a versioned subset of your home directory using a bare git repo.

## Quick start

```bash
hometree init
hometree track ~/.config/myapp/config.toml
hometree snapshot -m "track config"
hometree verify --strict
```

## Install / build

```bash
# install from this repo
cargo install --path crates/hometree-cli

# or build a release binary
cargo build -p hometree-cli --release
./target/release/hometree --help
```

## Why hometree
- Track only what you choose (managed roots + explicit extra files).
- Commit snapshots with git and deploy/rollback safely.
- Secrets are stored as age-encrypted sidecars; plaintext never goes into git.
- Optional daemon stages changes from filesystem events (no full-home scans).

## Documentation
- Install / build: `docs/install.md`
- Overview: `docs/index.md`
- Quick start: `docs/quickstart.md`
- CLI reference: `docs/cli.md`
- Config reference: `docs/config.md`
- Paths & XDG: `docs/paths.md`
- Secrets: `docs/secrets.md`
- Daemon / watcher: `docs/daemon.md`
- Deploy & rollback: `docs/deploy-rollback.md`
- Verify: `docs/verify.md`
- Safety model: `docs/safety.md`
- Troubleshooting: `docs/troubleshooting.md`
- FAQ: `docs/faq.md`

## Safety model (short)
- No full-home scans; only managed roots and extra files are considered.
- Auto-add requires an allowlist and rejects overly broad patterns.
- Symlink targets are validated to stay under `$HOME`.
- Secrets are opt-in and tracked via ciphertext sidecars only.

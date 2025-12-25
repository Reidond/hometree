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

## Safety model (short)
- No full-home scans; only managed roots and extra files are considered.
- Auto-add requires an allowlist and rejects overly broad patterns.
- Symlink targets are validated to stay under `$HOME`.
- Secrets are opt-in and tracked via ciphertext sidecars only.

## License
This project is licensed under the GNU General Public License v3.0 or later. See [LICENSE](LICENSE) for details.
# AGENTS.md

Short, practical guidance for agents working on this repo.

## Repo layout
- `crates/hometree-core/`: core logic (config, paths, managed set, git backend, deploy/rollback)
- `crates/hometree-cli/`: CLI commands, daemon (watcher), systemd integration
- Workspace root `Cargo.toml` lists both crates.

## Build & test
Prefer per-crate tests:

```bash
# Ensure rustc/cargo are on PATH in this environment
export PATH="$HOME/.cargo/bin:$PATH"

cargo test -p hometree-core
cargo test -p hometree-cli
```

Workspace-wide (optional):

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --workspace
```

Notes:
- In this environment, `rustc` may not be on PATH unless you add `~/.cargo/bin`.

## Lint/format (if needed)
No repo-local lint config is defined. If asked to lint/format, use:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

## Safety/behavior guardrails
- **No full-home scans**: status/daemon watch must remain scoped to managed roots/extra files.
- Daemon watcher is **event-driven** (notify + debounce), not a recursive scanner.
- Auto-add is allowlist-only and must still honor ignore/denylist rules.

## Useful entry points
- CLI: `crates/hometree-cli/src/main.rs`
- Deploy/rollback: `crates/hometree-core/src/deploy.rs`
- Config defaults/validation: `crates/hometree-core/src/config.rs`
- Managed set rules: `crates/hometree-core/src/managed_set.rs`

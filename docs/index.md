---
title: hometree
description: Documentation for hometree
---

# hometree docs

Linux-first CLI for managing a versioned subset of your home directory using a bare git repo.

## Start here
- Quick start: `docs/quickstart.md`
- Install / build: `docs/install.md`
- CLI reference: `docs/cli.md`
- Config reference: `docs/config.md`
- Paths & XDG layout: `docs/paths.md`
- Secrets: `docs/secrets.md`
- Daemon / watcher: `docs/daemon.md`
- Deploy & rollback: `docs/deploy-rollback.md`
- Verify: `docs/verify.md`
- Safety model: `docs/safety.md`
- Troubleshooting: `docs/troubleshooting.md`
- FAQ: `docs/faq.md`

## Core ideas
- Managed roots: you explicitly choose which parts of `$HOME` are tracked.
- Bare repo: hometree stores state in a bare git repo and deploys to your real files.
- Snapshots: `snapshot` commits staged changes into the repo.
- Deploy/Rollback: safely apply a commit to your home, or roll back to a previous generation.
- Secrets: plaintext is never stored in git; ciphertext sidecars (`.age` by default) are tracked instead.

## Safety-first defaults
- No full-home scan; only managed roots and extra files are considered.
- Auto-add is disabled unless you provide an allowlist.
- Symlink targets are validated to prevent escaping `$HOME`.

If you are new, start with the quick start and then the config reference.

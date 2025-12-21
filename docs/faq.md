---
title: FAQ
---

# FAQ

## Does hometree scan my entire home directory?

No. It only considers managed roots and extra files you specify.

## Is auto-add safe?

Auto-add is off by default and requires an allowlist. Patterns are validated to prevent broad matches.

## Does hometree encrypt my whole repo?

No. Only paths explicitly marked as secrets are encrypted and tracked as ciphertext sidecars.

## Can I use hometree without the daemon?

Yes. The daemon is optional; you can use `track`, `snapshot`, `deploy`, and `verify` manually.

## Where does hometree store its repo and state?

By default:
- Config: `~/.config/hometree/config.toml`
- Repo: `~/.local/share/hometree/repo.git`
- State: `~/.local/state/hometree/`

See `docs/paths.md` for full details.

## Is it Linux-only?

The CLI is Linux-first and the systemd integration is Linux/user-session only.
Other platforms may work but are not a primary target.

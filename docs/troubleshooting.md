---
title: Troubleshooting
---

# Troubleshooting

## "auto_add_allow_patterns contains overly broad pattern"

Your watch allowlist is too broad. Patterns must be scoped to paths with a `/`.

Examples of allowed patterns:

```toml
[watch]
auto_add_allow_patterns = [".config/**", ".local/bin/*"]
```

Examples that are rejected:
- `*`, `**`, `**/*`
- `*.txt` (no path separator)
- `/etc/passwd` (absolute path)

## "unsupported secrets backend"

Only the `age` backend is supported. Set:

```toml
[secrets]
backend = "age"
```

## "secrets.sidecar_suffix cannot be empty"

If secrets are enabled, you must have a non-empty suffix (default `.age`).

## Secrets decrypt errors

Common causes:
- Missing identity file
- Wrong recipients
- Ciphertext is corrupt

Check your config:

```toml
[secrets]
recipients = ["age1..."]
identity_files = ["~/.config/hometree/keys/identity.txt"]
```

## verify reports unexpected files (strict mode)

Strict mode flags files under managed roots that are not in the target tree.
Either remove them, ignore them, or re-run without `--strict`.

## Daemon status not reachable

If `hometree daemon status` cannot reach the socket:
- Confirm `watch.enabled = true`
- Start the daemon: `hometree daemon run --foreground`
- Or use systemd: `hometree daemon install-systemd` then `hometree daemon start`

## Deploy refuses to overwrite directories

Deploy will not replace a directory with a file or vice versa. Remove or move the conflicting path and retry.

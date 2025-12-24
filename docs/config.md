---
title: Configuration
---

# Configuration

This file describes the TOML configuration used by hometree. Defaults come from `crates/hometree-core/src/config.rs` and paths from `crates/hometree-core/src/paths.rs`.

## Location

- Default config file: `$XDG_CONFIG_HOME/hometree/config.toml` (falls back to `~/.config/hometree/config.toml`).
- With `--home-root` and `--xdg-root`, the config moves to `<xdg_root>/config/hometree/config.toml`.
- A fresh config can be written by the CLI (e.g. `hometree init`) using these defaults.

## Example

```toml
[repo]
git_dir = "/home/user/.local/share/hometree/repo.git"
work_tree = "/home/user"

[manage]
roots = [".config/", ".local/bin/"]
extra_files = []

[ignore]
patterns = [".ssh/**", "**/*secret*"]

[watch]
enabled = false
debounce_ms = 500
auto_stage_tracked_only = true
auto_add_new = false
auto_add_allow_patterns = []

[snapshot]
auto_message_template = "snapshot: auto"

[secrets]
enabled = true
backend = "age"
sidecar_suffix = ".age"
recipients = ["age1example..."]
identity_files = ["~/.config/hometree/keys/identity.txt"]
backup_policy = "encrypt" # encrypt | skip | plaintext

[[secrets.rules]]
path = ".config/app/secret.txt"
mode = 0o600
```

## Sections

### [repo]

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `git_dir` | path | `$XDG_DATA_HOME/hometree/repo.git` (bare) | Location of the hometree git repo. |
| `work_tree` | path | `$HOME` | Work tree that hometree manages. |

### [manage]

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `roots` | array of relative paths | `[".config/", ".local/bin/", ".local/share/systemd/user/", ".local/share/applications/"]` | Managed directories (relative to `work_tree`). Trailing `/` is allowed. |
| `extra_files` | array of relative paths | `[]` | Individual files to manage outside the roots list. |
| `allow_outside` | bool | `true` | Allow tracking files outside managed roots without `--allow-outside` flag. |

### [ignore]

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `patterns` | array of glob patterns | see list below | Patterns are relative to `work_tree`. Secret rules are auto-added here when secrets are enabled. |

Default ignore patterns:

```
.ssh/**
.gnupg/**
.local/share/keyrings/**
.local/share/kwalletd/**
.pki/**
.mozilla/**
.config/google-chrome/**
.config/chromium/**
.config/BraveSoftware/**
**/*token*
**/*secret*
```

### [watch]

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | Enables the foreground watcher. |
| `debounce_ms` | integer (ms) | `500` | Debounce window for filesystem events. |
| `auto_stage_tracked_only` | bool | `true` | If true, watcher stages only paths already tracked. |
| `auto_add_new` | bool | `false` | If true, watcher may add new files under managed roots/extra_files when allowlist matches. |
| `auto_add_allow_patterns` | array of glob patterns | `[]` | Allowlist for auto-add; ignored when `auto_add_new` is false. |

Auto-add validation rules:
- Maximum 50 non-empty entries.
- Empty/whitespace-only entries are ignored.
- Rejected as overly broad: `*`, `**`, `**/*`, `*/**`, `.**`, `.*/**`.
- Patterns without `/` are rejected unless they start with `.` (e.g. `.gitignore` is allowed).
- Absolute paths are rejected; patterns must be relative to `work_tree`.

### [snapshot]

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `auto_message_template` | string or `null` | `null` | Used by `hometree snapshot --auto`; required when `--auto` is used. |

### [secrets]

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | Turn secrets support on/off. Validation only runs when enabled. |
| `backend` | string | `"age"` | Only `"age"` is supported. |
| `sidecar_suffix` | string | `".age"` (when enabled) | Must be non-empty; defaults to `.age` if left blank. |
| `recipients` | array of strings | `[]` | Age recipient keys. |
| `identity_files` | array of paths | `[]` | Age identity files to decrypt. |
| `rules` | array of tables | `[]` | See `secrets.rules` below. |
| `backup_policy` | enum | `encrypt` | Allowed values: `encrypt`, `skip`, `plaintext`. |

`[[secrets.rules]]` entries:

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `path` | string (relative) | required | Path of the plaintext secret. Added to `[ignore.patterns]` automatically when secrets are enabled. |
| `ciphertext` | string or `null` | `null` | Optional ciphertext path; defaults to `path + sidecar_suffix` when omitted by CLI operations. |
| `mode` | integer or `null` | `null` | Optional file mode (e.g. `0o600`). |

Secrets validation rules:
- Runs only when `secrets.enabled` is true.
- `backend` must be `age`.
- `sidecar_suffix` must not be empty (auto-filled with `.age` if blank).
- Secret paths are appended to `[ignore.patterns]` to keep plaintext out of git.

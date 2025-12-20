# Paths and XDG mapping

Path resolution lives in `crates/hometree-core/src/paths.rs`. All paths are derived from XDG locations unless overrides are provided.

## Defaults (XDG-aware)

| Helper | Default location | Notes |
| --- | --- | --- |
| `home_dir()` | `$HOME` | Base for relative paths in the config. |
| `config_dir()` | `$XDG_CONFIG_HOME/hometree` (fallback `~/.config/hometree`) | Directory that stores `config.toml`. |
| `data_dir()` | `$XDG_DATA_HOME/hometree` (fallback `~/.local/share/hometree`) | Holds the bare git repo. |
| `state_dir()` | `$XDG_STATE_HOME/hometree` (fallback to `$XDG_DATA_HOME/hometree`) | Stores deploy/snapshot metadata. |
| `cache_dir()` | `$XDG_CACHE_HOME/hometree` (fallback `~/.cache/hometree`) | Scratch/cache space. |
| `config_home_dir()` | parent of `config_dir()` | Equivalent to `$XDG_CONFIG_HOME`. |
| `runtime_dir()` | `$HOMETREE_RUNTIME_DIR/hometree` or `$XDG_RUNTIME_DIR/hometree` | IPC socket and runtime files for the daemon. |

Derived files:

| Helper | Path | Notes |
| --- | --- | --- |
| `config_file()` | `config_dir()/config.toml` | Main configuration file. |
| `repo_dir()` | `data_dir()/repo.git` | Bare git repository used by hometree. |

## Overrides

- `--home-root <path>` sets `home_dir` to `<path>` and, if no `--xdg-root` is provided, maps XDG dirs under that home (e.g. `<home>/.config/hometree`).
- `--xdg-root <path>` forces XDG mapping to `<path>/config|data|state|cache/` with the `hometree` suffix, regardless of `home_root`.
- `HOMETREE_RUNTIME_DIR` overrides the runtime dir; otherwise `XDG_RUNTIME_DIR` is used.
- If neither flag is provided, standard XDG environment variables are honored via `directories::BaseDirs`.

### Override examples

- Home override only: `--home-root /tmp/home` -> config at `/tmp/home/.config/hometree/config.toml`, repo at `/tmp/home/.local/share/hometree/repo.git`.
- Home + XDG override: `--home-root /tmp/home --xdg-root /tmp/xdg` -> config at `/tmp/xdg/config/hometree/config.toml`, repo at `/tmp/xdg/data/hometree/repo.git`.

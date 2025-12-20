# Install / build

hometree is a Rust CLI. The project is Linux-first and uses systemd for optional daemon integration.

## Requirements
- Rust toolchain (stable)
- Git

If `rustc` or `cargo` is not on your PATH, install Rust via rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Install with cargo

If you are installing from a git checkout, run from the repo root:

```bash
cargo install --path crates/hometree-cli
```

This installs the `hometree` binary into `~/.cargo/bin`.

If `hometree` is not found after install, add Cargo's bin directory to PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Build from source

```bash
cargo build -p hometree-cli --release
```

The binary is built at:

```
target/release/hometree
```

## Run from source (without installing)

```bash
cargo run -p hometree-cli -- --help
cargo run -p hometree-cli -- init
```

## Verify installation

```bash
hometree --help
hometree init
```

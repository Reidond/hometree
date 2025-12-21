---
title: Verify
---

# Verify

`verify` compares the current home tree against a target commit.

## Basic usage

```bash
hometree verify
hometree verify --rev HEAD
```

## Strict mode

Strict mode also checks permissions and unexpected files under managed roots:

```bash
hometree verify --strict
```

## Secrets verification

Choose the secrets mode:
- `skip`: ignore secrets.
- `presence`: require plaintext on disk and ciphertext in the repo (default).
- `decrypt`: decrypt ciphertext from the repo and compare to plaintext.

```bash
hometree verify --with-secrets=presence
hometree verify --with-secrets=decrypt --show-paths
```

By default, plaintext secret paths are redacted. Use `--show-paths` to display them.

## JSON output

```bash
hometree verify --json
```

`verify` exits non-zero if the report is not clean.

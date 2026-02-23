+++
title = "Building from Source"
weight = 1
description = "How to build and test the Rust micasa workspace."
linkTitle = "Building"
+++

## Prerequisites

- **Rust stable toolchain** (`rustc`, `cargo`)
- **Nix** (optional but recommended for pinned tooling)
- **Go** (optional; only needed for temporary Go parity checks/tooling)

## Quick build

```sh
git clone https://github.com/cpcloud/micasa.git
cd micasa
cargo build --release --package micasa-cli
./target/release/micasa --check
```

## Local development commands

```sh
# Run the TUI
cargo run --package micasa-cli

# Format
cargo fmt --all

# Lint (warnings are treated as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Test suite
cargo test --workspace
```

## Dependency review

Third-party crate review notes are tracked in
[Dependency Audit]({{< ref "/docs/development/dependency-audit" >}}).

## Nix dev shell

The recommended reproducible environment uses Nix flakes:

```sh
nix develop
```

The shell includes Rust tooling, Go parity tooling, docs tooling, and repo
checks pinned to known versions.

## Nix builds

Build the Rust binary directly from the flake package:

```sh
nix build '.#micasa'
./result/bin/micasa --check
```

A temporary Go parity package remains available during migration:

```sh
nix build '.#micasa-go-parity'
```

## Nix flake apps

| Command | Description |
|---------|-------------|
| `nix run` | Run micasa directly |
| `nix run '.#website'` | Serve docs website with live reload |
| `nix run '.#docs'` | Build Hugo site into `website/` |
| `nix run '.#record-demo'` | Record the main demo GIF |
| `nix run '.#record-tape'` | Record a single VHS tape to WebP |
| `nix run '.#record-animated'` | Record all `using-*.tape` animated demos in parallel |
| `nix run '.#capture-one'` | Capture a single VHS tape as a WebP screenshot |
| `nix run '.#capture-screenshots'` | Capture screenshot tapes in parallel |
| `nix run '.#pre-commit'` | Run repo pre-commit hooks |
| `nix run '.#deadcode'` | Run Go dead-code analysis helper |
| `nix run '.#osv-scanner'` | Scan dependencies for known vulnerabilities |

## Container image

Build the OCI image via Nix:

```sh
nix build '.#micasa-container'
docker load < result
docker run -it --rm micasa:latest --help
```

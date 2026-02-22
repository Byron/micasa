+++
title = "Installation"
weight = 1
description = "How to install the Rust micasa binary."
linkTitle = "Installation"
+++

micasa is distributed as a single terminal binary.

## Pre-built binaries

Download from the
[latest release](https://github.com/cpcloud/micasa/releases/latest).

Current release artifacts are published for:

| OS      | Architecture |
|---------|--------------|
| Linux   | amd64        |
| macOS   | amd64        |
| Windows | amd64        |

Each release also includes `checksums.txt`.

## Nix

If you use [Nix](https://nixos.org) with flakes:

```sh
# Run directly
nix run github:cpcloud/micasa

# Or add to your own flake
{
  inputs.micasa.url = "github:cpcloud/micasa";
}
```

## Build from source (Rust)

```sh
git clone https://github.com/cpcloud/micasa.git
cd micasa
cargo build --release --package micasa-cli
./target/release/micasa --help
```

## Container

A container image is published to GitHub Container Registry:

```sh
docker pull ghcr.io/cpcloud/micasa:latest
docker run -it --rm ghcr.io/cpcloud/micasa:latest --help
```

`-it` is required because micasa is a terminal UI.

## Verify installation

```sh
micasa --help
micasa --check
```

+++
title = "Configuration v2"
weight = 3
description = "Rust config schema (`version = 2`) and migration steps."
linkTitle = "Configuration v2"
+++

Rust micasa uses a versioned config file with this top-level shape:

```toml
version = 2

[storage]
# Optional. Defaults to platform data dir.
# db_path = "/absolute/path/to/micasa.db"
max_document_size = 52428800
cache_ttl_days = 30

[ui]
show_dashboard = true

[llm]
enabled = true
base_url = "http://localhost:11434/v1"
model = "qwen3"
extra_context = ""
timeout = "5s"
```

## Config file path

By default:

- Linux: `$XDG_CONFIG_HOME/micasa/config.toml` (fallback `~/.config/micasa/config.toml`)
- macOS: `~/Library/Application Support/micasa/config.toml`
- Windows: `%APPDATA%\micasa\config.toml`

Override with `MICASA_CONFIG_PATH`.

## CLI helpers

```sh
micasa --print-config-path
micasa --print-example-config
micasa --check
```

## Duration format

`llm.timeout` accepts:

- `<N>ms` (example: `500ms`)
- `<N>s` (example: `5s`)
- `<N>m` (example: `2m`)

## Migration from legacy config

Legacy unversioned Go config is intentionally not auto-loaded.

1. Print a template with `micasa --print-example-config`.
2. Copy values from the old file into `[storage]`, `[ui]`, and `[llm]`.
3. Set `version = 2`.
4. Run `micasa --check`.

If migration is incomplete, startup prints an actionable error with the exact
missing step.

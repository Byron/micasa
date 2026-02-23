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

## Database path precedence

When micasa chooses a DB path:

1. `[storage].db_path` in config
2. `MICASA_DB_PATH` environment variable
3. Platform default data directory path

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

1. Find your active config path:

   ```sh
   micasa --print-config-path
   ```

2. Print a fresh v2 template and save it:

   ```sh
   micasa --print-example-config > config.toml
   ```

3. Copy values from your old file into v2 sections:

   | Legacy key location | v2 location |
   |---------------------|-------------|
   | top-level DB path / storage key | `[storage].db_path` |
   | top-level dashboard toggle | `[ui].show_dashboard` |
   | top-level or mixed LLM keys | `[llm].enabled`, `[llm].base_url`, `[llm].model`, `[llm].extra_context`, `[llm].timeout` |

4. Ensure `version = 2` is present at file top-level.
5. Validate before starting the TUI:

   ```sh
   micasa --check
   ```

If migration is incomplete, startup prints an actionable error with the exact
missing step.

Legacy Go config keys are intentionally not auto-consumed.

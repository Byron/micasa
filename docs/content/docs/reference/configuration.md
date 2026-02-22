+++
title = "Configuration"
weight = 2
description = "CLI flags, config file paths, environment variables, and LLM setup (Rust)."
linkTitle = "Configuration"
+++

micasa uses a versioned Rust config (`version = 2`) and a small CLI surface.

For the full schema and migration walkthrough, see
[Configuration v2]({{< ref "/docs/reference/configuration-v2" >}}).

## CLI flags

```
micasa [flags]

Flags:
  --config <path>          Use a specific config path
  --print-config-path      Print resolved config path
  --print-example-config   Print a v2 config template
  --check                  Validate config + DB + startup dependencies
  -h, --help               Show help
```

### Typical workflows

```sh
# Show where micasa will read/write config
micasa --print-config-path

# Generate a config template
micasa --print-example-config

# Validate configuration and startup dependencies without launching the TUI
micasa --check
```

## Environment variables

### `MICASA_CONFIG_PATH`

Overrides the config file location.

```sh
MICASA_CONFIG_PATH=/tmp/micasa.toml micasa --check
```

### `MICASA_DB_PATH`

Overrides the default data-directory database path when `[storage].db_path` is
not set in config.

```sh
MICASA_DB_PATH=/tmp/micasa.db micasa --check
```

## Config path and DB path resolution

### Config file path

By default:

- Linux: `$XDG_CONFIG_HOME/micasa/config.toml` (fallback `~/.config/micasa/config.toml`)
- macOS: `~/Library/Application Support/micasa/config.toml`
- Windows: `%APPDATA%\micasa\config.toml`

Order of precedence:

1. `--config <path>`
2. `MICASA_CONFIG_PATH`
3. Platform default config path

### Database path

Order of precedence:

1. `[storage].db_path` in config
2. `MICASA_DB_PATH`
3. Platform default data path

Default data paths:

- Linux: `$XDG_DATA_HOME/micasa/micasa.db` (fallback `~/.local/share/micasa/micasa.db`)
- macOS: `~/Library/Application Support/micasa/micasa.db`
- Windows: `%LOCALAPPDATA%\micasa\micasa.db`

## LLM configuration

LLM settings live under `[llm]` in `config.toml`.

- `enabled`
- `base_url`
- `model`
- `extra_context`
- `timeout`

micasa uses an OpenAI-compatible chat API with SSE streaming. Ollama is the
primary tested backend; LM Studio and llama.cpp server are compatible when
their OpenAI-style endpoints are enabled.

## Persistent preferences

Some preferences are stored in SQLite (not in `config.toml`) and persist across
restarts:

- Dashboard startup visibility
- Last selected LLM model

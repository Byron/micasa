+++
title = "Architecture"
weight = 2
description = "Rust workspace architecture and runtime flow."
linkTitle = "Architecture"
+++

micasa runs as a Rust workspace with a synchronous TUI runtime.

## Workspace layout

```
crates/
  micasa-cli/        CLI entrypoint (`micasa`)
  micasa-app/        Typed domain model + app state machine
  micasa-db/         rusqlite store, schema bootstrap, lifecycle guards
  micasa-tui/        ratatui rendering + key/event mapping
  micasa-llm/        OpenAI-compatible sync client + prompt helpers
  micasa-testkit/    fixtures and deterministic test helpers
```

## Runtime flow

1. `micasa-cli` loads config v2.
2. `micasa-db` opens/bootstraps SQLite and validates schema compatibility.
3. `micasa-cli` builds a runtime adapter (`DbRuntime`).
4. `micasa-tui` runs a blocking event loop (no async runtime).
5. App updates are handled through typed commands/events from `micasa-app`.

## Storage and integrity model

- SQLite (`micasa.db`) remains the source of truth.
- `cp micasa.db backup.db` is still a complete backup.
- Soft-delete/restore and FK guards are enforced in typed store APIs.
- Document data is stored as BLOBs in SQLite; filesystem cache is disposable.

## TUI model

- Primary modes: `nav`, `edit`, `form`.
- Overlays: dashboard, help, chat, date picker, column finder, note preview.
- Key handling and status feedback are synchronous and typed (no stringly
  internal dispatch).

## LLM pipeline

LLM is optional. When enabled, the runtime uses a synchronous two-stage flow:

1. NL question -> SQL generation
2. SQL execution -> answer summarization

If SQL generation/execution fails, fallback summarization from data snapshot is
used. Streaming output is handled via blocking SSE reads and thread message
passing.

Go runtime/parity sources were removed during the Rust cutover. The Rust
workspace is the only runtime and release surface.

<!-- Copyright 2026 Phillip Cloud -->
<!-- Licensed under the Apache License, Version 2.0 -->

# Rust Port Plan (`micasa`)

## Tracking

- Request date: 2026-02-19
- Tracking issue: [#2 Rust parity slice: tab CRUD and form flows](https://github.com/Byron/micasa/issues/2)
- Step 10 closure issue: [#36 docs migration and Go deprecation cleanup](https://github.com/Byron/micasa/issues/36)
- Migration owner: Codex session

## Goals

- Replace the Go runtime with a Rust workspace.
- Use `ratatui` for terminal rendering and event-driven UX.
- Use `rusqlite` for all database interactions.
- Use `anyhow` for application-level error handling.
- Keep database compatibility with existing `micasa.db` files.
- Port existing tests and add parity tests.
- Keep architecture synchronous (no async runtime).

## Non-goals

- Keep TOML config backward-compatible with current Go config schema.
- Keep Go as a permanently supported runtime.

## Architecture map

- `crates/micasa-cli`: executable entrypoint (`micasa`).
- `crates/micasa-app`: typed domain, state machine, commands/events.
- `crates/micasa-db`: `rusqlite` schema bootstrap, repositories, queries, guards.
- `crates/micasa-tui`: `ratatui` rendering and input mapping.
- `crates/micasa-llm`: OpenAI-compatible sync client, SQL prompt/query pipeline.
- `crates/micasa-testkit`: shared fixtures and scripted interaction helpers.

## Feature parity checklist (docs + code)

### Navigation and modes

- [x] Nav mode tables and cursor behavior
- [x] Edit mode table edits
- [x] Form mode create/edit flows
- [x] Status bar messaging for toggles
- [x] Keybinding parity from `docs/content/docs/reference/keybindings.md`

### Data surfaces

- [x] Dashboard
- [x] House profile
- [x] Projects
- [x] Quotes
- [x] Maintenance items
- [x] Maintenance logs
- [x] Incidents
- [x] Appliances
- [x] Vendors
- [x] Documents

### LLM chat

- [x] `@` chat overlay and session context
- [x] Slash commands
- [x] Model list/pull behavior
- [x] NL -> SQL -> summary pipeline
- [x] Streaming and cancellation
- [x] Actionable error messages

### Persistence and integrity

- [x] Soft-delete and restore lifecycle guards
- [x] Foreign-key guards and nullable links
- [x] Deterministic ordering with tiebreakers
- [x] Document BLOB storage
- [x] Document cache extraction/opening
- [x] Seed data defaults

### Tooling and packaging

- [x] Rust workspace and crate structure
- [x] `flake.nix` builds Rust binary as `micasa`
- [x] CI switched to Rust verification jobs (with temporary Go-vs-Rust parity job)
- [x] Release workflow switched to Rust artifact builds
- [x] Docs updated for Rust architecture and config v2 migration

## Test strategy

- Port all existing Go tests to Rust equivalents.
- Add parity tests for:
  - schema compatibility with existing DB fixtures
  - keybinding-driven mode transitions
  - LLM streaming cancellation and partial chunk handling
  - document cache lifecycle and BLOB round-trip
  - dashboard ordering determinism under ties

## Milestones

1. [x] Rust workspace bootstrap with compiling crates and typed domain skeleton.
2. [x] `rusqlite` schema/bootstrap and repository coverage.
3. [x] `ratatui` state machine + table navigation baseline.
4. [x] Full feature surface port and LLM parity.
5. [x] Test parity closure and CI/release cutover.
6. [x] Docs migration and Go deprecation cleanup.

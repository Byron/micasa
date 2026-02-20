<!-- Copyright 2026 Phillip Cloud -->
<!-- Licensed under the Apache License, Version 2.0 -->

# Rust Port Plan (`micasa`)

## Tracking

- Request date: 2026-02-19
- Tracking issue: blocked (`gh issue create` fails because issues are disabled for `Byron/micasa`)
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

- [ ] Nav mode tables and cursor behavior
- [ ] Edit mode table edits
- [ ] Form mode create/edit flows
- [ ] Status bar messaging for toggles
- [ ] Keybinding parity from `docs/content/docs/reference/keybindings.md`

### Data surfaces

- [ ] Dashboard
- [ ] House profile
- [ ] Projects
- [ ] Quotes
- [ ] Maintenance items
- [ ] Maintenance logs
- [ ] Incidents
- [ ] Appliances
- [ ] Vendors
- [ ] Documents

### LLM chat

- [ ] `@` chat overlay and session context
- [ ] Slash commands
- [ ] Model list/pull behavior
- [ ] NL -> SQL -> summary pipeline
- [ ] Streaming and cancellation
- [ ] Actionable error messages

### Persistence and integrity

- [ ] Soft-delete and restore lifecycle guards
- [ ] Foreign-key guards and nullable links
- [ ] Deterministic ordering with tiebreakers
- [ ] Document BLOB storage
- [ ] Document cache extraction/opening
- [ ] Seed data defaults

### Tooling and packaging

- [ ] Rust workspace and crate structure
- [ ] `flake.nix` builds Rust binary as `micasa`
- [ ] CI switched to Rust verification jobs
- [ ] Release workflow switched to Rust artifact builds
- [ ] Docs updated for Rust architecture and config v2 migration

## Test strategy

- Port all existing Go tests to Rust equivalents.
- Add parity tests for:
  - schema compatibility with existing DB fixtures
  - keybinding-driven mode transitions
  - LLM streaming cancellation and partial chunk handling
  - document cache lifecycle and BLOB round-trip
  - dashboard ordering determinism under ties

## Milestones

1. Rust workspace bootstrap with compiling crates and typed domain skeleton.
2. `rusqlite` schema/bootstrap and repository coverage.
3. `ratatui` state machine + table navigation baseline.
4. Full feature surface port and LLM parity.
5. Test parity closure and CI/release cutover.
6. Docs migration and Go deprecation cleanup.

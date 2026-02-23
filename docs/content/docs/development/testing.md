+++
title = "Testing"
weight = 4
description = "How to run and write tests in the Rust workspace."
linkTitle = "Testing"
+++

## Running tests

Run workspace tests from the repo root:

```sh
cargo test --workspace
```

Run full local verification before opening a PR:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Test philosophy

- **Pipeline-first behavior tests**: test the same path users hit in runtime
  and TUI (state transitions, key scripts, rendered surfaces), not detached
  helper internals.
- **SQLite-backed correctness**: data-layer tests use in-memory or temp-file
  SQLite stores through `micasa-db`, including lifecycle guards and ordering.
- **Determinism over luck**: assert stable ordering and edge-case behavior
  (tie-breakers, soft-delete visibility, typed filters, and restore guards).
- **Actionable failures**: validate user-visible errors include likely cause and
  remediation.

## Writing tests

When adding a new feature:

1. Add `micasa-db` tests when schema/store/query behavior changes.
2. Add `micasa-app` tests for typed state transitions and command handling.
3. Add `micasa-tui` scripted keybinding/snapshot tests for UX behavior changes.
4. Add `micasa-llm` tests for model listing, pull flow, streaming, and cancel.
5. Use `micasa-testkit` fixtures/builders when they cover your scenario.
6. Test through public APIs and runtime adapters, not private fields.

## CI

CI runs on every push to `main` and on pull requests.

- Rust gate: `fmt`, `clippy -D warnings`, build, and workspace tests across
  Linux/macOS/Windows.
- Temporary parity gate: Go-vs-Rust comparison job during migration cleanup.

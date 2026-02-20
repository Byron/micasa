<!-- Copyright 2026 Phillip Cloud -->
<!-- Licensed under the Apache License, Version 2.0 -->

# Rust Test Parity Matrix

## Tracking

- Primary migration issue: [#2](https://github.com/Byron/micasa/issues/2)
- Step 8 parity execution: [#5](https://github.com/Byron/micasa/issues/5)
- Strict matrix + remaining ports pass: [#6](https://github.com/Byron/micasa/issues/6)
- Snapshot date: 2026-02-20

## Totals

- Go tests discovered (`cmd/` + `internal/`): 870 test/benchmark functions across 50 files
- Rust tests currently (`crates/`): 145 tests
- Coverage posture: Partial; major gaps remain in high-count Go `internal/app` and `internal/data` suites.

## Status Keys

- `ported`: direct behavior parity covered by Rust tests
- `partial`: some equivalent behavior covered; additional ports still required
- `planned`: identified but not yet ported
- `n/a`: Go-specific implementation surface removed by Rust architecture

## File Matrix

| Go Test File | Go Tests | Rust Target(s) | Status | Notes |
|---|---:|---|---|---|
| `cmd/micasa/main_test.go` | 8 | `crates/micasa-cli/src/main.rs` | partial | Argument parsing test coverage added; additional CLI integration parity pending. |
| `internal/app/bench_test.go` | 16 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/calendar_test.go` | 22 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/chat_test.go` | 12 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/column_finder_test.go` | 27 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/compact_test.go` | 8 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/dashboard_load_test.go` | 9 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/dashboard_rows_test.go` | 6 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/dashboard_test.go` | 33 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/demo_data_test.go` | 3 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/detail_test.go` | 57 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/filter_test.go` | 39 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/form_save_test.go` | 18 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/form_select_test.go` | 6 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/form_validators_test.go` | 34 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/handler_crud_test.go` | 25 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/handlers_test.go` | 4 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/inline_edit_dispatch_test.go` | 5 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/inline_input_test.go` | 9 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/lazy_reload_test.go` | 7 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/lighter_forms_test.go` | 8 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/mag_test.go` | 14 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/mode_test.go` | 31 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/notes_test.go` | 7 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/overlay_status_test.go` | 6 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/rows_test.go` | 24 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/sort_test.go` | 16 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/testmain_test.go` | 1 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/undo_test.go` | 15 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/vendor_test.go` | 13 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/view_test.go` | 72 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/config/config_test.go` | 19 | `crates/micasa-cli/src/config.rs` | partial | Config v2 semantics intentionally differ; migration/error behavior partially covered. |
| `internal/data/bench_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/dashboard_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/query_test.go` | 17 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/seed_demo_test.go` | 4 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/seed_scaled_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/settings_integration_test.go` | 3 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/settings_test.go` | 10 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/sqlite/ddlmod_test.go` | 9 | `crates/micasa-db/src/lib.rs` | n/a | Go GORM sqlite dialector internals removed in Rust; behavior covered via rusqlite integration tests. |
| `internal/data/sqlite/sqlite_test.go` | 11 | `crates/micasa-db/src/lib.rs` | n/a | Go GORM sqlite dialector internals removed in Rust; behavior covered via rusqlite integration tests. |
| `internal/data/store_test.go` | 90 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/testmain_test.go` | 1 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/validate_path_test.go` | 4 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/validation_test.go` | 36 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/vendor_upsert_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/fake/fake_test.go` | 16 | (none yet) | planned | Rust testkit lacks seeded fake-data generator parity so far. |
| `internal/llm/client_test.go` | 18 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | partial | Streaming, prompt, and error-path parity improved; sqlfmt-equivalent surface still thin. |
| `internal/llm/prompt_test.go` | 29 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | partial | Streaming, prompt, and error-path parity improved; sqlfmt-equivalent surface still thin. |
| `internal/llm/sqlfmt_test.go` | 20 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | partial | Streaming, prompt, and error-path parity improved; sqlfmt-equivalent surface still thin. |

## Module Port Order

1. `cmd` and `config`: fast parity wins around argument/config validation and actionable errors.
2. `llm`: client error-paths, completion semantics, stream chunk/cancel behaviors.
3. `data`: long-tail query/document/seed lifecycle assertions.
4. `app`/TUI: high-count renderer and interaction regression suites.
5. `fake`: deterministic fixture generator parity (new Rust implementation required).

## Step 8 Additions In This Pass

- Added strict matrix and issue tracking for parity closure.
- Added new `cmd` parser tests in `crates/micasa-cli/src/main.rs`.
- Expanded config tests in `crates/micasa-cli/src/config.rs`.
- Expanded LLM client tests in `crates/micasa-llm/tests/client_tests.rs`.
- Expanded document/data regression tests in `crates/micasa-db/tests/store_tests.rs`.

## Known Gaps

- Go `internal/app` renderer-heavy suites significantly outnumber current Rust TUI tests.
- Go `internal/data/validation_test.go` parser/format helpers do not yet have a direct Rust utility-test equivalent.
- Go `internal/fake/fake_test.go` has no Rust parity crate implementation yet.

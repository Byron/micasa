<!-- Copyright 2026 Phillip Cloud -->
<!-- Licensed under the Apache License, Version 2.0 -->

# Rust Test Parity Matrix

## Tracking

- Primary migration issue: [#2](https://github.com/Byron/micasa/issues/2)
- Step 8 parity execution: [#5](https://github.com/Byron/micasa/issues/5)
- Strict matrix + remaining ports pass: [#6](https://github.com/Byron/micasa/issues/6)
- Snapshot date: 2026-02-22

## Totals

- Go tests discovered (`cmd/` + `internal/`): 870 test/benchmark functions across 50 files
- Rust tests currently (`crates/`): 342 tests
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
| `internal/data/query_test.go` | 17 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Full read-only query safety, identifier validation, and data-dump/column-hint parity suite is covered. |
| `internal/data/seed_demo_test.go` | 4 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/seed_scaled_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/settings_integration_test.go` | 3 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/settings_test.go` | 10 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/sqlite/ddlmod_test.go` | 9 | `crates/micasa-db/src/lib.rs` | n/a | Go GORM sqlite dialector internals removed in Rust; behavior covered via rusqlite integration tests. |
| `internal/data/sqlite/sqlite_test.go` | 11 | `crates/micasa-db/src/lib.rs` | n/a | Go GORM sqlite dialector internals removed in Rust; behavior covered via rusqlite integration tests. |
| `internal/data/store_test.go` | 90 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/testmain_test.go` | 1 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/validate_path_test.go` | 4 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/validation_test.go` | 36 | `crates/micasa-db/src/validation.rs` | ported | Full money/date/interval parser+formatter suite ported with overflow and month-end clamping regressions. |
| `internal/data/vendor_upsert_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/fake/fake_test.go` | 16 | `crates/micasa-testkit/src/lib.rs` | ported | Deterministic typed faker implemented and full fake suite parity tests added. |
| `internal/llm/client_test.go` | 18 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | partial | Ping/list/streaming/server-error/pull model and cancellation/disconnect semantics are covered; some client edge variants remain. |
| `internal/llm/prompt_test.go` | 29 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | partial | Prompt extraction and SQL/summary/fallback prompt coverage is strong, with additional long-tail prompt-shape parity still pending. |
| `internal/llm/sqlfmt_test.go` | 20 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | ported | Full SQL formatter/tokenizer parity suite (including subqueries, date functions, aggregate join, wrapping, tokenization) is covered. |

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
- Expanded LLM prompt construction/extraction tests in `crates/micasa-llm/src/lib.rs`.
- Expanded document/data regression tests in `crates/micasa-db/tests/store_tests.rs`.
- Added Rust unit parity tests for `is_safe_identifier`/`contains_word` in `crates/micasa-db/src/lib.rs`.
- Added additional query-surface parity tests from Go `internal/data/query_test.go` in `crates/micasa-db/tests/store_tests.rs`.
- Added deterministic fake-data generator and full parity tests from Go `internal/fake/fake_test.go` in `crates/micasa-testkit/src/lib.rs`.
- Added typed validation/value-format module and full Go parity tests from `internal/data/validation_test.go` in `crates/micasa-db/src/validation.rs`.
- Added overlay status-bar suppression parity tests from `internal/app/overlay_status_test.go` and column visibility helper parity tests from `internal/app/view_test.go` in `crates/micasa-tui/src/lib.rs`.
- Added calendar/month-end date navigation parity tests from `internal/app/calendar_test.go` in `crates/micasa-tui/src/lib.rs`.
- Added help overlay content parity tests from `internal/app/view_test.go` in `crates/micasa-tui/src/lib.rs`.
- Added filter parity tests from `internal/app/filter_test.go` for preview vs active filtering, dashboard-blocked pin/filter actions, and hide-column clearing pinned/filter state in `crates/micasa-tui/src/lib.rs`.
- Added dashboard parity tests from `internal/app/dashboard_test.go` for overlay navigation clamping, header-enter no-op, table-key blocking, tab-switch close behavior, and section ordering in `crates/micasa-tui/src/lib.rs`.
- Added view parity tests from `internal/app/view_test.go` for status width stability, header sort/link indicators, and table title sort/pin/filter/hidden flag rendering in `crates/micasa-tui/src/lib.rs`.
- Added SQL formatter/tokenizer parity surface and tests from `internal/llm/sqlfmt_test.go` in `crates/micasa-llm/src/lib.rs`.
- Updated runtime NL→SQL pipeline to keep executing raw extracted SQL while emitting formatted SQL to UI/events in `crates/micasa-cli/src/runtime.rs`.
- Added LLM client parity tests for stream server-error propagation, pull-model streaming scanner behavior, and generic JSON error sanitization in `crates/micasa-llm/tests/client_tests.rs`.
- Added runtime parity tests for model selection and persistence paths: available-model switch, Ollama auto-pull fallback, and non-Ollama missing-model actionable failure in `crates/micasa-cli/src/runtime.rs`.
- Added TUI sort parity tests from Go `internal/app/sort_test.go` for null-last ordering regardless of direction and deterministic ID tiebreaking on equal sort values in `crates/micasa-tui/src/lib.rs`, with a corresponding null-order fix in projection sorting.
- Added additional TUI sort parity tests from Go `internal/app/sort_test.go` in `crates/micasa-tui/src/lib.rs` for case-insensitive text sorting, money ascending ordering, date descending ordering, and multi-key sort ordering.
- Added DB settings/chat parity tests from Go `internal/data/settings_test.go` and `internal/data/settings_integration_test.go` in `crates/micasa-db/tests/store_tests.rs` for model/dashboard default+round-trip behavior, persistence across reopen, and non-consecutive chat history duplicates.
- Added DB parity regressions in `crates/micasa-db/tests/store_tests.rs` for project-spend stability across project edits, dashboard preference persistence across reopen, and empty chat-history defaults.
- Added DB dashboard parity tests from Go `internal/data/dashboard_test.go` in `crates/micasa-db/tests/store_tests.rs` for expiring-warranty lookback/lookahead windows, recent service-log ordering with limits, and open-incident severity ordering while excluding soft-deleted rows.
- Added DB cache-eviction parity regressions in `crates/micasa-db/tests/store_tests.rs` for nonexistent-dir handling, subdirectory skip behavior, and overflow-protected TTL validation.
- Added TUI filter-inversion parity from Go `internal/app/filter_test.go` and `internal/app/view_test.go` in `crates/micasa-tui/src/lib.rs`, including `!` key mapping, inverted preview/active filtering semantics, clear-pin reset behavior, and help/table-title inversion indicators.
- Added TUI deleted-row count parity from Go `internal/app/view_test.go` by surfacing deleted counts in table title metadata and adding a regression test in `crates/micasa-tui/src/lib.rs`.
- Expanded TUI filter parity from Go `internal/app/filter_test.go` and `internal/app/view_test.go` in `crates/micasa-tui/src/lib.rs` with null-pin inversion behavior, case-insensitive text pin matching/toggle semantics, and tab-row filter indicator markers (`▽`, `▼`, `△`, `▲`) for preview/active/inverted state.
- Added additional keybinding parity tests from Go `internal/app/filter_test.go` and `internal/app/view_test.go` in `crates/micasa-tui/src/lib.rs` for invert toggle round-trip without pins and full marker state transitions (`n`, `N`, `!`, `ctrl+n`).
- Added DB query parity tests from Go `internal/data/query_test.go` in `crates/micasa-db/tests/store_tests.rs` for invalid table-name rejection and explicit read-only query rejection of `INSERT`, `DELETE`, and multi-statement SQL with actionable errors.
- Added LLM SQL formatter parity tests from Go `internal/llm/sqlfmt_test.go` in `crates/micasa-llm/src/lib.rs` for subquery handling, nested subquery column layout, date-function formatting, already-formatted SQL stability, and aggregate-join formatting.
- Added LLM client parity test from Go `internal/llm/client_test.go` in `crates/micasa-llm/tests/client_tests.rs` for actionable server-down error handling on `list_models`.
- Added LLM client cancellation parity test from Go `internal/llm/client_test.go` in `crates/micasa-llm/tests/client_tests.rs` by verifying dropped stream readers disconnect the server-side stream promptly.
- Added DB Unicode round-trip parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for house profile fields, vendor names, and project notes/description persistence.
- Added DB deletion-record parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for project/vendor deletion record creation and restored-at clearing semantics on restore.
- Added LLM client parity test from Go `internal/llm/client_test.go` in `crates/micasa-llm/tests/client_tests.rs` for multi-model list response ordering in `list_models`.
- Added LLM client parity test from Go `internal/llm/client_test.go` in `crates/micasa-llm/tests/client_tests.rs` to verify `ping` accepts tagged model IDs (e.g. `qwen3:latest`) for base model names.
- Added DB update-flow parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for quote, appliance, and maintenance-item update persistence semantics.
- Added DB parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for soft-delete persistence across reopen, service-log update assigning vendors, and include-deleted maintenance service-log listing behavior.
- Added incident-guard parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for incident restore blocked by deleted appliances, vendor deletion blocked by active incidents, and appliance deletion blocked by active incidents.
- Added incident/document parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for incident restore without parent links and incident deletion while preserving attached document rows.
- Added document-parent lifecycle parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for appliance/vendor/quote/maintenance/service-log deletion while preserving linked document rows.
- Added additional lifecycle guard parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for project deletion blocked by active quotes (including partial quote deletion), quote restore blocked by deleted vendors, maintenance deletion blocked by active service logs, and service-log restore guard behavior with/without vendor links.
- Added typed-list filtering parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for maintenance-by-appliance (including include-deleted behavior), quote list/count by vendor/project, and service-log list/count by vendor semantics.
- Added chain and document-metadata parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for multi-entity delete/restore ordering (appliance→maintenance→service log and vendor→project→quote) and document metadata/list behavior with BLOB exclusion in list queries.
- Added document lifecycle API parity in `crates/micasa-db/src/lib.rs` and `crates/micasa-db/tests/store_tests.rs`: typed `update_document` with optional file replacement, typed `soft_delete_document`/`restore_document`, restore-parent guard checks for document targets, and regression tests for document delete/restore/update/metadata behavior.

## Known Gaps

- Go `internal/app` renderer-heavy suites significantly outnumber current Rust TUI tests.

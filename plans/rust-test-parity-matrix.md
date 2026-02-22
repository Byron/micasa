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
- Rust tests currently (`crates/`): 412 tests
- Coverage posture: Partial; major gaps remain in high-count Go `internal/app` and `internal/data` suites.

## Status Keys

- `ported`: direct behavior parity covered by Rust tests
- `partial`: some equivalent behavior covered; additional ports still required
- `planned`: identified but not yet ported
- `n/a`: Go-specific implementation surface removed by Rust architecture

## File Matrix

| Go Test File | Go Tests | Rust Target(s) | Status | Notes |
|---|---:|---|---|---|
| `cmd/micasa/main_test.go` | 8 | `crates/micasa-cli/src/main.rs`, `crates/micasa-cli/src/config.rs` | n/a | Go CLI-only surface (`--demo`, `--years`, ldflags-driven `--version`, positional DB path resolver) was intentionally replaced by documented Rust config-v2 CLI; equivalent Rust path precedence/error semantics are covered in config/main tests. |
| `internal/app/bench_test.go` | 16 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | n/a | Go file is benchmark-only (`Benchmark*`) throughput harnessing. Rust functional parity is enforced by tests; perf benchmarking is tracked separately and not a Step 8 behavior-port gate. |
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
| `internal/app/handlers_test.go` | 4 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | ported | Rust replaces Go tab handler objects with typed form dispatch (`form_for_tab`) and inline-edit routing. Parity tests now cover tab→form mapping and `e` edit dispatch behavior for supported vs unsupported tabs. |
| `internal/app/inline_edit_dispatch_test.go` | 5 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/inline_input_test.go` | 9 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/lazy_reload_test.go` | 7 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/lighter_forms_test.go` | 8 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/mag_test.go` | 14 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/mode_test.go` | 31 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/notes_test.go` | 7 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | ported | Note-preview parity is covered for enter-to-open, empty-note no-op with status, any-key dismiss/key swallowing, overlay text rendering/close hint, and contextual `enter` hint semantics on notes columns. |
| `internal/app/overlay_status_test.go` | 6 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/rows_test.go` | 24 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/sort_test.go` | 16 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/testmain_test.go` | 1 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/undo_test.go` | 15 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/vendor_test.go` | 13 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/app/view_test.go` | 72 | `crates/micasa-tui/src/lib.rs`, `crates/micasa-app/src/state.rs`, `crates/micasa-cli/src/runtime.rs` | partial | High-level keybinding/form/chat/drilldown coverage exists; many renderer/layout edge-case tests remain. |
| `internal/config/config_test.go` | 19 | `crates/micasa-cli/src/config.rs` | partial | Config v2 semantics intentionally differ; migration/error behavior plus DB-path precedence (`[storage].db_path` vs `MICASA_DB_PATH` vs default) are covered, with legacy v1/env surface intentionally excluded. |
| `internal/data/bench_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | n/a | Go file is benchmark-only (`Benchmark*`) query-throughput coverage. Rust behavior parity is gated by functional/regression tests rather than benchmark ports. |
| `internal/data/dashboard_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Dashboard helper/query parity is covered for active-project filtering, scheduled maintenance filtering, open-incident ordering, warranty windows, recent service-log limits, and spend calculations/regressions. |
| `internal/data/query_test.go` | 17 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Full read-only query safety, identifier validation, and data-dump/column-hint parity suite is covered. |
| `internal/data/seed_demo_test.go` | 4 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Deterministic/idempotent demo seeding parity is covered via typed `seed_demo_data{,_with_seed}` and full Rust regression tests. |
| `internal/data/seed_scaled_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Scaled seeding parity is covered with summary/count/FK integrity/growth/year-spread/idempotence regression tests and typed `SeedSummary` APIs. |
| `internal/data/settings_integration_test.go` | 3 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Model/show-dashboard persistence and chat-history persistence/trimming across reopen are covered in Rust parity tests. |
| `internal/data/settings_test.go` | 10 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Typed settings/chat-history parity is covered for defaults, round-trip updates, dedupe behavior, non-consecutive duplicates, and dashboard toggle semantics. |
| `internal/data/sqlite/ddlmod_test.go` | 9 | `crates/micasa-db/src/lib.rs` | n/a | Go GORM sqlite dialector internals removed in Rust; behavior covered via rusqlite integration tests. |
| `internal/data/sqlite/sqlite_test.go` | 11 | `crates/micasa-db/src/lib.rs` | n/a | Go GORM sqlite dialector internals removed in Rust; behavior covered via rusqlite integration tests. |
| `internal/data/store_test.go` | 90 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/testmain_test.go` | 1 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | partial | Core CRUD/lifecycle/query/doc-cache parity exists; substantial long-tail test ports remain. |
| `internal/data/validate_path_test.go` | 4 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | ported | Path-validation parity is covered with table-driven valid/invalid cases (URI/file/query/empty/edge forms), URL-like rejection checks, and `Store::open` URI rejection tests. |
| `internal/data/validation_test.go` | 36 | `crates/micasa-db/src/validation.rs` | ported | Full money/date/interval parser+formatter suite ported with overflow and month-end clamping regressions. |
| `internal/data/vendor_upsert_test.go` | 7 | `crates/micasa-db/tests/store_tests.rs`, `crates/micasa-db/src/lib.rs` | n/a | Go name-based vendor upsert path was removed in Rust typed-ID forms/runtime; quote/service-log flows require `VendorId`, and vendor mutation semantics are covered by typed CRUD/update tests. |
| `internal/fake/fake_test.go` | 16 | `crates/micasa-testkit/src/lib.rs` | ported | Deterministic typed faker implemented and full fake suite parity tests added. |
| `internal/llm/client_test.go` | 18 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-llm/tests/client_tests.rs`, `crates/micasa-cli/src/runtime.rs` | ported | Full client parity is covered for ping/list success+error paths, model-not-found remediation, chat complete/stream behavior, stream cancellation/disconnect, pull stream parsing, and cleaned OpenAI/Ollama/plain-text/unparsable error responses. |
| `internal/llm/prompt_test.go` | 29 | `crates/micasa-llm/src/lib.rs`, `crates/micasa-cli/src/runtime.rs` | ported | SQL/fallback/summary prompt builders, result-table formatting, SQL extraction (bare/fenced/trimmed), date/context sections, schema/relationship notes, and incident/group-by examples are all covered with direct Rust parity tests. |
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
- Added document restore-guard parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for deleted linked targets across project/appliance/vendor/quote/maintenance/service-log/incident entity kinds.
- Added additional document parity tests from Go `internal/data/store_test.go` in `crates/micasa-db/tests/store_tests.rs` for content survival across delete/restore, unlinked-document full lifecycle, entity-scoped list/count filtering (including include-deleted behavior), and deterministic multi-document ordering on `updated_at` ties via `id DESC`.
- Added more `internal/data/store_test.go` parity in `crates/micasa-db/tests/store_tests.rs` and `crates/micasa-db/src/lib.rs` for stale-cache eviction edge cases (remove-old/keep-recent/zero-ttl/empty-path), maintenance restore without appliance links, vendor-only incident restore guards, maintenance-by-appliance typed count filtering, vendor-delete unblocking after quote deletion, and document note-clearing with file metadata preserved.
- Added detail-view interaction parity from Go `internal/app/detail_test.go` in `crates/micasa-tui/src/lib.rs`: edit-mode `esc` keeps detail stack open, tab-switch commands are blocked while detail is open with actionable status, and column navigation continues to work inside detail stacks.
- Added more detail-view parity from Go `internal/app/detail_test.go` in `crates/micasa-tui/src/lib.rs`: `Tab` key is also blocked while detail is open, and following a linked FK from detail collapses the full detail stack and lands on the linked tab.
- Added additional detail/drilldown parity from Go `internal/app/detail_test.go` in `crates/micasa-tui/src/lib.rs`: multi-level breadcrumb rendering checks, maintenance→service-log and appliance→documents drill filters, and service-log performed-by link semantics (vendor follow vs self/no-link status).
- Added further detail-stack parity from Go `internal/app/detail_test.go` in `crates/micasa-tui/src/lib.rs`: selected row/cell resolution against detail projections, sort activation while in detail, and explicit close-all helper semantics for nested stacks and empty-stack no-op behavior.
- Added more column/drill semantics parity from Go `internal/app/detail_test.go` in `crates/micasa-tui/src/lib.rs`: maintenance/appliance/project projection drill columns (`log`, `maint`, `quotes`, `docs`) and service-log vendor link-target behavior depending on performed-by vendor presence.
- Added helper-level drilldown parity assertions from Go `internal/app/detail_test.go` in `crates/micasa-tui/src/lib.rs`: `pop_detail_snapshot` empty-stack return semantics and `drill_title_for` label-vs-fallback behavior for selected-row and blank-label cases.
- Added typed deterministic seeding APIs in `crates/micasa-db/src/lib.rs` (`seed_demo_data{,_with_seed}`, `seed_scaled_data{,_with_seed}`, `SeedSummary`) and ported full Go parity suites from `internal/data/seed_demo_test.go` and `internal/data/seed_scaled_test.go` in `crates/micasa-db/tests/store_tests.rs`.
- Added full path-validation parity from Go `internal/data/validate_path_test.go` in `crates/micasa-db/tests/store_tests.rs`: table-driven valid/invalid path coverage plus URL-like rejection and `Store::open` URI rejection checks.
- Expanded config-v2 parity tests in `crates/micasa-cli/src/config.rs` for DB path precedence and validation: `[storage].db_path` override, `MICASA_DB_PATH` fallback, platform default fallback, and URI-style path rejection, with test env mutation serialized to avoid cross-test races.
- Reclassified Go `cmd/micasa/main_test.go` parity status to `n/a` based on docs-backed CLI contract changes in Rust (`configuration-v2.md`): old Go-only `--demo`/`--years`/ldflags-version behavior is intentionally removed, while Rust CLI/config path behavior remains covered by tests.
- Added direct Go-equivalent prompt parity tests in `crates/micasa-llm/src/lib.rs` for DDL/date/context rendering and bare SQL extraction, and reclassified `internal/llm/client_test.go` and `internal/llm/prompt_test.go` to `ported`.
- Reclassified `internal/app/bench_test.go` and `internal/data/bench_test.go` to `n/a` because they are Go benchmark-only throughput suites, not functional behavior-parity tests.
- Added note-preview parity regressions from `internal/app/notes_test.go` in `crates/micasa-tui/src/lib.rs`: empty-note enter behavior (`no note to preview`), overlay key swallowing (dismiss before nav movement), and contextual `enter` hint rendering as `preview` on notes columns.
- Added note-preview overlay text rendering parity (`press any key to close` and note body/title content) and reclassified `internal/app/notes_test.go` to `ported`.
- Added handler-dispatch parity tests in `crates/micasa-tui/src/lib.rs` for full tab→form kind mapping and edit-key routing (`e`) across form-capable vs unsupported tabs, and reclassified `internal/app/handlers_test.go` to `ported`.

## Known Gaps

- Go `internal/app` renderer-heavy suites significantly outnumber current Rust TUI tests.
- Go `internal/data/store_test.go` still has a long-tail of lifecycle/query assertions not yet translated.
- Go `internal/config/config_test.go` remains intentionally partial because Rust `config.toml` v2 semantics replaced Go v1 compatibility behavior.

<!-- Copyright 2026 Phillip Cloud -->
<!-- Licensed under the Apache License, Version 2.0 -->

# Rust Table Rendering + State Parity (Step 2)

## Context

Issue: https://github.com/Byron/micasa/issues/4

The Rust TUI currently renders placeholder body text for non-dashboard tabs.
Step 2 replaces this with real table rendering and typed table state behavior.

## Goals

- Render non-dashboard tabs as real tables backed by typed snapshot data.
- Add synchronous table state in the TUI:
  - row/column cursor movement
  - per-tab sort state (column + direction)
  - pin/filter state and indicators
- Wire keybindings to mutate that state and emit status messages for toggles.
- Keep behavior deterministic and test-covered.

## Design

### Runtime contract

- Extend `micasa_tui::AppRuntime` with `load_tab_snapshot(tab, include_deleted)`.
- Return a typed `TabSnapshot` enum, one variant per tab surface.
- Keep `load_dashboard_counts` and `submit_form` unchanged.

### TUI model

- Add local `TableUiState` in TUI view state:
  - `selected_row`, `selected_col`
  - `sort: Option<SortSpec>`
  - `pin: Option<PinnedCell>`
  - `filter_active: bool`
- Add typed `TableCell` + projected table rows for rendering/sorting/filtering.

### Keybindings in scope

- Movement: `j/k/h/l`, arrows, `g/G`, `^/$`
- Sorting: `s` cycle, `S` clear
- Filtering: `n` pin toggle, `N` filter toggle, `ctrl+n` clear pins
- Existing keys remain supported (`tab`, `shift+tab`, `a`, `enter`, `esc`, `@`, etc.)

### Rendering

- Use `ratatui::widgets::Table` for non-dashboard tabs.
- Render selected row + selected cell styling.
- Render sort/pin/filter indicators in table title/status context.

## Tests

- TUI unit tests for movement bounds and keybinding state transitions.
- TUI unit tests for sort cycle and pin/filter toggle behavior.
- CLI runtime tests for typed snapshot loading across key tabs.

## Non-goals (this slice)

- Full Go-level detail drill stacks and all column semantics.
- Column hide/show and fuzzy column jump.
- Dashboard overlay parity.

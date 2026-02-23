<!-- Copyright 2026 Phillip Cloud -->
<!-- Licensed under the Apache License, Version 2.0 -->

+++
title = "Dependency Audit"
weight = 3
description = "Third-party Rust crate audit notes for the Rust runtime."
linkTitle = "Dependency Audit"
+++

This page tracks the security review posture for third-party Rust crates used
by the Rust workspace.

## Scope and policy

- Audit is required before introducing new third-party crates.
- Focus areas: network behavior, file writes, environment variable handling,
  unsafe code exposure, and injection risk.
- Runtime crates are preferred over convenience wrappers when risk is unclear.

## Current workspace crates

| Crate | Purpose | Risk notes | Outcome |
|------:|---------|------------|---------|
| `anyhow` | Application error propagation | No network/filesystem behavior; error wrapper only | Approved |
| `crossterm` | Terminal input/events | Terminal control surface only | Approved |
| `dirs` | Platform paths | Reads OS/user path conventions only | Approved |
| `ratatui` | TUI rendering | Rendering-only dependency | Approved |
| `rusqlite` (`bundled`) | SQLite access | DB I/O is intentional and scoped to SQLite files | Approved |
| `serde` / `serde_json` / `toml` | Serialization/parsing | Data parsing only; validate user inputs at boundaries | Approved |
| `sha2` | Checksums | Pure hashing implementation | Approved |
| `time` | Date/time types/parsing | No external side effects | Approved |
| `reqwest` (`blocking`, `rustls-tls`) | Synchronous HTTP for LLM integration | Network-capable; all user-facing failures must stay actionable | Approved |
| `url` | URL parsing/validation | Parsing only; used to harden endpoint validation | Approved |
| `tempfile` | Temp files in tests/runtime helpers | Filesystem writes are scoped to temp dirs | Approved |
| `tiny_http` (dev-dependency) | Local HTTP test server | Test-only transport harness | Approved |

## Step 9 status

- No new third-party Rust crates were introduced during Step 9.
- Existing crate set remains unchanged; this audit is a documentation
  checkpoint for the CI/release/tooling cutover.

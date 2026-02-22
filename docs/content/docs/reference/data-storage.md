+++
title = "Data Storage"
weight = 3
description = "SQLite database file, schema compatibility checks, backup, and portability."
linkTitle = "Data Storage"
+++

micasa stores application data in a single SQLite database file. This page
covers where the file lives, how schema compatibility works, and how to back it
up safely.

## Database file

By default, the database lives in your platform's data directory:

| Platform | Default path |
|----------|-------------|
| Linux    | `~/.local/share/micasa/micasa.db` |
| macOS    | `~/Library/Application Support/micasa/micasa.db` |
| Windows  | `%LOCALAPPDATA%\micasa\micasa.db` |

See [Configuration]({{< ref "/docs/reference/configuration" >}}) for path
override precedence (`[storage].db_path`, `MICASA_DB_PATH`, platform default).

The active database path is shown in the tab row so you always know which file
is open.

## Schema management

micasa uses `rusqlite` with a compatibility-first startup flow:

- New database file: create schema from `schema.sql`, then seed reference rows.
- Existing database file: validate required tables and columns, then continue
  without destructive migrations.
- Required indexes are ensured with `CREATE INDEX IF NOT EXISTS` semantics.

This keeps existing Go-era `micasa.db` files usable while preserving
deterministic query behavior.

### Tables

| Table                    | Description |
|--------------------------|-------------|
| `house_profiles`         | Single row with your home's details |
| `projects`               | Home improvement projects |
| `project_types`          | Pre-seeded project categories |
| `quotes`                 | Vendor quotes linked to projects |
| `vendors`                | Shared vendor records |
| `maintenance_items`      | Recurring maintenance tasks |
| `maintenance_categories` | Pre-seeded maintenance categories |
| `incidents`              | Household issues and repairs |
| `appliances`             | Physical equipment |
| `service_log_entries`    | Service history per maintenance item |
| `documents`              | File metadata + BLOB attachments linked to records |
| `deletion_records`       | Audit trail for soft deletes/restores |
| `settings`               | UI/runtime preferences persisted in DB |
| `chat_inputs`            | Prompt history for chat input recall |

### Pre-seeded data

On first run, micasa seeds default **project types** and
**maintenance categories** used in form select fields.

## Backup

Your database is a single file, so backups are file copies.

```sh
# Example (Linux default path)
cp ~/.local/share/micasa/micasa.db ~/backups/micasa-$(date +%F).db
```

If you configured `[storage].db_path`, copy that file path instead.

SQLite supports safe online backups, so copying while micasa is running is
acceptable for this use case.

## Restore

To restore, replace the active database file with a backup copy and relaunch
micasa.

```sh
cp ~/backups/micasa-2026-02-22.db ~/.local/share/micasa/micasa.db
```

## Soft delete

micasa uses soft delete across core entities. Deleting an item sets `deleted_at`
instead of removing the row.

- Deleted rows can be restored.
- `deletion_records` tracks delete/restore activity.
- The `x` toggle in Edit mode shows or hides deleted rows.

### Referential integrity guards

Delete and restore operations enforce FK-safe lifecycle rules:

- **Delete guards**: parent delete is blocked while active children exist.
- **Restore guards**: child restore is blocked while required parent is deleted.
- Guards cover project/quote, maintenance/service-log,
  appliance/maintenance-item, appliance/incident, and vendor-linked records.

## Documents and cache

Document files are stored as BLOBs in `documents.data` inside SQLite. The
database remains the source of truth.

When opening a document from the UI, micasa materializes a cache copy under the
platform cache directory (`.../micasa/documents`). Cache entries are disposable
and evicted by TTL (`[storage].cache_ttl_days`).

This preserves the single-file backup property: copying `micasa.db` captures all
application data, including attachments.

## Upgrades

Startup is non-destructive:

- Missing schema in a new DB is created automatically.
- Existing DBs are validated for required tables/columns.
- Missing required pieces fail fast with actionable errors.

No destructive migration runs automatically.

## Portability

The database is a standard SQLite file. You can:

- open it with `sqlite3` or DB Browser for SQLite,
- move it between machines by copying the file,
- run read-only SQL for diagnostics.

## LLM data exposure

If you enable optional [LLM chat]({{< ref "/docs/guide/llm-chat" >}}), micasa
sends selected schema/data context and query results to the configured LLM
endpoint.

Default config targets localhost (`http://localhost:11434/v1`), which keeps
traffic on your machine. If you point `base_url` at a remote host, your data is
sent over the network to that endpoint.

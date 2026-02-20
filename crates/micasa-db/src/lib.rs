// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result, anyhow, bail};
use micasa_app::{
    ChatInput, ChatInputId, DashboardCounts, Document, DocumentEntityKind, DocumentId,
    MaintenanceCategoryId, Project, ProjectId, ProjectStatus, ProjectTypeId,
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime, PrimitiveDateTime};

pub const APP_NAME: &str = "micasa";
pub const MAX_DOCUMENT_SIZE: i64 = 50 << 20;

const CHAT_HISTORY_MAX: i64 = 200;

const SETTING_LLM_MODEL: &str = "llm.model";
const SETTING_SHOW_DASHBOARD: &str = "ui.show_dashboard";

const DEFAULT_PROJECT_TYPES: [&str; 12] = [
    "Appliance",
    "Electrical",
    "Exterior",
    "Flooring",
    "HVAC",
    "Landscaping",
    "Painting",
    "Plumbing",
    "Remodel",
    "Roof",
    "Structural",
    "Windows",
];

const DEFAULT_MAINTENANCE_CATEGORIES: [&str; 9] = [
    "Appliance",
    "Electrical",
    "Exterior",
    "HVAC",
    "Interior",
    "Landscaping",
    "Plumbing",
    "Safety",
    "Structural",
];

const REQUIRED_SCHEMA: &[(&str, &[&str])] = &[
    (
        "house_profiles",
        &["id", "nickname", "created_at", "updated_at"],
    ),
    ("project_types", &["id", "name", "created_at", "updated_at"]),
    (
        "vendors",
        &["id", "name", "created_at", "updated_at", "deleted_at"],
    ),
    (
        "projects",
        &[
            "id",
            "title",
            "project_type_id",
            "status",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
    ),
    (
        "quotes",
        &[
            "id",
            "project_id",
            "vendor_id",
            "total_cents",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
    ),
    (
        "maintenance_categories",
        &["id", "name", "created_at", "updated_at"],
    ),
    (
        "appliances",
        &["id", "name", "created_at", "updated_at", "deleted_at"],
    ),
    (
        "maintenance_items",
        &[
            "id",
            "name",
            "category_id",
            "interval_months",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
    ),
    (
        "service_log_entries",
        &[
            "id",
            "maintenance_item_id",
            "serviced_at",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
    ),
    (
        "incidents",
        &[
            "id",
            "title",
            "status",
            "severity",
            "date_noticed",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
    ),
    (
        "documents",
        &[
            "id",
            "title",
            "file_name",
            "entity_kind",
            "entity_id",
            "mime_type",
            "size_bytes",
            "sha256",
            "data",
            "created_at",
            "updated_at",
            "deleted_at",
        ],
    ),
    (
        "deletion_records",
        &["id", "entity", "target_id", "deleted_at", "restored_at"],
    ),
    ("settings", &["key", "value", "updated_at"]),
    ("chat_inputs", &["id", "input", "created_at"]),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupValue<Id> {
    pub id: Id,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    pub title: String,
    pub project_type_id: ProjectTypeId,
    pub status: ProjectStatus,
    pub description: String,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,
    pub budget_cents: Option<i64>,
    pub actual_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDocument {
    pub title: String,
    pub file_name: String,
    pub entity_kind: DocumentEntityKind,
    pub entity_id: i64,
    pub mime_type: String,
    pub data: Vec<u8>,
    pub notes: String,
}

pub struct Store {
    conn: Connection,
    max_document_size: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let printable = path.to_string_lossy().to_string();
        validate_db_path(&printable)?;
        let conn = Connection::open(path)
            .with_context(|| format!("open database at {}", path.display()))?;
        configure_connection(&conn)?;
        Ok(Self {
            conn,
            max_document_size: MAX_DOCUMENT_SIZE,
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("open in-memory database")?;
        configure_connection(&conn)?;
        Ok(Self {
            conn,
            max_document_size: MAX_DOCUMENT_SIZE,
        })
    }

    pub fn raw_connection(&self) -> &Connection {
        &self.conn
    }

    pub fn bootstrap(&self) -> Result<()> {
        if has_user_tables(&self.conn)? {
            validate_schema(&self.conn)?;
        } else {
            self.conn
                .execute_batch(include_str!("sql/schema.sql"))
                .context("create schema")?;
        }

        self.seed_defaults()?;
        Ok(())
    }

    pub fn seed_defaults(&self) -> Result<()> {
        for project_type in DEFAULT_PROJECT_TYPES {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO project_types (name) VALUES (?)",
                    params![project_type],
                )
                .with_context(|| format!("insert default project type {project_type}"))?;
        }

        for category in DEFAULT_MAINTENANCE_CATEGORIES {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO maintenance_categories (name) VALUES (?)",
                    params![category],
                )
                .with_context(|| format!("insert default maintenance category {category}"))?;
        }
        Ok(())
    }

    pub fn set_max_document_size(&mut self, value: i64) -> Result<()> {
        if value <= 0 {
            bail!("max document size must be positive, got {value}");
        }
        self.max_document_size = value;
        Ok(())
    }

    pub fn max_document_size(&self) -> i64 {
        self.max_document_size
    }

    pub fn list_project_types(&self) -> Result<Vec<LookupValue<ProjectTypeId>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM project_types ORDER BY name ASC")
            .context("prepare project types query")?;
        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                Ok(LookupValue {
                    id: ProjectTypeId::new(id),
                    name,
                })
            })
            .context("query project types")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect project types")
    }

    pub fn list_maintenance_categories(&self) -> Result<Vec<LookupValue<MaintenanceCategoryId>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM maintenance_categories ORDER BY name ASC")
            .context("prepare maintenance categories query")?;
        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                Ok(LookupValue {
                    id: MaintenanceCategoryId::new(id),
                    name,
                })
            })
            .context("query maintenance categories")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect maintenance categories")
    }

    pub fn create_project(&self, new_project: &NewProject) -> Result<ProjectId> {
        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO projects (
                  title, project_type_id, status, description,
                  start_date, end_date, budget_cents, actual_cents,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    new_project.title,
                    new_project.project_type_id.get(),
                    new_project.status.as_str(),
                    new_project.description,
                    new_project.start_date.map(format_date),
                    new_project.end_date.map(format_date),
                    new_project.budget_cents,
                    new_project.actual_cents,
                    now,
                    now,
                ],
            )
            .context("insert project")?;

        Ok(ProjectId::new(self.conn.last_insert_rowid()))
    }

    pub fn list_projects(&self, include_deleted: bool) -> Result<Vec<Project>> {
        let mut sql = String::from(
            "
            SELECT
              id, title, project_type_id, status, description,
              start_date, end_date, budget_cents, actual_cents,
              created_at, updated_at, deleted_at
            FROM projects
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY updated_at DESC, id DESC");

        let mut stmt = self.conn.prepare(&sql).context("prepare projects query")?;
        let rows = stmt
            .query_map([], |row| {
                let status_raw: String = row.get(3)?;
                let status = ProjectStatus::parse(&status_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("unknown project status {status_raw}"),
                        )),
                    )
                })?;

                let created_at_raw: String = row.get(9)?;
                let updated_at_raw: String = row.get(10)?;
                let start_date_raw: Option<String> = row.get(5)?;
                let end_date_raw: Option<String> = row.get(6)?;
                let deleted_at_raw: Option<String> = row.get(11)?;

                Ok(Project {
                    id: ProjectId::new(row.get(0)?),
                    title: row.get(1)?,
                    project_type_id: ProjectTypeId::new(row.get(2)?),
                    status,
                    description: row.get(4)?,
                    start_date: parse_opt_date(start_date_raw).map_err(to_sql_error)?,
                    end_date: parse_opt_date(end_date_raw).map_err(to_sql_error)?,
                    budget_cents: row.get(7)?,
                    actual_cents: row.get(8)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query projects")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect projects")
    }

    pub fn soft_delete_project(&self, project_id: ProjectId) -> Result<()> {
        let quote_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM quotes WHERE project_id = ? AND deleted_at IS NULL",
                params![project_id.get()],
                |row| row.get(0),
            )
            .context("count quotes linked to project")?;
        if quote_count > 0 {
            bail!(
                "cannot delete project {} because {quote_count} quote(s) reference it; delete quotes first",
                project_id.get()
            );
        }

        let now = now_rfc3339()?;
        let affected = self
            .conn
            .execute(
                "UPDATE projects SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
                params![now, now, project_id.get()],
            )
            .context("soft delete project")?;
        if affected == 0 {
            bail!("project {} not found or already deleted", project_id.get());
        }

        self.conn
            .execute(
                "INSERT INTO deletion_records (entity, target_id, deleted_at) VALUES ('project', ?, ?)",
                params![project_id.get(), now],
            )
            .context("write deletion record")?;
        Ok(())
    }

    pub fn restore_project(&self, project_id: ProjectId) -> Result<()> {
        let now = now_rfc3339()?;
        let restored = self
            .conn
            .execute(
                "UPDATE projects SET deleted_at = NULL, updated_at = ? WHERE id = ? AND deleted_at IS NOT NULL",
                params![now, project_id.get()],
            )
            .context("restore project")?;
        if restored == 0 {
            bail!(
                "project {} is not deleted or does not exist",
                project_id.get()
            );
        }

        self.conn
            .execute(
                "
                UPDATE deletion_records
                SET restored_at = ?
                WHERE entity = 'project' AND target_id = ? AND restored_at IS NULL
                ",
                params![now, project_id.get()],
            )
            .context("mark deletion record restored")?;

        Ok(())
    }

    pub fn dashboard_counts(&self) -> Result<DashboardCounts> {
        let projects_due: i64 = self
            .conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM projects
                WHERE deleted_at IS NULL
                  AND status NOT IN ('completed', 'abandoned')
                ",
                [],
                |row| row.get(0),
            )
            .context("count projects due")?;

        let maintenance_due: i64 = self
            .conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM maintenance_items
                WHERE deleted_at IS NULL
                  AND (
                    last_serviced_at IS NULL
                    OR date(last_serviced_at, '+' || interval_months || ' months') <= date('now')
                  )
                ",
                [],
                |row| row.get(0),
            )
            .context("count maintenance due")?;

        let incidents_open: i64 = self
            .conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM incidents
                WHERE deleted_at IS NULL
                  AND status IN ('open', 'in_progress')
                ",
                [],
                |row| row.get(0),
            )
            .context("count open incidents")?;

        Ok(DashboardCounts {
            projects_due: usize::try_from(projects_due).unwrap_or(0),
            maintenance_due: usize::try_from(maintenance_due).unwrap_or(0),
            incidents_open: usize::try_from(incidents_open).unwrap_or(0),
        })
    }

    pub fn insert_document(&self, new_document: &NewDocument) -> Result<DocumentId> {
        let size = i64::try_from(new_document.data.len()).context("document size overflow")?;
        if size > self.max_document_size {
            bail!(
                "document is {} bytes but max allowed is {}; shrink the file and retry",
                size,
                self.max_document_size
            );
        }

        let checksum = checksum_sha256(&new_document.data);
        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO documents (
                  title, file_name, entity_kind, entity_id, mime_type,
                  size_bytes, sha256, data, notes, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    new_document.title,
                    new_document.file_name,
                    new_document.entity_kind.as_str(),
                    new_document.entity_id,
                    new_document.mime_type,
                    size,
                    checksum,
                    new_document.data,
                    new_document.notes,
                    now,
                    now,
                ],
            )
            .context("insert document")?;
        Ok(DocumentId::new(self.conn.last_insert_rowid()))
    }

    pub fn get_document(&self, document_id: DocumentId) -> Result<Document> {
        self.conn
            .query_row(
                "
                SELECT
                  id, title, file_name, entity_kind, entity_id, mime_type,
                  size_bytes, sha256, data, notes, created_at, updated_at, deleted_at
                FROM documents
                WHERE id = ?
                ",
                params![document_id.get()],
                |row| {
                    let kind_raw: String = row.get(3)?;
                    let kind = DocumentEntityKind::parse(&kind_raw).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("unknown document entity kind {kind_raw}"),
                            )),
                        )
                    })?;
                    let created_at_raw: String = row.get(10)?;
                    let updated_at_raw: String = row.get(11)?;
                    let deleted_at_raw: Option<String> = row.get(12)?;

                    Ok(Document {
                        id: DocumentId::new(row.get(0)?),
                        title: row.get(1)?,
                        file_name: row.get(2)?,
                        entity_kind: kind,
                        entity_id: row.get(4)?,
                        mime_type: row.get(5)?,
                        size_bytes: row.get(6)?,
                        checksum_sha256: row.get(7)?,
                        data: row.get(8)?,
                        notes: row.get(9)?,
                        created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                        updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                        deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                    })
                },
            )
            .with_context(|| format!("load document {}", document_id.get()))
    }

    pub fn extract_document(&self, document_id: DocumentId) -> Result<PathBuf> {
        let row = self
            .conn
            .query_row(
                "SELECT data, file_name, sha256, size_bytes FROM documents WHERE id = ?",
                params![document_id.get()],
                |row| {
                    let data: Vec<u8> = row.get(0)?;
                    let file_name: String = row.get(1)?;
                    let checksum: String = row.get(2)?;
                    let size_bytes: i64 = row.get(3)?;
                    Ok((data, file_name, checksum, size_bytes))
                },
            )
            .with_context(|| format!("load document content {}", document_id.get()))?;

        let (data, file_name, checksum, size_bytes) = row;
        if data.is_empty() {
            bail!("document {} has no content", document_id.get());
        }

        let cache_dir = document_cache_dir()?;
        let file_name = Path::new(&file_name)
            .file_name()
            .unwrap_or_else(|| OsStr::new("document.bin"))
            .to_string_lossy();
        let cache_path = cache_dir.join(format!("{checksum}-{file_name}"));

        // We intentionally rewrite the cache file on access to refresh mtime.
        let should_write = match fs::metadata(&cache_path) {
            Ok(metadata) => metadata.len() != u64::try_from(size_bytes).unwrap_or(0),
            Err(_) => true,
        };

        if should_write {
            fs::write(&cache_path, &data)
                .with_context(|| format!("write cache file {}", cache_path.display()))?;
            set_private_permissions(&cache_path)?;
        }

        Ok(cache_path)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .with_context(|| format!("read setting {key}"))
    }

    pub fn put_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO settings (key, value, updated_at)
                VALUES (?, ?, ?)
                ON CONFLICT(key) DO UPDATE SET
                  value = excluded.value,
                  updated_at = excluded.updated_at
                ",
                params![key, value, now],
            )
            .with_context(|| format!("upsert setting {key}"))?;
        Ok(())
    }

    pub fn get_last_model(&self) -> Result<Option<String>> {
        self.get_setting(SETTING_LLM_MODEL)
    }

    pub fn put_last_model(&self, model: &str) -> Result<()> {
        self.put_setting(SETTING_LLM_MODEL, model)
    }

    pub fn get_show_dashboard(&self) -> Result<bool> {
        Ok(match self.get_setting(SETTING_SHOW_DASHBOARD)? {
            None => true,
            Some(value) => value == "true",
        })
    }

    pub fn put_show_dashboard(&self, show: bool) -> Result<()> {
        let value = if show { "true" } else { "false" };
        self.put_setting(SETTING_SHOW_DASHBOARD, value)
    }

    pub fn append_chat_input(&self, input: &str) -> Result<()> {
        let last_input: Option<String> = self
            .conn
            .query_row(
                "SELECT input FROM chat_inputs ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("load last chat input")?;
        if last_input.as_deref() == Some(input) {
            return Ok(());
        }

        let now = now_rfc3339()?;
        self.conn
            .execute(
                "INSERT INTO chat_inputs (input, created_at) VALUES (?, ?)",
                params![input, now],
            )
            .context("insert chat input")?;

        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chat_inputs", [], |row| row.get(0))
            .context("count chat inputs")?;

        if count > CHAT_HISTORY_MAX {
            let excess = count - CHAT_HISTORY_MAX;
            self.conn
                .execute(
                    "
                    DELETE FROM chat_inputs
                    WHERE id IN (
                      SELECT id FROM chat_inputs
                      ORDER BY id ASC
                      LIMIT ?
                    )
                    ",
                    params![excess],
                )
                .context("trim chat input history")?;
        }

        Ok(())
    }

    pub fn load_chat_history(&self) -> Result<Vec<ChatInput>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, input, created_at FROM chat_inputs ORDER BY id ASC")
            .context("prepare chat history query")?;

        let rows = stmt
            .query_map([], |row| {
                let created_at_raw: String = row.get(2)?;
                Ok(ChatInput {
                    id: ChatInputId::new(row.get(0)?),
                    input: row.get(1)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query chat history")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect chat history")
    }
}

pub fn default_db_path() -> Result<PathBuf> {
    if let Some(override_path) = env::var_os("MICASA_DB_PATH") {
        return Ok(PathBuf::from(override_path));
    }

    let data_root = dirs::data_local_dir().ok_or_else(|| {
        anyhow!("cannot resolve data directory; set MICASA_DB_PATH to a writable database path")
    })?;

    let app_dir = data_root.join(APP_NAME);
    fs::create_dir_all(&app_dir)
        .with_context(|| format!("create data directory {}", app_dir.display()))?;
    Ok(app_dir.join("micasa.db"))
}

pub fn document_cache_dir() -> Result<PathBuf> {
    let cache_root = dirs::cache_dir().ok_or_else(|| {
        anyhow!("cannot resolve cache directory; set XDG_CACHE_HOME or platform equivalent")
    })?;
    let dir = cache_root.join(APP_NAME).join("documents");
    fs::create_dir_all(&dir)
        .with_context(|| format!("create cache directory {}", dir.display()))?;
    Ok(dir)
}

pub fn evict_stale_cache(dir: &Path, ttl_days: i64) -> Result<usize> {
    if ttl_days <= 0 {
        return Ok(0);
    }
    if !dir.exists() {
        return Ok(0);
    }

    let ttl_secs = u64::try_from(ttl_days)
        .ok()
        .and_then(|days| days.checked_mul(24 * 60 * 60))
        .ok_or_else(|| anyhow!("ttl_days is too large: {ttl_days}"))?;
    let ttl = Duration::from_secs(ttl_secs);
    let now = std::time::SystemTime::now();

    let mut removed = 0usize;
    for entry in fs::read_dir(dir).with_context(|| format!("read cache dir {}", dir.display()))? {
        let entry = entry?;
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.is_dir() {
            continue;
        }
        let modified = match metadata.modified() {
            Ok(modified) => modified,
            Err(_) => continue,
        };
        if now.duration_since(modified).unwrap_or(Duration::ZERO) > ttl
            && fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }

    Ok(removed)
}

pub fn validate_db_path(path: &str) -> Result<()> {
    if path.is_empty() {
        bail!("database path must not be empty");
    }
    if path == ":memory:" {
        return Ok(());
    }

    if let Some(index) = path.find("://")
        && index > 0
    {
        let scheme = &path[..index];
        if scheme.chars().all(char::is_alphabetic) {
            bail!(
                "database path {path:?} looks like a URI ({scheme}://); pass a filesystem path instead"
            );
        }
    }

    if path.starts_with("file:") {
        bail!("database path {path:?} uses file: URI syntax; pass a plain filesystem path");
    }

    if path.contains('?') {
        bail!(
            "database path {path:?} contains '?'; remove query parameters and use a plain file path"
        );
    }

    Ok(())
}

fn has_user_tables(conn: &Connection) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
              AND name NOT LIKE 'sqlite_%'
            ",
            [],
            |row| row.get(0),
        )
        .context("count user tables")?;
    Ok(count > 0)
}

fn validate_schema(conn: &Connection) -> Result<()> {
    for (table, required_columns) in REQUIRED_SCHEMA {
        if !table_exists(conn, table)? {
            bail!(
                "database is missing required table `{table}`; use a micasa-compatible database or migrate first"
            );
        }

        let columns = table_columns(conn, table)?;
        let missing: Vec<&str> = required_columns
            .iter()
            .copied()
            .filter(|column| !columns.contains(*column))
            .collect();

        if !missing.is_empty() {
            bail!(
                "table `{table}` is missing required columns: {}; run migration before launching",
                missing.join(", ")
            );
        }
    }

    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "
            SELECT EXISTS(
              SELECT 1
              FROM sqlite_master
              WHERE type = 'table' AND name = ?
            )
            ",
            params![table],
            |row| row.get::<_, i64>(0),
        )
        .with_context(|| format!("check table existence for {table}"))?;
    Ok(exists == 1)
}

fn table_columns(conn: &Connection, table: &str) -> Result<BTreeSet<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("inspect columns for {table}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("query column info for {table}"))?;

    let names = rows
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .with_context(|| format!("collect columns for {table}"))?;
    Ok(names)
}

fn configure_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA busy_timeout = 5000;
        ",
    )
    .context("configure sqlite pragmas")
}

fn now_rfc3339() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format current timestamp")
}

fn parse_datetime(raw: &str) -> Result<OffsetDateTime> {
    if let Ok(value) = OffsetDateTime::parse(raw, &Rfc3339) {
        return Ok(value);
    }

    if let Ok(value) = OffsetDateTime::parse(
        raw,
        &format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond][offset_hour sign:mandatory]:[offset_minute]"
        ),
    ) {
        return Ok(value);
    }

    if let Ok(value) = OffsetDateTime::parse(
        raw,
        &format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"
        ),
    ) {
        return Ok(value);
    }

    if let Ok(value) = PrimitiveDateTime::parse(
        raw,
        &format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond]"),
    ) {
        return Ok(value.assume_utc());
    }

    if let Ok(value) = PrimitiveDateTime::parse(
        raw,
        &format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    ) {
        return Ok(value.assume_utc());
    }

    if let Ok(value) = PrimitiveDateTime::parse(
        raw,
        &format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]"),
    ) {
        return Ok(value.assume_utc());
    }

    if let Ok(value) = PrimitiveDateTime::parse(
        raw,
        &format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]"),
    ) {
        return Ok(value.assume_utc());
    }

    bail!("unsupported datetime format {raw:?}")
}

fn parse_date(raw: &str) -> Result<Date> {
    if let Ok(value) = Date::parse(raw, &format_description!("[year]-[month]-[day]")) {
        return Ok(value);
    }

    // GORM may store date values as full timestamps; normalize to date.
    let date_time = parse_datetime(raw)?;
    Ok(date_time.date())
}

fn parse_opt_datetime(raw: Option<String>) -> Result<Option<OffsetDateTime>> {
    raw.as_deref().map(parse_datetime).transpose()
}

fn parse_opt_date(raw: Option<String>) -> Result<Option<Date>> {
    raw.as_deref().map(parse_date).transpose()
}

fn to_sql_error(error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

fn format_date(value: Date) -> String {
    value
        .format(&format_description!("[year]-[month]-[day]"))
        .unwrap_or_else(|_| "1970-01-01".to_owned())
}

fn checksum_sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn set_private_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("set permissions on {}", path.display()))?;
    }
    Ok(())
}

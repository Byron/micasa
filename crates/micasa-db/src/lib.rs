// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result, anyhow, bail};
use micasa_app::{
    Appliance, ApplianceId, ChatInput, ChatInputId, DashboardCounts, Document, DocumentEntityKind,
    DocumentId, Incident, IncidentId, IncidentSeverity, IncidentStatus, MaintenanceCategoryId,
    MaintenanceItem, MaintenanceItemId, Project, ProjectId, ProjectStatus, ProjectTypeId, Quote,
    QuoteId, Vendor, VendorId,
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
pub struct UpdateProject {
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
pub struct NewVendor {
    pub name: String,
    pub contact_name: String,
    pub email: String,
    pub phone: String,
    pub website: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateVendor {
    pub name: String,
    pub contact_name: String,
    pub email: String,
    pub phone: String,
    pub website: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuote {
    pub project_id: ProjectId,
    pub vendor_id: VendorId,
    pub total_cents: i64,
    pub labor_cents: Option<i64>,
    pub materials_cents: Option<i64>,
    pub other_cents: Option<i64>,
    pub received_date: Option<Date>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateQuote {
    pub project_id: ProjectId,
    pub vendor_id: VendorId,
    pub total_cents: i64,
    pub labor_cents: Option<i64>,
    pub materials_cents: Option<i64>,
    pub other_cents: Option<i64>,
    pub received_date: Option<Date>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAppliance {
    pub name: String,
    pub brand: String,
    pub model_number: String,
    pub serial_number: String,
    pub purchase_date: Option<Date>,
    pub warranty_expiry: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAppliance {
    pub name: String,
    pub brand: String,
    pub model_number: String,
    pub serial_number: String,
    pub purchase_date: Option<Date>,
    pub warranty_expiry: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMaintenanceItem {
    pub name: String,
    pub category_id: MaintenanceCategoryId,
    pub appliance_id: Option<ApplianceId>,
    pub last_serviced_at: Option<Date>,
    pub interval_months: i32,
    pub manual_url: String,
    pub manual_text: String,
    pub notes: String,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateMaintenanceItem {
    pub name: String,
    pub category_id: MaintenanceCategoryId,
    pub appliance_id: Option<ApplianceId>,
    pub last_serviced_at: Option<Date>,
    pub interval_months: i32,
    pub manual_url: String,
    pub manual_text: String,
    pub notes: String,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewIncident {
    pub title: String,
    pub description: String,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub date_noticed: Date,
    pub date_resolved: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub appliance_id: Option<ApplianceId>,
    pub vendor_id: Option<VendorId>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateIncident {
    pub title: String,
    pub description: String,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub date_noticed: Date,
    pub date_resolved: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub appliance_id: Option<ApplianceId>,
    pub vendor_id: Option<VendorId>,
    pub notes: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityKind {
    Project,
    Quote,
    MaintenanceItem,
    Appliance,
    Vendor,
    Incident,
}

impl EntityKind {
    const fn table(self) -> &'static str {
        match self {
            Self::Project => "projects",
            Self::Quote => "quotes",
            Self::MaintenanceItem => "maintenance_items",
            Self::Appliance => "appliances",
            Self::Vendor => "vendors",
            Self::Incident => "incidents",
        }
    }

    const fn deleted_tag(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Quote => "quote",
            Self::MaintenanceItem => "maintenance",
            Self::Appliance => "appliance",
            Self::Vendor => "vendor",
            Self::Incident => "incident",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentKind {
    Project,
    Vendor,
    Appliance,
}

impl ParentKind {
    const fn table(self) -> &'static str {
        match self {
            Self::Project => "projects",
            Self::Vendor => "vendors",
            Self::Appliance => "appliances",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Vendor => "vendor",
            Self::Appliance => "appliance",
        }
    }
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

    pub fn update_project(&self, project_id: ProjectId, update: &UpdateProject) -> Result<()> {
        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE projects
                SET
                  title = ?,
                  project_type_id = ?,
                  status = ?,
                  description = ?,
                  start_date = ?,
                  end_date = ?,
                  budget_cents = ?,
                  actual_cents = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.title,
                    update.project_type_id.get(),
                    update.status.as_str(),
                    update.description,
                    update.start_date.map(format_date),
                    update.end_date.map(format_date),
                    update.budget_cents,
                    update.actual_cents,
                    now,
                    project_id.get(),
                ],
            )
            .context("update project")?;
        if rows_affected == 0 {
            bail!(
                "project {} not found or deleted -- choose an existing project and retry",
                project_id.get()
            );
        }
        Ok(())
    }

    pub fn get_project(&self, project_id: ProjectId) -> Result<Project> {
        self.conn
            .query_row(
                "
                SELECT
                  id, title, project_type_id, status, description,
                  start_date, end_date, budget_cents, actual_cents,
                  created_at, updated_at, deleted_at
                FROM projects
                WHERE id = ?
                ",
                params![project_id.get()],
                |row| {
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

                    let start_date_raw: Option<String> = row.get(5)?;
                    let end_date_raw: Option<String> = row.get(6)?;
                    let created_at_raw: String = row.get(9)?;
                    let updated_at_raw: String = row.get(10)?;
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
                },
            )
            .with_context(|| format!("load project {}", project_id.get()))
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

        self.soft_delete_entity(EntityKind::Project, project_id.get())
    }

    pub fn restore_project(&self, project_id: ProjectId) -> Result<()> {
        self.restore_entity(EntityKind::Project, project_id.get())
    }

    pub fn list_vendors(&self, include_deleted: bool) -> Result<Vec<Vendor>> {
        let mut sql = String::from(
            "
            SELECT
              id, name, contact_name, email, phone, website, notes,
              created_at, updated_at, deleted_at
            FROM vendors
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY name ASC, id DESC");

        let mut stmt = self.conn.prepare(&sql).context("prepare vendors query")?;
        let rows = stmt
            .query_map([], |row| {
                let created_at_raw: String = row.get(7)?;
                let updated_at_raw: String = row.get(8)?;
                let deleted_at_raw: Option<String> = row.get(9)?;

                Ok(Vendor {
                    id: VendorId::new(row.get(0)?),
                    name: row.get(1)?,
                    contact_name: row.get(2)?,
                    email: row.get(3)?,
                    phone: row.get(4)?,
                    website: row.get(5)?,
                    notes: row.get(6)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query vendors")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect vendors")
    }

    pub fn create_vendor(&self, vendor: &NewVendor) -> Result<VendorId> {
        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO vendors (
                  name, contact_name, email, phone, website, notes,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    vendor.name,
                    vendor.contact_name,
                    vendor.email,
                    vendor.phone,
                    vendor.website,
                    vendor.notes,
                    now,
                    now,
                ],
            )
            .context("insert vendor")?;
        Ok(VendorId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_vendor(&self, vendor_id: VendorId, update: &UpdateVendor) -> Result<()> {
        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE vendors
                SET
                  name = ?,
                  contact_name = ?,
                  email = ?,
                  phone = ?,
                  website = ?,
                  notes = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.name,
                    update.contact_name,
                    update.email,
                    update.phone,
                    update.website,
                    update.notes,
                    now,
                    vendor_id.get(),
                ],
            )
            .context("update vendor")?;
        if rows_affected == 0 {
            bail!(
                "vendor {} not found or deleted -- choose an existing vendor and retry",
                vendor_id.get()
            );
        }
        Ok(())
    }

    pub fn soft_delete_vendor(&self, vendor_id: VendorId) -> Result<()> {
        let quote_count = self
            .count_active_dependents("quotes", "vendor_id", vendor_id.get())
            .context("count quotes linked to vendor")?;
        if quote_count > 0 {
            bail!(
                "vendor {} has {quote_count} active quote(s) -- delete quotes first",
                vendor_id.get()
            );
        }

        let incident_count = self
            .count_active_dependents("incidents", "vendor_id", vendor_id.get())
            .context("count incidents linked to vendor")?;
        if incident_count > 0 {
            bail!(
                "vendor {} has {incident_count} active incident(s) -- delete incidents first",
                vendor_id.get()
            );
        }

        self.soft_delete_entity(EntityKind::Vendor, vendor_id.get())
    }

    pub fn restore_vendor(&self, vendor_id: VendorId) -> Result<()> {
        self.restore_entity(EntityKind::Vendor, vendor_id.get())
    }

    pub fn list_quotes(&self, include_deleted: bool) -> Result<Vec<Quote>> {
        let mut sql = String::from(
            "
            SELECT
              id, project_id, vendor_id, total_cents, labor_cents,
              materials_cents, other_cents, received_date, notes,
              created_at, updated_at, deleted_at
            FROM quotes
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY updated_at DESC, id DESC");

        let mut stmt = self.conn.prepare(&sql).context("prepare quotes query")?;
        let rows = stmt
            .query_map([], |row| {
                let received_date_raw: Option<String> = row.get(7)?;
                let created_at_raw: String = row.get(9)?;
                let updated_at_raw: String = row.get(10)?;
                let deleted_at_raw: Option<String> = row.get(11)?;

                Ok(Quote {
                    id: QuoteId::new(row.get(0)?),
                    project_id: ProjectId::new(row.get(1)?),
                    vendor_id: VendorId::new(row.get(2)?),
                    total_cents: row.get(3)?,
                    labor_cents: row.get(4)?,
                    materials_cents: row.get(5)?,
                    other_cents: row.get(6)?,
                    received_date: parse_opt_date(received_date_raw).map_err(to_sql_error)?,
                    notes: row.get(8)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query quotes")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect quotes")
    }

    pub fn create_quote(&self, quote: &NewQuote) -> Result<QuoteId> {
        self.require_parent_alive(ParentKind::Project, quote.project_id.get())?;
        self.require_parent_alive(ParentKind::Vendor, quote.vendor_id.get())?;

        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO quotes (
                  project_id, vendor_id, total_cents, labor_cents,
                  materials_cents, other_cents, received_date, notes,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    quote.project_id.get(),
                    quote.vendor_id.get(),
                    quote.total_cents,
                    quote.labor_cents,
                    quote.materials_cents,
                    quote.other_cents,
                    quote.received_date.map(format_date),
                    quote.notes,
                    now,
                    now,
                ],
            )
            .context("insert quote")?;
        Ok(QuoteId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_quote(&self, quote_id: QuoteId, update: &UpdateQuote) -> Result<()> {
        self.require_parent_alive(ParentKind::Project, update.project_id.get())?;
        self.require_parent_alive(ParentKind::Vendor, update.vendor_id.get())?;

        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE quotes
                SET
                  project_id = ?,
                  vendor_id = ?,
                  total_cents = ?,
                  labor_cents = ?,
                  materials_cents = ?,
                  other_cents = ?,
                  received_date = ?,
                  notes = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.project_id.get(),
                    update.vendor_id.get(),
                    update.total_cents,
                    update.labor_cents,
                    update.materials_cents,
                    update.other_cents,
                    update.received_date.map(format_date),
                    update.notes,
                    now,
                    quote_id.get(),
                ],
            )
            .context("update quote")?;
        if rows_affected == 0 {
            bail!(
                "quote {} not found or deleted -- choose an existing quote and retry",
                quote_id.get()
            );
        }
        Ok(())
    }

    pub fn soft_delete_quote(&self, quote_id: QuoteId) -> Result<()> {
        self.soft_delete_entity(EntityKind::Quote, quote_id.get())
    }

    pub fn restore_quote(&self, quote_id: QuoteId) -> Result<()> {
        let (project_id, vendor_id): (i64, i64) = self
            .conn
            .query_row(
                "SELECT project_id, vendor_id FROM quotes WHERE id = ?",
                params![quote_id.get()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .with_context(|| format!("load quote {}", quote_id.get()))?;

        self.require_parent_alive(ParentKind::Project, project_id)?;
        self.require_parent_alive(ParentKind::Vendor, vendor_id)?;
        self.restore_entity(EntityKind::Quote, quote_id.get())
    }

    pub fn list_appliances(&self, include_deleted: bool) -> Result<Vec<Appliance>> {
        let mut sql = String::from(
            "
            SELECT
              id, name, brand, model_number, serial_number,
              purchase_date, warranty_expiry, location, cost_cents, notes,
              created_at, updated_at, deleted_at
            FROM appliances
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY updated_at DESC, id DESC");

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare appliances query")?;
        let rows = stmt
            .query_map([], |row| {
                let purchase_date_raw: Option<String> = row.get(5)?;
                let warranty_expiry_raw: Option<String> = row.get(6)?;
                let created_at_raw: String = row.get(10)?;
                let updated_at_raw: String = row.get(11)?;
                let deleted_at_raw: Option<String> = row.get(12)?;

                Ok(Appliance {
                    id: ApplianceId::new(row.get(0)?),
                    name: row.get(1)?,
                    brand: row.get(2)?,
                    model_number: row.get(3)?,
                    serial_number: row.get(4)?,
                    purchase_date: parse_opt_date(purchase_date_raw).map_err(to_sql_error)?,
                    warranty_expiry: parse_opt_date(warranty_expiry_raw).map_err(to_sql_error)?,
                    location: row.get(7)?,
                    cost_cents: row.get(8)?,
                    notes: row.get(9)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query appliances")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect appliances")
    }

    pub fn create_appliance(&self, appliance: &NewAppliance) -> Result<ApplianceId> {
        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO appliances (
                  name, brand, model_number, serial_number, purchase_date,
                  warranty_expiry, location, cost_cents, notes,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    appliance.name,
                    appliance.brand,
                    appliance.model_number,
                    appliance.serial_number,
                    appliance.purchase_date.map(format_date),
                    appliance.warranty_expiry.map(format_date),
                    appliance.location,
                    appliance.cost_cents,
                    appliance.notes,
                    now,
                    now,
                ],
            )
            .context("insert appliance")?;
        Ok(ApplianceId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_appliance(
        &self,
        appliance_id: ApplianceId,
        update: &UpdateAppliance,
    ) -> Result<()> {
        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE appliances
                SET
                  name = ?,
                  brand = ?,
                  model_number = ?,
                  serial_number = ?,
                  purchase_date = ?,
                  warranty_expiry = ?,
                  location = ?,
                  cost_cents = ?,
                  notes = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.name,
                    update.brand,
                    update.model_number,
                    update.serial_number,
                    update.purchase_date.map(format_date),
                    update.warranty_expiry.map(format_date),
                    update.location,
                    update.cost_cents,
                    update.notes,
                    now,
                    appliance_id.get(),
                ],
            )
            .context("update appliance")?;
        if rows_affected == 0 {
            bail!(
                "appliance {} not found or deleted -- choose an existing appliance and retry",
                appliance_id.get()
            );
        }
        Ok(())
    }

    pub fn soft_delete_appliance(&self, appliance_id: ApplianceId) -> Result<()> {
        let maintenance_count = self
            .count_active_dependents("maintenance_items", "appliance_id", appliance_id.get())
            .context("count maintenance items linked to appliance")?;
        if maintenance_count > 0 {
            bail!(
                "appliance {} has {maintenance_count} active maintenance item(s) -- delete or reassign them first",
                appliance_id.get()
            );
        }

        let incident_count = self
            .count_active_dependents("incidents", "appliance_id", appliance_id.get())
            .context("count incidents linked to appliance")?;
        if incident_count > 0 {
            bail!(
                "appliance {} has {incident_count} active incident(s) -- delete incidents first",
                appliance_id.get()
            );
        }

        self.soft_delete_entity(EntityKind::Appliance, appliance_id.get())
    }

    pub fn restore_appliance(&self, appliance_id: ApplianceId) -> Result<()> {
        self.restore_entity(EntityKind::Appliance, appliance_id.get())
    }

    pub fn list_maintenance_items(&self, include_deleted: bool) -> Result<Vec<MaintenanceItem>> {
        let mut sql = String::from(
            "
            SELECT
              id, name, category_id, appliance_id, last_serviced_at,
              interval_months, manual_url, manual_text, notes, cost_cents,
              created_at, updated_at, deleted_at
            FROM maintenance_items
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY updated_at DESC, id DESC");

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare maintenance items query")?;
        let rows = stmt
            .query_map([], |row| {
                let appliance_id: Option<i64> = row.get(3)?;
                let last_serviced_at_raw: Option<String> = row.get(4)?;
                let created_at_raw: String = row.get(10)?;
                let updated_at_raw: String = row.get(11)?;
                let deleted_at_raw: Option<String> = row.get(12)?;

                Ok(MaintenanceItem {
                    id: MaintenanceItemId::new(row.get(0)?),
                    name: row.get(1)?,
                    category_id: MaintenanceCategoryId::new(row.get(2)?),
                    appliance_id: appliance_id.map(ApplianceId::new),
                    last_serviced_at: parse_opt_date(last_serviced_at_raw).map_err(to_sql_error)?,
                    interval_months: row.get(5)?,
                    manual_url: row.get(6)?,
                    manual_text: row.get(7)?,
                    notes: row.get(8)?,
                    cost_cents: row.get(9)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query maintenance items")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect maintenance items")
    }

    pub fn create_maintenance_item(&self, item: &NewMaintenanceItem) -> Result<MaintenanceItemId> {
        if let Some(appliance_id) = item.appliance_id {
            self.require_parent_alive(ParentKind::Appliance, appliance_id.get())?;
        }

        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO maintenance_items (
                  name, category_id, appliance_id, last_serviced_at,
                  interval_months, manual_url, manual_text, notes, cost_cents,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    item.name,
                    item.category_id.get(),
                    item.appliance_id.map(ApplianceId::get),
                    item.last_serviced_at.map(format_date),
                    item.interval_months,
                    item.manual_url,
                    item.manual_text,
                    item.notes,
                    item.cost_cents,
                    now,
                    now,
                ],
            )
            .context("insert maintenance item")?;
        Ok(MaintenanceItemId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_maintenance_item(
        &self,
        maintenance_id: MaintenanceItemId,
        update: &UpdateMaintenanceItem,
    ) -> Result<()> {
        if let Some(appliance_id) = update.appliance_id {
            self.require_parent_alive(ParentKind::Appliance, appliance_id.get())?;
        }

        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE maintenance_items
                SET
                  name = ?,
                  category_id = ?,
                  appliance_id = ?,
                  last_serviced_at = ?,
                  interval_months = ?,
                  manual_url = ?,
                  manual_text = ?,
                  notes = ?,
                  cost_cents = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.name,
                    update.category_id.get(),
                    update.appliance_id.map(ApplianceId::get),
                    update.last_serviced_at.map(format_date),
                    update.interval_months,
                    update.manual_url,
                    update.manual_text,
                    update.notes,
                    update.cost_cents,
                    now,
                    maintenance_id.get(),
                ],
            )
            .context("update maintenance item")?;
        if rows_affected == 0 {
            bail!(
                "maintenance item {} not found or deleted -- choose an existing item and retry",
                maintenance_id.get()
            );
        }
        Ok(())
    }

    pub fn soft_delete_maintenance_item(&self, maintenance_id: MaintenanceItemId) -> Result<()> {
        let service_count = self
            .count_active_dependents(
                "service_log_entries",
                "maintenance_item_id",
                maintenance_id.get(),
            )
            .context("count service logs linked to maintenance item")?;
        if service_count > 0 {
            bail!(
                "maintenance item {} has {service_count} service log(s) -- delete service logs first",
                maintenance_id.get()
            );
        }
        self.soft_delete_entity(EntityKind::MaintenanceItem, maintenance_id.get())
    }

    pub fn restore_maintenance_item(&self, maintenance_id: MaintenanceItemId) -> Result<()> {
        let appliance_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT appliance_id FROM maintenance_items WHERE id = ?",
                params![maintenance_id.get()],
                |row| row.get(0),
            )
            .with_context(|| format!("load maintenance item {}", maintenance_id.get()))?;
        if let Some(appliance_id) = appliance_id {
            self.require_parent_alive(ParentKind::Appliance, appliance_id)?;
        }
        self.restore_entity(EntityKind::MaintenanceItem, maintenance_id.get())
    }

    pub fn list_incidents(&self, include_deleted: bool) -> Result<Vec<Incident>> {
        let mut sql = String::from(
            "
            SELECT
              id, title, description, status, severity, date_noticed,
              date_resolved, location, cost_cents, appliance_id, vendor_id,
              notes, created_at, updated_at, deleted_at
            FROM incidents
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY updated_at DESC, id DESC");

        let mut stmt = self.conn.prepare(&sql).context("prepare incidents query")?;
        let rows = stmt
            .query_map([], |row| {
                let status_raw: String = row.get(3)?;
                let status = IncidentStatus::parse(&status_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("unknown incident status {status_raw}"),
                        )),
                    )
                })?;

                let severity_raw: String = row.get(4)?;
                let severity = IncidentSeverity::parse(&severity_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("unknown incident severity {severity_raw}"),
                        )),
                    )
                })?;

                let date_noticed_raw: String = row.get(5)?;
                let date_resolved_raw: Option<String> = row.get(6)?;
                let appliance_id: Option<i64> = row.get(9)?;
                let vendor_id: Option<i64> = row.get(10)?;
                let created_at_raw: String = row.get(12)?;
                let updated_at_raw: String = row.get(13)?;
                let deleted_at_raw: Option<String> = row.get(14)?;

                Ok(Incident {
                    id: IncidentId::new(row.get(0)?),
                    title: row.get(1)?,
                    description: row.get(2)?,
                    status,
                    severity,
                    date_noticed: parse_date(&date_noticed_raw).map_err(to_sql_error)?,
                    date_resolved: parse_opt_date(date_resolved_raw).map_err(to_sql_error)?,
                    location: row.get(7)?,
                    cost_cents: row.get(8)?,
                    appliance_id: appliance_id.map(ApplianceId::new),
                    vendor_id: vendor_id.map(VendorId::new),
                    notes: row.get(11)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query incidents")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect incidents")
    }

    pub fn create_incident(&self, incident: &NewIncident) -> Result<IncidentId> {
        if let Some(appliance_id) = incident.appliance_id {
            self.require_parent_alive(ParentKind::Appliance, appliance_id.get())?;
        }
        if let Some(vendor_id) = incident.vendor_id {
            self.require_parent_alive(ParentKind::Vendor, vendor_id.get())?;
        }

        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO incidents (
                  title, description, status, severity, date_noticed,
                  date_resolved, location, cost_cents, appliance_id, vendor_id,
                  notes, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    incident.title,
                    incident.description,
                    incident.status.as_str(),
                    incident.severity.as_str(),
                    format_date(incident.date_noticed),
                    incident.date_resolved.map(format_date),
                    incident.location,
                    incident.cost_cents,
                    incident.appliance_id.map(ApplianceId::get),
                    incident.vendor_id.map(VendorId::get),
                    incident.notes,
                    now,
                    now,
                ],
            )
            .context("insert incident")?;
        Ok(IncidentId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_incident(&self, incident_id: IncidentId, update: &UpdateIncident) -> Result<()> {
        if let Some(appliance_id) = update.appliance_id {
            self.require_parent_alive(ParentKind::Appliance, appliance_id.get())?;
        }
        if let Some(vendor_id) = update.vendor_id {
            self.require_parent_alive(ParentKind::Vendor, vendor_id.get())?;
        }

        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE incidents
                SET
                  title = ?,
                  description = ?,
                  status = ?,
                  severity = ?,
                  date_noticed = ?,
                  date_resolved = ?,
                  location = ?,
                  cost_cents = ?,
                  appliance_id = ?,
                  vendor_id = ?,
                  notes = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.title,
                    update.description,
                    update.status.as_str(),
                    update.severity.as_str(),
                    format_date(update.date_noticed),
                    update.date_resolved.map(format_date),
                    update.location,
                    update.cost_cents,
                    update.appliance_id.map(ApplianceId::get),
                    update.vendor_id.map(VendorId::get),
                    update.notes,
                    now,
                    incident_id.get(),
                ],
            )
            .context("update incident")?;
        if rows_affected == 0 {
            bail!(
                "incident {} not found or deleted -- choose an existing incident and retry",
                incident_id.get()
            );
        }
        Ok(())
    }

    pub fn soft_delete_incident(&self, incident_id: IncidentId) -> Result<()> {
        self.soft_delete_entity(EntityKind::Incident, incident_id.get())
    }

    pub fn restore_incident(&self, incident_id: IncidentId) -> Result<()> {
        let (appliance_id, vendor_id): (Option<i64>, Option<i64>) = self
            .conn
            .query_row(
                "SELECT appliance_id, vendor_id FROM incidents WHERE id = ?",
                params![incident_id.get()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .with_context(|| format!("load incident {}", incident_id.get()))?;
        if let Some(appliance_id) = appliance_id {
            self.require_parent_alive(ParentKind::Appliance, appliance_id)?;
        }
        if let Some(vendor_id) = vendor_id {
            self.require_parent_alive(ParentKind::Vendor, vendor_id)?;
        }
        self.restore_entity(EntityKind::Incident, incident_id.get())
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

    pub fn list_documents(&self, include_deleted: bool) -> Result<Vec<Document>> {
        let mut sql = String::from(
            "
            SELECT
              id, title, file_name, entity_kind, entity_id, mime_type,
              size_bytes, sha256, notes, created_at, updated_at, deleted_at
            FROM documents
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY updated_at DESC, id DESC");

        let mut stmt = self.conn.prepare(&sql).context("prepare documents query")?;
        let rows = stmt
            .query_map([], |row| {
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
                let created_at_raw: String = row.get(9)?;
                let updated_at_raw: String = row.get(10)?;
                let deleted_at_raw: Option<String> = row.get(11)?;

                Ok(Document {
                    id: DocumentId::new(row.get(0)?),
                    title: row.get(1)?,
                    file_name: row.get(2)?,
                    entity_kind: kind,
                    entity_id: row.get(4)?,
                    mime_type: row.get(5)?,
                    size_bytes: row.get(6)?,
                    checksum_sha256: row.get(7)?,
                    data: Vec::new(),
                    notes: row.get(8)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query documents")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect documents")
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

    fn count_active_dependents(&self, table: &str, fk_column: &str, parent_id: i64) -> Result<i64> {
        let sql =
            format!("SELECT COUNT(*) FROM {table} WHERE {fk_column} = ? AND deleted_at IS NULL");
        self.conn
            .query_row(&sql, params![parent_id], |row| row.get(0))
            .with_context(|| format!("count dependents in {table} for {fk_column}={parent_id}"))
    }

    fn soft_delete_entity(&self, kind: EntityKind, entity_id: i64) -> Result<()> {
        let now = now_rfc3339()?;
        let sql = format!(
            "UPDATE {} SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
            kind.table()
        );
        let rows_affected = self
            .conn
            .execute(&sql, params![now, now, entity_id])
            .with_context(|| format!("soft delete {} {}", kind.deleted_tag(), entity_id))?;
        if rows_affected == 0 {
            bail!(
                "{} {} not found or already deleted",
                kind.deleted_tag(),
                entity_id
            );
        }
        self.conn
            .execute(
                "INSERT INTO deletion_records (entity, target_id, deleted_at) VALUES (?, ?, ?)",
                params![kind.deleted_tag(), entity_id, now],
            )
            .with_context(|| format!("record deletion for {} {}", kind.deleted_tag(), entity_id))?;
        Ok(())
    }

    fn restore_entity(&self, kind: EntityKind, entity_id: i64) -> Result<()> {
        let now = now_rfc3339()?;
        let sql = format!(
            "UPDATE {} SET deleted_at = NULL, updated_at = ? WHERE id = ? AND deleted_at IS NOT NULL",
            kind.table()
        );
        let rows_affected = self
            .conn
            .execute(&sql, params![now, entity_id])
            .with_context(|| format!("restore {} {}", kind.deleted_tag(), entity_id))?;
        if rows_affected == 0 {
            bail!(
                "{} {} is not deleted or does not exist",
                kind.deleted_tag(),
                entity_id
            );
        }
        self.conn
            .execute(
                "
                UPDATE deletion_records
                SET restored_at = ?
                WHERE entity = ? AND target_id = ? AND restored_at IS NULL
                ",
                params![now, kind.deleted_tag(), entity_id],
            )
            .with_context(|| {
                format!(
                    "mark deletion record restored for {} {}",
                    kind.deleted_tag(),
                    entity_id
                )
            })?;
        Ok(())
    }

    fn require_parent_alive(&self, parent_kind: ParentKind, parent_id: i64) -> Result<()> {
        let sql = format!(
            "SELECT deleted_at FROM {} WHERE id = ?",
            parent_kind.table()
        );
        let deleted_at: Option<Option<String>> = self
            .conn
            .query_row(&sql, params![parent_id], |row| row.get(0))
            .optional()
            .with_context(|| {
                format!(
                    "load parent {} {} for relationship check",
                    parent_kind.label(),
                    parent_id
                )
            })?;

        match deleted_at {
            Some(None) => Ok(()),
            Some(Some(_)) => bail!("{} is deleted -- restore it first", parent_kind.label()),
            None => bail!("{} no longer exists", parent_kind.label()),
        }
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

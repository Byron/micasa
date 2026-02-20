// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result, anyhow, bail};
use micasa_app::{
    AppSetting, Appliance, ApplianceId, ChatInput, ChatInputId, DashboardCounts, Document,
    DocumentEntityKind, DocumentId, HouseProfile, HouseProfileId, Incident, IncidentId,
    IncidentSeverity, IncidentStatus, MaintenanceCategoryId, MaintenanceItem, MaintenanceItemId,
    Project, ProjectId, ProjectStatus, ProjectTypeId, Quote, QuoteId, ServiceLogEntry,
    ServiceLogEntryId, SettingKey, SettingValue, Vendor, VendorId,
};
use rusqlite::types::ValueRef;
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
const MAX_QUERY_ROWS: usize = 200;

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
        &[
            "id",
            "nickname",
            "address_line_1",
            "address_line_2",
            "city",
            "state",
            "postal_code",
            "year_built",
            "square_feet",
            "lot_square_feet",
            "bedrooms",
            "bathrooms",
            "foundation_type",
            "wiring_type",
            "roof_type",
            "exterior_type",
            "heating_type",
            "cooling_type",
            "water_source",
            "sewer_type",
            "parking_type",
            "basement_type",
            "insurance_carrier",
            "insurance_policy",
            "insurance_renewal",
            "property_tax_cents",
            "hoa_name",
            "hoa_fee_cents",
            "created_at",
            "updated_at",
        ],
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
            "vendor_id",
            "cost_cents",
            "notes",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequiredIndex {
    name: &'static str,
    create_sql: &'static str,
}

const REQUIRED_INDEXES: &[RequiredIndex] = &[
    RequiredIndex {
        name: "idx_project_types_name",
        create_sql: "CREATE UNIQUE INDEX IF NOT EXISTS idx_project_types_name ON project_types (name);",
    },
    RequiredIndex {
        name: "idx_vendors_name",
        create_sql: "CREATE UNIQUE INDEX IF NOT EXISTS idx_vendors_name ON vendors (name);",
    },
    RequiredIndex {
        name: "idx_vendors_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_vendors_deleted_at ON vendors (deleted_at);",
    },
    RequiredIndex {
        name: "idx_projects_project_type_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_projects_project_type_id ON projects (project_type_id);",
    },
    RequiredIndex {
        name: "idx_projects_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_projects_deleted_at ON projects (deleted_at);",
    },
    RequiredIndex {
        name: "idx_quotes_project_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_quotes_project_id ON quotes (project_id);",
    },
    RequiredIndex {
        name: "idx_quotes_vendor_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_quotes_vendor_id ON quotes (vendor_id);",
    },
    RequiredIndex {
        name: "idx_quotes_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_quotes_deleted_at ON quotes (deleted_at);",
    },
    RequiredIndex {
        name: "idx_maintenance_categories_name",
        create_sql: "CREATE UNIQUE INDEX IF NOT EXISTS idx_maintenance_categories_name ON maintenance_categories (name);",
    },
    RequiredIndex {
        name: "idx_appliances_warranty_expiry",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_appliances_warranty_expiry ON appliances (warranty_expiry);",
    },
    RequiredIndex {
        name: "idx_appliances_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_appliances_deleted_at ON appliances (deleted_at);",
    },
    RequiredIndex {
        name: "idx_maintenance_items_category_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_maintenance_items_category_id ON maintenance_items (category_id);",
    },
    RequiredIndex {
        name: "idx_maintenance_items_appliance_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_maintenance_items_appliance_id ON maintenance_items (appliance_id);",
    },
    RequiredIndex {
        name: "idx_maintenance_items_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_maintenance_items_deleted_at ON maintenance_items (deleted_at);",
    },
    RequiredIndex {
        name: "idx_incidents_appliance_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_incidents_appliance_id ON incidents (appliance_id);",
    },
    RequiredIndex {
        name: "idx_incidents_vendor_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_incidents_vendor_id ON incidents (vendor_id);",
    },
    RequiredIndex {
        name: "idx_incidents_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_incidents_deleted_at ON incidents (deleted_at);",
    },
    RequiredIndex {
        name: "idx_service_log_entries_maintenance_item_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_service_log_entries_maintenance_item_id ON service_log_entries (maintenance_item_id);",
    },
    RequiredIndex {
        name: "idx_service_log_entries_vendor_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_service_log_entries_vendor_id ON service_log_entries (vendor_id);",
    },
    RequiredIndex {
        name: "idx_service_log_entries_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_service_log_entries_deleted_at ON service_log_entries (deleted_at);",
    },
    RequiredIndex {
        name: "idx_doc_entity",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_doc_entity ON documents (entity_kind, entity_id);",
    },
    RequiredIndex {
        name: "idx_documents_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_documents_deleted_at ON documents (deleted_at);",
    },
    RequiredIndex {
        name: "idx_deletion_records_entity",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_deletion_records_entity ON deletion_records (entity);",
    },
    RequiredIndex {
        name: "idx_deletion_records_target_id",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_deletion_records_target_id ON deletion_records (target_id);",
    },
    RequiredIndex {
        name: "idx_deletion_records_deleted_at",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_deletion_records_deleted_at ON deletion_records (deleted_at);",
    },
    RequiredIndex {
        name: "idx_entity_restored",
        create_sql: "CREATE INDEX IF NOT EXISTS idx_entity_restored ON deletion_records (entity, restored_at);",
    },
];

const COLUMN_HINTS: &[(&str, &str)] = &[
    (
        "project statuses (stored values)",
        "SELECT DISTINCT status FROM projects WHERE deleted_at IS NULL ORDER BY status ASC",
    ),
    (
        "project types",
        "SELECT DISTINCT name FROM project_types ORDER BY name ASC",
    ),
    (
        "vendor names",
        "SELECT DISTINCT name FROM vendors WHERE deleted_at IS NULL ORDER BY name ASC",
    ),
    (
        "appliance names",
        "SELECT DISTINCT name FROM appliances WHERE deleted_at IS NULL ORDER BY name ASC",
    ),
    (
        "maintenance categories",
        "SELECT DISTINCT name FROM maintenance_categories ORDER BY name ASC",
    ),
    (
        "maintenance item names",
        "SELECT DISTINCT name FROM maintenance_items WHERE deleted_at IS NULL ORDER BY name ASC",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupValue<Id> {
    pub id: Id,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PragmaColumn {
    pub cid: i32,
    pub name: String,
    pub column_type: String,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub primary_key: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HouseProfileInput {
    pub nickname: String,
    pub address_line_1: String,
    pub address_line_2: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub year_built: Option<i32>,
    pub square_feet: Option<i32>,
    pub lot_square_feet: Option<i32>,
    pub bedrooms: Option<i32>,
    pub bathrooms: Option<f64>,
    pub foundation_type: String,
    pub wiring_type: String,
    pub roof_type: String,
    pub exterior_type: String,
    pub heating_type: String,
    pub cooling_type: String,
    pub water_source: String,
    pub sewer_type: String,
    pub parking_type: String,
    pub basement_type: String,
    pub insurance_carrier: String,
    pub insurance_policy: String,
    pub insurance_renewal: Option<Date>,
    pub property_tax_cents: Option<i64>,
    pub hoa_name: String,
    pub hoa_fee_cents: Option<i64>,
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
pub struct NewServiceLogEntry {
    pub maintenance_item_id: MaintenanceItemId,
    pub serviced_at: Date,
    pub vendor_id: Option<VendorId>,
    pub cost_cents: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateServiceLogEntry {
    pub maintenance_item_id: MaintenanceItemId,
    pub serviced_at: Date,
    pub vendor_id: Option<VendorId>,
    pub cost_cents: Option<i64>,
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
pub enum LifecycleEntityRef {
    Project(ProjectId),
    Quote(QuoteId),
    MaintenanceItem(MaintenanceItemId),
    Appliance(ApplianceId),
    ServiceLogEntry(ServiceLogEntryId),
    Vendor(VendorId),
    Incident(IncidentId),
}

impl LifecycleEntityRef {
    const fn kind(self) -> EntityKind {
        match self {
            Self::Project(_) => EntityKind::Project,
            Self::Quote(_) => EntityKind::Quote,
            Self::MaintenanceItem(_) => EntityKind::MaintenanceItem,
            Self::Appliance(_) => EntityKind::Appliance,
            Self::ServiceLogEntry(_) => EntityKind::ServiceLogEntry,
            Self::Vendor(_) => EntityKind::Vendor,
            Self::Incident(_) => EntityKind::Incident,
        }
    }

    const fn id(self) -> i64 {
        match self {
            Self::Project(id) => id.get(),
            Self::Quote(id) => id.get(),
            Self::MaintenanceItem(id) => id.get(),
            Self::Appliance(id) => id.get(),
            Self::ServiceLogEntry(id) => id.get(),
            Self::Vendor(id) => id.get(),
            Self::Incident(id) => id.get(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentEntityRef {
    Project(ProjectId),
    Vendor(VendorId),
    Appliance(ApplianceId),
    MaintenanceItem(MaintenanceItemId),
}

impl ParentEntityRef {
    const fn kind(self) -> ParentKind {
        match self {
            Self::Project(_) => ParentKind::Project,
            Self::Vendor(_) => ParentKind::Vendor,
            Self::Appliance(_) => ParentKind::Appliance,
            Self::MaintenanceItem(_) => ParentKind::MaintenanceItem,
        }
    }

    const fn id(self) -> i64 {
        match self {
            Self::Project(id) => id.get(),
            Self::Vendor(id) => id.get(),
            Self::Appliance(id) => id.get(),
            Self::MaintenanceItem(id) => id.get(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DependentRelation {
    ProjectQuotes,
    VendorQuotes,
    VendorIncidents,
    VendorServiceLogEntries,
    ApplianceMaintenanceItems,
    ApplianceIncidents,
    MaintenanceItemServiceLogEntries,
}

impl DependentRelation {
    const fn table(self) -> &'static str {
        match self {
            Self::ProjectQuotes | Self::VendorQuotes => "quotes",
            Self::VendorIncidents | Self::ApplianceIncidents => "incidents",
            Self::VendorServiceLogEntries | Self::MaintenanceItemServiceLogEntries => {
                "service_log_entries"
            }
            Self::ApplianceMaintenanceItems => "maintenance_items",
        }
    }

    const fn fk_column(self) -> &'static str {
        match self {
            Self::ProjectQuotes => "project_id",
            Self::VendorQuotes | Self::VendorIncidents | Self::VendorServiceLogEntries => {
                "vendor_id"
            }
            Self::ApplianceMaintenanceItems | Self::ApplianceIncidents => "appliance_id",
            Self::MaintenanceItemServiceLogEntries => "maintenance_item_id",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityKind {
    Project,
    Quote,
    MaintenanceItem,
    Appliance,
    ServiceLogEntry,
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
            Self::ServiceLogEntry => "service_log_entries",
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
            Self::ServiceLogEntry => "service_log",
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
    MaintenanceItem,
}

impl ParentKind {
    const fn table(self) -> &'static str {
        match self {
            Self::Project => "projects",
            Self::Vendor => "vendors",
            Self::Appliance => "appliances",
            Self::MaintenanceItem => "maintenance_items",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Vendor => "vendor",
            Self::Appliance => "appliance",
            Self::MaintenanceItem => "maintenance item",
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

        ensure_required_indexes(&self.conn)?;

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

    pub fn table_names(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "
                SELECT name
                FROM sqlite_master
                WHERE type = 'table'
                  AND name NOT LIKE 'sqlite_%'
                ORDER BY name ASC
                ",
            )
            .context("prepare table names query")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("query table names")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect table names")
    }

    pub fn table_columns(&self, table: &str) -> Result<Vec<PragmaColumn>> {
        if !is_safe_identifier(table) {
            bail!("invalid table name: {table:?}");
        }

        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .with_context(|| format!("inspect columns for {table}"))?;
        let rows = stmt
            .query_map([], |row| {
                let not_null: i32 = row.get(3)?;
                let primary_key: i32 = row.get(5)?;
                Ok(PragmaColumn {
                    cid: row.get(0)?,
                    name: row.get(1)?,
                    column_type: row.get(2)?,
                    not_null: not_null != 0,
                    default_value: row.get(4)?,
                    primary_key,
                })
            })
            .with_context(|| format!("query column info for {table}"))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| format!("collect columns for {table}"))
    }

    pub fn read_only_query(&self, query: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            bail!("empty query");
        }
        if trimmed.contains(';') {
            bail!("multiple statements are not allowed");
        }

        let upper = trimmed.to_ascii_uppercase();
        if !upper.starts_with("SELECT") {
            bail!("only SELECT queries are allowed");
        }

        const DISALLOWED_KEYWORDS: &[&str] = &[
            "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE", "ATTACH", "DETACH", "PRAGMA",
            "REINDEX", "VACUUM",
        ];
        for keyword in DISALLOWED_KEYWORDS {
            if contains_word(&upper, keyword) {
                bail!("query contains disallowed keyword: {keyword}");
            }
        }

        let mut stmt = self
            .conn
            .prepare(trimmed)
            .context("prepare read-only query")?;
        let columns = stmt
            .column_names()
            .iter()
            .map(|column| (*column).to_owned())
            .collect::<Vec<_>>();
        let mut rows = stmt.query([]).context("execute read-only query")?;

        let mut output_rows = Vec::new();
        while let Some(row) = rows.next().context("scan read-only query rows")? {
            if output_rows.len() >= MAX_QUERY_ROWS {
                break;
            }

            let mut output = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                let value = row
                    .get_ref(index)
                    .map(value_ref_to_string)
                    .with_context(|| format!("read column {index} from query result"))?;
                output.push(value);
            }
            output_rows.push(output);
        }

        Ok((columns, output_rows))
    }

    pub fn data_dump(&self) -> String {
        let names = match self.table_names() {
            Ok(names) => names,
            Err(_) => return String::new(),
        };

        let mut output = String::new();
        for table in names {
            let mut stmt = match self.conn.prepare(&format!("SELECT * FROM {table}")) {
                Ok(stmt) => stmt,
                Err(_) => continue,
            };
            let columns = stmt
                .column_names()
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>();
            let deleted_at_index = columns
                .iter()
                .position(|column| column.eq_ignore_ascii_case("deleted_at"));
            let mut query = match stmt.query([]) {
                Ok(query) => query,
                Err(_) => continue,
            };

            let mut rows = Vec::new();
            while let Ok(Some(row)) = query.next() {
                if let Some(index) = deleted_at_index
                    && !matches!(row.get_ref(index), Ok(ValueRef::Null))
                {
                    continue;
                }

                let mut values = Vec::with_capacity(columns.len());
                let mut failed = false;
                for index in 0..columns.len() {
                    match row.get_ref(index) {
                        Ok(value) => values.push(value_ref_to_string(value)),
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                if !failed {
                    rows.push(values);
                }
            }

            if rows.is_empty() {
                continue;
            }

            output.push_str(&format!("### {table} ({} rows)\n\n", rows.len()));
            for row in rows {
                let mut parts = Vec::new();
                for (column, value) in columns.iter().zip(row.iter()) {
                    if value.is_empty() || is_noise_column(column) {
                        continue;
                    }
                    parts.push(format_column_value(column, value));
                }
                output.push_str(&format!("- {}\n", parts.join(", ")));
            }
            output.push('\n');
        }

        output
    }

    pub fn column_hints(&self) -> String {
        let mut output = String::new();

        for (label, query) in COLUMN_HINTS {
            let mut stmt = match self.conn.prepare(query) {
                Ok(stmt) => stmt,
                Err(_) => continue,
            };
            let values = match stmt.query_map([], |row| row.get::<_, String>(0)) {
                Ok(values) => values,
                Err(_) => continue,
            };
            let values = match values.collect::<rusqlite::Result<Vec<_>>>() {
                Ok(values) if !values.is_empty() => values,
                _ => continue,
            };
            output.push_str(&format!("- {label}: {}\n", values.join(", ")));
        }

        output
    }

    pub fn get_house_profile(&self) -> Result<Option<HouseProfile>> {
        self.conn
            .query_row(
                "
                SELECT
                  id, nickname, address_line_1, address_line_2, city, state, postal_code,
                  year_built, square_feet, lot_square_feet, bedrooms, bathrooms,
                  foundation_type, wiring_type, roof_type, exterior_type,
                  heating_type, cooling_type, water_source, sewer_type, parking_type,
                  basement_type, insurance_carrier, insurance_policy, insurance_renewal,
                  property_tax_cents, hoa_name, hoa_fee_cents, created_at, updated_at
                FROM house_profiles
                ORDER BY id ASC
                LIMIT 1
                ",
                [],
                |row| {
                    let insurance_renewal_raw: Option<String> = row.get(24)?;
                    let created_at_raw: String = row.get(28)?;
                    let updated_at_raw: String = row.get(29)?;
                    Ok(HouseProfile {
                        id: HouseProfileId::new(row.get(0)?),
                        nickname: row.get(1)?,
                        address_line_1: row.get(2)?,
                        address_line_2: row.get(3)?,
                        city: row.get(4)?,
                        state: row.get(5)?,
                        postal_code: row.get(6)?,
                        year_built: row.get(7)?,
                        square_feet: row.get(8)?,
                        lot_square_feet: row.get(9)?,
                        bedrooms: row.get(10)?,
                        bathrooms: row.get(11)?,
                        foundation_type: row.get(12)?,
                        wiring_type: row.get(13)?,
                        roof_type: row.get(14)?,
                        exterior_type: row.get(15)?,
                        heating_type: row.get(16)?,
                        cooling_type: row.get(17)?,
                        water_source: row.get(18)?,
                        sewer_type: row.get(19)?,
                        parking_type: row.get(20)?,
                        basement_type: row.get(21)?,
                        insurance_carrier: row.get(22)?,
                        insurance_policy: row.get(23)?,
                        insurance_renewal: parse_opt_date(insurance_renewal_raw)
                            .map_err(to_sql_error)?,
                        property_tax_cents: row.get(25)?,
                        hoa_name: row.get(26)?,
                        hoa_fee_cents: row.get(27)?,
                        created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                        updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    })
                },
            )
            .optional()
            .context("load house profile")
    }

    pub fn create_house_profile(&self, profile: &HouseProfileInput) -> Result<HouseProfileId> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM house_profiles", [], |row| row.get(0))
            .context("count existing house profiles")?;
        if count > 0 {
            bail!("house profile already exists -- edit the existing profile instead");
        }

        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO house_profiles (
                  nickname, address_line_1, address_line_2, city, state, postal_code,
                  year_built, square_feet, lot_square_feet, bedrooms, bathrooms,
                  foundation_type, wiring_type, roof_type, exterior_type,
                  heating_type, cooling_type, water_source, sewer_type, parking_type,
                  basement_type, insurance_carrier, insurance_policy, insurance_renewal,
                  property_tax_cents, hoa_name, hoa_fee_cents, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    profile.nickname,
                    profile.address_line_1,
                    profile.address_line_2,
                    profile.city,
                    profile.state,
                    profile.postal_code,
                    profile.year_built,
                    profile.square_feet,
                    profile.lot_square_feet,
                    profile.bedrooms,
                    profile.bathrooms,
                    profile.foundation_type,
                    profile.wiring_type,
                    profile.roof_type,
                    profile.exterior_type,
                    profile.heating_type,
                    profile.cooling_type,
                    profile.water_source,
                    profile.sewer_type,
                    profile.parking_type,
                    profile.basement_type,
                    profile.insurance_carrier,
                    profile.insurance_policy,
                    profile.insurance_renewal.map(format_date),
                    profile.property_tax_cents,
                    profile.hoa_name,
                    profile.hoa_fee_cents,
                    now,
                    now,
                ],
            )
            .context("insert house profile")?;
        Ok(HouseProfileId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_house_profile(&self, profile: &HouseProfileInput) -> Result<()> {
        let house_profile_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM house_profiles ORDER BY id ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("load existing house profile id")?;
        let Some(house_profile_id) = house_profile_id else {
            bail!("house profile not found -- create one before updating");
        };

        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE house_profiles
                SET
                  nickname = ?,
                  address_line_1 = ?,
                  address_line_2 = ?,
                  city = ?,
                  state = ?,
                  postal_code = ?,
                  year_built = ?,
                  square_feet = ?,
                  lot_square_feet = ?,
                  bedrooms = ?,
                  bathrooms = ?,
                  foundation_type = ?,
                  wiring_type = ?,
                  roof_type = ?,
                  exterior_type = ?,
                  heating_type = ?,
                  cooling_type = ?,
                  water_source = ?,
                  sewer_type = ?,
                  parking_type = ?,
                  basement_type = ?,
                  insurance_carrier = ?,
                  insurance_policy = ?,
                  insurance_renewal = ?,
                  property_tax_cents = ?,
                  hoa_name = ?,
                  hoa_fee_cents = ?,
                  updated_at = ?
                WHERE id = ?
                ",
                params![
                    profile.nickname,
                    profile.address_line_1,
                    profile.address_line_2,
                    profile.city,
                    profile.state,
                    profile.postal_code,
                    profile.year_built,
                    profile.square_feet,
                    profile.lot_square_feet,
                    profile.bedrooms,
                    profile.bathrooms,
                    profile.foundation_type,
                    profile.wiring_type,
                    profile.roof_type,
                    profile.exterior_type,
                    profile.heating_type,
                    profile.cooling_type,
                    profile.water_source,
                    profile.sewer_type,
                    profile.parking_type,
                    profile.basement_type,
                    profile.insurance_carrier,
                    profile.insurance_policy,
                    profile.insurance_renewal.map(format_date),
                    profile.property_tax_cents,
                    profile.hoa_name,
                    profile.hoa_fee_cents,
                    now,
                    house_profile_id,
                ],
            )
            .context("update house profile")?;
        if rows_affected == 0 {
            bail!("house profile update failed -- retry after reloading the database");
        }
        Ok(())
    }

    pub fn upsert_house_profile(&self, profile: &HouseProfileInput) -> Result<HouseProfileId> {
        if let Some(existing) = self.get_house_profile()? {
            self.update_house_profile(profile)?;
            return Ok(existing.id);
        }
        self.create_house_profile(profile)
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

    pub fn soft_delete(&self, target: LifecycleEntityRef) -> Result<()> {
        self.ensure_can_soft_delete(target)?;
        self.soft_delete_entity(target.kind(), target.id())
    }

    pub fn restore(&self, target: LifecycleEntityRef) -> Result<()> {
        self.ensure_can_restore(target)?;
        self.restore_entity(target.kind(), target.id())
    }

    pub fn soft_delete_project(&self, project_id: ProjectId) -> Result<()> {
        self.soft_delete(LifecycleEntityRef::Project(project_id))
    }

    pub fn restore_project(&self, project_id: ProjectId) -> Result<()> {
        self.restore(LifecycleEntityRef::Project(project_id))
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
        self.soft_delete(LifecycleEntityRef::Vendor(vendor_id))
    }

    pub fn restore_vendor(&self, vendor_id: VendorId) -> Result<()> {
        self.restore(LifecycleEntityRef::Vendor(vendor_id))
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
        self.require_parent_alive(ParentEntityRef::Project(quote.project_id))?;
        self.require_parent_alive(ParentEntityRef::Vendor(quote.vendor_id))?;

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
        self.require_parent_alive(ParentEntityRef::Project(update.project_id))?;
        self.require_parent_alive(ParentEntityRef::Vendor(update.vendor_id))?;

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
        self.soft_delete(LifecycleEntityRef::Quote(quote_id))
    }

    pub fn restore_quote(&self, quote_id: QuoteId) -> Result<()> {
        self.restore(LifecycleEntityRef::Quote(quote_id))
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
        self.soft_delete(LifecycleEntityRef::Appliance(appliance_id))
    }

    pub fn restore_appliance(&self, appliance_id: ApplianceId) -> Result<()> {
        self.restore(LifecycleEntityRef::Appliance(appliance_id))
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
            self.require_parent_alive(ParentEntityRef::Appliance(appliance_id))?;
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
            self.require_parent_alive(ParentEntityRef::Appliance(appliance_id))?;
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
        self.soft_delete(LifecycleEntityRef::MaintenanceItem(maintenance_id))
    }

    pub fn restore_maintenance_item(&self, maintenance_id: MaintenanceItemId) -> Result<()> {
        self.restore(LifecycleEntityRef::MaintenanceItem(maintenance_id))
    }

    pub fn list_service_log_entries(&self, include_deleted: bool) -> Result<Vec<ServiceLogEntry>> {
        let mut sql = String::from(
            "
            SELECT
              id, maintenance_item_id, serviced_at, vendor_id, cost_cents, notes,
              created_at, updated_at, deleted_at
            FROM service_log_entries
            ",
        );
        if !include_deleted {
            sql.push_str("WHERE deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY serviced_at DESC, id DESC");

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare service log query")?;
        let rows = stmt
            .query_map([], |row| {
                let serviced_at_raw: String = row.get(2)?;
                let vendor_id: Option<i64> = row.get(3)?;
                let created_at_raw: String = row.get(6)?;
                let updated_at_raw: String = row.get(7)?;
                let deleted_at_raw: Option<String> = row.get(8)?;

                Ok(ServiceLogEntry {
                    id: ServiceLogEntryId::new(row.get(0)?),
                    maintenance_item_id: MaintenanceItemId::new(row.get(1)?),
                    serviced_at: parse_date(&serviced_at_raw).map_err(to_sql_error)?,
                    vendor_id: vendor_id.map(VendorId::new),
                    cost_cents: row.get(4)?,
                    notes: row.get(5)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query service log entries")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect service log entries")
    }

    pub fn list_service_log_for_maintenance(
        &self,
        maintenance_id: MaintenanceItemId,
        include_deleted: bool,
    ) -> Result<Vec<ServiceLogEntry>> {
        let mut sql = String::from(
            "
            SELECT
              id, maintenance_item_id, serviced_at, vendor_id, cost_cents, notes,
              created_at, updated_at, deleted_at
            FROM service_log_entries
            WHERE maintenance_item_id = ?
            ",
        );
        if !include_deleted {
            sql.push_str("AND deleted_at IS NULL\n");
        }
        sql.push_str("ORDER BY serviced_at DESC, id DESC");

        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("prepare maintenance service log query")?;
        let rows = stmt
            .query_map(params![maintenance_id.get()], |row| {
                let serviced_at_raw: String = row.get(2)?;
                let vendor_id: Option<i64> = row.get(3)?;
                let created_at_raw: String = row.get(6)?;
                let updated_at_raw: String = row.get(7)?;
                let deleted_at_raw: Option<String> = row.get(8)?;

                Ok(ServiceLogEntry {
                    id: ServiceLogEntryId::new(row.get(0)?),
                    maintenance_item_id: MaintenanceItemId::new(row.get(1)?),
                    serviced_at: parse_date(&serviced_at_raw).map_err(to_sql_error)?,
                    vendor_id: vendor_id.map(VendorId::new),
                    cost_cents: row.get(4)?,
                    notes: row.get(5)?,
                    created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                    updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                    deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                })
            })
            .context("query maintenance service log entries")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect maintenance service log entries")
    }

    pub fn get_service_log_entry(&self, entry_id: ServiceLogEntryId) -> Result<ServiceLogEntry> {
        self.conn
            .query_row(
                "
                SELECT
                  id, maintenance_item_id, serviced_at, vendor_id, cost_cents, notes,
                  created_at, updated_at, deleted_at
                FROM service_log_entries
                WHERE id = ?
                ",
                params![entry_id.get()],
                |row| {
                    let serviced_at_raw: String = row.get(2)?;
                    let vendor_id: Option<i64> = row.get(3)?;
                    let created_at_raw: String = row.get(6)?;
                    let updated_at_raw: String = row.get(7)?;
                    let deleted_at_raw: Option<String> = row.get(8)?;

                    Ok(ServiceLogEntry {
                        id: ServiceLogEntryId::new(row.get(0)?),
                        maintenance_item_id: MaintenanceItemId::new(row.get(1)?),
                        serviced_at: parse_date(&serviced_at_raw).map_err(to_sql_error)?,
                        vendor_id: vendor_id.map(VendorId::new),
                        cost_cents: row.get(4)?,
                        notes: row.get(5)?,
                        created_at: parse_datetime(&created_at_raw).map_err(to_sql_error)?,
                        updated_at: parse_datetime(&updated_at_raw).map_err(to_sql_error)?,
                        deleted_at: parse_opt_datetime(deleted_at_raw).map_err(to_sql_error)?,
                    })
                },
            )
            .with_context(|| format!("load service log entry {}", entry_id.get()))
    }

    pub fn create_service_log_entry(
        &self,
        entry: &NewServiceLogEntry,
    ) -> Result<ServiceLogEntryId> {
        self.require_parent_alive(ParentEntityRef::MaintenanceItem(entry.maintenance_item_id))?;
        if let Some(vendor_id) = entry.vendor_id {
            self.require_parent_alive(ParentEntityRef::Vendor(vendor_id))?;
        }

        let now = now_rfc3339()?;
        self.conn
            .execute(
                "
                INSERT INTO service_log_entries (
                  maintenance_item_id, serviced_at, vendor_id, cost_cents, notes,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    entry.maintenance_item_id.get(),
                    format_date(entry.serviced_at),
                    entry.vendor_id.map(VendorId::get),
                    entry.cost_cents,
                    entry.notes,
                    now,
                    now,
                ],
            )
            .context("insert service log entry")?;
        Ok(ServiceLogEntryId::new(self.conn.last_insert_rowid()))
    }

    pub fn update_service_log_entry(
        &self,
        entry_id: ServiceLogEntryId,
        update: &UpdateServiceLogEntry,
    ) -> Result<()> {
        self.require_parent_alive(ParentEntityRef::MaintenanceItem(update.maintenance_item_id))?;
        if let Some(vendor_id) = update.vendor_id {
            self.require_parent_alive(ParentEntityRef::Vendor(vendor_id))?;
        }

        let now = now_rfc3339()?;
        let rows_affected = self
            .conn
            .execute(
                "
                UPDATE service_log_entries
                SET
                  maintenance_item_id = ?,
                  serviced_at = ?,
                  vendor_id = ?,
                  cost_cents = ?,
                  notes = ?,
                  updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                ",
                params![
                    update.maintenance_item_id.get(),
                    format_date(update.serviced_at),
                    update.vendor_id.map(VendorId::get),
                    update.cost_cents,
                    update.notes,
                    now,
                    entry_id.get(),
                ],
            )
            .context("update service log entry")?;
        if rows_affected == 0 {
            bail!(
                "service log entry {} not found or deleted -- choose an existing entry and retry",
                entry_id.get()
            );
        }
        Ok(())
    }

    pub fn soft_delete_service_log_entry(&self, entry_id: ServiceLogEntryId) -> Result<()> {
        self.soft_delete(LifecycleEntityRef::ServiceLogEntry(entry_id))
    }

    pub fn restore_service_log_entry(&self, entry_id: ServiceLogEntryId) -> Result<()> {
        self.restore(LifecycleEntityRef::ServiceLogEntry(entry_id))
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
            self.require_parent_alive(ParentEntityRef::Appliance(appliance_id))?;
        }
        if let Some(vendor_id) = incident.vendor_id {
            self.require_parent_alive(ParentEntityRef::Vendor(vendor_id))?;
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
            self.require_parent_alive(ParentEntityRef::Appliance(appliance_id))?;
        }
        if let Some(vendor_id) = update.vendor_id {
            self.require_parent_alive(ParentEntityRef::Vendor(vendor_id))?;
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
        self.soft_delete(LifecycleEntityRef::Incident(incident_id))
    }

    pub fn restore_incident(&self, incident_id: IncidentId) -> Result<()> {
        self.restore(LifecycleEntityRef::Incident(incident_id))
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

    pub fn list_maintenance_with_schedule(&self) -> Result<Vec<MaintenanceItem>> {
        let mut items = self.list_maintenance_items(false)?;
        items.retain(|item| item.interval_months > 0);
        Ok(items)
    }

    pub fn list_active_projects(&self) -> Result<Vec<Project>> {
        let mut projects = self.list_projects(false)?;
        projects.retain(|project| {
            matches!(
                project.status,
                ProjectStatus::Underway | ProjectStatus::Delayed
            )
        });
        Ok(projects)
    }

    pub fn list_open_incidents(&self) -> Result<Vec<Incident>> {
        let mut incidents = self.list_incidents(false)?;
        incidents.retain(|incident| {
            matches!(
                incident.status,
                IncidentStatus::Open | IncidentStatus::InProgress
            )
        });
        incidents.sort_by(|left, right| {
            severity_rank(left.severity)
                .cmp(&severity_rank(right.severity))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(incidents)
    }

    pub fn list_expiring_warranties(
        &self,
        now: Date,
        look_back_days: i64,
        horizon_days: i64,
    ) -> Result<Vec<Appliance>> {
        if look_back_days < 0 {
            bail!("look_back_days must be non-negative, got {look_back_days}");
        }
        if horizon_days < 0 {
            bail!("horizon_days must be non-negative, got {horizon_days}");
        }

        let from = now - time::Duration::days(look_back_days);
        let to = now + time::Duration::days(horizon_days);

        let mut appliances = self.list_appliances(false)?;
        appliances.retain(|appliance| {
            appliance
                .warranty_expiry
                .is_some_and(|warranty| warranty >= from && warranty <= to)
        });
        appliances.sort_by(|left, right| {
            left.warranty_expiry
                .cmp(&right.warranty_expiry)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(appliances)
    }

    pub fn list_recent_service_logs(&self, limit: usize) -> Result<Vec<ServiceLogEntry>> {
        let mut logs = self.list_service_log_entries(false)?;
        logs.truncate(limit);
        Ok(logs)
    }

    pub fn ytd_service_spend_cents(&self, year_start: Date) -> Result<i64> {
        let total: i64 = self
            .conn
            .query_row(
                "
                SELECT COALESCE(SUM(cost_cents), 0)
                FROM service_log_entries
                WHERE deleted_at IS NULL
                  AND serviced_at >= ?
                ",
                params![format_date(year_start)],
                |row| row.get(0),
            )
            .context("sum year-to-date service spend")?;
        Ok(total)
    }

    pub fn total_project_spend_cents(&self) -> Result<i64> {
        let total: i64 = self
            .conn
            .query_row(
                "
                SELECT COALESCE(SUM(actual_cents), 0)
                FROM projects
                WHERE deleted_at IS NULL
                ",
                [],
                |row| row.get(0),
            )
            .context("sum total project spend")?;
        Ok(total)
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

        // Refresh mtime on every access so TTL eviction treats active files as in-use.
        let cache_hit = match fs::metadata(&cache_path) {
            Ok(metadata) => metadata.len() == u64::try_from(size_bytes).unwrap_or(0),
            Err(_) => false,
        };

        if cache_hit {
            // stdlib has no portable chtime API, so rewrite to refresh mtime.
            fs::write(&cache_path, &data)
                .with_context(|| format!("refresh cache file {}", cache_path.display()))?;
            set_private_permissions(&cache_path)?;
            return Ok(cache_path);
        }

        fs::write(&cache_path, &data)
            .with_context(|| format!("write cache file {}", cache_path.display()))?;
        set_private_permissions(&cache_path)?;

        Ok(cache_path)
    }

    fn get_setting_raw(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .with_context(|| format!("read setting {key}"))
    }

    fn put_setting_raw(&self, key: &str, value: &str) -> Result<()> {
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

    pub fn get_setting(&self, key: SettingKey) -> Result<Option<SettingValue>> {
        let raw = self.get_setting_raw(key.as_str())?;
        raw.map(|value| {
            SettingValue::parse_for_key(key, &value).ok_or_else(|| {
                anyhow!(
                    "setting `{}` has invalid value `{}`; run `micasa --check`, then set a valid value in Settings",
                    key.as_str(),
                    value
                )
            })
        })
        .transpose()
    }

    pub fn put_setting(&self, key: SettingKey, value: SettingValue) -> Result<()> {
        let raw = value.to_storage(key).ok_or_else(|| {
            anyhow!(
                "setting `{}` expected {:?} value; reopen Settings and choose a valid option",
                key.as_str(),
                key.expected_value_kind()
            )
        })?;
        self.put_setting_raw(key.as_str(), &raw)
    }

    pub fn list_settings(&self) -> Result<Vec<AppSetting>> {
        let mut settings = Vec::with_capacity(SettingKey::ALL.len());
        for key in SettingKey::ALL {
            let value = self
                .get_setting(key)?
                .unwrap_or_else(|| default_setting_value(key));
            settings.push(AppSetting { key, value });
        }
        Ok(settings)
    }

    pub fn get_last_model(&self) -> Result<Option<String>> {
        match self.get_setting(SettingKey::LlmModel)? {
            Some(SettingValue::Text(value)) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_owned()))
                }
            }
            Some(SettingValue::Bool(_)) => bail!(
                "setting `{}` must be text; open Settings and choose a model name",
                SettingKey::LlmModel.as_str()
            ),
            None => Ok(None),
        }
    }

    pub fn put_last_model(&self, model: &str) -> Result<()> {
        self.put_setting(SettingKey::LlmModel, SettingValue::Text(model.to_owned()))
    }

    pub fn get_show_dashboard(&self) -> Result<bool> {
        match self.get_setting(SettingKey::UiShowDashboard)? {
            Some(SettingValue::Bool(value)) => Ok(value),
            Some(SettingValue::Text(_)) => bail!(
                "setting `{}` must be on/off; open Settings and toggle it",
                SettingKey::UiShowDashboard.as_str()
            ),
            None => Ok(true),
        }
    }

    pub fn get_show_dashboard_override(&self) -> Result<Option<bool>> {
        match self.get_setting(SettingKey::UiShowDashboard)? {
            Some(SettingValue::Bool(value)) => Ok(Some(value)),
            Some(SettingValue::Text(_)) => bail!(
                "setting `{}` must be on/off; open Settings and toggle it",
                SettingKey::UiShowDashboard.as_str()
            ),
            None => Ok(None),
        }
    }

    pub fn put_show_dashboard(&self, show: bool) -> Result<()> {
        self.put_setting(SettingKey::UiShowDashboard, SettingValue::Bool(show))
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

    fn count_active_dependents(&self, relation: DependentRelation, parent_id: i64) -> Result<i64> {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE {} = ? AND deleted_at IS NULL",
            relation.table(),
            relation.fk_column()
        );
        self.conn
            .query_row(&sql, params![parent_id], |row| row.get(0))
            .with_context(|| {
                format!(
                    "count dependents in {} for {}={parent_id}",
                    relation.table(),
                    relation.fk_column()
                )
            })
    }

    fn ensure_can_soft_delete(&self, target: LifecycleEntityRef) -> Result<()> {
        match target {
            LifecycleEntityRef::Project(project_id) => {
                let quote_count = self
                    .count_active_dependents(DependentRelation::ProjectQuotes, project_id.get())
                    .context("count quotes linked to project")?;
                if quote_count > 0 {
                    bail!(
                        "cannot delete project {} because {quote_count} quote(s) reference it; delete quotes first",
                        project_id.get()
                    );
                }
            }
            LifecycleEntityRef::Vendor(vendor_id) => {
                let quote_count = self
                    .count_active_dependents(DependentRelation::VendorQuotes, vendor_id.get())
                    .context("count quotes linked to vendor")?;
                if quote_count > 0 {
                    bail!(
                        "vendor {} has {quote_count} active quote(s) -- delete quotes first",
                        vendor_id.get()
                    );
                }

                let incident_count = self
                    .count_active_dependents(DependentRelation::VendorIncidents, vendor_id.get())
                    .context("count incidents linked to vendor")?;
                if incident_count > 0 {
                    bail!(
                        "vendor {} has {incident_count} active incident(s) -- delete incidents first",
                        vendor_id.get()
                    );
                }

                let service_log_count = self
                    .count_active_dependents(
                        DependentRelation::VendorServiceLogEntries,
                        vendor_id.get(),
                    )
                    .context("count service logs linked to vendor")?;
                if service_log_count > 0 {
                    bail!(
                        "vendor {} has {service_log_count} active service log(s) -- delete service logs first",
                        vendor_id.get()
                    );
                }
            }
            LifecycleEntityRef::Appliance(appliance_id) => {
                let maintenance_count = self
                    .count_active_dependents(
                        DependentRelation::ApplianceMaintenanceItems,
                        appliance_id.get(),
                    )
                    .context("count maintenance items linked to appliance")?;
                if maintenance_count > 0 {
                    bail!(
                        "appliance {} has {maintenance_count} active maintenance item(s) -- delete or reassign them first",
                        appliance_id.get()
                    );
                }

                let incident_count = self
                    .count_active_dependents(
                        DependentRelation::ApplianceIncidents,
                        appliance_id.get(),
                    )
                    .context("count incidents linked to appliance")?;
                if incident_count > 0 {
                    bail!(
                        "appliance {} has {incident_count} active incident(s) -- delete incidents first",
                        appliance_id.get()
                    );
                }
            }
            LifecycleEntityRef::MaintenanceItem(maintenance_id) => {
                let service_count = self
                    .count_active_dependents(
                        DependentRelation::MaintenanceItemServiceLogEntries,
                        maintenance_id.get(),
                    )
                    .context("count service logs linked to maintenance item")?;
                if service_count > 0 {
                    bail!(
                        "maintenance item {} has {service_count} service log(s) -- delete service logs first",
                        maintenance_id.get()
                    );
                }
            }
            LifecycleEntityRef::Quote(_)
            | LifecycleEntityRef::ServiceLogEntry(_)
            | LifecycleEntityRef::Incident(_) => {}
        }
        Ok(())
    }

    fn ensure_can_restore(&self, target: LifecycleEntityRef) -> Result<()> {
        match target {
            LifecycleEntityRef::Quote(quote_id) => {
                let (project_id, vendor_id): (i64, i64) = self
                    .conn
                    .query_row(
                        "SELECT project_id, vendor_id FROM quotes WHERE id = ?",
                        params![quote_id.get()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .with_context(|| format!("load quote {}", quote_id.get()))?;
                self.require_parent_alive(ParentEntityRef::Project(ProjectId::new(project_id)))?;
                self.require_parent_alive(ParentEntityRef::Vendor(VendorId::new(vendor_id)))?;
            }
            LifecycleEntityRef::MaintenanceItem(maintenance_id) => {
                let appliance_id: Option<i64> = self
                    .conn
                    .query_row(
                        "SELECT appliance_id FROM maintenance_items WHERE id = ?",
                        params![maintenance_id.get()],
                        |row| row.get(0),
                    )
                    .with_context(|| format!("load maintenance item {}", maintenance_id.get()))?;
                if let Some(appliance_id) = appliance_id {
                    self.require_parent_alive(ParentEntityRef::Appliance(ApplianceId::new(
                        appliance_id,
                    )))?;
                }
            }
            LifecycleEntityRef::ServiceLogEntry(entry_id) => {
                let (maintenance_item_id, vendor_id): (i64, Option<i64>) = self
                    .conn
                    .query_row(
                        "SELECT maintenance_item_id, vendor_id FROM service_log_entries WHERE id = ?",
                        params![entry_id.get()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .with_context(|| format!("load service log entry {}", entry_id.get()))?;
                self.require_parent_alive(ParentEntityRef::MaintenanceItem(
                    MaintenanceItemId::new(maintenance_item_id),
                ))?;
                if let Some(vendor_id) = vendor_id {
                    self.require_parent_alive(ParentEntityRef::Vendor(VendorId::new(vendor_id)))?;
                }
            }
            LifecycleEntityRef::Incident(incident_id) => {
                let (appliance_id, vendor_id): (Option<i64>, Option<i64>) = self
                    .conn
                    .query_row(
                        "SELECT appliance_id, vendor_id FROM incidents WHERE id = ?",
                        params![incident_id.get()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .with_context(|| format!("load incident {}", incident_id.get()))?;
                if let Some(appliance_id) = appliance_id {
                    self.require_parent_alive(ParentEntityRef::Appliance(ApplianceId::new(
                        appliance_id,
                    )))?;
                }
                if let Some(vendor_id) = vendor_id {
                    self.require_parent_alive(ParentEntityRef::Vendor(VendorId::new(vendor_id)))?;
                }
            }
            LifecycleEntityRef::Project(_)
            | LifecycleEntityRef::Vendor(_)
            | LifecycleEntityRef::Appliance(_) => {}
        }
        Ok(())
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

    fn require_parent_alive(&self, parent: ParentEntityRef) -> Result<()> {
        let parent_kind = parent.kind();
        let parent_id = parent.id();
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

fn value_ref_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned(),
        ValueRef::Blob(value) => format!("{value:?}"),
    }
}

fn severity_rank(severity: IncidentSeverity) -> i32 {
    match severity {
        IncidentSeverity::Urgent => 0,
        IncidentSeverity::Soon => 1,
        IncidentSeverity::Whenever => 2,
    }
}

fn is_noise_column(column: &str) -> bool {
    matches!(
        column.to_ascii_lowercase().as_str(),
        "id" | "created_at" | "updated_at" | "deleted_at" | "data"
    )
}

fn format_column_value(column: &str, value: &str) -> String {
    if column.to_ascii_lowercase().ends_with("_cents")
        && let Ok(cents) = value.parse::<i64>()
    {
        let dollars = (cents as f64) / 100.0;
        let label = column.strip_suffix("_cents").unwrap_or(column);
        return format!("{label}: ${dollars:.2}");
    }
    format!("{column}: {value}")
}

fn is_safe_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn contains_word(source: &str, keyword: &str) -> bool {
    let bytes = source.as_bytes();
    let keyword_len = keyword.len();
    if keyword_len == 0 || keyword_len > bytes.len() {
        return false;
    }

    let mut index = 0usize;
    while let Some(offset) = source[index..].find(keyword) {
        let start = index + offset;
        let end = start + keyword_len;
        let left_ok = start == 0 || !is_identifier_char(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_identifier_char(bytes[end]);
        if left_ok && right_ok {
            return true;
        }
        index = start + 1;
    }
    false
}

fn is_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
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

fn ensure_required_indexes(conn: &Connection) -> Result<()> {
    for index in REQUIRED_INDEXES {
        conn.execute_batch(index.create_sql)
            .with_context(|| format!("ensure required index `{}`", index.name))?;
    }

    let existing_indexes = index_names(conn)?;
    let missing = REQUIRED_INDEXES
        .iter()
        .filter(|index| !existing_indexes.contains(index.name))
        .map(|index| index.name)
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "database is missing required indexes: {}; run migration before launching",
            missing.join(", ")
        );
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

fn index_names(conn: &Connection) -> Result<BTreeSet<String>> {
    let mut stmt = conn
        .prepare(
            "
            SELECT name
            FROM sqlite_master
            WHERE type = 'index'
              AND name NOT LIKE 'sqlite_%'
            ORDER BY name ASC
            ",
        )
        .context("prepare index names query")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .context("query index names")?;
    rows.collect::<rusqlite::Result<BTreeSet<_>>>()
        .context("collect index names")
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

fn default_setting_value(key: SettingKey) -> SettingValue {
    match key {
        SettingKey::UiShowDashboard => SettingValue::Bool(true),
        SettingKey::LlmModel => SettingValue::Text(String::new()),
    }
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

#[cfg(test)]
mod tests {
    use super::Store;
    use anyhow::Result;
    use micasa_app::{SettingKey, SettingValue};

    #[test]
    fn list_settings_returns_typed_defaults() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let settings = store.list_settings()?;
        assert_eq!(settings.len(), 2);
        assert_eq!(settings[0].key, SettingKey::UiShowDashboard);
        assert_eq!(settings[0].value, SettingValue::Bool(true));
        assert_eq!(settings[1].key, SettingKey::LlmModel);
        assert_eq!(settings[1].value, SettingValue::Text(String::new()));
        Ok(())
    }

    #[test]
    fn typed_settings_round_trip() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        store.put_show_dashboard(false)?;
        store.put_last_model("qwen3:32b")?;

        assert!(!store.get_show_dashboard()?);
        assert_eq!(store.get_last_model()?.as_deref(), Some("qwen3:32b"));

        let settings = store.list_settings()?;
        assert!(
            settings
                .iter()
                .any(|setting| setting.key == SettingKey::UiShowDashboard
                    && setting.value == SettingValue::Bool(false))
        );
        assert!(
            settings
                .iter()
                .any(|setting| setting.key == SettingKey::LlmModel
                    && setting.value == SettingValue::Text("qwen3:32b".to_owned()))
        );
        Ok(())
    }

    #[test]
    fn invalid_bool_setting_is_actionable() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        store.put_setting_raw(SettingKey::UiShowDashboard.as_str(), "maybe")?;
        let error = store
            .get_show_dashboard()
            .expect_err("invalid bool should be rejected");
        assert!(error
            .to_string()
            .contains("set a valid value in Settings"));
        Ok(())
    }
}

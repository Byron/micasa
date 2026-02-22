// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::Result;
use micasa_app::{
    DocumentEntityKind, IncidentSeverity, IncidentStatus, ProjectStatus, SettingKey, SettingValue,
};
use micasa_db::{
    HouseProfileInput, LifecycleEntityRef, NewAppliance, NewDocument, NewIncident,
    NewMaintenanceItem, NewProject, NewQuote, NewServiceLogEntry, NewVendor, SeedSummary, Store,
    UpdateAppliance, UpdateDocument, UpdateIncident, UpdateMaintenanceItem, UpdateProject,
    UpdateQuote, UpdateServiceLogEntry, UpdateVendor, default_db_path, document_cache_dir,
    evict_stale_cache, validate_db_path,
};
use std::collections::BTreeSet;
use std::fs;
use std::time::{Duration, SystemTime};
use time::{Date, Month};

fn index_exists(store: &Store, name: &str) -> Result<bool> {
    let exists: i64 = store.raw_connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?)",
        rusqlite::params![name],
        |row| row.get(0),
    )?;
    Ok(exists == 1)
}

fn days_ago(days: u64) -> SystemTime {
    SystemTime::now()
        .checked_sub(Duration::from_secs(days * 24 * 60 * 60))
        .expect("system clock should support subtraction")
}

fn house_profile_input(nickname: &str, city: &str) -> HouseProfileInput {
    HouseProfileInput {
        nickname: nickname.to_owned(),
        address_line_1: "123 Test St".to_owned(),
        address_line_2: String::new(),
        city: city.to_owned(),
        state: "OR".to_owned(),
        postal_code: "97201".to_owned(),
        year_built: Some(1999),
        square_feet: Some(1800),
        lot_square_feet: Some(5000),
        bedrooms: Some(3),
        bathrooms: Some(2.0),
        foundation_type: "Slab".to_owned(),
        wiring_type: "Copper".to_owned(),
        roof_type: "Asphalt".to_owned(),
        exterior_type: "Siding".to_owned(),
        heating_type: "Gas".to_owned(),
        cooling_type: "Central".to_owned(),
        water_source: "Municipal".to_owned(),
        sewer_type: "Municipal".to_owned(),
        parking_type: "Driveway".to_owned(),
        basement_type: "None".to_owned(),
        insurance_carrier: "Carrier".to_owned(),
        insurance_policy: "POL-123".to_owned(),
        insurance_renewal: Some(Date::from_calendar_date(2026, Month::July, 1).expect("date")),
        property_tax_cents: Some(300_000),
        hoa_name: String::new(),
        hoa_fee_cents: None,
    }
}

#[test]
fn validate_db_path_rejects_uri_forms() {
    assert!(validate_db_path("file:test.db").is_err());
    assert!(validate_db_path("https://example.com/db.sqlite").is_err());
    assert!(validate_db_path("db.sqlite?mode=ro").is_err());
    assert!(validate_db_path("/tmp/micasa.db").is_ok());
}

#[test]
fn validate_db_path_accepts_and_rejects_expected_cases() {
    let cases = [
        (":memory:", true, ""),
        ("/home/user/micasa.db", true, ""),
        ("relative/path.db", true, ""),
        ("./local.db", true, ""),
        ("../parent/db.sqlite", true, ""),
        ("/tmp/micasa test.db", true, ""),
        ("C:\\Users\\me\\micasa.db", true, ""),
        ("https://evil.com/db", false, "looks like a URI"),
        ("http://localhost/db", false, "looks like a URI"),
        ("ftp://files.example.com/data.db", false, "looks like a URI"),
        ("file://localhost/tmp/test.db", false, "looks like a URI"),
        ("file:/tmp/test.db", false, "file: URI syntax"),
        ("file:test.db", false, "file: URI syntax"),
        ("file:test.db?mode=ro", false, "file: URI syntax"),
        (
            "/tmp/test.db?_pragma=journal_mode(wal)",
            false,
            "contains '?'",
        ),
        ("test.db?cache=shared", false, "contains '?'"),
        ("", false, "must not be empty"),
        ("/path/with://in/middle", true, ""),
        ("123://not-a-scheme", true, ""),
    ];

    for (path, valid, needle) in cases {
        let outcome = validate_db_path(path);
        if valid {
            assert!(
                outcome.is_ok(),
                "expected {path:?} to be accepted, got {outcome:?}"
            );
        } else {
            let error = outcome.expect_err("path should be rejected");
            assert!(
                error.to_string().contains(needle),
                "expected error for {path:?} to contain {needle:?}, got {error:#}"
            );
        }
    }
}

#[test]
fn validate_db_path_rejects_url_like_inputs() {
    let candidates = [
        "https://example.com/micasa.db",
        "http://localhost:8080/db.sqlite",
        "ftp://files.example.com/data.db",
        "file://localhost/tmp/test.db",
        "ssh://remote.example.com/var/db.sqlite",
        "ws://localhost/socket",
        "wss://localhost/socket",
        "mongodb://localhost:27017/micasa",
        "postgres://localhost/micasa",
        "mysql://localhost:3306/micasa",
    ];
    for candidate in candidates {
        assert!(
            validate_db_path(candidate).is_err(),
            "validate_db_path({candidate:?}) should reject URL-like paths"
        );
    }
}

#[test]
fn store_open_rejects_uri_paths() {
    let candidates = [
        "https://evil.example/db",
        "file:test.db",
        "http://localhost/db.sqlite",
        "postgres://localhost/micasa",
    ];

    for candidate in candidates {
        let outcome = Store::open(std::path::Path::new(candidate));
        assert!(
            outcome.is_err(),
            "Store::open({candidate:?}) should reject URI-style input"
        );
    }
}

#[test]
fn default_db_path_uses_env_override_or_micasa_suffix() -> Result<()> {
    let configured = std::env::var("MICASA_DB_PATH").ok();
    let path = default_db_path()?;

    if let Some(raw) = configured {
        assert_eq!(path, std::path::PathBuf::from(raw));
    } else {
        assert_eq!(
            path.file_name().and_then(std::ffi::OsStr::to_str),
            Some("micasa.db")
        );
    }

    Ok(())
}

#[test]
fn bootstrap_creates_schema_and_seed_defaults() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_types = store.list_project_types()?;
    let categories = store.list_maintenance_categories()?;

    assert!(!project_types.is_empty());
    assert!(!categories.is_empty());
    assert!(
        project_types.iter().any(|pt| pt.name == "Plumbing"),
        "expected default project type"
    );
    assert!(
        categories.iter().any(|c| c.name == "Safety"),
        "expected default maintenance category"
    );
    Ok(())
}

#[test]
fn bootstrap_rejects_schema_missing_required_column() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    store.raw_connection().execute_batch(
        "
            ALTER TABLE projects RENAME TO projects_old;
            CREATE TABLE projects (
              id INTEGER PRIMARY KEY,
              title TEXT NOT NULL,
              project_type_id INTEGER NOT NULL,
              description TEXT NOT NULL DEFAULT '',
              start_date TEXT,
              end_date TEXT,
              budget_cents INTEGER,
              actual_cents INTEGER,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            DROP TABLE projects_old;
            ",
    )?;

    let err = store
        .bootstrap()
        .expect_err("schema validation should fail");
    let message = err.to_string();
    assert!(message.contains("table `projects` is missing required columns"));
    assert!(message.contains("status"));
    Ok(())
}

#[test]
fn bootstrap_recreates_missing_required_index() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    store
        .raw_connection()
        .execute_batch("DROP INDEX idx_quotes_vendor_id;")?;
    assert!(!index_exists(&store, "idx_quotes_vendor_id")?);

    store.bootstrap()?;
    assert!(index_exists(&store, "idx_quotes_vendor_id")?);
    Ok(())
}

#[test]
fn bootstrap_accepts_go_schema_fixture_db() -> Result<()> {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("go-schema-v1.db");
    assert!(
        fixture_path.exists(),
        "expected fixture at {}",
        fixture_path.display()
    );

    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("go-schema-v1-copy.db");
    fs::copy(&fixture_path, &db_path)?;

    let store = Store::open(&db_path)?;
    store.bootstrap()?;

    let project_types = store.list_project_types()?;
    let categories = store.list_maintenance_categories()?;
    assert!(!project_types.is_empty());
    assert!(!categories.is_empty());
    assert!(
        project_types.iter().any(|entry| entry.name == "Plumbing"),
        "fixture should include seeded project types"
    );
    assert!(
        categories.iter().any(|entry| entry.name == "Safety"),
        "fixture should include seeded maintenance categories"
    );
    Ok(())
}

#[test]
fn sqlite_pragmas_are_configured_on_open_and_reopen() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("pragma.db");

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;

        let foreign_keys: i64 =
            store
                .raw_connection()
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        assert_eq!(foreign_keys, 1);

        let journal_mode: String =
            store
                .raw_connection()
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        assert!(journal_mode.eq_ignore_ascii_case("wal"));

        let synchronous: i64 =
            store
                .raw_connection()
                .query_row("PRAGMA synchronous", [], |row| row.get(0))?;
        assert_eq!(synchronous, 1);

        let busy_timeout: i64 =
            store
                .raw_connection()
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        assert_eq!(busy_timeout, 5_000);
    }

    {
        let store = Store::open(&db_path)?;
        let foreign_keys: i64 =
            store
                .raw_connection()
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        assert_eq!(foreign_keys, 1);

        let journal_mode: String =
            store
                .raw_connection()
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        assert!(journal_mode.eq_ignore_ascii_case("wal"));
    }

    Ok(())
}

#[test]
fn query_api_validates_identifiers_and_caps_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let names = store.table_names()?;
    assert!(names.iter().any(|name| name == "projects"));

    let columns = store.table_columns("projects")?;
    assert!(columns.iter().any(|column| column.name == "status"));
    let invalid = store.table_columns("projects;DROP TABLE projects");
    assert!(invalid.is_err());

    for idx in 0..220 {
        store.append_chat_input(&format!("prompt-{idx}"))?;
    }

    let (query_columns, rows) =
        store.read_only_query("SELECT id FROM chat_inputs ORDER BY id ASC")?;
    assert_eq!(query_columns, vec!["id".to_owned()]);
    assert_eq!(rows.len(), 200);

    assert!(
        store
            .read_only_query("SELECT * FROM projects; SELECT * FROM vendors")
            .is_err()
    );
    assert!(
        store
            .read_only_query("UPDATE projects SET title = 'x'")
            .is_err()
    );

    Ok(())
}

#[test]
fn table_names_include_core_tables_and_exclude_sqlite_internals() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let names = store.table_names()?;
    assert!(names.iter().any(|name| name == "house_profiles"));
    assert!(names.iter().any(|name| name == "projects"));
    assert!(names.iter().any(|name| name == "vendors"));
    assert!(names.iter().any(|name| name == "maintenance_items"));
    assert!(names.iter().any(|name| name == "appliances"));
    for name in names {
        assert!(
            !name.contains("sqlite_"),
            "unexpected internal table {name}"
        );
    }
    Ok(())
}

#[test]
fn table_columns_include_primary_key_metadata() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let columns = store.table_columns("projects")?;
    assert!(!columns.is_empty());

    let id_col = columns
        .iter()
        .find(|column| column.name == "id")
        .expect("projects.id should exist");
    assert!(id_col.primary_key > 0);
    Ok(())
}

#[test]
fn table_columns_invalid_name_is_rejected_with_actionable_message() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let error = store
        .table_columns("'; DROP TABLE projects; --")
        .expect_err("invalid identifier should fail");
    assert!(error.to_string().contains("invalid table name"));
    Ok(())
}

#[test]
fn read_only_query_rejects_attach_and_pragma_keywords() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let attach_error = store
        .read_only_query("SELECT * FROM (SELECT 1) ATTACH DATABASE '/tmp/x' AS x")
        .expect_err("attach should be rejected");
    assert!(
        attach_error
            .to_string()
            .contains("disallowed keyword: ATTACH")
    );

    let pragma_error = store
        .read_only_query(
            "SELECT * FROM pragma_table_info('projects') WHERE 1=1 PRAGMA journal_mode",
        )
        .expect_err("pragma keyword should be rejected");
    assert!(
        pragma_error
            .to_string()
            .contains("disallowed keyword: PRAGMA")
    );
    Ok(())
}

#[test]
fn read_only_query_rejects_insert_statement() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let error = store
        .read_only_query("INSERT INTO projects (title) VALUES ('hack')")
        .expect_err("insert should be rejected");
    assert!(
        error
            .to_string()
            .contains("only SELECT queries are allowed")
    );
    Ok(())
}

#[test]
fn read_only_query_rejects_delete_statement() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let error = store
        .read_only_query("DELETE FROM projects WHERE id = 1")
        .expect_err("delete should be rejected");
    assert!(
        error
            .to_string()
            .contains("only SELECT queries are allowed")
    );
    Ok(())
}

#[test]
fn read_only_query_rejects_multi_statement_queries() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let error = store
        .read_only_query("SELECT * FROM projects; DROP TABLE projects")
        .expect_err("multiple statements should be rejected");
    assert!(
        error
            .to_string()
            .contains("multiple statements are not allowed")
    );
    Ok(())
}

#[test]
fn read_only_query_rejects_empty_query() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let error = store
        .read_only_query("")
        .expect_err("empty query should fail");
    assert!(error.to_string().contains("empty query"));
    Ok(())
}

#[test]
fn read_only_query_allows_deleted_at_identifier() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let (columns, _rows) =
        store.read_only_query("SELECT id FROM projects WHERE deleted_at IS NULL LIMIT 1")?;
    assert_eq!(columns, vec!["id".to_owned()]);
    Ok(())
}

#[test]
fn read_only_query_select_returns_expected_columns_and_row_count() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let (columns, rows) =
        store.read_only_query("SELECT name FROM project_types ORDER BY name LIMIT 3")?;
    assert_eq!(columns, vec!["name".to_owned()]);
    assert_eq!(rows.len(), 3);
    Ok(())
}

#[test]
fn data_dump_and_column_hints_skip_deleted_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let keep_id = store.create_project(&NewProject {
        title: "Keep Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: Some(12_345),
    })?;
    let remove_id = store.create_project(&NewProject {
        title: "Remove Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Abandoned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: Some(22_222),
    })?;
    store.soft_delete_project(remove_id)?;

    let dump = store.data_dump();
    assert!(dump.contains("title: Keep Project"));
    assert!(!dump.contains("title: Remove Project"));

    let hints = store.column_hints();
    assert!(hints.contains("project statuses (stored values)"));
    assert!(hints.contains("planned"));

    let keep = store.get_project(keep_id)?;
    assert_eq!(keep.title, "Keep Project");
    Ok(())
}

#[test]
fn data_dump_includes_row_headers_and_bullets() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let dump = store.data_dump();
    assert!(!dump.is_empty());
    assert!(dump.contains("rows)"));
    assert!(dump.contains("- "));
    Ok(())
}

#[test]
fn column_hints_on_unpopulated_db_omit_vendor_names() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let hints = store.column_hints();
    assert!(!hints.contains("vendor names"));
    Ok(())
}

#[test]
fn list_projects_uses_deterministic_tiebreaker() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let first = store.create_project(&NewProject {
        title: "A".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let second = store.create_project(&NewProject {
        title: "B".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    store.raw_connection().execute(
        "UPDATE projects SET updated_at = ? WHERE id IN (?, ?)",
        rusqlite::params!["2026-02-19T12:00:00Z", first.get(), second.get()],
    )?;

    let projects = store.list_projects(false)?;
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].id, second);
    assert_eq!(projects[1].id, first);
    Ok(())
}

#[test]
fn dashboard_query_helpers_filter_and_summarize() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let underway_id = store.create_project(&NewProject {
        title: "Underway".to_owned(),
        project_type_id,
        status: ProjectStatus::Underway,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: Some(10_000),
    })?;
    let delayed_id = store.create_project(&NewProject {
        title: "Delayed".to_owned(),
        project_type_id,
        status: ProjectStatus::Delayed,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: Some(20_000),
    })?;
    store.create_project(&NewProject {
        title: "Completed".to_owned(),
        project_type_id,
        status: ProjectStatus::Completed,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: Some(30_000),
    })?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_active = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Scheduled".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "No schedule".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 0,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let warranty_in = Date::from_calendar_date(2026, Month::January, 20)?;
    let warranty_out = Date::from_calendar_date(2026, Month::April, 1)?;
    let appliance_in = store.create_appliance(&NewAppliance {
        name: "Washer".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: Some(warranty_in),
        location: "Laundry".to_owned(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_appliance(&NewAppliance {
        name: "Old unit".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: Some(warranty_out),
        location: "Garage".to_owned(),
        cost_cents: None,
        notes: String::new(),
    })?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Tech".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_active,
        serviced_at: Date::from_calendar_date(2025, Month::December, 25)?,
        vendor_id: Some(vendor_id),
        cost_cents: Some(4_000),
        notes: String::new(),
    })?;
    store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_active,
        serviced_at: Date::from_calendar_date(2026, Month::January, 10)?,
        vendor_id: Some(vendor_id),
        cost_cents: Some(6_000),
        notes: String::new(),
    })?;

    store.create_incident(&NewIncident {
        title: "Urgent leak".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 5)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: Some(appliance_in),
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;
    store.create_incident(&NewIncident {
        title: "Soon issue".to_owned(),
        description: String::new(),
        status: IncidentStatus::InProgress,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::January, 6)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: Some(appliance_in),
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;
    store.create_incident(&NewIncident {
        title: "Closed issue".to_owned(),
        description: String::new(),
        status: IncidentStatus::Resolved,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 7)?,
        date_resolved: Some(Date::from_calendar_date(2026, Month::January, 8)?),
        location: String::new(),
        cost_cents: None,
        appliance_id: Some(appliance_in),
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;

    let active_projects = store.list_active_projects()?;
    assert_eq!(active_projects.len(), 2);
    assert!(
        active_projects
            .iter()
            .any(|project| project.id == underway_id)
    );
    assert!(
        active_projects
            .iter()
            .any(|project| project.id == delayed_id)
    );

    let scheduled = store.list_maintenance_with_schedule()?;
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0].id, maintenance_active);

    let open_incidents = store.list_open_incidents()?;
    assert_eq!(open_incidents.len(), 2);
    assert_eq!(open_incidents[0].severity, IncidentSeverity::Urgent);

    let expiring = store.list_expiring_warranties(
        Date::from_calendar_date(2026, Month::January, 15)?,
        30,
        30,
    )?;
    assert_eq!(expiring.len(), 1);
    assert_eq!(expiring[0].id, appliance_in);

    let recent_logs = store.list_recent_service_logs(1)?;
    assert_eq!(recent_logs.len(), 1);
    assert_eq!(
        recent_logs[0].serviced_at,
        Date::from_calendar_date(2026, Month::January, 10)?
    );

    let ytd = store.ytd_service_spend_cents(Date::from_calendar_date(2026, Month::January, 1)?)?;
    assert_eq!(ytd, 6_000);
    assert_eq!(store.total_project_spend_cents()?, 60_000);

    Ok(())
}

#[test]
fn dashboard_counts_tracks_due_projects_maintenance_and_open_incidents() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    store.create_project(&NewProject {
        title: "Planned".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    store.create_project(&NewProject {
        title: "Underway".to_owned(),
        project_type_id,
        status: ProjectStatus::Underway,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    store.create_project(&NewProject {
        title: "Completed".to_owned(),
        project_type_id,
        status: ProjectStatus::Completed,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    let category_id = store.list_maintenance_categories()?[0].id;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "No service date".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "Clearly due".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: Some(Date::from_calendar_date(2020, Month::January, 1)?),
        interval_months: 1,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "Not due".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: Some(Date::from_calendar_date(2099, Month::January, 1)?),
        interval_months: 12,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    store.create_incident(&NewIncident {
        title: "Open".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::January, 2)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    store.create_incident(&NewIncident {
        title: "In progress".to_owned(),
        description: String::new(),
        status: IncidentStatus::InProgress,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 2)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    store.create_incident(&NewIncident {
        title: "Resolved".to_owned(),
        description: String::new(),
        status: IncidentStatus::Resolved,
        severity: IncidentSeverity::Whenever,
        date_noticed: Date::from_calendar_date(2026, Month::January, 2)?,
        date_resolved: Some(Date::from_calendar_date(2026, Month::January, 3)?),
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;

    let counts = store.dashboard_counts()?;
    assert_eq!(counts.projects_due, 2);
    assert_eq!(counts.maintenance_due, 2);
    assert_eq!(counts.incidents_open, 2);
    Ok(())
}

#[test]
fn total_project_spend_unaffected_by_project_edits() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Kitchen Remodel".to_owned(),
        project_type_id,
        status: ProjectStatus::Completed,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: Some(50_000),
    })?;

    store.raw_connection().execute(
        "UPDATE projects SET updated_at = ? WHERE id = ?",
        rusqlite::params!["2024-06-01T00:00:00Z", project_id.get()],
    )?;
    let before = store.total_project_spend_cents()?;
    assert_eq!(before, 50_000);

    let mut project = store.get_project(project_id)?;
    project.description = "added new countertops".to_owned();
    store.update_project(
        project_id,
        &UpdateProject {
            title: project.title,
            project_type_id: project.project_type_id,
            status: project.status,
            description: project.description,
            start_date: project.start_date,
            end_date: project.end_date,
            budget_cents: project.budget_cents,
            actual_cents: project.actual_cents,
        },
    )?;

    let after = store.total_project_spend_cents()?;
    assert_eq!(after, before);
    Ok(())
}

#[test]
fn list_expiring_warranties_respects_lookback_and_lookahead_windows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    store.create_appliance(&NewAppliance {
        name: "Soon".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: Some(Date::from_calendar_date(2026, Month::March, 10)?),
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_appliance(&NewAppliance {
        name: "Recent".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: Some(Date::from_calendar_date(2026, Month::January, 29)?),
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_appliance(&NewAppliance {
        name: "Old".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: Some(Date::from_calendar_date(2025, Month::December, 1)?),
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_appliance(&NewAppliance {
        name: "Far".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: Some(Date::from_calendar_date(2026, Month::June, 8)?),
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_appliance(&NewAppliance {
        name: "None".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;

    let expiring = store.list_expiring_warranties(
        Date::from_calendar_date(2026, Month::February, 8)?,
        30,
        90,
    )?;
    let names = expiring
        .into_iter()
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Recent", "Soon"]);
    Ok(())
}

#[test]
fn list_recent_service_logs_returns_latest_first_with_limit() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_item_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "SL Item".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let months = [
        Month::January,
        Month::February,
        Month::March,
        Month::April,
        Month::May,
        Month::June,
        Month::July,
        Month::August,
        Month::September,
        Month::October,
    ];
    for month in months {
        store.create_service_log_entry(&NewServiceLogEntry {
            maintenance_item_id,
            serviced_at: Date::from_calendar_date(2025, month, 1)?,
            vendor_id: None,
            cost_cents: None,
            notes: String::new(),
        })?;
    }

    let logs = store.list_recent_service_logs(5)?;
    assert_eq!(logs.len(), 5);
    assert_eq!(
        logs[0].serviced_at,
        Date::from_calendar_date(2025, Month::October, 1)?
    );
    assert_eq!(
        logs[4].serviced_at,
        Date::from_calendar_date(2025, Month::June, 1)?
    );
    Ok(())
}

#[test]
fn list_open_incidents_prioritizes_severity_and_skips_deleted() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let urgent_id = store.create_incident(&NewIncident {
        title: "Urgent leak".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 5)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    let whenever_id = store.create_incident(&NewIncident {
        title: "Cracked tile".to_owned(),
        description: String::new(),
        status: IncidentStatus::InProgress,
        severity: IncidentSeverity::Whenever,
        date_noticed: Date::from_calendar_date(2026, Month::January, 6)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    let deleted_id = store.create_incident(&NewIncident {
        title: "Fixed fence".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::January, 7)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    store.soft_delete_incident(deleted_id)?;

    let incidents = store.list_open_incidents()?;
    assert_eq!(incidents.len(), 2);
    assert_eq!(incidents[0].id, urgent_id);
    assert_eq!(incidents[1].id, whenever_id);
    Ok(())
}

#[test]
fn document_blob_round_trip_and_cache_extract() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let payload = b"invoice placeholder".to_vec();
    let document_id = store.insert_document(&NewDocument {
        title: "Invoice".to_owned(),
        file_name: "invoice.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: 7,
        mime_type: "application/pdf".to_owned(),
        data: payload.clone(),
        notes: String::new(),
    })?;

    let from_db = store.get_document(document_id)?;
    assert_eq!(from_db.data, payload);
    assert_eq!(from_db.entity_kind, DocumentEntityKind::Project);

    let extracted_path = store.extract_document(document_id)?;
    assert!(extracted_path.exists());
    let extracted = std::fs::read(extracted_path)?;
    assert_eq!(extracted, from_db.data);
    Ok(())
}

#[test]
fn document_cache_extract_refreshes_existing_cache_file() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let payload = b"123456789".to_vec();
    let document_id = store.insert_document(&NewDocument {
        title: "Manual".to_owned(),
        file_name: "manual.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Appliance,
        entity_id: 4,
        mime_type: "application/pdf".to_owned(),
        data: payload.clone(),
        notes: String::new(),
    })?;

    let extracted_path = store.extract_document(document_id)?;
    fs::write(&extracted_path, b"xxxxxxxxx")?;

    let extracted_again = store.extract_document(document_id)?;
    assert_eq!(extracted_again, extracted_path);
    let refreshed = fs::read(extracted_again)?;
    assert_eq!(refreshed, payload);
    Ok(())
}

#[test]
fn insert_document_rejects_oversized_payload() -> Result<()> {
    let mut store = Store::open_memory()?;
    store.bootstrap()?;
    store.set_max_document_size(4)?;

    let error = store
        .insert_document(&NewDocument {
            title: "Too big".to_owned(),
            file_name: "big.bin".to_owned(),
            entity_kind: DocumentEntityKind::Project,
            entity_id: 1,
            mime_type: "application/octet-stream".to_owned(),
            data: vec![1, 2, 3, 4, 5],
            notes: String::new(),
        })
        .expect_err("oversized document should be rejected");
    assert!(error.to_string().contains("max allowed"));
    assert!(error.to_string().contains("shrink the file and retry"));
    Ok(())
}

#[test]
fn extract_document_fails_actionably_for_empty_blob() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let document_id = store.insert_document(&NewDocument {
        title: "Empty".to_owned(),
        file_name: "empty.bin".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: 1,
        mime_type: "application/octet-stream".to_owned(),
        data: Vec::new(),
        notes: String::new(),
    })?;

    let error = store
        .extract_document(document_id)
        .expect_err("empty blob should not be extractable");
    assert!(error.to_string().contains("has no content"));
    Ok(())
}

#[test]
fn deleting_project_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Docs-linked project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    let document_id = store.insert_document(&NewDocument {
        title: "Scope".to_owned(),
        file_name: "scope.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: project_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: b"scope".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_project(project_id)?;
    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_id, project_id.get());
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::Project);
    Ok(())
}

#[test]
fn deleting_appliance_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Garage freezer".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: "Garage".to_owned(),
        cost_cents: None,
        notes: String::new(),
    })?;

    let document_id = store.insert_document(&NewDocument {
        title: "Manual".to_owned(),
        file_name: "freezer-manual.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Appliance,
        entity_id: appliance_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: b"manual".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_appliance(appliance_id)?;
    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_id, appliance_id.get());
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::Appliance);
    Ok(())
}

#[test]
fn deleting_vendor_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Local Electric".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let document_id = store.insert_document(&NewDocument {
        title: "Contract".to_owned(),
        file_name: "contract.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Vendor,
        entity_id: vendor_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: b"contract".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_vendor(vendor_id)?;
    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_id, vendor_id.get());
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::Vendor);
    Ok(())
}

#[test]
fn deleting_quote_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Quote parent".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Quote vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 42_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let document_id = store.insert_document(&NewDocument {
        title: "Quote PDF".to_owned(),
        file_name: "quote.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Quote,
        entity_id: quote_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: b"quote".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_quote(quote_id)?;
    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_id, quote_id.get());
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::Quote);
    Ok(())
}

#[test]
fn deleting_maintenance_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Sump pump check".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 12,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let document_id = store.insert_document(&NewDocument {
        title: "Maintenance checklist".to_owned(),
        file_name: "checklist.txt".to_owned(),
        entity_kind: DocumentEntityKind::Maintenance,
        entity_id: maintenance_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"checklist".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_maintenance_item(maintenance_id)?;
    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_id, maintenance_id.get());
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::Maintenance);
    Ok(())
}

#[test]
fn deleting_service_log_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Drain line flush".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::May, 1)?,
        vendor_id: None,
        cost_cents: None,
        notes: String::new(),
    })?;

    let document_id = store.insert_document(&NewDocument {
        title: "Service receipt".to_owned(),
        file_name: "receipt.pdf".to_owned(),
        entity_kind: DocumentEntityKind::ServiceLog,
        entity_id: service_log_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: b"receipt".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_service_log_entry(service_log_id)?;
    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_id, service_log_id.get());
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::ServiceLog);
    Ok(())
}

#[test]
fn settings_round_trip_and_defaults_via_generic_api() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    assert_eq!(store.get_setting(SettingKey::UiShowDashboard)?, None);
    assert_eq!(store.get_setting(SettingKey::LlmModel)?, None);

    let defaults = store.list_settings()?;
    assert!(
        defaults
            .iter()
            .any(|setting| setting.key == SettingKey::UiShowDashboard
                && setting.value == SettingValue::Bool(true))
    );
    assert!(
        defaults
            .iter()
            .any(|setting| setting.key == SettingKey::LlmModel
                && setting.value == SettingValue::Text(String::new()))
    );

    store.put_setting(SettingKey::UiShowDashboard, SettingValue::Bool(false))?;
    store.put_setting(
        SettingKey::LlmModel,
        SettingValue::Text("qwen3:32b".to_owned()),
    )?;

    assert_eq!(
        store.get_setting(SettingKey::UiShowDashboard)?,
        Some(SettingValue::Bool(false))
    );
    assert_eq!(
        store.get_setting(SettingKey::LlmModel)?,
        Some(SettingValue::Text("qwen3:32b".to_owned()))
    );

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
fn put_setting_rejects_wrong_value_kind() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let bool_as_text = store
        .put_setting(
            SettingKey::UiShowDashboard,
            SettingValue::Text("off".to_owned()),
        )
        .expect_err("bool setting should reject text value");
    assert!(bool_as_text.to_string().contains("expected Bool value"));

    let text_as_bool = store
        .put_setting(SettingKey::LlmModel, SettingValue::Bool(true))
        .expect_err("text setting should reject bool value");
    assert!(text_as_bool.to_string().contains("expected Text value"));
    Ok(())
}

#[test]
fn show_dashboard_override_reports_presence_correctly() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    assert_eq!(store.get_show_dashboard_override()?, None);
    store.put_show_dashboard(false)?;
    assert_eq!(store.get_show_dashboard_override()?, Some(false));
    store.put_show_dashboard(true)?;
    assert_eq!(store.get_show_dashboard_override()?, Some(true));
    Ok(())
}

#[test]
fn last_model_defaults_to_none_and_round_trips() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    assert_eq!(store.get_last_model()?, None);

    store.put_last_model("qwen3:8b")?;
    assert_eq!(store.get_last_model()?.as_deref(), Some("qwen3:8b"));

    store.put_last_model("llama3.3")?;
    assert_eq!(store.get_last_model()?.as_deref(), Some("llama3.3"));
    Ok(())
}

#[test]
fn show_dashboard_defaults_true_and_round_trips() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    assert!(store.get_show_dashboard()?);

    store.put_show_dashboard(false)?;
    assert!(!store.get_show_dashboard()?);

    store.put_show_dashboard(true)?;
    assert!(store.get_show_dashboard()?);
    Ok(())
}

#[test]
fn model_setting_persists_across_reopen() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("settings-persist.db");

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        store.put_last_model("qwen3:8b")?;
    }

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        assert_eq!(store.get_last_model()?.as_deref(), Some("qwen3:8b"));
    }
    Ok(())
}

#[test]
fn show_dashboard_setting_persists_across_reopen() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("dashboard-setting.db");

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        store.put_show_dashboard(false)?;
    }

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        assert!(!store.get_show_dashboard()?);
    }
    Ok(())
}

#[test]
fn chat_history_is_empty_by_default() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let history = store.load_chat_history()?;
    assert!(history.is_empty());
    Ok(())
}

#[test]
fn chat_history_persists_across_reopen() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("chat-history.db");

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        store.append_chat_input("how many projects?")?;
        store.append_chat_input("oldest appliance?")?;
    }

    {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        let history = store
            .load_chat_history()?
            .into_iter()
            .map(|entry| entry.input)
            .collect::<Vec<_>>();
        assert_eq!(history, vec!["how many projects?", "oldest appliance?"]);
    }
    Ok(())
}

#[test]
fn chat_history_allows_non_consecutive_duplicates() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    store.append_chat_input("a")?;
    store.append_chat_input("b")?;
    store.append_chat_input("a")?;

    let history = store
        .load_chat_history()?
        .into_iter()
        .map(|entry| entry.input)
        .collect::<Vec<_>>();
    assert_eq!(history, vec!["a", "b", "a"]);
    Ok(())
}

#[test]
fn chat_history_deduplicates_and_caps_size() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    store.append_chat_input("same")?;
    store.append_chat_input("same")?;
    assert_eq!(store.load_chat_history()?.len(), 1);

    for idx in 0..210 {
        store.append_chat_input(&format!("prompt-{idx}"))?;
    }

    let history = store.load_chat_history()?;
    assert_eq!(history.len(), 200);
    let first = history.first().expect("history should not be empty");
    assert!(first.input.starts_with("prompt-"));
    Ok(())
}

#[test]
fn cache_eviction_handles_empty_dir() -> Result<()> {
    let dir = document_cache_dir()?;
    let removed = evict_stale_cache(&dir, 0)?;
    assert_eq!(removed, 0);
    Ok(())
}

#[test]
fn cache_eviction_returns_zero_for_nonexistent_dir() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let missing_dir = temp_dir.path().join("missing-cache-dir");

    let removed = evict_stale_cache(&missing_dir, 1)?;
    assert_eq!(removed, 0);
    Ok(())
}

#[test]
fn cache_eviction_skips_subdirectories() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let nested_dir = temp_dir.path().join("nested");
    fs::create_dir(&nested_dir)?;

    let removed = evict_stale_cache(temp_dir.path(), 1)?;
    assert_eq!(removed, 0);
    assert!(nested_dir.exists());
    Ok(())
}

#[test]
fn cache_eviction_rejects_overflowing_ttl_days() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let error =
        evict_stale_cache(temp_dir.path(), i64::MAX).expect_err("overflowing ttl_days should fail");
    assert!(error.to_string().contains("ttl_days is too large"));
    Ok(())
}

#[test]
fn cache_eviction_removes_stale_files() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let fresh_path = temp_dir.path().join("fresh.txt");
    let stale_path = temp_dir.path().join("stale.txt");
    fs::write(&fresh_path, b"fresh")?;
    fs::write(&stale_path, b"stale")?;

    let stale_file = fs::OpenOptions::new().write(true).open(&stale_path)?;
    stale_file.set_modified(days_ago(40))?;

    let removed = evict_stale_cache(temp_dir.path(), 30)?;
    assert_eq!(removed, 1);
    assert!(fresh_path.exists());
    assert!(!stale_path.exists());
    Ok(())
}

#[test]
fn cache_eviction_keeps_recent_files_within_ttl() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let recent_path = temp_dir.path().join("recent.txt");
    fs::write(&recent_path, b"keep")?;

    let recent_file = fs::OpenOptions::new().write(true).open(&recent_path)?;
    recent_file.set_modified(days_ago(29))?;

    let removed = evict_stale_cache(temp_dir.path(), 30)?;
    assert_eq!(removed, 0);
    assert!(recent_path.exists());
    Ok(())
}

#[test]
fn cache_eviction_zero_ttl_disables_removal_even_for_old_files() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let old_path = temp_dir.path().join("old.txt");
    fs::write(&old_path, b"old")?;

    let old_file = fs::OpenOptions::new().write(true).open(&old_path)?;
    old_file.set_modified(days_ago(365))?;

    let removed = evict_stale_cache(temp_dir.path(), 0)?;
    assert_eq!(removed, 0);
    assert!(old_path.exists());
    Ok(())
}

#[test]
fn cache_eviction_empty_path_is_noop() -> Result<()> {
    let removed = evict_stale_cache(std::path::Path::new(""), 30)?;
    assert_eq!(removed, 0);
    Ok(())
}

#[test]
fn vendor_crud_and_delete_guards() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Vendor A".to_owned(),
        contact_name: "Alice".to_owned(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let vendors = store.list_vendors(false)?;
    assert_eq!(vendors.len(), 1);
    assert_eq!(vendors[0].id, vendor_id);

    store.update_vendor(
        vendor_id,
        &UpdateVendor {
            name: "Vendor A+".to_owned(),
            contact_name: "Alice Updated".to_owned(),
            email: "a@example.com".to_owned(),
            phone: "555-0000".to_owned(),
            website: "https://example.com".to_owned(),
            notes: "Preferred".to_owned(),
        },
    )?;
    let vendors = store.list_vendors(false)?;
    assert_eq!(vendors[0].name, "Vendor A+");

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Project for guard".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 25_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let delete_error = store
        .soft_delete_vendor(vendor_id)
        .expect_err("vendor with active quote should not delete");
    assert!(delete_error.to_string().contains("active quote"));

    store.soft_delete_quote(quote_id)?;
    store.soft_delete_vendor(vendor_id)?;
    assert!(store.list_vendors(false)?.is_empty());

    Ok(())
}

#[test]
fn vendor_deletion_record_is_created_and_cleared_on_restore() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Record Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    store.soft_delete_vendor(vendor_id)?;

    let active_count: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM deletion_records WHERE entity = 'vendor' AND target_id = ? AND restored_at IS NULL",
        rusqlite::params![vendor_id.get()],
        |row| row.get(0),
    )?;
    assert_eq!(active_count, 1);

    store.restore_vendor(vendor_id)?;

    let remaining_active: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM deletion_records WHERE entity = 'vendor' AND target_id = ? AND restored_at IS NULL",
        rusqlite::params![vendor_id.get()],
        |row| row.get(0),
    )?;
    assert_eq!(remaining_active, 0);

    let restored_count: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM deletion_records WHERE entity = 'vendor' AND target_id = ? AND restored_at IS NOT NULL",
        rusqlite::params![vendor_id.get()],
        |row| row.get(0),
    )?;
    assert_eq!(restored_count, 1);
    Ok(())
}

#[test]
fn quote_restore_requires_live_parents() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 10_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    store.soft_delete_quote(quote_id)?;
    store.soft_delete_project(project_id)?;

    let restore_error = store
        .restore_quote(quote_id)
        .expect_err("restore should fail when parent project is deleted");
    assert!(restore_error.to_string().contains("project is deleted"));

    store.restore_project(project_id)?;
    store.restore_quote(quote_id)?;
    let quotes = store.list_quotes(false)?;
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].id, quote_id);

    Ok(())
}

#[test]
fn delete_project_blocked_by_active_quotes() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Project with quotes".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Quote Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 33_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let delete_error = store
        .soft_delete_project(project_id)
        .expect_err("project with active quotes should be protected");
    assert!(
        delete_error
            .to_string()
            .contains("quote(s) reference it; delete quotes first")
    );

    store.soft_delete_quote(quote_id)?;
    store.soft_delete_project(project_id)?;
    Ok(())
}

#[test]
fn partial_quote_deletion_still_blocks_project_delete() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Project with multiple quotes".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Shared Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let first_quote = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 20_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: "first".to_owned(),
    })?;
    let second_quote = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 25_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: "second".to_owned(),
    })?;

    store.soft_delete_quote(first_quote)?;
    let delete_error = store
        .soft_delete_project(project_id)
        .expect_err("remaining active quote should still block project deletion");
    assert!(
        delete_error
            .to_string()
            .contains("quote(s) reference it; delete quotes first")
    );

    store.soft_delete_quote(second_quote)?;
    store.soft_delete_project(project_id)?;
    Ok(())
}

#[test]
fn restore_quote_blocked_by_deleted_vendor() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Quote parent project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Doomed Quote Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 44_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    store.soft_delete_quote(quote_id)?;
    store.soft_delete_vendor(vendor_id)?;

    let restore_error = store
        .restore_quote(quote_id)
        .expect_err("quote restore should fail while vendor is deleted");
    assert!(restore_error.to_string().contains("vendor is deleted"));

    store.restore_vendor(vendor_id)?;
    store.restore_quote(quote_id)?;
    Ok(())
}

#[test]
fn typed_lifecycle_api_soft_delete_and_restore_project() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Lifecycle API".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    store.soft_delete(LifecycleEntityRef::Project(project_id))?;
    assert!(store.list_projects(false)?.is_empty());
    assert_eq!(store.list_projects(true)?.len(), 1);

    store.restore(LifecycleEntityRef::Project(project_id))?;
    let projects = store.list_projects(false)?;
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, project_id);
    Ok(())
}

#[test]
fn project_deletion_record_is_created_and_cleared_on_restore() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Record Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    store.soft_delete_project(project_id)?;

    let active_count: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM deletion_records WHERE entity = 'project' AND target_id = ? AND restored_at IS NULL",
        rusqlite::params![project_id.get()],
        |row| row.get(0),
    )?;
    assert_eq!(active_count, 1);

    store.restore_project(project_id)?;

    let remaining_active: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM deletion_records WHERE entity = 'project' AND target_id = ? AND restored_at IS NULL",
        rusqlite::params![project_id.get()],
        |row| row.get(0),
    )?;
    assert_eq!(remaining_active, 0);

    let restored_count: i64 = store.raw_connection().query_row(
        "SELECT COUNT(*) FROM deletion_records WHERE entity = 'project' AND target_id = ? AND restored_at IS NOT NULL",
        rusqlite::params![project_id.get()],
        |row| row.get(0),
    )?;
    assert_eq!(restored_count, 1);
    Ok(())
}

#[test]
fn typed_lifecycle_api_restore_guard_for_quote_parent() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Parent guard".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Guard Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 50_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    store.soft_delete(LifecycleEntityRef::Quote(quote_id))?;
    store.soft_delete(LifecycleEntityRef::Project(project_id))?;

    let error = store
        .restore(LifecycleEntityRef::Quote(quote_id))
        .expect_err("quote restore should fail when parent project is deleted");
    assert!(error.to_string().contains("project is deleted"));
    Ok(())
}

#[test]
fn appliance_and_maintenance_delete_restore_flow() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Dryer".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: "Laundry".to_owned(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Clean vent".to_owned(),
        category_id,
        appliance_id: Some(appliance_id),
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let delete_error = store
        .soft_delete_appliance(appliance_id)
        .expect_err("appliance should be protected while maintenance is active");
    assert!(delete_error.to_string().contains("maintenance item"));

    store.soft_delete_maintenance_item(maintenance_id)?;
    store.soft_delete_appliance(appliance_id)?;

    let restore_maintenance_error = store
        .restore_maintenance_item(maintenance_id)
        .expect_err("maintenance restore should fail when appliance deleted");
    assert!(
        restore_maintenance_error
            .to_string()
            .contains("appliance is deleted")
    );

    store.restore_appliance(appliance_id)?;
    store.restore_maintenance_item(maintenance_id)?;

    let maintenance = store.list_maintenance_items(false)?;
    assert_eq!(maintenance.len(), 1);
    assert_eq!(maintenance[0].id, maintenance_id);
    Ok(())
}

#[test]
fn restore_maintenance_item_allowed_without_appliance_link() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Gutter cleaning".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    store.soft_delete_maintenance_item(maintenance_id)?;
    store.restore_maintenance_item(maintenance_id)?;

    let items = store.list_maintenance_items(false)?;
    assert!(items.iter().any(|item| item.id == maintenance_id));
    Ok(())
}

#[test]
fn incident_crud_and_restore_parent_guards() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Dishwasher".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: "Kitchen".to_owned(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Repair Co".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Leak".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 10)?,
        date_resolved: None,
        location: "Kitchen".to_owned(),
        cost_cents: Some(8_000),
        appliance_id: Some(appliance_id),
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;

    let incidents = store.list_incidents(false)?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].id, incident_id);

    store.soft_delete_incident(incident_id)?;
    store.soft_delete_vendor(vendor_id)?;

    let restore_error = store
        .restore_incident(incident_id)
        .expect_err("incident restore should fail when vendor is deleted");
    assert!(restore_error.to_string().contains("vendor is deleted"));

    store.restore_vendor(vendor_id)?;
    store.restore_incident(incident_id)?;
    let incidents = store.list_incidents(false)?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].id, incident_id);
    Ok(())
}

#[test]
fn incident_update_persists_fields_and_optional_parent_links() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Water Heater".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Fixers".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let incident_id = store.create_incident(&NewIncident {
        title: "Leak".to_owned(),
        description: "Initial".to_owned(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::January, 3)?,
        date_resolved: None,
        location: "Garage".to_owned(),
        cost_cents: None,
        appliance_id: Some(appliance_id),
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;

    store.update_incident(
        incident_id,
        &UpdateIncident {
            title: "Leak fixed".to_owned(),
            description: "Resolved and tested".to_owned(),
            status: IncidentStatus::Resolved,
            severity: IncidentSeverity::Whenever,
            date_noticed: Date::from_calendar_date(2026, Month::January, 3)?,
            date_resolved: Some(Date::from_calendar_date(2026, Month::January, 6)?),
            location: "Garage".to_owned(),
            cost_cents: Some(12_500),
            appliance_id: None,
            vendor_id: None,
            notes: "completed".to_owned(),
        },
    )?;

    let updated = store
        .list_incidents(false)?
        .into_iter()
        .find(|incident| incident.id == incident_id)
        .expect("updated incident should be present");
    assert_eq!(updated.title, "Leak fixed");
    assert_eq!(updated.description, "Resolved and tested");
    assert_eq!(updated.status, IncidentStatus::Resolved);
    assert_eq!(updated.severity, IncidentSeverity::Whenever);
    assert_eq!(
        updated.date_resolved,
        Some(Date::from_calendar_date(2026, Month::January, 6)?)
    );
    assert_eq!(updated.cost_cents, Some(12_500));
    assert_eq!(updated.appliance_id, None);
    assert_eq!(updated.vendor_id, None);
    assert_eq!(updated.notes, "completed");
    Ok(())
}

#[test]
fn incident_update_rejects_deleted_rows_with_actionable_error() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Trip".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Whenever,
        date_noticed: Date::from_calendar_date(2026, Month::January, 1)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    store.soft_delete_incident(incident_id)?;

    let error = store
        .update_incident(
            incident_id,
            &UpdateIncident {
                title: "Updated".to_owned(),
                description: String::new(),
                status: IncidentStatus::Open,
                severity: IncidentSeverity::Whenever,
                date_noticed: Date::from_calendar_date(2026, Month::January, 1)?,
                date_resolved: None,
                location: String::new(),
                cost_cents: None,
                appliance_id: None,
                vendor_id: None,
                notes: String::new(),
            },
        )
        .expect_err("updating a deleted incident should fail");
    assert!(error.to_string().contains("not found or deleted"));
    Ok(())
}

#[test]
fn incident_restore_blocked_by_deleted_appliance() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Doomed Washer".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Washer leak".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 10)?,
        date_resolved: None,
        location: "Laundry".to_owned(),
        cost_cents: None,
        appliance_id: Some(appliance_id),
        vendor_id: None,
        notes: String::new(),
    })?;

    store.soft_delete_incident(incident_id)?;
    store.soft_delete_appliance(appliance_id)?;

    let restore_error = store
        .restore_incident(incident_id)
        .expect_err("incident restore should fail when appliance is deleted");
    assert!(restore_error.to_string().contains("appliance is deleted"));

    store.restore_appliance(appliance_id)?;
    store.restore_incident(incident_id)?;
    Ok(())
}

#[test]
fn incident_restore_blocked_by_deleted_vendor() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Doomed Exterminator".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Termites".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::January, 10)?,
        date_resolved: None,
        location: "Basement".to_owned(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;

    store.soft_delete_incident(incident_id)?;
    store.soft_delete_vendor(vendor_id)?;

    let restore_error = store
        .restore_incident(incident_id)
        .expect_err("incident restore should fail when vendor is deleted");
    assert!(restore_error.to_string().contains("vendor is deleted"));

    store.restore_vendor(vendor_id)?;
    store.restore_incident(incident_id)?;
    Ok(())
}

#[test]
fn delete_vendor_blocked_by_active_incident() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Busy Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let incident_id = store.create_incident(&NewIncident {
        title: "Clogged drain".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::February, 12)?,
        date_resolved: None,
        location: "Kitchen".to_owned(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: Some(vendor_id),
        notes: String::new(),
    })?;

    let delete_error = store
        .soft_delete_vendor(vendor_id)
        .expect_err("vendor with active incidents should be protected");
    assert!(delete_error.to_string().contains("active incident"));

    store.soft_delete_incident(incident_id)?;
    store.soft_delete_vendor(vendor_id)?;
    Ok(())
}

#[test]
fn delete_appliance_blocked_by_active_incident() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Busy Fridge".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let incident_id = store.create_incident(&NewIncident {
        title: "Fridge leaking".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Urgent,
        date_noticed: Date::from_calendar_date(2026, Month::March, 3)?,
        date_resolved: None,
        location: "Kitchen".to_owned(),
        cost_cents: None,
        appliance_id: Some(appliance_id),
        vendor_id: None,
        notes: String::new(),
    })?;

    let delete_error = store
        .soft_delete_appliance(appliance_id)
        .expect_err("appliance with active incidents should be protected");
    assert!(delete_error.to_string().contains("active incident"));

    store.soft_delete_incident(incident_id)?;
    store.soft_delete_appliance(appliance_id)?;
    Ok(())
}

#[test]
fn incident_restore_allowed_without_appliance_or_vendor() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Loose trim".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Whenever,
        date_noticed: Date::from_calendar_date(2026, Month::April, 9)?,
        date_resolved: None,
        location: "Hallway".to_owned(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;

    store.soft_delete_incident(incident_id)?;
    store.restore_incident(incident_id)?;

    let incidents = store.list_incidents(false)?;
    assert!(incidents.iter().any(|incident| incident.id == incident_id));
    Ok(())
}

#[test]
fn deleting_incident_with_documents_is_allowed_and_preserves_document_rows() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Leaky pipe".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::April, 15)?,
        date_resolved: None,
        location: "Basement".to_owned(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Pipe photo".to_owned(),
        file_name: "pipe.jpg".to_owned(),
        entity_kind: DocumentEntityKind::Incident,
        entity_id: incident_id.get(),
        mime_type: "image/jpeg".to_owned(),
        data: b"jpeg".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_incident(incident_id)?;

    let documents = store.list_documents(false)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, document_id);
    assert_eq!(documents[0].entity_kind, DocumentEntityKind::Incident);
    assert_eq!(documents[0].entity_id, incident_id.get());
    Ok(())
}

#[test]
fn project_update_persists_fields() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Initial".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: Some(120_000),
        actual_cents: None,
    })?;

    store.update_project(
        project_id,
        &UpdateProject {
            title: "Updated".to_owned(),
            project_type_id,
            status: ProjectStatus::Underway,
            description: "In progress".to_owned(),
            start_date: Some(Date::from_calendar_date(2026, Month::January, 1)?),
            end_date: Some(Date::from_calendar_date(2026, Month::March, 1)?),
            budget_cents: Some(150_000),
            actual_cents: Some(90_000),
        },
    )?;

    let project = store.get_project(project_id)?;
    assert_eq!(project.title, "Updated");
    assert_eq!(project.status, ProjectStatus::Underway);
    assert_eq!(project.actual_cents, Some(90_000));
    Ok(())
}

#[test]
fn quote_update_persists_fields() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Quote Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let first_vendor_id = store.create_vendor(&NewVendor {
        name: "Acme Corp".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let replacement_vendor_id = store.create_vendor(&NewVendor {
        name: "Acme Corp 2".to_owned(),
        contact_name: "John Doe".to_owned(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id: first_vendor_id,
        total_cents: 100_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: "initial".to_owned(),
    })?;

    store.update_quote(
        quote_id,
        &UpdateQuote {
            project_id,
            vendor_id: replacement_vendor_id,
            total_cents: 200_000,
            labor_cents: Some(120_000),
            materials_cents: Some(60_000),
            other_cents: Some(20_000),
            received_date: Some(Date::from_calendar_date(2026, Month::March, 5)?),
            notes: "updated".to_owned(),
        },
    )?;

    let quotes = store.list_quotes(false)?;
    let quote = quotes
        .iter()
        .find(|entry| entry.id == quote_id)
        .expect("updated quote should be present");
    assert_eq!(quote.total_cents, 200_000);
    assert_eq!(quote.vendor_id, replacement_vendor_id);
    assert_eq!(quote.labor_cents, Some(120_000));
    assert_eq!(quote.materials_cents, Some(60_000));
    assert_eq!(quote.other_cents, Some(20_000));
    assert_eq!(
        quote.received_date,
        Some(Date::from_calendar_date(2026, Month::March, 5)?)
    );
    assert_eq!(quote.notes, "updated");
    Ok(())
}

#[test]
fn appliance_update_persists_fields() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Fridge".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;

    store.update_appliance(
        appliance_id,
        &UpdateAppliance {
            name: "Fridge".to_owned(),
            brand: "Samsung".to_owned(),
            model_number: "RF28".to_owned(),
            serial_number: "SN-123".to_owned(),
            purchase_date: Some(Date::from_calendar_date(2026, Month::January, 2)?),
            warranty_expiry: Some(Date::from_calendar_date(2028, Month::January, 2)?),
            location: "Kitchen".to_owned(),
            cost_cents: Some(210_000),
            notes: "counter depth".to_owned(),
        },
    )?;

    let appliances = store.list_appliances(false)?;
    let appliance = appliances
        .iter()
        .find(|entry| entry.id == appliance_id)
        .expect("updated appliance should be present");
    assert_eq!(appliance.brand, "Samsung");
    assert_eq!(appliance.model_number, "RF28");
    assert_eq!(appliance.serial_number, "SN-123");
    assert_eq!(appliance.location, "Kitchen");
    assert_eq!(appliance.cost_cents, Some(210_000));
    assert_eq!(appliance.notes, "counter depth");
    assert_eq!(
        appliance.purchase_date,
        Some(Date::from_calendar_date(2026, Month::January, 2)?)
    );
    assert_eq!(
        appliance.warranty_expiry,
        Some(Date::from_calendar_date(2028, Month::January, 2)?)
    );
    Ok(())
}

#[test]
fn maintenance_item_update_persists_fields() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Furnace".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Filter Change".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    store.update_maintenance_item(
        maintenance_id,
        &UpdateMaintenanceItem {
            name: "HVAC Filter Change".to_owned(),
            category_id,
            appliance_id: Some(appliance_id),
            last_serviced_at: Some(Date::from_calendar_date(2026, Month::February, 14)?),
            interval_months: 3,
            manual_url: "https://example.com/manual".to_owned(),
            manual_text: "Steps".to_owned(),
            notes: "quarterly".to_owned(),
            cost_cents: Some(3_500),
        },
    )?;

    let items = store.list_maintenance_items(false)?;
    let item = items
        .iter()
        .find(|entry| entry.id == maintenance_id)
        .expect("updated maintenance item should be present");
    assert_eq!(item.name, "HVAC Filter Change");
    assert_eq!(item.appliance_id, Some(appliance_id));
    assert_eq!(item.interval_months, 3);
    assert_eq!(
        item.last_serviced_at,
        Some(Date::from_calendar_date(2026, Month::February, 14)?)
    );
    assert_eq!(item.manual_url, "https://example.com/manual");
    assert_eq!(item.manual_text, "Steps");
    assert_eq!(item.notes, "quarterly");
    assert_eq!(item.cost_cents, Some(3_500));
    Ok(())
}

#[test]
fn create_house_profile_enforces_single_record() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let first = house_profile_input("Primary Residence", "Portland");
    let first_id = store.create_house_profile(&first)?;
    let fetched = store
        .get_house_profile()?
        .expect("house profile should exist after create");
    assert_eq!(fetched.id, first_id);
    assert_eq!(fetched.nickname, "Primary Residence");

    let second = house_profile_input("Second Home", "Seattle");
    let error = store
        .create_house_profile(&second)
        .expect_err("creating a second house profile should fail");
    assert!(error.to_string().contains("already exists"));
    Ok(())
}

#[test]
fn update_house_profile_requires_existing_row_then_persists_changes() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let missing_error = store
        .update_house_profile(&house_profile_input("No Profile", "Nowhere"))
        .expect_err("update should fail before any profile exists");
    assert!(
        missing_error
            .to_string()
            .contains("create one before updating")
    );

    store.create_house_profile(&house_profile_input("Primary Residence", "Portland"))?;
    store.update_house_profile(&house_profile_input("Primary Residence", "Seattle"))?;

    let updated = store
        .get_house_profile()?
        .expect("house profile should exist after update");
    assert_eq!(updated.nickname, "Primary Residence");
    assert_eq!(updated.city, "Seattle");
    Ok(())
}

#[test]
fn house_profile_upsert_and_update() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let first_id = store.upsert_house_profile(&HouseProfileInput {
        nickname: "Elm Street".to_owned(),
        address_line_1: "123 Elm".to_owned(),
        address_line_2: String::new(),
        city: "Springfield".to_owned(),
        state: "IL".to_owned(),
        postal_code: "62701".to_owned(),
        year_built: Some(1987),
        square_feet: Some(2400),
        lot_square_feet: Some(6000),
        bedrooms: Some(4),
        bathrooms: Some(2.5),
        foundation_type: "Slab".to_owned(),
        wiring_type: String::new(),
        roof_type: String::new(),
        exterior_type: String::new(),
        heating_type: String::new(),
        cooling_type: String::new(),
        water_source: String::new(),
        sewer_type: String::new(),
        parking_type: String::new(),
        basement_type: String::new(),
        insurance_carrier: String::new(),
        insurance_policy: String::new(),
        insurance_renewal: Some(Date::from_calendar_date(2026, Month::May, 1)?),
        property_tax_cents: Some(420_000),
        hoa_name: String::new(),
        hoa_fee_cents: None,
    })?;
    let first_profile = store
        .get_house_profile()?
        .expect("house profile should exist after first upsert");
    assert_eq!(first_profile.id, first_id);
    assert_eq!(first_profile.nickname, "Elm Street");

    let second_id = store.upsert_house_profile(&HouseProfileInput {
        nickname: "Elm Street Updated".to_owned(),
        address_line_1: "123 Elm".to_owned(),
        address_line_2: String::new(),
        city: "Springfield".to_owned(),
        state: "IL".to_owned(),
        postal_code: "62701".to_owned(),
        year_built: Some(1987),
        square_feet: Some(2500),
        lot_square_feet: Some(6000),
        bedrooms: Some(4),
        bathrooms: Some(2.5),
        foundation_type: "Slab".to_owned(),
        wiring_type: String::new(),
        roof_type: String::new(),
        exterior_type: String::new(),
        heating_type: String::new(),
        cooling_type: String::new(),
        water_source: String::new(),
        sewer_type: String::new(),
        parking_type: String::new(),
        basement_type: String::new(),
        insurance_carrier: String::new(),
        insurance_policy: String::new(),
        insurance_renewal: Some(Date::from_calendar_date(2026, Month::June, 1)?),
        property_tax_cents: Some(430_000),
        hoa_name: String::new(),
        hoa_fee_cents: None,
    })?;
    assert_eq!(second_id, first_id);

    let profile = store
        .get_house_profile()?
        .expect("house profile should still exist");
    assert_eq!(profile.id, first_id);
    assert_eq!(profile.nickname, "Elm Street Updated");
    assert_eq!(profile.square_feet, Some(2500));
    assert_eq!(profile.property_tax_cents, Some(430_000));
    Ok(())
}

#[test]
fn unicode_round_trip_house_profile_fields() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let cases = [
        ("Casa de Garc\u{00ED}a", "San Jos\u{00E9}"),
        ("\u{6211}\u{7684}\u{5BB6}", "\u{6771}\u{4EAC}"),
        ("Home \u{1F3E0}", "City \u{2605}"),
        ("Haus M\u{00FC}ller \u{2014} \u{6771}\u{4EAC}", ""),
        ("\u{00BD} acre lot", "\u{00A7}5 district"),
    ];

    for (nickname, city) in cases {
        store.upsert_house_profile(&HouseProfileInput {
            nickname: nickname.to_owned(),
            address_line_1: String::new(),
            address_line_2: String::new(),
            city: city.to_owned(),
            state: String::new(),
            postal_code: String::new(),
            year_built: None,
            square_feet: None,
            lot_square_feet: None,
            bedrooms: None,
            bathrooms: None,
            foundation_type: String::new(),
            wiring_type: String::new(),
            roof_type: String::new(),
            exterior_type: String::new(),
            heating_type: String::new(),
            cooling_type: String::new(),
            water_source: String::new(),
            sewer_type: String::new(),
            parking_type: String::new(),
            basement_type: String::new(),
            insurance_carrier: String::new(),
            insurance_policy: String::new(),
            insurance_renewal: None,
            property_tax_cents: None,
            hoa_name: String::new(),
            hoa_fee_cents: None,
        })?;

        let profile = store
            .get_house_profile()?
            .expect("house profile should exist");
        assert_eq!(profile.nickname, nickname);
        assert_eq!(profile.city, city);
    }

    Ok(())
}

#[test]
fn unicode_round_trip_vendor_names() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let names = [
        "Garc\u{00ED}a Plumbing",
        "M\u{00FC}ller HVAC",
        "\u{6771}\u{829D}\u{30B5}\u{30FC}\u{30D3}\u{30B9}",
        "O'Brien & Sons",
    ];

    for name in names {
        store.create_vendor(&NewVendor {
            name: name.to_owned(),
            contact_name: String::new(),
            email: String::new(),
            phone: String::new(),
            website: String::new(),
            notes: String::new(),
        })?;
    }

    let vendors = store.list_vendors(false)?;
    let vendor_names = vendors
        .into_iter()
        .map(|vendor| vendor.name)
        .collect::<Vec<_>>();
    for name in names {
        assert!(
            vendor_names.iter().any(|candidate| candidate == name),
            "vendor {name:?} should survive round trip"
        );
    }

    Ok(())
}

#[test]
fn unicode_round_trip_project_description() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let notes = "Technician Jos\u{00E9} used \u{00BD}-inch fittings per \u{00A7}5.2";
    let project_id = store.create_project(&NewProject {
        title: "Unicode notes test".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: notes.to_owned(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    let project = store.get_project(project_id)?;
    assert_eq!(project.description, notes);
    Ok(())
}

#[test]
fn soft_deleted_project_persists_across_reopen() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("soft-delete-persists.db");

    let project_id = {
        let store = Store::open(&db_path)?;
        store.bootstrap()?;
        let project_type_id = store.list_project_types()?[0].id;
        let project_id = store.create_project(&NewProject {
            title: "Persist Test".to_owned(),
            project_type_id,
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: None,
            actual_cents: None,
        })?;
        store.soft_delete_project(project_id)?;
        project_id
    };

    let store = Store::open(&db_path)?;
    store.bootstrap()?;

    let visible_projects = store.list_projects(false)?;
    assert!(
        !visible_projects
            .iter()
            .any(|project| project.id == project_id),
        "soft-deleted project should not appear in normal listing after reopen"
    );

    let all_projects = store.list_projects(true)?;
    let deleted_project = all_projects
        .iter()
        .find(|project| project.id == project_id)
        .expect("soft-deleted project should appear in include-deleted listing");
    assert!(deleted_project.deleted_at.is_some());

    store.restore_project(project_id)?;
    let restored_projects = store.list_projects(false)?;
    assert!(
        restored_projects
            .iter()
            .any(|project| project.id == project_id),
        "restored project should appear in normal listing"
    );
    Ok(())
}

#[test]
fn service_log_crud_and_restore_parent_guards() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Tech Co".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "HVAC filter".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::January, 15)?,
        vendor_id: Some(vendor_id),
        cost_cents: Some(12_500),
        notes: "Initial service".to_owned(),
    })?;
    let entry = store.get_service_log_entry(service_log_id)?;
    assert_eq!(entry.maintenance_item_id, maintenance_id);
    assert_eq!(entry.vendor_id, Some(vendor_id));
    assert_eq!(entry.cost_cents, Some(12_500));

    store.update_service_log_entry(
        service_log_id,
        &UpdateServiceLogEntry {
            maintenance_item_id: maintenance_id,
            serviced_at: Date::from_calendar_date(2026, Month::February, 1)?,
            vendor_id: None,
            cost_cents: Some(13_000),
            notes: "Updated entry".to_owned(),
        },
    )?;
    let updated = store.get_service_log_entry(service_log_id)?;
    assert_eq!(updated.vendor_id, None);
    assert_eq!(updated.cost_cents, Some(13_000));
    assert_eq!(updated.notes, "Updated entry");

    let by_maintenance = store.list_service_log_for_maintenance(maintenance_id, false)?;
    assert_eq!(by_maintenance.len(), 1);
    assert_eq!(by_maintenance[0].id, service_log_id);

    let second_service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::March, 1)?,
        vendor_id: Some(vendor_id),
        cost_cents: Some(15_000),
        notes: "Vendor service".to_owned(),
    })?;
    let vendor_delete_error = store
        .soft_delete_vendor(vendor_id)
        .expect_err("vendor with service logs should be protected");
    assert!(
        vendor_delete_error
            .to_string()
            .contains("active service log")
    );

    store.soft_delete_service_log_entry(second_service_log_id)?;
    store.soft_delete_vendor(vendor_id)?;
    let restore_error = store
        .restore_service_log_entry(second_service_log_id)
        .expect_err("restoring service log should fail when vendor is deleted");
    assert!(restore_error.to_string().contains("vendor is deleted"));

    store.restore_vendor(vendor_id)?;
    store.restore_service_log_entry(second_service_log_id)?;
    let logs = store.list_service_log_entries(false)?;
    assert_eq!(logs.len(), 2);
    Ok(())
}

#[test]
fn service_log_update_can_assign_vendor() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "HVAC Pros".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "HVAC filter".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let entry_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::January, 15)?,
        vendor_id: None,
        cost_cents: None,
        notes: "initial".to_owned(),
    })?;

    store.update_service_log_entry(
        entry_id,
        &UpdateServiceLogEntry {
            maintenance_item_id: maintenance_id,
            serviced_at: Date::from_calendar_date(2026, Month::January, 15)?,
            vendor_id: Some(vendor_id),
            cost_cents: Some(9_500),
            notes: "updated".to_owned(),
        },
    )?;

    let updated = store.get_service_log_entry(entry_id)?;
    assert_eq!(updated.vendor_id, Some(vendor_id));
    assert_eq!(updated.cost_cents, Some(9_500));
    assert_eq!(updated.notes, "updated");
    Ok(())
}

#[test]
fn list_service_log_for_maintenance_respects_include_deleted_flag() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Filter change".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 3,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let first_entry_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::January, 5)?,
        vendor_id: None,
        cost_cents: None,
        notes: "first".to_owned(),
    })?;
    let second_entry_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::February, 5)?,
        vendor_id: None,
        cost_cents: None,
        notes: "second".to_owned(),
    })?;

    store.soft_delete_service_log_entry(first_entry_id)?;

    let visible = store.list_service_log_for_maintenance(maintenance_id, false)?;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, second_entry_id);

    let include_deleted = store.list_service_log_for_maintenance(maintenance_id, true)?;
    assert_eq!(include_deleted.len(), 2);
    assert!(
        include_deleted
            .iter()
            .any(|entry| entry.id == first_entry_id)
    );
    assert!(
        include_deleted
            .iter()
            .any(|entry| entry.id == first_entry_id && entry.deleted_at.is_some())
    );

    store.restore_service_log_entry(first_entry_id)?;
    let restored = store.list_service_log_for_maintenance(maintenance_id, false)?;
    assert_eq!(restored.len(), 2);
    Ok(())
}

#[test]
fn delete_maintenance_blocked_by_active_service_logs() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Drain check".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 4,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::June, 7)?,
        vendor_id: None,
        cost_cents: None,
        notes: "completed".to_owned(),
    })?;

    let delete_error = store
        .soft_delete_maintenance_item(maintenance_id)
        .expect_err("maintenance with active service log should be protected");
    assert!(
        delete_error
            .to_string()
            .contains("service log(s) -- delete service logs first")
    );

    store.soft_delete_service_log_entry(service_log_id)?;
    store.soft_delete_maintenance_item(maintenance_id)?;
    Ok(())
}

#[test]
fn restore_service_log_blocked_by_deleted_maintenance() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Sump check".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::June, 1)?,
        vendor_id: None,
        cost_cents: None,
        notes: String::new(),
    })?;

    store.soft_delete_service_log_entry(service_log_id)?;
    store.soft_delete_maintenance_item(maintenance_id)?;

    let restore_error = store
        .restore_service_log_entry(service_log_id)
        .expect_err("restoring service log should fail while maintenance is deleted");
    assert!(
        restore_error
            .to_string()
            .contains("maintenance item is deleted")
    );

    store.restore_maintenance_item(maintenance_id)?;
    store.restore_service_log_entry(service_log_id)?;
    Ok(())
}

#[test]
fn restore_service_log_allowed_without_vendor_link() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Old Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Filter swap".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 3,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::June, 3)?,
        vendor_id: Some(vendor_id),
        cost_cents: None,
        notes: String::new(),
    })?;

    store.update_service_log_entry(
        service_log_id,
        &UpdateServiceLogEntry {
            maintenance_item_id: maintenance_id,
            serviced_at: Date::from_calendar_date(2026, Month::June, 3)?,
            vendor_id: None,
            cost_cents: None,
            notes: "vendor removed".to_owned(),
        },
    )?;
    store.soft_delete_service_log_entry(service_log_id)?;
    store.soft_delete_vendor(vendor_id)?;

    store.restore_service_log_entry(service_log_id)?;
    let restored = store.get_service_log_entry(service_log_id)?;
    assert_eq!(restored.vendor_id, None);
    Ok(())
}

#[test]
fn list_maintenance_items_filtered_by_appliance_via_typed_list() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Fridge".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "Clean coils".to_owned(),
        category_id,
        appliance_id: Some(appliance_id),
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "Check smoke detectors".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let filtered = store
        .list_maintenance_items(false)?
        .into_iter()
        .filter(|item| item.appliance_id == Some(appliance_id))
        .collect::<Vec<_>>();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "Clean coils");
    Ok(())
}

#[test]
fn count_maintenance_items_filtered_by_appliance_via_typed_list() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Range".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "Clean burners".to_owned(),
        category_id,
        appliance_id: Some(appliance_id),
        last_serviced_at: None,
        interval_months: 4,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "Inspect igniter".to_owned(),
        category_id,
        appliance_id: Some(appliance_id),
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.create_maintenance_item(&NewMaintenanceItem {
        name: "General house check".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;

    let count = store
        .list_maintenance_items(false)?
        .into_iter()
        .filter(|item| item.appliance_id == Some(appliance_id))
        .count();
    assert_eq!(count, 2);
    Ok(())
}

#[test]
fn list_maintenance_items_filtered_by_appliance_include_deleted_via_typed_list() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Furnace".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Filter change".to_owned(),
        category_id,
        appliance_id: Some(appliance_id),
        last_serviced_at: None,
        interval_months: 3,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    store.soft_delete_maintenance_item(maintenance_id)?;

    let visible = store
        .list_maintenance_items(false)?
        .into_iter()
        .filter(|item| item.appliance_id == Some(appliance_id))
        .collect::<Vec<_>>();
    assert!(visible.is_empty());

    let include_deleted = store
        .list_maintenance_items(true)?
        .into_iter()
        .filter(|item| item.appliance_id == Some(appliance_id))
        .collect::<Vec<_>>();
    assert_eq!(include_deleted.len(), 1);
    assert!(include_deleted[0].deleted_at.is_some());
    Ok(())
}

#[test]
fn list_quotes_filtered_by_vendor_via_typed_list() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "P1".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_a = store.create_vendor(&NewVendor {
        name: "TestVendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let vendor_b = store.create_vendor(&NewVendor {
        name: "OtherVendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    store.create_quote(&NewQuote {
        project_id,
        vendor_id: vendor_a,
        total_cents: 1_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;
    store.create_quote(&NewQuote {
        project_id,
        vendor_id: vendor_b,
        total_cents: 2_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let filtered = store
        .list_quotes(false)?
        .into_iter()
        .filter(|quote| quote.vendor_id == vendor_a)
        .collect::<Vec<_>>();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].total_cents, 1_000);
    Ok(())
}

#[test]
fn list_quotes_filtered_by_project_via_typed_list() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_a = store.create_project(&NewProject {
        title: "P1".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let project_b = store.create_project(&NewProject {
        title: "P2".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "V1".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;

    store.create_quote(&NewQuote {
        project_id: project_a,
        vendor_id,
        total_cents: 1_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;
    store.create_quote(&NewQuote {
        project_id: project_b,
        vendor_id,
        total_cents: 5_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let filtered = store
        .list_quotes(false)?
        .into_iter()
        .filter(|quote| quote.project_id == project_a)
        .collect::<Vec<_>>();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].total_cents, 1_000);
    Ok(())
}

#[test]
fn count_quotes_by_project_via_typed_list_filtering() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "P1".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_a = store.create_vendor(&NewVendor {
        name: "V1".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let vendor_b = store.create_vendor(&NewVendor {
        name: "V2".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    store.create_quote(&NewQuote {
        project_id,
        vendor_id: vendor_a,
        total_cents: 5_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;
    store.create_quote(&NewQuote {
        project_id,
        vendor_id: vendor_b,
        total_cents: 7_500,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let count = store
        .list_quotes(false)?
        .into_iter()
        .filter(|quote| quote.project_id == project_id)
        .count();
    assert_eq!(count, 2);
    Ok(())
}

#[test]
fn list_and_count_service_logs_by_vendor_via_typed_list_filtering() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Filter".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let vendor_a = store.create_vendor(&NewVendor {
        name: "LogVendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let vendor_b = store.create_vendor(&NewVendor {
        name: "OtherVendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::July, 1)?,
        vendor_id: Some(vendor_a),
        cost_cents: None,
        notes: String::new(),
    })?;
    store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::July, 2)?,
        vendor_id: Some(vendor_b),
        cost_cents: None,
        notes: String::new(),
    })?;

    let filtered = store
        .list_service_log_entries(false)?
        .into_iter()
        .filter(|entry| entry.vendor_id == Some(vendor_a))
        .collect::<Vec<_>>();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].maintenance_item_id, maintenance_id);
    Ok(())
}

#[test]
fn three_level_delete_restore_chain_enforces_parent_order() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "HVAC Unit".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Filter change".to_owned(),
        category_id,
        appliance_id: Some(appliance_id),
        last_serviced_at: None,
        interval_months: 3,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::July, 4)?,
        vendor_id: None,
        cost_cents: None,
        notes: String::new(),
    })?;

    let delete_error = store
        .soft_delete_maintenance_item(maintenance_id)
        .expect_err("active service log should block maintenance delete");
    assert!(delete_error.to_string().contains("service log"));

    store.soft_delete_service_log_entry(service_log_id)?;
    store.soft_delete_maintenance_item(maintenance_id)?;
    store.soft_delete_appliance(appliance_id)?;

    let restore_log_error = store
        .restore_service_log_entry(service_log_id)
        .expect_err("service log restore should fail while maintenance is deleted");
    assert!(
        restore_log_error
            .to_string()
            .contains("maintenance item is deleted")
    );
    let restore_maintenance_error = store
        .restore_maintenance_item(maintenance_id)
        .expect_err("maintenance restore should fail while appliance is deleted");
    assert!(
        restore_maintenance_error
            .to_string()
            .contains("appliance is deleted")
    );

    store.restore_appliance(appliance_id)?;
    store.restore_maintenance_item(maintenance_id)?;
    store.restore_service_log_entry(service_log_id)?;

    let maintenance_items = store.list_maintenance_items(false)?;
    assert_eq!(maintenance_items.len(), 1);
    assert_eq!(maintenance_items[0].appliance_id, Some(appliance_id));
    let logs = store.list_service_log_for_maintenance(maintenance_id, false)?;
    assert_eq!(logs.len(), 1);
    Ok(())
}

#[test]
fn vendor_quote_project_delete_restore_chain_enforces_parent_order() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Chain Vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Chain Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 1_000,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;

    let vendor_delete_error = store
        .soft_delete_vendor(vendor_id)
        .expect_err("active quote should block vendor delete");
    assert!(vendor_delete_error.to_string().contains("active quote"));
    let project_delete_error = store
        .soft_delete_project(project_id)
        .expect_err("active quote should block project delete");
    assert!(
        project_delete_error
            .to_string()
            .contains("quote(s) reference it")
    );

    store.soft_delete_quote(quote_id)?;
    store.soft_delete_project(project_id)?;
    store.soft_delete_vendor(vendor_id)?;

    let restore_quote_error = store
        .restore_quote(quote_id)
        .expect_err("quote restore should fail while project is deleted");
    assert!(
        restore_quote_error
            .to_string()
            .contains("project is deleted")
    );

    store.restore_project(project_id)?;
    let restore_quote_vendor_error = store
        .restore_quote(quote_id)
        .expect_err("quote restore should fail while vendor is deleted");
    assert!(
        restore_quote_vendor_error
            .to_string()
            .contains("vendor is deleted")
    );

    store.restore_vendor(vendor_id)?;
    store.restore_quote(quote_id)?;

    assert_eq!(store.list_vendors(false)?.len(), 1);
    assert_eq!(store.list_quotes(false)?.len(), 1);
    Ok(())
}

#[test]
fn document_metadata_round_trip_and_list_excludes_blob_data() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Doc project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    let content = b"fake pdf content".to_vec();
    let document_id = store.insert_document(&NewDocument {
        title: "Quote PDF".to_owned(),
        file_name: "invoice.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: project_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: content.clone(),
        notes: "first draft".to_owned(),
    })?;
    let docs = store.list_documents(false)?;
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, document_id);
    assert_eq!(docs[0].title, "Quote PDF");
    assert_eq!(docs[0].file_name, "invoice.pdf");
    assert_eq!(docs[0].size_bytes, i64::try_from(content.len())?);
    assert_eq!(docs[0].mime_type, "application/pdf");
    assert_eq!(docs[0].entity_kind, DocumentEntityKind::Project);
    assert_eq!(docs[0].entity_id, project_id.get());
    assert_eq!(docs[0].notes, "first draft");
    assert!(
        docs[0].data.is_empty(),
        "list_documents should not hydrate BLOB content"
    );

    let full = store.get_document(document_id)?;
    assert_eq!(full.data, content);
    assert_eq!(full.size_bytes, i64::try_from(content.len())?);

    // Simulate document soft-delete/restore to verify list filtering parity.
    store.raw_connection().execute(
        "UPDATE documents SET deleted_at = datetime('now'), updated_at = datetime('now') WHERE id = ?",
        rusqlite::params![document_id.get()],
    )?;
    assert!(store.list_documents(false)?.is_empty());
    let include_deleted = store.list_documents(true)?;
    assert_eq!(include_deleted.len(), 1);
    assert!(include_deleted[0].deleted_at.is_some());

    store.raw_connection().execute(
        "UPDATE documents SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?",
        rusqlite::params![document_id.get()],
    )?;
    assert_eq!(store.list_documents(false)?.len(), 1);
    Ok(())
}

#[test]
fn document_soft_delete_restore_round_trip() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let document_id = store.insert_document(&NewDocument {
        title: "Contract".to_owned(),
        file_name: "contract.pdf".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "application/pdf".to_owned(),
        data: b"contract".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    assert!(store.list_documents(false)?.is_empty());
    let deleted = store.list_documents(true)?;
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].id, document_id);
    assert!(deleted[0].deleted_at.is_some());

    store.restore_document(document_id)?;
    let restored = store.list_documents(false)?;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, document_id);
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_project() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Doc Restore Project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Project Note".to_owned(),
        file_name: "note.txt".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: project_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_project(project_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while project is deleted");
    assert!(restore_error.to_string().contains("project is deleted"));

    store.restore_project(project_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_appliance() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let appliance_id = store.create_appliance(&NewAppliance {
        name: "Doomed appliance".to_owned(),
        brand: String::new(),
        model_number: String::new(),
        serial_number: String::new(),
        purchase_date: None,
        warranty_expiry: None,
        location: String::new(),
        cost_cents: None,
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Appliance Note".to_owned(),
        file_name: "appliance.txt".to_owned(),
        entity_kind: DocumentEntityKind::Appliance,
        entity_id: appliance_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_appliance(appliance_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while appliance is deleted");
    assert!(restore_error.to_string().contains("appliance is deleted"));

    store.restore_appliance(appliance_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_vendor() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Doomed vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Vendor Note".to_owned(),
        file_name: "vendor.txt".to_owned(),
        entity_kind: DocumentEntityKind::Vendor,
        entity_id: vendor_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_vendor(vendor_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while vendor is deleted");
    assert!(restore_error.to_string().contains("vendor is deleted"));

    store.restore_vendor(vendor_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_quote() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Quote parent".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let vendor_id = store.create_vendor(&NewVendor {
        name: "Quote vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let quote_id = store.create_quote(&NewQuote {
        project_id,
        vendor_id,
        total_cents: 1_234,
        labor_cents: None,
        materials_cents: None,
        other_cents: None,
        received_date: None,
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Quote Note".to_owned(),
        file_name: "quote.txt".to_owned(),
        entity_kind: DocumentEntityKind::Quote,
        entity_id: quote_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_quote(quote_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while quote is deleted");
    assert!(restore_error.to_string().contains("quote is deleted"));

    store.restore_quote(quote_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_maintenance() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Doomed maintenance".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 12,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Maintenance Note".to_owned(),
        file_name: "maintenance.txt".to_owned(),
        entity_kind: DocumentEntityKind::Maintenance,
        entity_id: maintenance_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_maintenance_item(maintenance_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while maintenance is deleted");
    assert!(
        restore_error
            .to_string()
            .contains("maintenance item is deleted")
    );

    store.restore_maintenance_item(maintenance_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_service_log() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let category_id = store.list_maintenance_categories()?[0].id;
    let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
        name: "Service parent".to_owned(),
        category_id,
        appliance_id: None,
        last_serviced_at: None,
        interval_months: 6,
        manual_url: String::new(),
        manual_text: String::new(),
        notes: String::new(),
        cost_cents: None,
    })?;
    let service_log_id = store.create_service_log_entry(&NewServiceLogEntry {
        maintenance_item_id: maintenance_id,
        serviced_at: Date::from_calendar_date(2026, Month::August, 2)?,
        vendor_id: None,
        cost_cents: None,
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Service Log Note".to_owned(),
        file_name: "service-log.txt".to_owned(),
        entity_kind: DocumentEntityKind::ServiceLog,
        entity_id: service_log_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_service_log_entry(service_log_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while service log is deleted");
    assert!(restore_error.to_string().contains("service log is deleted"));

    store.restore_service_log_entry(service_log_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn restore_document_blocked_by_deleted_incident() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Doomed incident".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::August, 5)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Incident Note".to_owned(),
        file_name: "incident.txt".to_owned(),
        entity_kind: DocumentEntityKind::Incident,
        entity_id: incident_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"note".to_vec(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.soft_delete_incident(incident_id)?;

    let restore_error = store
        .restore_document(document_id)
        .expect_err("restoring document should fail while incident is deleted");
    assert!(restore_error.to_string().contains("incident is deleted"));

    store.restore_incident(incident_id)?;
    store.restore_document(document_id)?;
    Ok(())
}

#[test]
fn update_document_metadata_preserves_blob_and_link() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Doc update project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Invoice".to_owned(),
        file_name: "invoice.pdf".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: project_id.get(),
        mime_type: "application/pdf".to_owned(),
        data: b"original-data".to_vec(),
        notes: "draft".to_owned(),
    })?;
    let original = store.get_document(document_id)?;

    store.update_document(
        document_id,
        &UpdateDocument {
            title: "Invoice Final".to_owned(),
            file_name: "invoice.pdf".to_owned(),
            entity_kind: DocumentEntityKind::Project,
            entity_id: project_id.get(),
            mime_type: "application/pdf".to_owned(),
            data: None,
            notes: String::new(),
        },
    )?;

    let updated = store.get_document(document_id)?;
    assert_eq!(updated.title, "Invoice Final");
    assert_eq!(updated.notes, "");
    assert_eq!(updated.entity_kind, DocumentEntityKind::Project);
    assert_eq!(updated.entity_id, project_id.get());
    assert_eq!(updated.data, original.data);
    assert_eq!(updated.size_bytes, original.size_bytes);
    assert_eq!(updated.checksum_sha256, original.checksum_sha256);
    Ok(())
}

#[test]
fn update_document_replaces_blob_and_cache_content() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let document_id = store.insert_document(&NewDocument {
        title: "Receipt".to_owned(),
        file_name: "receipt.txt".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "text/plain".to_owned(),
        data: b"old-content".to_vec(),
        notes: String::new(),
    })?;
    let old = store.get_document(document_id)?;

    store.update_document(
        document_id,
        &UpdateDocument {
            title: "Receipt Updated".to_owned(),
            file_name: "receipt-v2.txt".to_owned(),
            entity_kind: DocumentEntityKind::None,
            entity_id: 0,
            mime_type: "text/plain".to_owned(),
            data: Some(b"new-content-v2".to_vec()),
            notes: "replaced".to_owned(),
        },
    )?;

    let updated = store.get_document(document_id)?;
    assert_eq!(updated.title, "Receipt Updated");
    assert_eq!(updated.file_name, "receipt-v2.txt");
    assert_eq!(updated.notes, "replaced");
    assert_eq!(updated.data, b"new-content-v2".to_vec());
    assert_ne!(updated.checksum_sha256, old.checksum_sha256);
    assert_ne!(updated.size_bytes, old.size_bytes);

    let extracted_path = store.extract_document(document_id)?;
    let extracted = fs::read(extracted_path)?;
    assert_eq!(extracted, b"new-content-v2".to_vec());
    Ok(())
}

#[test]
fn update_document_can_clear_notes_while_preserving_file_metadata() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let payload = b"receipt-data".to_vec();
    let document_id = store.insert_document(&NewDocument {
        title: "Receipt".to_owned(),
        file_name: "receipt.pdf".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "application/pdf".to_owned(),
        data: payload.clone(),
        notes: "plumber visit 2026-01".to_owned(),
    })?;

    store.update_document(
        document_id,
        &UpdateDocument {
            title: "Receipt".to_owned(),
            file_name: "receipt.pdf".to_owned(),
            entity_kind: DocumentEntityKind::None,
            entity_id: 0,
            mime_type: "application/pdf".to_owned(),
            data: None,
            notes: String::new(),
        },
    )?;

    let updated = store.get_document(document_id)?;
    assert_eq!(updated.notes, "");
    assert_eq!(updated.file_name, "receipt.pdf");
    assert_eq!(updated.mime_type, "application/pdf");
    assert_eq!(updated.data, payload);
    Ok(())
}

#[test]
fn document_content_survives_delete_restore_round_trip() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let payload = b"survive-me".to_vec();
    let document_id = store.insert_document(&NewDocument {
        title: "Survivor".to_owned(),
        file_name: "survivor.txt".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "text/plain".to_owned(),
        data: payload.clone(),
        notes: String::new(),
    })?;

    store.soft_delete_document(document_id)?;
    store.restore_document(document_id)?;
    let restored = store.get_document(document_id)?;
    assert_eq!(restored.data, payload);
    Ok(())
}

#[test]
fn unlinked_document_full_lifecycle_round_trip() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let document_id = store.insert_document(&NewDocument {
        title: "Unlinked".to_owned(),
        file_name: "unlinked.txt".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "text/plain".to_owned(),
        data: b"v1".to_vec(),
        notes: "start".to_owned(),
    })?;
    store.update_document(
        document_id,
        &UpdateDocument {
            title: "Unlinked v2".to_owned(),
            file_name: "unlinked-v2.txt".to_owned(),
            entity_kind: DocumentEntityKind::None,
            entity_id: 0,
            mime_type: "text/plain".to_owned(),
            data: Some(b"v2-content".to_vec()),
            notes: String::new(),
        },
    )?;
    store.soft_delete_document(document_id)?;
    store.restore_document(document_id)?;

    let restored = store.get_document(document_id)?;
    assert_eq!(restored.title, "Unlinked v2");
    assert_eq!(restored.file_name, "unlinked-v2.txt");
    assert_eq!(restored.data, b"v2-content".to_vec());
    assert_eq!(restored.notes, "");
    assert_eq!(restored.entity_kind, DocumentEntityKind::None);
    assert_eq!(restored.entity_id, 0);
    Ok(())
}

#[test]
fn list_documents_for_entity_via_typed_filtering() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Doc list target".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    let other_project_id = store.create_project(&NewProject {
        title: "Doc list other".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;
    store.insert_document(&NewDocument {
        title: "Target doc".to_owned(),
        file_name: "target.txt".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: project_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"target".to_vec(),
        notes: String::new(),
    })?;
    store.insert_document(&NewDocument {
        title: "Other doc".to_owned(),
        file_name: "other.txt".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: other_project_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"other".to_vec(),
        notes: String::new(),
    })?;

    let filtered = store
        .list_documents(false)?
        .into_iter()
        .filter(|document| {
            document.entity_kind == DocumentEntityKind::Project
                && document.entity_id == project_id.get()
        })
        .collect::<Vec<_>>();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].title, "Target doc");
    Ok(())
}

#[test]
fn list_documents_for_entity_include_deleted_via_typed_filtering() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let incident_id = store.create_incident(&NewIncident {
        title: "Doc incident".to_owned(),
        description: String::new(),
        status: IncidentStatus::Open,
        severity: IncidentSeverity::Soon,
        date_noticed: Date::from_calendar_date(2026, Month::September, 1)?,
        date_resolved: None,
        location: String::new(),
        cost_cents: None,
        appliance_id: None,
        vendor_id: None,
        notes: String::new(),
    })?;
    let document_id = store.insert_document(&NewDocument {
        title: "Incident doc".to_owned(),
        file_name: "incident.txt".to_owned(),
        entity_kind: DocumentEntityKind::Incident,
        entity_id: incident_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"incident".to_vec(),
        notes: String::new(),
    })?;
    store.soft_delete_document(document_id)?;

    let visible = store
        .list_documents(false)?
        .into_iter()
        .filter(|document| {
            document.entity_kind == DocumentEntityKind::Incident
                && document.entity_id == incident_id.get()
        })
        .collect::<Vec<_>>();
    assert!(visible.is_empty());

    let include_deleted = store
        .list_documents(true)?
        .into_iter()
        .filter(|document| {
            document.entity_kind == DocumentEntityKind::Incident
                && document.entity_id == incident_id.get()
        })
        .collect::<Vec<_>>();
    assert_eq!(include_deleted.len(), 1);
    assert!(include_deleted[0].deleted_at.is_some());
    Ok(())
}

#[test]
fn count_documents_for_entity_via_typed_filtering() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let vendor_id = store.create_vendor(&NewVendor {
        name: "Doc vendor".to_owned(),
        contact_name: String::new(),
        email: String::new(),
        phone: String::new(),
        website: String::new(),
        notes: String::new(),
    })?;
    let project_type_id = store.list_project_types()?[0].id;
    let project_id = store.create_project(&NewProject {
        title: "Doc count project".to_owned(),
        project_type_id,
        status: ProjectStatus::Planned,
        description: String::new(),
        start_date: None,
        end_date: None,
        budget_cents: None,
        actual_cents: None,
    })?;

    store.insert_document(&NewDocument {
        title: "Vendor doc 1".to_owned(),
        file_name: "v1.txt".to_owned(),
        entity_kind: DocumentEntityKind::Vendor,
        entity_id: vendor_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"v1".to_vec(),
        notes: String::new(),
    })?;
    store.insert_document(&NewDocument {
        title: "Vendor doc 2".to_owned(),
        file_name: "v2.txt".to_owned(),
        entity_kind: DocumentEntityKind::Vendor,
        entity_id: vendor_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"v2".to_vec(),
        notes: String::new(),
    })?;
    store.insert_document(&NewDocument {
        title: "Project doc".to_owned(),
        file_name: "p.txt".to_owned(),
        entity_kind: DocumentEntityKind::Project,
        entity_id: project_id.get(),
        mime_type: "text/plain".to_owned(),
        data: b"p".to_vec(),
        notes: String::new(),
    })?;

    let vendor_count = store
        .list_documents(false)?
        .into_iter()
        .filter(|document| {
            document.entity_kind == DocumentEntityKind::Vendor
                && document.entity_id == vendor_id.get()
        })
        .count();
    assert_eq!(vendor_count, 2);

    let empty_count = store
        .list_documents(false)?
        .into_iter()
        .filter(|document| {
            document.entity_kind == DocumentEntityKind::Appliance && document.entity_id == 999
        })
        .count();
    assert_eq!(empty_count, 0);
    Ok(())
}

#[test]
fn multiple_documents_list_order_uses_updated_at_then_id_desc() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;

    let first_id = store.insert_document(&NewDocument {
        title: "First".to_owned(),
        file_name: "first.txt".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "text/plain".to_owned(),
        data: b"first".to_vec(),
        notes: String::new(),
    })?;
    let second_id = store.insert_document(&NewDocument {
        title: "Second".to_owned(),
        file_name: "second.txt".to_owned(),
        entity_kind: DocumentEntityKind::None,
        entity_id: 0,
        mime_type: "text/plain".to_owned(),
        data: b"second".to_vec(),
        notes: String::new(),
    })?;

    // Force identical timestamps to assert deterministic id-desc tiebreaking.
    store.raw_connection().execute(
        "UPDATE documents SET updated_at = '2026-09-01T00:00:00Z' WHERE id IN (?, ?)",
        rusqlite::params![first_id.get(), second_id.get()],
    )?;

    let docs = store.list_documents(false)?;
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].id, second_id);
    assert_eq!(docs[1].id, first_id);
    Ok(())
}

fn new_store_with_demo_data(seed: u64) -> Result<Store> {
    let store = Store::open_memory()?;
    store.bootstrap()?;
    store.seed_demo_data_with_seed(seed)?;
    Ok(store)
}

fn new_store_with_scaled_data(seed: u64, years: i32) -> Result<(Store, SeedSummary)> {
    let store = Store::open_memory()?;
    store.bootstrap()?;
    let summary = store.seed_scaled_data_with_seed(seed, years)?;
    Ok((store, summary))
}

#[test]
fn seed_demo_data_populates_all_entities() -> Result<()> {
    let store = new_store_with_demo_data(42)?;

    let house = store
        .get_house_profile()?
        .expect("seeded demo house profile should exist");
    assert!(!house.nickname.is_empty());
    assert!(house.year_built.unwrap_or_default() > 0);

    assert!(!store.list_vendors(false)?.is_empty());
    assert!(!store.list_projects(false)?.is_empty());
    assert!(!store.list_appliances(false)?.is_empty());
    assert!(!store.list_maintenance_items(false)?.is_empty());
    Ok(())
}

#[test]
fn seed_demo_data_is_deterministic_for_same_seed() -> Result<()> {
    let store1 = new_store_with_demo_data(42)?;
    let store2 = new_store_with_demo_data(42)?;

    let house1 = store1
        .get_house_profile()?
        .expect("first seeded house profile should exist");
    let house2 = store2
        .get_house_profile()?
        .expect("second seeded house profile should exist");

    assert_eq!(house1.nickname, house2.nickname);
    Ok(())
}

#[test]
fn seed_demo_data_varies_across_seeds() -> Result<()> {
    let mut names = BTreeSet::new();
    for offset in 0..5_u64 {
        let store = new_store_with_demo_data(42 + offset)?;
        let house = store
            .get_house_profile()?
            .expect("seeded house profile should exist");
        names.insert(house.nickname);
    }
    assert!(
        names.len() >= 3,
        "expected at least 3 distinct house names, got {names:?}"
    );
    Ok(())
}

#[test]
fn seed_demo_data_skips_when_data_exists() -> Result<()> {
    let store = new_store_with_demo_data(42)?;
    let first_count = store.list_vendors(false)?.len();

    store.seed_demo_data()?;
    let second_count = store.list_vendors(false)?.len();

    assert_eq!(first_count, second_count);
    Ok(())
}

#[test]
fn seed_scaled_data_populates_all_entities() -> Result<()> {
    let (store, summary) = new_store_with_scaled_data(42, 3)?;

    let house = store
        .get_house_profile()?
        .expect("seeded scaled house profile should exist");
    assert!(!house.nickname.is_empty());

    let vendors = store.list_vendors(false)?;
    assert!(!vendors.is_empty());
    assert_eq!(summary.vendors, vendors.len());

    let projects = store.list_projects(false)?;
    assert!(!projects.is_empty());
    assert_eq!(summary.projects, projects.len());

    let appliances = store.list_appliances(false)?;
    assert!(!appliances.is_empty());
    assert_eq!(summary.appliances, appliances.len());

    let maintenance = store.list_maintenance_items(false)?;
    assert!(!maintenance.is_empty());
    assert_eq!(summary.maintenance, maintenance.len());

    assert!(summary.service_logs > 0);
    assert!(summary.quotes > 0);
    assert!(summary.documents > 0);
    Ok(())
}

#[test]
fn seed_scaled_data_is_deterministic_for_same_seed() -> Result<()> {
    let (store1, _) = new_store_with_scaled_data(42, 5)?;
    let (store2, _) = new_store_with_scaled_data(42, 5)?;

    let house1 = store1
        .get_house_profile()?
        .expect("first seeded house profile should exist");
    let house2 = store2
        .get_house_profile()?
        .expect("second seeded house profile should exist");
    assert_eq!(house1.nickname, house2.nickname);
    Ok(())
}

#[test]
fn seed_scaled_data_grows_with_years() -> Result<()> {
    let (_, summary1) = new_store_with_scaled_data(42, 1)?;
    let (_, summary5) = new_store_with_scaled_data(42, 5)?;
    let (_, summary10) = new_store_with_scaled_data(42, 10)?;

    assert!(summary1.service_logs < summary5.service_logs);
    assert!(summary5.service_logs < summary10.service_logs);
    assert!(summary1.projects < summary10.projects);
    assert!(summary1.vendors < summary10.vendors);
    Ok(())
}

#[test]
fn seed_scaled_data_spreads_service_logs_across_years() -> Result<()> {
    let (store, _) = new_store_with_scaled_data(42, 5)?;
    let maintenance = store.list_maintenance_items(false)?;

    let mut years_seen = BTreeSet::new();
    for item in maintenance {
        let logs = store.list_service_log_for_maintenance(item.id, false)?;
        for log in logs {
            years_seen.insert(log.serviced_at.year());
        }
    }

    let current_year = time::OffsetDateTime::now_utc().year();
    assert!(
        years_seen.len() >= 3,
        "expected logs across multiple years, got {years_seen:?}"
    );
    assert!(
        years_seen.contains(&current_year),
        "expected logs in current year {current_year}, got {years_seen:?}"
    );
    Ok(())
}

#[test]
fn seed_scaled_data_is_idempotent() -> Result<()> {
    let (store, first_summary) = new_store_with_scaled_data(42, 3)?;

    let second_summary = store.seed_scaled_data(3)?;
    assert_eq!(second_summary, SeedSummary::default());
    assert_ne!(first_summary, SeedSummary::default());
    Ok(())
}

#[test]
fn seed_scaled_data_preserves_fk_integrity() -> Result<()> {
    let (store, _) = new_store_with_scaled_data(42, 3)?;

    let projects = store.list_projects(false)?;
    let project_types = store.list_project_types()?;
    let project_type_ids = project_types
        .iter()
        .map(|entry| entry.id.get())
        .collect::<BTreeSet<_>>();
    for project in projects {
        assert!(
            project_type_ids.contains(&project.project_type_id.get()),
            "project {:?} has invalid project type id {}",
            project.title,
            project.project_type_id.get()
        );
    }

    let maintenance = store.list_maintenance_items(false)?;
    let categories = store.list_maintenance_categories()?;
    let category_ids = categories
        .iter()
        .map(|entry| entry.id.get())
        .collect::<BTreeSet<_>>();
    for item in &maintenance {
        assert!(
            category_ids.contains(&item.category_id.get()),
            "maintenance {:?} has invalid category id {}",
            item.name,
            item.category_id.get()
        );
    }

    let maintenance_ids = maintenance
        .iter()
        .map(|item| item.id.get())
        .collect::<BTreeSet<_>>();
    for item in maintenance {
        let logs = store.list_service_log_for_maintenance(item.id, false)?;
        for entry in logs {
            assert!(
                maintenance_ids.contains(&entry.maintenance_item_id.get()),
                "service log references invalid maintenance item id {}",
                entry.maintenance_item_id.get()
            );
        }
    }
    Ok(())
}

#[test]
fn seed_scaled_data_summary_matches_database_counts() -> Result<()> {
    let store = Store::open_memory()?;
    store.bootstrap()?;
    let summary = store.seed_scaled_data_with_seed(42, 5)?;

    let vendors = store.list_vendors(false)?;
    assert_eq!(summary.vendors, vendors.len());

    let projects = store.list_projects(false)?;
    assert_eq!(summary.projects, projects.len());

    let appliances = store.list_appliances(false)?;
    assert_eq!(summary.appliances, appliances.len());

    let maintenance = store.list_maintenance_items(false)?;
    assert_eq!(summary.maintenance, maintenance.len());

    let mut total_service_logs = 0usize;
    for item in maintenance {
        total_service_logs += store
            .list_service_log_for_maintenance(item.id, false)?
            .len();
    }
    assert_eq!(summary.service_logs, total_service_logs);
    Ok(())
}

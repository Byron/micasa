// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::Result;
use micasa_app::{DocumentEntityKind, IncidentSeverity, IncidentStatus, ProjectStatus};
use micasa_db::{
    NewAppliance, NewDocument, NewIncident, NewMaintenanceItem, NewProject, NewQuote, NewVendor,
    Store, UpdateProject, UpdateVendor, document_cache_dir, evict_stale_cache, validate_db_path,
};
use time::{Date, Month};

#[test]
fn validate_db_path_rejects_uri_forms() {
    assert!(validate_db_path("file:test.db").is_err());
    assert!(validate_db_path("https://example.com/db.sqlite").is_err());
    assert!(validate_db_path("db.sqlite?mode=ro").is_err());
    assert!(validate_db_path("/tmp/micasa.db").is_ok());
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
    let _quote_id = store.create_quote(&NewQuote {
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

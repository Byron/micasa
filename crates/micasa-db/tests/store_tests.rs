// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::Result;
use micasa_app::{DocumentEntityKind, ProjectStatus};
use micasa_db::{
    NewDocument, NewProject, Store, document_cache_dir, evict_stale_cache, validate_db_path,
};

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

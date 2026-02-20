// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn temp_db_path() -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().context("create temp dir")?;
    let db_path = dir.path().join("micasa.db");
    Ok((dir, db_path))
}

pub fn fixture_datetime() -> &'static str {
    "2026-02-19T12:34:56Z"
}

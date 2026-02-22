// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CONFIG_VERSION: i64 = 2;
const DEFAULT_LLM_BASE_URL: &str = "http://localhost:11434/v1";
const DEFAULT_LLM_MODEL: &str = "qwen3";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub version: i64,
    #[serde(default)]
    pub storage: Storage,
    #[serde(default)]
    pub ui: Ui,
    #[serde(default)]
    pub llm: Llm,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            storage: Storage::default(),
            ui: Ui::default(),
            llm: Llm::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Storage {
    pub db_path: Option<String>,
    pub max_document_size: Option<i64>,
    pub cache_ttl_days: Option<i64>,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            db_path: None,
            max_document_size: Some(micasa_db::MAX_DOCUMENT_SIZE),
            cache_ttl_days: Some(30),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ui {
    pub show_dashboard: Option<bool>,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            show_dashboard: Some(true),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Llm {
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub extra_context: Option<String>,
    pub timeout: Option<String>,
}

impl Default for Llm {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            base_url: Some(DEFAULT_LLM_BASE_URL.to_owned()),
            model: Some(DEFAULT_LLM_MODEL.to_owned()),
            extra_context: Some(String::new()),
            timeout: Some("5s".to_owned()),
        }
    }
}

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        if let Some(path) = env::var_os("MICASA_CONFIG_PATH") {
            return Ok(PathBuf::from(path));
        }

        let config_root = dirs::config_dir().ok_or_else(|| {
            anyhow!("cannot resolve config directory; set MICASA_CONFIG_PATH to the config file")
        })?;

        let app_dir = config_root.join(micasa_db::APP_NAME);
        fs::create_dir_all(&app_dir)
            .with_context(|| format!("create config directory {}", app_dir.display()))?;
        Ok(app_dir.join("config.toml"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("read config file {}", path.display()))?;
        let value: toml::Value = toml::from_str(&raw)
            .with_context(|| format!("parse TOML config {}", path.display()))?;

        let version = value
            .get("version")
            .and_then(toml::Value::as_integer)
            .ok_or_else(|| {
                anyhow!(
                    "config file {} is not versioned for Rust config v2. Add `version = 2` and move values under [storage], [ui], and [llm]",
                    path.display()
                )
            })?;

        if version != CONFIG_VERSION {
            bail!(
                "unsupported config version {} in {}; expected version = 2. Migrate your config to the v2 schema",
                version,
                path.display()
            );
        }

        let config: Config = value
            .try_into()
            .with_context(|| format!("decode config {}", path.display()))?;
        config.validate(path)?;
        Ok(config)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.version != CONFIG_VERSION {
            bail!(
                "config {} has version {}; expected 2",
                path.display(),
                self.version
            );
        }

        if let Some(db_path) = &self.storage.db_path {
            micasa_db::validate_db_path(db_path)?;
        }

        if let Some(max_size) = self.storage.max_document_size
            && max_size <= 0
        {
            bail!(
                "storage.max_document_size in {} must be positive, got {}",
                path.display(),
                max_size
            );
        }

        if let Some(ttl_days) = self.storage.cache_ttl_days
            && ttl_days < 0
        {
            bail!(
                "storage.cache_ttl_days in {} must be non-negative, got {}",
                path.display(),
                ttl_days
            );
        }

        if let Some(timeout) = &self.llm.timeout {
            let parsed = parse_duration(timeout)?;
            if parsed <= Duration::ZERO {
                bail!(
                    "llm.timeout in {} must be positive, got {}",
                    path.display(),
                    timeout
                );
            }
        }

        Ok(())
    }

    pub fn db_path(&self) -> Result<PathBuf> {
        match &self.storage.db_path {
            Some(path) => Ok(PathBuf::from(path)),
            None => micasa_db::default_db_path(),
        }
    }

    pub fn show_dashboard(&self) -> bool {
        self.ui.show_dashboard.unwrap_or(true)
    }

    pub fn max_document_size(&self) -> i64 {
        self.storage
            .max_document_size
            .unwrap_or(micasa_db::MAX_DOCUMENT_SIZE)
    }

    pub fn cache_ttl_days(&self) -> i64 {
        self.storage.cache_ttl_days.unwrap_or(30)
    }

    pub fn llm_enabled(&self) -> bool {
        self.llm.enabled.unwrap_or(true)
    }

    pub fn llm_base_url(&self) -> &str {
        self.llm
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_LLM_BASE_URL)
            .trim_end_matches('/')
    }

    pub fn llm_model(&self) -> &str {
        self.llm.model.as_deref().unwrap_or(DEFAULT_LLM_MODEL)
    }

    pub fn llm_timeout(&self) -> Result<Duration> {
        parse_duration(self.llm.timeout.as_deref().unwrap_or("5s"))
    }

    pub fn llm_extra_context(&self) -> &str {
        self.llm.extra_context.as_deref().unwrap_or("")
    }

    pub fn example_config(path: &Path) -> String {
        format!(
            "# micasa Rust config\n# Place this file at: {}\n\nversion = 2\n\n[storage]\n# Optional. Default is platform data dir (for example ~/.local/share/micasa/micasa.db)\n# db_path = \"/absolute/path/to/micasa.db\"\nmax_document_size = {}\ncache_ttl_days = 30\n\n[ui]\nshow_dashboard = true\n\n[llm]\nenabled = true\nbase_url = \"{}\"\nmodel = \"{}\"\nextra_context = \"\"\ntimeout = \"5s\"\n",
            path.display(),
            micasa_db::MAX_DOCUMENT_SIZE,
            DEFAULT_LLM_BASE_URL,
            DEFAULT_LLM_MODEL,
        )
    }
}

fn parse_duration(raw: &str) -> Result<Duration> {
    if let Some(value) = raw.strip_suffix("ms") {
        let millis: u64 = value
            .parse()
            .with_context(|| format!("invalid timeout duration {raw:?}"))?;
        return Ok(Duration::from_millis(millis));
    }
    if let Some(value) = raw.strip_suffix('s') {
        let secs: u64 = value
            .parse()
            .with_context(|| format!("invalid timeout duration {raw:?}"))?;
        return Ok(Duration::from_secs(secs));
    }
    if let Some(value) = raw.strip_suffix('m') {
        let mins: u64 = value
            .parse()
            .with_context(|| format!("invalid timeout duration {raw:?}"))?;
        return Ok(Duration::from_secs(mins * 60));
    }

    bail!("invalid duration {raw:?}; use one of: <N>ms, <N>s, <N>m (for example 500ms or 5s)")
}

#[cfg(test)]
mod tests {
    use super::{Config, parse_duration};
    use anyhow::Result;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    fn write_config(content: &str) -> Result<(tempfile::TempDir, PathBuf)> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("config.toml");
        std::fs::write(&path, content)?;
        Ok((temp, path))
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        match ENV_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[test]
    fn missing_config_uses_defaults() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let config = Config::load(&temp.path().join("missing.toml"))?;
        assert_eq!(config.version, 2);
        assert!(config.show_dashboard());
        Ok(())
    }

    #[test]
    fn old_unversioned_config_is_rejected_with_actionable_message() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("config.toml");
        std::fs::write(&path, "[llm]\nmodel=\"qwen3\"\n")?;

        let error = Config::load(&path).expect_err("old schema should fail");
        let message = error.to_string();
        assert!(message.contains("version = 2"));
        assert!(message.contains("[storage], [ui], and [llm]"));
        Ok(())
    }

    #[test]
    fn v2_config_parses() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            "version = 2\n[storage]\nmax_document_size = 1024\n[ui]\nshow_dashboard = false\n[llm]\nbase_url=\"http://localhost:11434/v1\"\nmodel=\"qwen3\"\ntimeout=\"2s\"\n",
        )?;

        let config = Config::load(&path)?;
        assert_eq!(config.max_document_size(), 1024);
        assert!(!config.show_dashboard());
        assert_eq!(config.llm_model(), "qwen3");
        Ok(())
    }

    #[test]
    fn malformed_config_returns_parse_error() -> Result<()> {
        let (_temp, path) = write_config("{{not toml")?;
        let error = Config::load(&path).expect_err("malformed config should fail");
        assert!(error.to_string().contains("parse TOML config"));
        Ok(())
    }

    #[test]
    fn unsupported_config_version_is_rejected() -> Result<()> {
        let (_temp, path) = write_config("version = 1\n")?;
        let error = Config::load(&path).expect_err("v1 config should fail");
        assert!(error.to_string().contains("unsupported config version 1"));
        Ok(())
    }

    #[test]
    fn default_path_honors_env_override() -> Result<()> {
        let _guard = env_lock();
        let temp = tempfile::tempdir()?;
        let override_path = temp.path().join("custom-config.toml");
        // SAFETY: test-only process-local env mutation.
        unsafe {
            std::env::set_var("MICASA_CONFIG_PATH", &override_path);
        }
        let resolved = Config::default_path()?;
        // SAFETY: test cleanup for process-local env mutation.
        unsafe {
            std::env::remove_var("MICASA_CONFIG_PATH");
        }
        assert_eq!(resolved, override_path);
        Ok(())
    }

    #[test]
    fn default_path_uses_config_toml_suffix_when_no_env_override() -> Result<()> {
        let _guard = env_lock();
        // SAFETY: test-only process-local env mutation.
        unsafe {
            std::env::remove_var("MICASA_CONFIG_PATH");
        }
        let path = Config::default_path()?;
        assert!(path.ends_with("config.toml"));
        Ok(())
    }

    #[test]
    fn db_path_prefers_storage_config_over_env_override() -> Result<()> {
        let _guard = env_lock();
        let (_temp, path) =
            write_config("version = 2\n[storage]\ndb_path = \"/explicit/from-config.db\"\n")?;
        // SAFETY: test-only process-local env mutation.
        unsafe {
            std::env::set_var("MICASA_DB_PATH", "/from/env.db");
        }
        let config = Config::load(&path)?;
        // SAFETY: test cleanup for process-local env mutation.
        unsafe {
            std::env::remove_var("MICASA_DB_PATH");
        }
        assert_eq!(config.db_path()?, PathBuf::from("/explicit/from-config.db"));
        Ok(())
    }

    #[test]
    fn db_path_uses_env_override_when_storage_db_path_missing() -> Result<()> {
        let _guard = env_lock();
        let (_temp, path) = write_config("version = 2\n")?;
        // SAFETY: test-only process-local env mutation.
        unsafe {
            std::env::set_var("MICASA_DB_PATH", "/from/env-only.db");
        }
        let config = Config::load(&path)?;
        let resolved = config.db_path()?;
        // SAFETY: test cleanup for process-local env mutation.
        unsafe {
            std::env::remove_var("MICASA_DB_PATH");
        }
        assert_eq!(resolved, PathBuf::from("/from/env-only.db"));
        Ok(())
    }

    #[test]
    fn db_path_defaults_to_micasa_db_when_unset() -> Result<()> {
        let _guard = env_lock();
        let (_temp, path) = write_config("version = 2\n")?;
        // SAFETY: test-only process-local env mutation.
        unsafe {
            std::env::remove_var("MICASA_DB_PATH");
        }
        let config = Config::load(&path)?;
        let resolved = config.db_path()?;
        assert!(
            resolved.ends_with("micasa.db"),
            "got {}",
            resolved.display()
        );
        Ok(())
    }

    #[test]
    fn db_path_rejects_uri_style_storage_value() -> Result<()> {
        let (_temp, path) =
            write_config("version = 2\n[storage]\ndb_path = \"https://evil.example/micasa.db\"\n")?;
        let error = Config::load(&path).expect_err("URI db_path should fail validation");
        let message = error.to_string();
        assert!(
            message.contains("looks like a URI") || message.contains("filesystem path"),
            "unexpected message: {message}"
        );
        Ok(())
    }

    #[test]
    fn llm_base_url_trims_trailing_slashes() -> Result<()> {
        let (_temp, path) = write_config(
            "version = 2\n[llm]\nbase_url = \"http://localhost:11434/v1///\"\nmodel = \"qwen3\"\n",
        )?;
        let config = Config::load(&path)?;
        assert_eq!(config.llm_base_url(), "http://localhost:11434/v1");
        Ok(())
    }

    #[test]
    fn llm_timeout_parses_ms_seconds_and_minutes() -> Result<()> {
        assert_eq!(parse_duration("500ms")?, Duration::from_millis(500));
        assert_eq!(parse_duration("5s")?, Duration::from_secs(5));
        assert_eq!(parse_duration("2m")?, Duration::from_secs(120));
        Ok(())
    }

    #[test]
    fn llm_timeout_rejects_invalid_duration() {
        let error = parse_duration("oops").expect_err("invalid duration should fail");
        let message = error.to_string();
        assert!(
            message.contains("invalid duration") || message.contains("invalid timeout duration"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn llm_timeout_rejects_non_positive_values_in_config() -> Result<()> {
        let (_temp, path) = write_config(
            "version = 2\n[llm]\nbase_url = \"http://localhost:11434/v1\"\nmodel = \"qwen3\"\ntimeout = \"0s\"\n",
        )?;
        let error = Config::load(&path).expect_err("zero timeout should fail");
        assert!(error.to_string().contains("must be positive"));
        Ok(())
    }

    #[test]
    fn storage_limits_are_validated() -> Result<()> {
        let (_temp, path) =
            write_config("version = 2\n[storage]\nmax_document_size = 0\ncache_ttl_days = -1\n")?;
        let error = Config::load(&path).expect_err("invalid storage values should fail");
        let message = error.to_string();
        assert!(
            message.contains("must be positive") || message.contains("must be non-negative"),
            "unexpected message: {message}"
        );
        Ok(())
    }

    #[test]
    fn example_config_includes_required_sections() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("config.toml");
        let example = Config::example_config(&path);
        assert!(example.contains("version = 2"));
        assert!(example.contains("[storage]"));
        assert!(example.contains("[ui]"));
        assert!(example.contains("[llm]"));
        Ok(())
    }
}

// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

mod config;

use anyhow::{Context, Result};
use config::Config;
use micasa_app::{AppState, DashboardCounts, TabKind};
use micasa_db::Store;
use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let mut config_path = Config::default_path()?;
    let mut print_path = false;
    let mut print_example = false;
    let mut check_only = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--config requires a file path"))?;
                config_path = PathBuf::from(value);
            }
            "--print-config-path" => {
                print_path = true;
            }
            "--print-example-config" => {
                print_example = true;
            }
            "--check" => {
                check_only = true;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "unknown argument {arg:?}; run with --help to see supported options"
                ));
            }
        }
    }

    if print_path {
        println!("{}", config_path.display());
        return Ok(());
    }

    if print_example {
        print!("{}", Config::example_config(&config_path));
        return Ok(());
    }

    let config = Config::load(&config_path).with_context(|| {
        format!(
            "load config {}; run `micasa --print-example-config` to generate a v2 template",
            config_path.display()
        )
    })?;

    let db_path = config.db_path()?;
    let mut store = Store::open(&db_path).with_context(|| {
        format!(
            "open database {} -- if this path is wrong, set [storage].db_path or MICASA_DB_PATH",
            db_path.display()
        )
    })?;
    store.bootstrap()?;
    store.set_max_document_size(config.max_document_size())?;

    let cache_dir = micasa_db::document_cache_dir()?;
    let _removed = micasa_db::evict_stale_cache(&cache_dir, config.cache_ttl_days())?;

    if config.llm_enabled() {
        let _client = micasa_llm::Client::new(
            config.llm_base_url(),
            config.llm_model(),
            config.llm_timeout()?,
        )
        .with_context(|| {
            format!(
                "invalid [llm] config in {}; fix base_url/model/timeout values",
                config_path.display()
            )
        })?;
        let _llm_extra_context = config.llm_extra_context();
    }

    if check_only {
        return Ok(());
    }

    let mut state = AppState::default();
    if !config.show_dashboard() {
        state.active_tab = TabKind::Projects;
    }

    let counts = if config.show_dashboard() {
        store.dashboard_counts()?
    } else {
        DashboardCounts {
            projects_due: 0,
            maintenance_due: 0,
            incidents_open: 0,
        }
    };

    micasa_tui::run_app(&mut state, counts)
}

fn print_help() {
    println!("micasa (Rust)");
    println!("  --config <path>          Use a specific config path");
    println!("  --print-config-path      Print resolved config path");
    println!("  --print-example-config   Print a v2 config template");
    println!("  --check                  Validate config + DB + startup dependencies");
    println!("  --help                   Show this help");
}

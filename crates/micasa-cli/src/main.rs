// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

mod config;
mod runtime;

use anyhow::{Context, Result};
use config::Config;
use micasa_app::{AppState, TabKind};
use micasa_db::Store;
use runtime::DbRuntime;
use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let options = parse_cli_args(env::args().skip(1), Config::default_path()?)?;
    if options.show_help {
        print_help();
        return Ok(());
    }

    if options.print_config_path {
        println!("{}", options.config_path.display());
        return Ok(());
    }

    if options.print_example {
        print!("{}", Config::example_config(&options.config_path));
        return Ok(());
    }

    let config = Config::load(&options.config_path).with_context(|| {
        format!(
            "load config {}; run `micasa --print-example-config` to generate a v2 template",
            options.config_path.display()
        )
    })?;

    let db_path = if options.demo {
        PathBuf::from(":memory:")
    } else {
        config.db_path()?
    };
    if options.print_db_path {
        println!("{}", db_path.display());
        return Ok(());
    }

    let mut store = Store::open(&db_path).with_context(|| {
        format!(
            "open database {} -- if this path is wrong, set [storage].db_path or MICASA_DB_PATH",
            db_path.display()
        )
    })?;
    store.bootstrap()?;
    store.set_max_document_size(config.max_document_size())?;
    if options.demo {
        store.seed_demo_data()?;
    }

    let cache_dir = micasa_db::document_cache_dir()?;
    let _removed = micasa_db::evict_stale_cache(&cache_dir, config.cache_ttl_days())?;

    let llm_client = if config.llm_enabled() {
        Some(
            micasa_llm::Client::new(
                config.llm_base_url(),
                config.llm_model(),
                config.llm_timeout()?,
            )
            .with_context(|| {
                format!(
                    "invalid [llm] config in {}; fix base_url/model/timeout values",
                    options.config_path.display()
                )
            })?,
        )
    } else {
        None
    };
    if options.check_only {
        return Ok(());
    }

    let show_dashboard = store
        .get_show_dashboard_override()?
        .unwrap_or_else(|| config.show_dashboard());

    let mut state = AppState::default();
    if !show_dashboard {
        state.active_tab = TabKind::Projects;
    }

    let mut runtime = DbRuntime::with_llm_client_context_and_db_path(
        &store,
        llm_client,
        config.llm_extra_context(),
        Some(db_path),
    );
    micasa_tui::run_app(&mut state, &mut runtime)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    config_path: PathBuf,
    print_config_path: bool,
    print_db_path: bool,
    demo: bool,
    print_example: bool,
    check_only: bool,
    show_help: bool,
}

fn parse_cli_args<I, S>(args: I, default_config_path: PathBuf) -> Result<CliOptions>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut options = CliOptions {
        config_path: default_config_path,
        print_config_path: false,
        print_db_path: false,
        demo: false,
        print_example: false,
        check_only: false,
        show_help: false,
    };

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--config" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--config requires a file path"))?;
                options.config_path = PathBuf::from(value.as_ref());
            }
            "--print-config-path" => {
                options.print_config_path = true;
            }
            "--print-path" => {
                options.print_db_path = true;
            }
            "--print-example-config" => {
                options.print_example = true;
            }
            "--demo" => {
                options.demo = true;
            }
            "--check" => {
                options.check_only = true;
            }
            "--help" | "-h" => {
                options.show_help = true;
            }
            unknown => {
                return Err(anyhow::anyhow!(
                    "unknown argument {unknown:?}; run with --help to see supported options"
                ));
            }
        }
    }

    Ok(options)
}

fn print_help() {
    println!("micasa (Rust)");
    println!("  --config <path>          Use a specific config path");
    println!("  --print-config-path      Print resolved config path");
    println!("  --print-path             Print resolved database path");
    println!("  --print-example-config   Print a v2 config template");
    println!("  --demo                   Launch with seeded demo data (in-memory)");
    println!("  --check                  Validate config + DB + startup dependencies");
    println!("  --help                   Show this help");
}

#[cfg(test)]
mod tests {
    use super::{CliOptions, parse_cli_args};
    use anyhow::Result;
    use std::path::PathBuf;

    fn default_options_path() -> PathBuf {
        PathBuf::from("/tmp/micasa-config.toml")
    }

    #[test]
    fn parse_cli_args_defaults_to_provided_config_path() -> Result<()> {
        let options = parse_cli_args(Vec::<String>::new(), default_options_path())?;
        assert_eq!(
            options,
            CliOptions {
                config_path: default_options_path(),
                print_config_path: false,
                print_db_path: false,
                demo: false,
                print_example: false,
                check_only: false,
                show_help: false,
            }
        );
        Ok(())
    }

    #[test]
    fn parse_cli_args_sets_config_path_override() -> Result<()> {
        let options = parse_cli_args(
            vec!["--config", "/custom/config.toml"],
            default_options_path(),
        )?;
        assert_eq!(options.config_path, PathBuf::from("/custom/config.toml"));
        Ok(())
    }

    #[test]
    fn parse_cli_args_errors_for_missing_config_value() {
        let error = parse_cli_args(vec!["--config"], default_options_path())
            .expect_err("missing config value should fail");
        assert!(error.to_string().contains("--config requires a file path"));
    }

    #[test]
    fn parse_cli_args_errors_for_unknown_argument() {
        let error = parse_cli_args(vec!["--wat"], default_options_path())
            .expect_err("unknown arg should fail");
        let message = error.to_string();
        assert!(message.contains("unknown argument"));
        assert!(message.contains("--help"));
    }

    #[test]
    fn parse_cli_args_sets_print_and_check_flags() -> Result<()> {
        let options = parse_cli_args(
            vec!["--print-config-path", "--print-example-config", "--check"],
            default_options_path(),
        )?;
        assert!(options.print_config_path);
        assert!(!options.print_db_path);
        assert!(!options.demo);
        assert!(options.print_example);
        assert!(options.check_only);
        assert!(!options.show_help);
        Ok(())
    }

    #[test]
    fn parse_cli_args_sets_demo_and_db_path_print_flags() -> Result<()> {
        let options = parse_cli_args(vec!["--demo", "--print-path"], default_options_path())?;
        assert!(!options.print_config_path);
        assert!(options.print_db_path);
        assert!(options.demo);
        Ok(())
    }

    #[test]
    fn parse_cli_args_sets_help_flag_for_long_and_short_variants() -> Result<()> {
        let long = parse_cli_args(vec!["--help"], default_options_path())?;
        assert!(long.show_help);

        let short = parse_cli_args(vec!["-h"], default_options_path())?;
        assert!(short.show_help);
        Ok(())
    }
}

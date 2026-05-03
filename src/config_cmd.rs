//! Config command implementation
//!
//! Shows the current Magellan configuration.

use anyhow::Result;
use magellan::config::{self, Config};
use magellan::output::OutputFormat;
use std::path::PathBuf;

/// Show the current configuration
pub fn run_config_show(
    output_format: OutputFormat,
) -> Result<()> {
    let cfg = config::load()?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            println!("{}", serde_json::to_string_pretty(&cfg)?);
        }
        OutputFormat::Human => {
            println!("Magellan Configuration");
            println!("====================");
            println!();
            println!("LLM Provider: {:?}", cfg.llm.provider);
            println!("  Base URL: {}", cfg.llm.base_url);
            println!("  Model: {}", cfg.llm.model);
            if !cfg.llm.api_key.is_empty() {
                println!("  API Key: (set)");
            } else {
                println!("  API Key: (not set)");
            }
            println!();
            println!("Registry:");
            println!("  Auto-scan: {}", cfg.registry.auto_scan);
            println!("  Scan roots: {:?}", cfg.registry.scan_roots);
            println!();
            println!("Config path: {}", config::default_config_path().display());
        }
    }

    Ok(())
}

/// Create a default config file
pub fn run_config_init(
    force: bool,
) -> Result<()> {
    let path = config::default_config_path();

    if path.exists() && !force {
        println!("Config already exists at {}. Use --force to overwrite.", path.display());
        return Ok(());
    }

    let cfg = Config::default();
    config::save(&cfg)?;
    println!("Created default config at {}", path.display());
    println!("Edit this file to configure LLM providers and registry settings.");

    Ok(())
}
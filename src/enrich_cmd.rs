//! Enrich command implementation
//!
//! Enriches symbols with type signatures and documentation from LSP tools.

use anyhow::Result;
use magellan::lsp;
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the enrich command
///
/// Enriches symbols with LSP data (type signatures, documentation).
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `files` - Optional list of files to enrich (None = all files)
/// * `timeout_secs` - Timeout per file in seconds
///
/// # Returns
/// Result indicating success or failure
pub fn run_enrich(
    db_path: PathBuf,
    files: Option<Vec<PathBuf>>,
    timeout_secs: u64,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    // Build command args for execution tracking
    let mut args = vec!["enrich".to_string()];
    if let Some(ref files) = files {
        for file in files {
            args.push("--file".to_string());
            args.push(file.to_string_lossy().to_string());
        }
    }
    if timeout_secs != 30 {
        args.push("--timeout".to_string());
        args.push(timeout_secs.to_string());
    }

    // Start execution tracking
    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path.to_string_lossy(),
    )?;

    // Create enrichment configuration
    let config = lsp::enrich::EnrichConfig {
        analyzers: None, // Use all available analyzers
        files,
        timeout_secs,
    };

    // Run enrichment
    let result = lsp::enrich::enrich_symbols(&mut graph, &config)?;

    // Finish execution tracking
    graph.execution_log().finish_execution(
        &exec_id,
        "success",
        None,
        0, // files_indexed
        0, // symbols_indexed
        0, // references_indexed
    )?;

    println!("\nEnrichment Summary:");
    println!("  Files processed: {}", result.files_processed);
    println!("  Symbols enriched: {}", result.symbols_enriched);
    println!("  Errors: {}", result.errors);

    Ok(())
}

//! Enrich command implementation
//!
//! Enriches symbols with type signatures and documentation from LSP tools.

use anyhow::Result;
use magellan::lsp;
use magellan::CodeGraph;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

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
pub fn run_enrich(db_path: PathBuf, files: Option<Vec<PathBuf>>, timeout_secs: u64) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
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

    let mut tracker = ExecutionTracker::new(args, None, db_path.to_string_lossy().to_string());
    tracker.start(&graph)?;

    let result: Result<()> = {
        let config = lsp::enrich::EnrichConfig {
            analyzers: None,
            files,
            timeout_secs,
        };

        let enrich_result = lsp::enrich::enrich_symbols(&mut graph, &config)?;

        println!("\nEnrichment Summary:");
        println!("  Files processed: {}", enrich_result.files_processed);
        println!("  Symbols enriched: {}", enrich_result.symbols_enriched);
        println!("  Errors: {}", enrich_result.errors);

        Ok(())
    };

    if let Err(err) = &result {
        tracker.set_error(format!("{err:#}"));
    }
    tracker.finish(&graph)?;
    result
}

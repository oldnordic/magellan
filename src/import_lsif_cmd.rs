//! Import LSIF command implementation
//!
//! Imports LSIF data from external packages for cross-repository symbol resolution.

use anyhow::Result;
use magellan::lsif;
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the import-lsif command
///
/// Imports LSIF data from external packages.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `lsif_paths` - Paths to LSIF files to import
///
/// # Returns
/// Result indicating success or failure
pub fn run_import_lsif(db_path: PathBuf, lsif_paths: Vec<PathBuf>) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    // Build command args for execution tracking
    let mut args = vec!["import-lsif".to_string()];
    for path in &lsif_paths {
        args.push(path.to_string_lossy().to_string());
    }

    // Start execution tracking
    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path.to_string_lossy(),
    )?;

    let mut total_imported = 0usize;
    let mut total_symbols = 0usize;

    for lsif_path in &lsif_paths {
        println!("Importing {:?}", lsif_path);
        
        match lsif::import::import_lsif(lsif_path) {
            Ok(pkg) => {
                println!(
                    "  Package: {} v{} ({} symbols, {} documents)",
                    pkg.package.name, pkg.package.version, pkg.symbol_count, pkg.document_count
                );
                total_imported += 1;
                total_symbols += pkg.symbol_count;
            }
            Err(e) => {
                eprintln!("  Warning: Failed to import {:?}: {}", lsif_path, e);
            }
        }
    }

    println!("\nImported {} package(s) with {} total symbols", total_imported, total_symbols);
    println!("Note: LSIF data is currently parsed for information only.");
    println!("Cross-repo symbol resolution will be available in a future version.");

    // Finish execution tracking
    graph.execution_log().finish_execution(
        &exec_id,
        "success",
        None,
        0, // files_indexed
        0, // symbols_indexed
        0, // references_indexed
    )?;

    Ok(())
}

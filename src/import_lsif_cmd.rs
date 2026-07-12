//! Import LSIF command implementation
//!
//! Imports LSIF data from external packages for cross-repository symbol resolution.

use anyhow::Result;
use magellan::lsif;
use magellan::CodeGraph;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

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
    let graph = CodeGraph::open(&db_path)?;
    let mut args = vec!["import-lsif".to_string()];
    for path in &lsif_paths {
        args.push(path.to_string_lossy().to_string());
    }

    let mut tracker = ExecutionTracker::new(args, None, db_path.to_string_lossy().to_string());
    tracker.start(&graph)?;
    let exec_id = tracker.exec_id().to_string();

    let result: Result<()> = {
        let mut total_imported = 0usize;
        let mut total_symbols = 0usize;

        graph
            .telemetry()
            .record_phase_start(&exec_id, "import_lsif")?;
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
        graph
            .telemetry()
            .record_phase_end(&exec_id, "import_lsif")?;

        graph.telemetry().record_phase_start(&exec_id, "output")?;
        println!(
            "\nImported {} package(s) with {} total symbols",
            total_imported, total_symbols
        );
        println!("Note: LSIF data is currently parsed for information only.");
        println!("Cross-repo symbol resolution will be available in a future version.");
        graph.telemetry().record_phase_end(&exec_id, "output")?;

        Ok(())
    };

    if let Err(err) = &result {
        tracker.set_error(format!("{err:#}"));
    }
    tracker.finish(&graph)?;
    result
}

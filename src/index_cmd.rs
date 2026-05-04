//! Index command - Single-file indexing
//!
//! Indexes a single source file into the database.

use anyhow::{Context, Result};
use magellan::CodeGraph;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

/// Run the index command
///
/// Usage: magellan index --db <FILE> --file <PATH> [--root <DIR>]
pub fn run_index(db_path: PathBuf, file_path: PathBuf, root: Option<PathBuf>) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let path_str = if let Some(ref root_path) = root {
        root_path.join(&file_path).to_string_lossy().to_string()
    } else {
        file_path.to_string_lossy().to_string()
    };

    let tracker = ExecutionTracker::new(
        vec!["index".to_string(), path_str.clone()],
        root.as_ref().map(|p| p.to_string_lossy().to_string()),
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let source =
        std::fs::read(&path_str).with_context(|| format!("Failed to read file: {}", path_str))?;

    let symbol_count = graph.index_file(&path_str, &source)?;

    println!("Indexed {}: {} symbols", path_str, symbol_count);

    tracker.finish(&graph)?;
    Ok(())
}

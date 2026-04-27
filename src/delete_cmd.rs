//! Delete command - Remove a file from the index
//!
//! Deletes all facts associated with a specific file path.

use anyhow::{Context, Result};
use magellan::CodeGraph;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

/// Run the delete command
///
/// Usage: magellan delete --db <FILE> --file <PATH> [--root <DIR>]
pub fn run_delete(db_path: PathBuf, file_path: PathBuf, root: Option<PathBuf>) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let path_str = if let Some(ref root_path) = root {
        root_path.join(&file_path).to_string_lossy().to_string()
    } else {
        file_path.to_string_lossy().to_string()
    };

    let tracker = ExecutionTracker::new(
        vec!["delete".to_string(), path_str.clone()],
        root.as_ref().map(|p| p.to_string_lossy().to_string()),
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let result = graph
        .delete_file_facts(&path_str)
        .with_context(|| format!("Failed to delete file facts: {}", path_str))?;

    println!("Deleted {}:", path_str);
    println!("  symbols: {}", result.symbols_deleted);
    println!("  references: {}", result.references_deleted);
    println!("  calls: {}", result.calls_deleted);
    println!("  chunks: {}", result.chunks_deleted);
    println!("  ast_nodes: {}", result.ast_nodes_deleted);
    println!("  cfg_blocks: {}", result.cfg_blocks_deleted);
    println!("  edges: {}", result.edges_deleted);
    println!("  total: {}", result.total_deleted());

    tracker.finish(&graph)?;
    Ok(())
}

//! Backfill command - Recompute metrics and derived data
//!
//! Triggers recomputation of all metrics in the database.

use anyhow::Result;
use magellan::CodeGraph;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

/// Run the backfill command
///
/// Usage: magellan backfill --db <FILE>
pub fn run_backfill(db_path: PathBuf) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let mut tracker = ExecutionTracker::new(
        vec!["backfill".to_string()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let result = (|| -> Result<()> {
        graph.backfill_metrics(None)?;
        println!("Backfill complete");
        Ok(())
    })();

    if let Err(err) = &result {
        tracker.set_error(format!("{err:#}"));
    }
    tracker.finish(&graph)?;
    result
}

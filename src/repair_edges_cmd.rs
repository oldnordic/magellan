//! Repair-edges command implementation
//!
//! Recomputes CALLER/CALLS edges from the stable symbol IDs persisted on Call
//! nodes (BUG-2 repair pass). Dry-run by default; `--apply` rewrites the
//! mis-wired edges transactionally.

use anyhow::Result;
use magellan::output::{output_json, OutputFormat};
use magellan::repair_call_edges;
use std::path::PathBuf;

/// Run the repair-edges command
///
/// Reports how many CALLER/CALLS edges are mis-wired relative to the stable
/// symbol IDs persisted on Call nodes. With `apply = true`, rewrites them.
pub fn run_repair_edges(db_path: PathBuf, apply: bool, output_format: OutputFormat) -> Result<()> {
    let report = repair_call_edges(&db_path, apply)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => output_json(&report, output_format)?,
        OutputFormat::Human => {
            let mode = if report.applied { "apply" } else { "dry-run" };
            println!("repair-edges ({mode}): {}", db_path.display());
            println!("  call nodes inspected:      {}", report.call_nodes_total);
            println!(
                "  with persisted stable IDs: {}",
                report.call_nodes_with_stable_ids
            );
            println!(
                "  mis-wired CALLER edges:    {}",
                report.caller_edges_miswired
            );
            println!(
                "  mis-wired CALLS edges:     {}",
                report.calls_edges_miswired
            );
            println!(
                "  missing CALLER edges:      {}",
                report.caller_edges_missing
            );
            println!(
                "  missing CALLS edges:       {}",
                report.calls_edges_missing
            );
            println!(
                "  unresolved stable IDs:     {}",
                report.unresolved_stable_ids
            );
            if report.applied {
                println!("  edges deleted:             {}", report.edges_deleted);
                println!("  edges inserted:            {}", report.edges_inserted);
            } else {
                println!("  (dry-run: no changes written; rerun with --apply to rewrite)");
            }
        }
    }

    Ok(())
}

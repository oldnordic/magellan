//! Cross-file references command implementation
//!
//! Shows references to a symbol that originate from other files.

use anyhow::Result;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::{cross_file_references_to, CodeGraph};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

/// Response for cross-file-refs command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossFileRefMatch {
    pub from_symbol_id: String,
    pub file_path: String,
    pub line_number: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Run cross-file references query
/// Usage: magellan cross-file-refs --db <FILE> --fqn <FQN>
pub fn run_cross_file_refs(
    db_path: PathBuf,
    fqn: String,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let tracker = ExecutionTracker::new(
        vec!["cross-file-refs".to_string(), fqn.clone()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let refs = cross_file_references_to(&graph, &fqn)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let matches: Vec<CrossFileRefMatch> = refs
                .into_iter()
                .map(|r| CrossFileRefMatch {
                    from_symbol_id: r.from_symbol_id,
                    file_path: r.file_path,
                    line_number: r.line_number,
                    byte_start: r.byte_start,
                    byte_end: r.byte_end,
                })
                .collect();

            let exec_id = tracker.exec_id().to_string();
            let json_response = JsonResponse::new(matches, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            if refs.is_empty() {
                println!("No cross-file references to '{}'", fqn);
            } else {
                println!("{} cross-file reference(s) to '{}':", refs.len(), fqn);
                for r in refs {
                    println!(
                        "  {} at {}:{} (bytes {}-{})",
                        r.from_symbol_id, r.file_path, r.line_number, r.byte_start, r.byte_end
                    );
                }
            }
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}

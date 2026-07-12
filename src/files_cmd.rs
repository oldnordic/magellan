//! Files command implementation
//!
//! Lists all indexed files with optional symbol counts.

use anyhow::Result;
use magellan::graph::query::symbols_in_file;
use magellan::output::{output_json, FilesResponse, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

/// Run the files command
///
/// Lists all indexed files from the database. Optionally includes symbol counts
/// per file when the `with_symbols` flag is set.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `with_symbols` - Whether to include symbol counts per file
/// * `output_format` - Output format (Human or Json)
///
/// # Returns
/// Result indicating success or failure
pub fn run_files(db_path: PathBuf, with_symbols: bool, output_format: OutputFormat) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let mut tracker = ExecutionTracker::new(
        vec!["files".to_string()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;
    let exec_id = tracker.exec_id().to_string();

    let result = (|| -> Result<()> {
        graph
            .telemetry()
            .record_phase_start(&exec_id, "query_files")?;

        let file_nodes = graph.all_file_nodes()?;

        let symbol_counts = if with_symbols {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for file_path in file_nodes.keys() {
                if let Ok(symbols) = symbols_in_file(&mut graph, file_path) {
                    counts.insert(file_path.clone(), symbols.len());
                } else {
                    counts.insert(file_path.clone(), 0);
                }
            }
            Some(counts)
        } else {
            None
        };

        let mut files: Vec<String> = file_nodes.keys().cloned().collect();
        files.sort();

        graph
            .telemetry()
            .record_phase_end(&exec_id, "query_files")?;

        let file_count = files.len();
        let symbol_count = symbol_counts
            .as_ref()
            .map(|c| c.values().sum())
            .unwrap_or(0);
        tracker.set_counts(file_count, symbol_count, 0);

        match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                graph
                    .telemetry()
                    .record_phase_start(&exec_id, "build_response")?;

                let response = FilesResponse {
                    files,
                    symbol_counts,
                };

                let json_response = JsonResponse::new(response, &exec_id);
                output_json(&json_response, output_format)?;

                graph
                    .telemetry()
                    .record_phase_end(&exec_id, "build_response")?;
            }
            OutputFormat::Human => {
                graph.telemetry().record_phase_start(&exec_id, "output")?;

                if files.is_empty() {
                    println!("0 indexed files");
                } else {
                    println!("{} indexed files:", files.len());
                    for path in &files {
                        if let Some(ref counts) = symbol_counts {
                            let count = counts.get(path).unwrap_or(&0);
                            println!("  {} ({} symbols)", path, count);
                        } else {
                            println!("  {}", path);
                        }
                    }
                }

                graph.telemetry().record_phase_end(&exec_id, "output")?;
            }
        }

        Ok(())
    })();

    if let Err(err) = &result {
        tracker.set_error(format!("{err:#}"));
    }
    tracker.finish(&graph)?;
    result
}

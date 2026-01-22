//! Files command implementation
//!
//! Lists all indexed files with optional symbol counts.

use anyhow::Result;
use magellan::CodeGraph;
use magellan::graph::query::symbols_in_file;
use magellan::output::{FilesResponse, JsonResponse, generate_execution_id, output_json, OutputFormat};
use std::collections::HashMap;
use std::path::PathBuf;

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
pub fn run_files(
    db_path: PathBuf,
    with_symbols: bool,
    output_format: OutputFormat,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    // Start execution tracking
    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &["files".to_string()],
        None,
        &db_path.to_string_lossy(),
    )?;

    let file_nodes = graph.all_file_nodes()?;

    // Build symbol counts map if requested
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

    // Sort files deterministically (alphabetically)
    let mut files: Vec<String> = file_nodes.keys().cloned().collect();
    files.sort();

    // Get counts for execution tracking before moving
    let file_count = files.len();
    let symbol_count = symbol_counts.as_ref().map(|c| c.values().sum()).unwrap_or(0);

    // Handle output based on format
    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = FilesResponse {
                files,
                symbol_counts,
            };

            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
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
        }
    }

    // Finish execution tracking
    graph.execution_log().finish_execution(
        &exec_id,
        "success",
        None,
        file_count,
        symbol_count,
        0,
    )?;

    Ok(())
}

//! Export command implementation
//!
//! Exports graph data to JSON/JSONL formats with stable IDs.

use anyhow::Result;
use magellan::CodeGraph;
use magellan::graph::export::{export_graph, ExportConfig, ExportFormat};
use magellan::output::generate_execution_id;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Run the export command
///
/// Exports graph data to JSON or JSONL format with stable IDs.
/// Output goes to stdout by default, or to a file if --output is specified.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `format` - Export format (Json, JsonL, Dot, Csv)
/// * `output` - Optional file path for output
/// * `include_symbols` - Whether to include symbols in export
/// * `include_references` - Whether to include references in export
/// * `include_calls` - Whether to include calls in export
/// * `minify` - Whether to minify JSON output
///
/// # Returns
/// Result indicating success or failure
pub fn run_export(
    db_path: PathBuf,
    format: ExportFormat,
    output: Option<PathBuf>,
    include_symbols: bool,
    include_references: bool,
    include_calls: bool,
    minify: bool,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    // Build command args for execution tracking
    let mut args = vec!["export".to_string()];
    args.push("--format".to_string());
    args.push(format_name(format));
    if let Some(ref out_path) = output {
        args.push("--output".to_string());
        args.push(out_path.to_string_lossy().to_string());
    }
    if !include_symbols {
        args.push("--no-symbols".to_string());
    }
    if !include_references {
        args.push("--no-references".to_string());
    }
    if !include_calls {
        args.push("--no-calls".to_string());
    }
    if minify {
        args.push("--minify".to_string());
    }

    // Start execution tracking
    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path.to_string_lossy(),
    )?;

    // Build export config
    let config = ExportConfig {
        format,
        include_symbols,
        include_references,
        include_calls,
        minify,
        filters: None,
    };

    // Export graph data
    let exported_data = export_graph(&mut graph, &config)?;

    // Write output
    match output {
        Some(path) => {
            // Write to file
            let mut file = File::create(&path)?;
            file.write_all(exported_data.as_bytes())?;
            file.write_all(b"\n")?;
        }
        None => {
            // Write to stdout
            println!("{}", exported_data);
        }
    }

    // Finish execution tracking
    graph.execution_log().finish_execution(
        &exec_id,
        "success",
        None,
        0,  // files_indexed
        0,  // symbols_indexed
        0,  // references_indexed
    )?;

    Ok(())
}

/// Get format name for display/logging
fn format_name(format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => "json".to_string(),
        ExportFormat::JsonL => "jsonl".to_string(),
        ExportFormat::Dot => "dot".to_string(),
        ExportFormat::Csv => "csv".to_string(),
    }
}

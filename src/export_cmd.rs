//! Export command implementation
//!
//! Exports graph data to JSON/JSONL/CSV/DOT/SCIP formats with stable IDs.

use anyhow::Result;
use magellan::CodeGraph;
use magellan::graph::export::{export_graph, scip, stream_json, stream_json_minified, stream_ndjson, ExportConfig, ExportFilters, ExportFormat};
use magellan::output::generate_execution_id;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

/// Run the export command
///
/// Exports graph data to JSON/JSONL/CSV/DOT/SCIP format with stable IDs.
/// Output goes to stdout by default, or to a file if --output is specified.
/// Note: SCIP format requires --output file since it outputs binary data.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `format` - Export format (Json, JsonL, Dot, Csv, Scip)
/// * `output` - Optional file path for output
/// * `include_symbols` - Whether to include symbols in export
/// * `include_references` - Whether to include references in export
/// * `include_calls` - Whether to include calls in export
/// * `minify` - Whether to minify JSON output
/// * `filters` - Export filters for DOT format (file, symbol, kind, max_depth, cluster)
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
    filters: ExportFilters,
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
    if let Some(ref file) = filters.file {
        args.push("--file".to_string());
        args.push(file.clone());
    }
    if let Some(ref symbol) = filters.symbol {
        args.push("--symbol".to_string());
        args.push(symbol.clone());
    }
    if let Some(ref kind) = filters.kind {
        args.push("--kind".to_string());
        args.push(kind.clone());
    }
    if let Some(max_depth) = filters.max_depth {
        args.push("--max-depth".to_string());
        args.push(max_depth.to_string());
    }
    if filters.cluster {
        args.push("--cluster".to_string());
    }

    // Start execution tracking
    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path.to_string_lossy(),
    )?;

    // Handle SCIP format specially (binary output)
    if format == ExportFormat::Scip {
        let scip_config = scip::ScipExportConfig {
            project_root: ".".to_string(),
            project_name: None,
            version: None,
        };
        let scip_bytes = scip::export_scip(&mut graph, &scip_config)?;

        // Write output (SCIP requires file output)
        match output {
            Some(path) => {
                let mut file = File::create(&path)?;
                file.write_all(&scip_bytes)?;
            }
            None => {
                // SCIP is binary, warn user but still write to stdout
                eprintln!("Warning: SCIP format is binary. Use --output file.scip for proper output.");
                io::stdout().write_all(&scip_bytes)?;
            }
        }
    } else {
        // Text-based formats
        let config = ExportConfig {
            format,
            include_symbols,
            include_references,
            include_calls,
            minify,
            filters,
        };

        // Use streaming for JSON and JSONL formats to reduce memory for large graphs
        match format {
            ExportFormat::Json => {
                // Stream JSON output to avoid loading entire graph into memory
                // Use minified version if requested
                if minify {
                    match output {
                        Some(path) => {
                            let mut file = File::create(&path)?;
                            stream_json_minified(&mut graph, &config, &mut file)?;
                        }
                        None => {
                            let stdout = io::stdout();
                            let mut handle = stdout.lock();
                            stream_json_minified(&mut graph, &config, &mut handle)?;
                        }
                    }
                } else {
                    match output {
                        Some(path) => {
                            let mut file = File::create(&path)?;
                            stream_json(&mut graph, &config, &mut file)?;
                        }
                        None => {
                            let stdout = io::stdout();
                            let mut handle = stdout.lock();
                            stream_json(&mut graph, &config, &mut handle)?;
                        }
                    }
                }
            }
            ExportFormat::JsonL => {
                // Stream JSONL output (naturally streaming-friendly)
                match output {
                    Some(path) => {
                        let mut file = File::create(&path)?;
                        stream_ndjson(&mut graph, &config, &mut file)?;
                    }
                    None => {
                        let stdout = io::stdout();
                        let mut handle = stdout.lock();
                        stream_ndjson(&mut graph, &config, &mut handle)?;
                    }
                }
            }
            _ => {
                // Other formats (DOT, CSV) use the existing in-memory export
                let exported_data = export_graph(&mut graph, &config)?;

                // Write output
                match output {
                    Some(path) => {
                        let mut file = File::create(&path)?;
                        file.write_all(exported_data.as_bytes())?;
                        file.write_all(b"\n")?;
                    }
                    None => {
                        println!("{}", exported_data);
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
        ExportFormat::Scip => "scip".to_string(),
    }
}

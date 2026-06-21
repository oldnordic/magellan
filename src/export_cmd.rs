//! Export command implementation
//!
//! Exports graph data to JSON/JSONL/CSV/DOT/SCIP formats with stable IDs.

use anyhow::Result;
use magellan::common::{find_repo_root, magellan_dir};
use magellan::graph::export::{
    export_graph, scip, stream_json, stream_json_minified, stream_ndjson, ExportConfig,
    ExportFilters, ExportFormat,
};
use magellan::graph::query::CollisionField;
use magellan::output::generate_execution_id;
use magellan::CodeGraph;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

/// Get default output path for repo-root export
///
/// When no output is specified, this function determines the appropriate
/// default file path in the .magellan directory based on the export format.
///
/// Uses current working directory for repo-root search since DB may be
/// outside the repository (e.g., ~/.magellan/).
///
/// # Arguments
/// * `format` - Export format
///
/// # Returns
/// Option containing the default output path, or None if not in a repository
fn get_default_repo_root_output(db_path: &std::path::Path, format: &ExportFormat) -> Option<PathBuf> {
    // Only use repo-root convention when we're in a consistent working directory
    // If the db is in a different location than cwd, assume ad-hoc/test usage
    let cwd = std::env::current_dir().ok()?;

    // Check if db is in a temp directory or different from cwd
    let db_parent = db_path.parent();
    let is_temp_or_adhoc = if let Some(parent) = db_parent {
        // Check if db is in a temp directory (starts with /tmp/) or different from cwd
        let parent_str = parent.to_string_lossy();
        let cwd_str = cwd.to_string_lossy();
        parent_str.starts_with("/tmp/") ||
        parent_str != cwd_str
    } else {
        // No parent, assume ad-hoc
        true
    };

    if is_temp_or_adhoc {
        // Ad-hoc or test usage - don't use repo-root convention
        return None;
    }

    // Normal usage - try to find repo root from current directory
    let repo_root = find_repo_root(&cwd)?;

    let mag_dir = magellan_dir(&repo_root);

    // Create .magellan directory if it doesn't exist
    fs::create_dir_all(&mag_dir).ok()?;

    let filename = match format {
        ExportFormat::Json => "export.json",
        ExportFormat::Impact => "impact.json",
        ExportFormat::JsonL => "export.jsonl",
        _ => return None,
    };

    Some(mag_dir.join(filename))
}

/// Format file size in human-readable format (KB, MB, etc.)
fn format_file_size(size_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size_bytes >= GB {
        format!("{:.1} GB", size_bytes as f64 / GB as f64)
    } else if size_bytes >= MB {
        format!("{:.1} MB", size_bytes as f64 / MB as f64)
    } else if size_bytes >= KB {
        format!("{:.1} KB", size_bytes as f64 / KB as f64)
    } else {
        format!("{} B", size_bytes)
    }
}

/// Print export success message
fn print_export_summary(
    output_path: &PathBuf,
    format: ExportFormat,
    graph: &mut CodeGraph,
) -> Result<()> {
    // Get file size
    let metadata = std::fs::metadata(output_path)?;
    let size = format_file_size(metadata.len());

    // Get counts from database
    let files = graph.count_files()?;
    let symbols = graph.count_symbols()?;
    let calls = graph.count_calls()?;

    // Print summary (to stderr so it doesn't interfere with stdout exports)
    eprintln!("Export complete: {}", output_path.display());
    eprintln!("  Format: {}", format_name(format));
    eprintln!("  Size: {}", size);
    eprintln!("  Files: {}", files);
    eprintln!("  Symbols: {}", symbols);
    eprintln!("  Calls: {}", calls);

    Ok(())
}

/// Run the export command
///
/// Exports graph data to JSON/JSONL/CSV/DOT/SCIP/Impact format with stable IDs.
/// Output goes to stdout by default, or to a file if --output is specified.
/// Note: SCIP format requires --output file since it outputs binary data.
/// Impact format requires --symbol parameter.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `format` - Export format (Json, JsonL, Dot, Csv, Scip, Impact)
/// * `output` - Optional file path for output
/// * `include_symbols` - Whether to include symbols in export
/// * `include_references` - Whether to include references in export
/// * `include_calls` - Whether to include calls in export
/// * `minify` - Whether to minify JSON output
/// * `filters` - Export filters for DOT format (file, symbol, kind, max_depth, cluster)
/// * `impact_symbol` - Symbol name for impact export (required for Impact format)
/// * `impact_file` - Optional file path for impact export symbol disambiguation
/// * `impact_depth` - Max depth for impact export BFS traversal
///
/// # Returns
/// Result indicating success or failure
#[allow(
    clippy::too_many_arguments,
    reason = "CLI command surface: each arg maps to a flag"
)]
pub fn run_export(
    db_path: PathBuf,
    format: ExportFormat,
    output: Option<PathBuf>,
    include_symbols: bool,
    include_references: bool,
    include_calls: bool,
    minify: bool,
    include_collisions: bool,
    collisions_field: CollisionField,
    filters: ExportFilters,
    impact_symbol: Option<String>,
    impact_file: Option<String>,
    impact_depth: usize,
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
    if include_collisions {
        args.push("--include-collisions".to_string());
        if collisions_field != CollisionField::Fqn {
            args.push("--collisions-field".to_string());
            args.push(collisions_field.as_str().to_string());
        }
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

    // Phase: export
    graph.telemetry().record_phase_start(&exec_id, "export")?;

    // Handle SCIP format specially (binary output)
    if format == ExportFormat::Scip {
        let scip_config = scip::ScipExportConfig {
            project_root: ".".to_string(),
            project_name: None,
            version: None,
        };
        let scip_bytes = scip::export_scip(&graph, &scip_config)?;

        // Write output (SCIP requires file output)
        match output {
            Some(path) => {
                let mut file = File::create(&path)?;
                file.write_all(&scip_bytes)?;
                print_export_summary(&path, format, &mut graph)?;
            }
            None => {
                // SCIP is binary, warn user but still write to stdout
                eprintln!(
                    "Warning: SCIP format is binary. Use --output file.scip for proper output."
                );
                io::stdout().write_all(&scip_bytes)?;
            }
        }
    // Handle LSIF format specially (JSONL output)
    } else if format == ExportFormat::Lsif {
        use magellan::lsif;

        // Get package info from Cargo.toml if available
        let (package_name, package_version) = detect_package_info(&db_path);

        // Export to LSIF
        match output {
            Some(path) => {
                let _count =
                    lsif::export::export_lsif(&mut graph, &path, &package_name, &package_version)?;
                print_export_summary(&path, format, &mut graph)?;
            }
            None => {
                eprintln!("Warning: LSIF format requires --output file.lsif");
                eprintln!("Use: magellan export --db code.db --format lsif --output output.lsif");
            }
        }
    // Handle Impact format specially (requires --symbol parameter)
    } else if format == ExportFormat::Impact {
        use magellan::context::impact_analysis;

        let symbol_name = impact_symbol.ok_or_else(|| {
            anyhow::anyhow!("Impact export requires --symbol parameter")
        })?;

        // Run impact analysis
        let impacted = impact_analysis(&mut graph, &symbol_name, impact_file.as_deref(), impact_depth)?;

        // Create export data structure
        let impact_export = serde_json::json!({
            "symbol": symbol_name,
            "file": impact_file,
            "depth": impact_depth,
            "total_impacted": impacted.len(),
            "impacted_symbols": impacted
        });

        // Write output
        let json_str = if minify {
            serde_json::to_string(&impact_export)?
        } else {
            serde_json::to_string_pretty(&impact_export)?
        };

        match output {
            Some(path) => {
                let mut file = File::create(&path)?;
                file.write_all(json_str.as_bytes())?;
                file.write_all(b"\n")?;
                eprintln!("Export complete: {}", path.display());
                eprintln!("  Format: impact");
                eprintln!("  Symbol: {}", symbol_name);
                eprintln!("  Total impacted: {}", impacted.len());
            }
            None => {
                // Use repo-root convention if available, otherwise stdout
                if let Some(default_path) = get_default_repo_root_output(&db_path, &format) {
                    let mut file = File::create(&default_path)?;
                    file.write_all(json_str.as_bytes())?;
                    file.write_all(b"\n")?;
                    eprintln!("Export complete: {}", default_path.display());
                    eprintln!("  Format: impact");
                    eprintln!("  Symbol: {}", symbol_name);
                    eprintln!("  Total impacted: {}", impacted.len());
                } else {
                    // Fall back to stdout
                    io::stdout().write_all(json_str.as_bytes())?;
                    io::stdout().write_all(b"\n")?;
                }
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
            include_collisions,
            collisions_field,
        };

        // Use streaming for JSON and JSONL formats to reduce memory for large graphs
        match format {
            ExportFormat::Json => {
                // Stream JSON output to avoid loading entire graph into memory
                // Use minified version if requested
                if minify {
                    match output {
                        Some(ref path) => {
                            let mut file = File::create(path)?;
                            stream_json_minified(&mut graph, &config, &mut file)?;
                            print_export_summary(path, format, &mut graph)?;
                        }
                        None => {
                            let stdout = io::stdout();
                            let mut handle = stdout.lock();
                            stream_json_minified(&mut graph, &config, &mut handle)?;
                        }
                    }
                } else {
                    match output {
                        Some(ref path) => {
                            let mut file = File::create(path)?;
                            stream_json(&mut graph, &config, &mut file)?;
                            print_export_summary(path, format, &mut graph)?;
                        }
                        None => {
                            // Use repo-root convention if available, otherwise stdout
                            if let Some(default_path) = get_default_repo_root_output(&db_path, &format) {
                                let mut file = File::create(&default_path)?;
                                stream_json(&mut graph, &config, &mut file)?;
                                print_export_summary(&default_path, format, &mut graph)?;
                            } else {
                                let stdout = io::stdout();
                                let mut handle = stdout.lock();
                                stream_json(&mut graph, &config, &mut handle)?;
                            }
                        }
                    }
                }
            }
            ExportFormat::JsonL => {
                // Stream JSONL output (naturally streaming-friendly)
                match output {
                    Some(ref path) => {
                        let mut file = File::create(path)?;
                        stream_ndjson(&mut graph, &config, &mut file)?;
                        print_export_summary(path, format, &mut graph)?;
                    }
                    None => {
                        // Use repo-root convention if available, otherwise stdout
                        if let Some(default_path) = get_default_repo_root_output(&db_path, &format) {
                            let mut file = File::create(&default_path)?;
                            stream_ndjson(&mut graph, &config, &mut file)?;
                            print_export_summary(&default_path, format, &mut graph)?;
                        } else {
                            let stdout = io::stdout();
                            let mut handle = stdout.lock();
                            stream_ndjson(&mut graph, &config, &mut handle)?;
                        }
                    }
                }
            }
            _ => {
                // Other formats (DOT, CSV) use the existing in-memory export
                let exported_data = export_graph(&mut graph, &config)?;

                // Write output
                match output {
                    Some(ref path) => {
                        let mut file = File::create(path)?;
                        file.write_all(exported_data.as_bytes())?;
                        file.write_all(b"\n")?;
                        print_export_summary(path, format, &mut graph)?;
                    }
                    None => {
                        println!("{}", exported_data);
                    }
                }
            }
        }
    }

    // Finish execution tracking
    graph.telemetry().record_phase_end(&exec_id, "export")?;
    graph.execution_log().finish_execution(
        &exec_id, "success", None, 0, // files_indexed
        0, // symbols_indexed
        0, // references_indexed
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
        ExportFormat::Lsif => "lsif".to_string(),
        ExportFormat::Impact => "impact".to_string(),
    }
}

/// Detect package name and version from Cargo.toml or directory name
fn detect_package_info(db_path: &std::path::Path) -> (String, String) {
    // Try to find Cargo.toml in parent directories
    let mut current = db_path.parent().unwrap_or(std::path::Path::new("."));

    for _ in 0..10 {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                let name = content
                    .lines()
                    .find(|l| l.starts_with("name = "))
                    .and_then(|l| l.split('"').nth(1))
                    .unwrap_or("unknown")
                    .to_string();

                let version = content
                    .lines()
                    .find(|l| l.starts_with("version = "))
                    .and_then(|l| l.split('"').nth(1))
                    .unwrap_or("0.1.0")
                    .to_string();

                return (name, version);
            }
        }
        current = current.parent().unwrap_or(std::path::Path::new("."));
        if current.as_os_str().is_empty() {
            break;
        }
    }

    // Fallback to directory name
    let dir_name = db_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    (dir_name, "0.1.0".to_string())
}

//! Refs command implementation
//!
//! Shows calls (incoming/outgoing) for a symbol.

use crate::service::registry::Registry;
use anyhow::Context;
use anyhow::Result;
use magellan::common::{detect_language_from_path, resolve_path};
use magellan::graph::query;
use magellan::graph::MultiDbContext;
use magellan::output::rich::{SpanChecksums, SpanContext};
use magellan::output::{
    output_json, JsonResponse, OutputFormat, ReferenceMatch, RefsResponse, Span,
};
use magellan::{CallFact, CodeGraph};
use std::path::{Path, PathBuf};

/// Represents a found symbol with its file path for refs lookup
struct RefsSymbol {
    file_path: String,
}

/// Find a symbol across all files by name
///
/// Returns all matching symbols with their file paths
fn find_symbol_all_files(graph: &mut CodeGraph, name: &str) -> Result<Vec<RefsSymbol>> {
    let mut results = Vec::new();

    // Get all indexed files
    let file_nodes = graph.all_file_nodes()?;

    // Search each file for the symbol
    for file_path in file_nodes.keys() {
        let entries = query::symbol_nodes_in_file_with_ids(graph, file_path)?;
        for (_node_id, symbol, _symbol_id) in entries {
            if let Some(symbol_name) = &symbol.name {
                if symbol_name == name {
                    results.push(RefsSymbol {
                        file_path: file_path.clone(),
                    });
                    break; // Found in this file, move to next
                }
            }
        }
    }

    Ok(results)
}

/// Run the refs command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `name` - Symbol name to query
/// * `root` - Optional root directory for resolving relative paths
/// * `path` - Optional file path containing the symbol (if None, searches all files)
/// * `symbol_id` - Optional stable SymbolId for precise lookup
/// * `direction` - "in" for callers, "out" for calls
/// * `output_format` - Output format (Human or Json)
/// * `with_context` - Include context lines
/// * `with_semantics` - Include semantic information (kind, language)
/// * `with_checksums` - Include SHA-256 checksums
/// * `context_lines` - Number of context lines before/after (capped at 100)
///
/// # Displays
/// Human-readable list of calls or JSON output
#[allow(
    clippy::too_many_arguments,
    reason = "CLI command surface: each arg maps to a flag"
)]
pub fn run_refs(
    db_path: PathBuf,
    name: String,
    root: Option<PathBuf>,
    path: Option<PathBuf>,
    symbol_id: Option<String>,
    direction: String,
    output_format: OutputFormat,
    with_context: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
    all: bool,
) -> Result<()> {
    if all {
        return run_refs_all(
            &name,
            &direction,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
        );
    }

    // Build args for execution tracking
    let mut args = vec!["refs".to_string()];
    args.push("--name".to_string());
    args.push(name.clone());
    if let Some(ref root_path) = root {
        args.push("--root".to_string());
        args.push(root_path.to_string_lossy().to_string());
    }
    if let Some(ref p) = path {
        args.push("--path".to_string());
        args.push(p.to_string_lossy().to_string());
    }
    if let Some(ref sid) = symbol_id {
        args.push("--symbol-id".to_string());
        args.push(sid.clone());
    }
    args.push("--direction".to_string());
    args.push(direction.clone());

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = magellan::output::generate_execution_id();
    let root_str = root.as_ref().map(|p| p.to_string_lossy().to_string());
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        root_str.as_deref(),
        &db_path_str,
    )?;

    // Phase: resolve_target
    graph
        .telemetry()
        .record_phase_start(&exec_id, "resolve_target")?;

    // Handle --symbol-id alternative
    if let Some(sid) = symbol_id {
        let mut graph_mut = CodeGraph::open(&db_path)?;
        let target_symbol = query::find_by_symbol_id(&mut graph_mut, &sid)?;

        match target_symbol {
            Some(symbol) => {
                // Query references by SymbolId
                // Fall back to FQN-based if symbol has display_fqn
                if let Some(ref fqn) = symbol.display_fqn {
                    // Use existing FQN-based reference query via name/path
                    // For now, we use the name from the symbol
                    let symbol_name = symbol.name.clone().unwrap_or_else(|| fqn.clone());
                    // Use the provided path for FQN lookup (required when using --symbol-id)
                    let path_str = match path {
                        Some(p) => resolve_path(&p, &root),
                        None => {
                            graph.execution_log().finish_execution(
                                &exec_id,
                                "error",
                                Some("--path is required when using --symbol-id"),
                                0,
                                0,
                                0,
                            )?;
                            eprintln!("Error: --path is required when using --symbol-id");
                            return Ok(());
                        }
                    };

                    let calls: Vec<CallFact> = match direction.as_str() {
                        "in" | "incoming" => {
                            graph_mut.callers_of_symbol(&path_str, &symbol_name)?
                        }
                        "out" | "outgoing" => {
                            graph_mut.calls_from_symbol(&path_str, &symbol_name)?
                        }
                        _ => {
                            let err_msg =
                                format!("Invalid direction: '{}'. Use 'in' or 'out'", direction);
                            graph.execution_log().finish_execution(
                                &exec_id,
                                "error",
                                Some(err_msg.as_str()),
                                0,
                                0,
                                0,
                            )?;
                            anyhow::bail!(err_msg);
                        }
                    };

                    // Handle JSON output mode
                    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty
                    {
                        graph
                            .execution_log()
                            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
                        return output_json_mode(
                            &db_path,
                            &symbol_name,
                            &path_str,
                            &direction,
                            calls,
                            &exec_id,
                            output_format,
                            with_context,
                            with_semantics,
                            with_checksums,
                            context_lines,
                        );
                    }

                    // Human mode
                    if direction == "in" || direction == "incoming" {
                        if calls.is_empty() {
                            println!("No incoming calls to \"{}\"", symbol_name);
                        } else {
                            println!("Calls TO \"{}\":", symbol_name);
                            for call in &calls {
                                println!(
                                    "  From: {} (Function) at {}:{}",
                                    call.caller,
                                    call.file_path.display(),
                                    call.start_line
                                );
                            }
                        }
                    } else if calls.is_empty() {
                        println!("No outgoing calls from \"{}\"", symbol_name);
                    } else {
                        println!("Calls FROM \"{}\":", symbol_name);
                        for call in &calls {
                            println!(
                                "  To: {} at {}:{}",
                                call.callee,
                                call.file_path.display(),
                                call.start_line
                            );
                        }
                    }

                    graph
                        .execution_log()
                        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
                    return Ok(());
                } else {
                    // Symbol has no display_fqn, cannot lookup references
                    graph.execution_log().finish_execution(
                        &exec_id,
                        "error",
                        Some("Symbol has no display FQN for reference lookup"),
                        0,
                        0,
                        0,
                    )?;
                    eprintln!(
                        "Symbol ID '{}' has no display FQN, cannot lookup references",
                        sid
                    );
                    return Ok(());
                }
            }
            None => {
                graph.execution_log().finish_execution(
                    &exec_id,
                    "error",
                    Some("Symbol ID not found"),
                    0,
                    0,
                    0,
                )?;
                eprintln!("Symbol ID '{}' not found", sid);
                return Ok(());
            }
        }
    }

    // Determine the file path to use for the symbol lookup
    let path_str = match path {
        Some(p) => {
            // User provided a specific path - use existing behavior
            resolve_path(&p, &root)
        }
        None => {
            // End resolve_target phase, start search phase
            graph
                .telemetry()
                .record_phase_end(&exec_id, "resolve_target")?;
            graph.telemetry().record_phase_start(&exec_id, "search")?;

            // No path provided - search all files for the symbol
            let mut graph_mut = CodeGraph::open(&db_path)?;
            let matches = find_symbol_all_files(&mut graph_mut, &name)?;

            match matches.len() {
                0 => {
                    graph.execution_log().finish_execution(
                        &exec_id,
                        "error",
                        Some("Symbol not found"),
                        0,
                        0,
                        0,
                    )?;
                    eprintln!("Symbol '{}' not found anywhere", name);
                    return Ok(());
                }
                1 => {
                    // Exactly one match - use it automatically
                    let matched_symbol = &matches[0];
                    if output_format == OutputFormat::Human {
                        println!("Found '{}' in {}", name, matched_symbol.file_path);
                    }
                    matched_symbol.file_path.clone()
                }
                _ => {
                    // Multiple matches - show ranked list
                    graph.execution_log().finish_execution(
                        &exec_id,
                        "error",
                        Some("Ambiguous symbol name"),
                        0,
                        0,
                        0,
                    )?;
                    eprintln!("Symbol '{}' found in multiple locations:", name);
                    for (i, matched_symbol) in matches.iter().enumerate() {
                        eprintln!("  [{}] {}", i + 1, matched_symbol.file_path);
                    }
                    eprintln!("\nUse --path <file> to specify which one to use");
                    return Ok(());
                }
            }
        }
    };

    let calls: Vec<CallFact> = match direction.as_str() {
        "in" | "incoming" => {
            // End resolve_target phase, start query phase
            graph
                .telemetry()
                .record_phase_end(&exec_id, "resolve_target")?;
            graph
                .telemetry()
                .record_phase_start(&exec_id, "query_refs")?;

            // Get callers of this symbol
            {
                let mut graph_mut = CodeGraph::open(&db_path)?;
                graph_mut.callers_of_symbol(&path_str, &name)?
            }
        }
        "out" | "outgoing" => {
            // End resolve_target phase, start query phase
            graph
                .telemetry()
                .record_phase_end(&exec_id, "resolve_target")?;
            graph
                .telemetry()
                .record_phase_start(&exec_id, "query_refs")?;

            // Get calls from this symbol
            {
                let mut graph_mut = CodeGraph::open(&db_path)?;
                graph_mut.calls_from_symbol(&path_str, &name)?
            }
        }
        _ => {
            let err_msg = format!("Invalid direction: '{}'. Use 'in' or 'out'", direction);
            graph.execution_log().finish_execution(
                &exec_id,
                "error",
                Some(err_msg.as_str()),
                0,
                0,
                0,
            )?;
            anyhow::bail!(err_msg);
        }
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        // End query phase, start build_response phase
        graph.telemetry().record_phase_end(&exec_id, "query_refs")?;
        graph
            .telemetry()
            .record_phase_start(&exec_id, "build_response")?;

        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            &db_path,
            &name,
            &path_str,
            &direction,
            calls,
            &exec_id,
            output_format,
            with_context,
            with_semantics,
            with_checksums,
            context_lines,
        );
    }

    // Human mode (existing behavior)
    if direction == "in" || direction == "incoming" {
        // End query phase for human output
        graph.telemetry().record_phase_end(&exec_id, "query_refs")?;
        if calls.is_empty() {
            println!("No incoming calls to \"{}\"", name);
        } else {
            println!("Calls TO \"{}\":", name);
            for call in &calls {
                println!(
                    "  From: {} (Function) at {}:{}",
                    call.caller,
                    call.file_path.display(),
                    call.start_line
                );
            }
        }
    } else if calls.is_empty() {
        println!("No outgoing calls from \"{}\"", name);
    } else {
        println!("Calls FROM \"{}\":", name);
        for call in &calls {
            println!(
                "  To: {} at {}:{}",
                call.callee,
                call.file_path.display(),
                call.start_line
            );
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;

    // Record output phase
    graph.telemetry().record_phase_start(&exec_id, "output")?;
    graph.telemetry().record_phase_end(&exec_id, "output")?;

    Ok(())
}

/// Output refs results in JSON format
#[allow(
    clippy::too_many_arguments,
    reason = "JSON output needs all query parameters"
)]
fn output_json_mode(
    _db_path: &Path,
    symbol_name: &str,
    file_path: &str,
    direction: &str,
    mut calls: Vec<CallFact>,
    exec_id: &str,
    output_format: OutputFormat,
    with_context: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
) -> Result<()> {
    // Sort deterministically: by file_path, byte_start
    calls.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.byte_start.cmp(&b.byte_start))
    });

    // Convert CallFact to ReferenceMatch
    // Use symbol_id from CallFact directly (populated during indexing)
    let references: Vec<ReferenceMatch> = calls
        .into_iter()
        .map(|call| {
            let mut span = Span::new(
                call.file_path.to_string_lossy().to_string(),
                call.byte_start,
                call.byte_end,
                call.start_line,
                call.start_col,
                call.end_line,
                call.end_col,
            );

            // Add context if requested
            if with_context {
                if let Some(context) = SpanContext::extract(
                    call.file_path.to_string_lossy().as_ref(),
                    call.start_line,
                    call.end_line,
                    context_lines,
                ) {
                    span = span.with_context(context);
                }
            }

            // Add semantics if requested
            if with_semantics {
                let kind = "call".to_string();
                let language = detect_language_from_path(call.file_path.to_string_lossy().as_ref());
                span = span.with_semantics_from(kind, language);
            }

            // Add checksums if requested
            if with_checksums {
                let checksums = SpanChecksums::compute(
                    call.file_path.to_string_lossy().as_ref(),
                    call.byte_start,
                    call.byte_end,
                );
                span = span.with_checksums(checksums);
            }

            // For "in" direction, referenced_symbol is the caller
            // For "out" direction, referenced_symbol is the callee
            let (referenced_symbol, target_symbol_id) =
                if direction == "in" || direction == "incoming" {
                    (call.caller.clone(), call.caller_symbol_id)
                } else {
                    (call.callee.clone(), call.callee_symbol_id)
                };

            ReferenceMatch::new(
                span,
                referenced_symbol,
                Some("call".to_string()),
                target_symbol_id,
            )
        })
        .collect();

    let response = RefsResponse {
        references,
        symbol_name: symbol_name.to_string(),
        file_path: file_path.to_string(),
        direction: direction.to_string(),
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "registry fan-out path mirrors CLI flag surface"
)]
fn run_refs_all(
    _name: &str,
    direction: &str,
    output_format: OutputFormat,
    _with_context: bool,
    _with_semantics: bool,
    _with_checksums: bool,
    _context_lines: usize,
) -> Result<()> {
    let registry = Registry::load().with_context(|| "Failed to load project registry")?;
    let enabled: Vec<_> = registry.projects.iter().filter(|p| p.enabled).collect();

    if enabled.is_empty() {
        println!("No enabled projects in registry.");
        println!("Hint: use `magellan catalog` to list registered projects, then `magellan watch` to index one.");
        return Ok(());
    }

    let db_paths: Vec<PathBuf> = enabled.iter().map(|p| p.db.clone()).collect();
    let mut mdb = MultiDbContext::from_paths(&db_paths)?;

    let include_callers = direction == "in" || direction == "incoming";
    let include_callees = direction == "out" || direction == "outgoing";
    let matches = mdb.search_symbol(_name, None, Some(1), include_callers, include_callees);

    if matches.is_empty() {
        if include_callers {
            println!(
                "No incoming references to '{}' across {} project(s).",
                _name,
                enabled.len()
            );
        } else {
            println!(
                "No outgoing references from '{}' across {} project(s).",
                _name,
                enabled.len()
            );
        }
        return Ok(());
    }

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let exec_id = magellan::output::generate_execution_id();
            let response = JsonResponse::new(&matches, &exec_id);
            if output_format == OutputFormat::Pretty {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&response)
                        .expect("invariant: JsonResponse serialization is infallible")
                );
            } else {
                println!(
                    "{}",
                    serde_json::to_string(&response)
                        .expect("invariant: JsonResponse serialization is infallible")
                );
            }
        }
        OutputFormat::Human => {
            if include_callers {
                println!(
                    "Incoming references for '{}' across {} project(s):",
                    _name,
                    enabled.len()
                );
                for m in matches {
                    if let Some(ref callers) = m.callers {
                        for caller in callers {
                            println!(
                                "  called by {} in {}:{}",
                                caller.name, caller.file_path, caller.line
                            );
                        }
                    }
                }
            } else {
                println!(
                    "Outgoing references for '{}' across {} project(s):",
                    _name,
                    enabled.len()
                );
                for m in matches {
                    if let Some(ref callees) = m.callees {
                        for callee in callees {
                            println!(
                                "  calls {} in {}:{}",
                                callee.name, callee.file_path, callee.line
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

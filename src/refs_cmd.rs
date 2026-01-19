//! Refs command implementation
//!
//! Shows calls (incoming/outgoing) for a symbol.

use anyhow::Result;
use magellan::{CallFact, CodeGraph};
use magellan::graph::query;
use magellan::output::{JsonResponse, OutputFormat, RefsResponse, ReferenceMatch, Span, output_json};
use std::collections::HashMap;
use std::path::PathBuf;

/// Resolve a file path against an optional root directory
fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }

    if let Some(ref root) = root {
        root.join(file_path).to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string())
    }
}

/// Run the refs command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `name` - Symbol name to query
/// * `root` - Optional root directory for resolving relative paths
/// * `path` - File path containing the symbol
/// * `direction` - "in" for callers, "out" for calls
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable list of calls or JSON output
pub fn run_refs(
    db_path: PathBuf,
    name: String,
    root: Option<PathBuf>,
    path: PathBuf,
    direction: String,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec!["refs".to_string()];
    args.push("--name".to_string());
    args.push(name.clone());
    if let Some(ref root_path) = root {
        args.push("--root".to_string());
        args.push(root_path.to_string_lossy().to_string());
    }
    args.push("--path".to_string());
    args.push(path.to_string_lossy().to_string());
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

    let path_str = resolve_path(&path, &root);

    let calls: Vec<CallFact> = match direction.as_str() {
        "in" | "incoming" => {
            // Get callers of this symbol
            {
                let mut graph_mut = CodeGraph::open(&db_path)?;
                graph_mut.callers_of_symbol(&path_str, &name)?
            }
        }
        "out" | "outgoing" => {
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
                0, 0, 0,
            )?;
            anyhow::bail!(err_msg);
        }
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json {
        graph.execution_log().finish_execution(
            &exec_id,
            "success",
            None,
            0, 0, 0,
        )?;
        return output_json_mode(&db_path, &name, &path_str, &direction, calls, &exec_id);
    }

    // Human mode (existing behavior)
    if direction == "in" || direction == "incoming" {
        if calls.is_empty() {
            println!("No incoming calls to \"{}\"", name);
        } else {
            println!("Calls TO \"{}\":", name);
            for call in &calls {
                println!(
                    "  From: {} ({}) at {}:{}",
                    call.caller,
                    "Function",
                    call.file_path.display(),
                    call.start_line
                );
            }
        }
    } else {
        if calls.is_empty() {
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
    }

    graph.execution_log().finish_execution(
        &exec_id,
        "success",
        None,
        0, 0, 0,
    )?;
    Ok(())
}

/// Output refs results in JSON format
fn output_json_mode(
    db_path: &PathBuf,
    symbol_name: &str,
    file_path: &str,
    direction: &str,
    mut calls: Vec<CallFact>,
    exec_id: &str,
) -> Result<()> {
    // Sort deterministically: by file_path, byte_start
    calls.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.byte_start.cmp(&b.byte_start))
    });

    // Build symbol_id lookup map: (file_path, symbol_name) -> symbol_id
    let mut symbol_id_map: HashMap<(String, String), Option<String>> = HashMap::new();

    // Try to open the graph and populate symbol IDs
    if let Ok(mut graph) = CodeGraph::open(db_path) {
        // For each unique (file, symbol) pair in the calls, look up the symbol_id
        for call in &calls {
            let target_name = if direction == "in" || direction == "incoming" {
                &call.caller
            } else {
                &call.callee
            };

            let key = (call.file_path.to_string_lossy().to_string(), target_name.clone());
            if !symbol_id_map.contains_key(&key) {
                // Look up symbol nodes in the file to find symbol_id
                if let Ok(symbol_nodes) = query::symbol_nodes_in_file_with_ids(
                    &mut graph,
                    &call.file_path.to_string_lossy()
                ) {
                    let symbol_id = symbol_nodes
                        .into_iter()
                        .find(|(_, fact, _)| fact.name.as_deref() == Some(target_name.as_str()))
                        .and_then(|(_, _, id)| id);
                    symbol_id_map.insert(key, symbol_id);
                } else {
                    symbol_id_map.insert(key, None);
                }
            }
        }
    }

    // Convert CallFact to ReferenceMatch
    let references: Vec<ReferenceMatch> = calls
        .into_iter()
        .map(|call| {
            let span = Span::new(
                call.file_path.to_string_lossy().to_string(),
                call.byte_start,
                call.byte_end,
                call.start_line,
                call.start_col,
                call.end_line,
                call.end_col,
            );

            // For "in" direction, referenced_symbol is the caller
            // For "out" direction, referenced_symbol is the callee
            let referenced_symbol = if direction == "in" || direction == "incoming" {
                call.caller.clone()
            } else {
                call.callee.clone()
            };

            let key = (call.file_path.to_string_lossy().to_string(), referenced_symbol.clone());
            let target_symbol_id = symbol_id_map.get(&key).and_then(|id| id.clone());

            ReferenceMatch::new(span, referenced_symbol, Some("call".to_string()), target_symbol_id)
        })
        .collect();

    let response = RefsResponse {
        references,
        symbol_name: symbol_name.to_string(),
        file_path: file_path.to_string(),
        direction: direction.to_string(),
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response)?;

    Ok(())
}

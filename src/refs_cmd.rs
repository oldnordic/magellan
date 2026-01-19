//! Refs command implementation
//!
//! Shows calls (incoming/outgoing) for a symbol.

use anyhow::Result;
use magellan::{CallFact, CodeGraph};
use magellan::output::{JsonResponse, OutputFormat, RefsResponse, ReferenceMatch, Span, generate_execution_id, output_json};
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
    let mut graph = CodeGraph::open(&db_path)?;

    let path_str = resolve_path(&path, &root);

    let calls: Vec<CallFact> = match direction.as_str() {
        "in" | "incoming" => {
            // Get callers of this symbol
            graph.callers_of_symbol(&path_str, &name)?
        }
        "out" | "outgoing" => {
            // Get calls from this symbol
            graph.calls_from_symbol(&path_str, &name)?
        }
        _ => {
            anyhow::bail!("Invalid direction: '{}'. Use 'in' or 'out'", direction);
        }
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json {
        return output_json_mode(&name, &path_str, &direction, calls);
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

    Ok(())
}

/// Output refs results in JSON format
fn output_json_mode(
    symbol_name: &str,
    file_path: &str,
    direction: &str,
    mut calls: Vec<CallFact>,
) -> Result<()> {
    // Sort deterministically: by file_path, byte_start
    calls.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.byte_start.cmp(&b.byte_start))
    });

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

            ReferenceMatch::new(span, referenced_symbol, Some("call".to_string()))
        })
        .collect();

    let response = RefsResponse {
        references,
        symbol_name: symbol_name.to_string(),
        file_path: file_path.to_string(),
        direction: direction.to_string(),
    };

    let exec_id = generate_execution_id();
    let json_response = JsonResponse::new(response, &exec_id);
    output_json(&json_response)?;

    Ok(())
}

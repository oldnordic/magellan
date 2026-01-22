//! Refs command implementation
//!
//! Shows calls (incoming/outgoing) for a symbol.

use anyhow::Result;
use magellan::{CallFact, CodeGraph};
use magellan::common::{detect_language_from_path, resolve_path};
use magellan::output::{JsonResponse, OutputFormat, RefsResponse, ReferenceMatch, Span, output_json};
use magellan::output::rich::{SpanContext, SpanChecksums};
use std::path::PathBuf;

/// Run the refs command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `name` - Symbol name to query
/// * `root` - Optional root directory for resolving relative paths
/// * `path` - File path containing the symbol
/// * `direction` - "in" for callers, "out" for calls
/// * `output_format` - Output format (Human or Json)
/// * `with_context` - Include context lines
/// * `with_semantics` - Include semantic information (kind, language)
/// * `with_checksums` - Include SHA-256 checksums
/// * `context_lines` - Number of context lines before/after (capped at 100)
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
    with_context: bool,
    with_semantics: bool,
    with_checksums: bool,
    context_lines: usize,
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
        return output_json_mode(&db_path, &name, &path_str, &direction, calls, &exec_id, with_context, with_semantics, with_checksums, context_lines);
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
    _db_path: &PathBuf,
    symbol_name: &str,
    file_path: &str,
    direction: &str,
    mut calls: Vec<CallFact>,
    exec_id: &str,
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
                    &call.file_path.to_string_lossy().to_string(),
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
                let language = detect_language_from_path(&call.file_path.to_string_lossy().to_string());
                span = span.with_semantics_from(kind, language);
            }

            // Add checksums if requested
            if with_checksums {
                let checksums = SpanChecksums::compute(
                    &call.file_path.to_string_lossy().to_string(),
                    call.byte_start,
                    call.byte_end,
                );
                span = span.with_checksums(checksums);
            }

            // For "in" direction, referenced_symbol is the caller
            // For "out" direction, referenced_symbol is the callee
            let (referenced_symbol, target_symbol_id) = if direction == "in" || direction == "incoming" {
                (call.caller.clone(), call.caller_symbol_id)
            } else {
                (call.callee.clone(), call.callee_symbol_id)
            };

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

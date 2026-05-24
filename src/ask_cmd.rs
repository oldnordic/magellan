use anyhow::{bail, Context, Result};
use magellan::OutputFormat;
use std::path::PathBuf;

/// Run the `magellan ask` intent-router command
pub fn run_ask(question: String, db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let q = question.to_lowercase();

    // Extract symbol name: prefer quoted, then last token
    let symbol_name =
        extract_quoted_symbol(&q).or_else(|| q.split_whitespace().last().map(|s| s.to_string()));

    let Some(name) = symbol_name else {
        bail!("Could not determine symbol name from question. Try: \"who calls 'run_find'\"");
    };

    // Intent routing based on keyword guards
    let is_caller = [
        "who calls",
        "who uses",
        "callers of",
        "who references",
        "who invokes",
        "dependencies of",
        "dependents of",
        "who depends on",
    ]
    .iter()
    .any(|phrase| q.contains(phrase));

    let is_callee = [
        "who is called by",
        "callees of",
        "calls from",
        "outgoing calls",
        "called by",
    ]
    .iter()
    .any(|phrase| q.contains(phrase));

    if is_caller {
        route_refs(db_path, name, "in".to_string(), output_format)
    } else if is_callee {
        route_refs(db_path, name, "out".to_string(), output_format)
    } else {
        route_find(db_path, name, output_format)
    }
}

/// Extract a single- or double-quoted symbol from a query string.
fn extract_quoted_symbol(q: &str) -> Option<String> {
    for (open, close) in [('\'', '\''), ('\"', '\"')] {
        if let Some(start) = q.find(open) {
            let rest = &q[start + 1..];
            if let Some(end) = rest.find(close) {
                let candidate = &rest[..end];
                if !candidate.is_empty() {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

fn route_find(db_path: PathBuf, name: String, output_format: OutputFormat) -> Result<()> {
    crate::find_cmd::run_find(
        db_path,
        Some(name),
        None,  // root
        None,  // path
        None,  // glob_pattern
        None,  // symbol_id
        None,  // ambiguous_name
        false, // first
        output_format,
        true,  // with_context
        true,  // with_callers
        true,  // with_callees
        true,  // with_semantics
        false, // with_checksums
        3,     // context_lines
        false, // all
    )
    .with_context(|| "Ask → find routing failed")
}

fn route_refs(
    db_path: PathBuf,
    name: String,
    direction: String,
    output_format: OutputFormat,
) -> Result<()> {
    crate::refs_cmd::run_refs(
        db_path,
        name,
        None, // root
        None, // path
        None, // symbol_id
        direction,
        output_format,
        true,  // with_context
        true,  // with_semantics
        false, // with_checksums
        3,     // context_lines
        false, // all
    )
    .with_context(|| "Ask → refs routing failed")
}

//! Refs command implementation
//!
//! Shows calls (incoming/outgoing) for a symbol.

use anyhow::Result;
use magellan::{CodeGraph, CallFact};
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
///
/// # Displays
/// Human-readable list of calls
pub fn run_refs(
    db_path: PathBuf,
    name: String,
    root: Option<PathBuf>,
    path: PathBuf,
    direction: String,
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

    if direction == "in" || direction == "incoming" {
        if calls.is_empty() {
            println!("No incoming calls to \"{}\"", name);
        } else {
            println!("Calls TO \"{}\":", name);
            for call in &calls {
                println!("  From: {} ({}) at {}:{}",
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
                println!("  To: {} at {}:{}",
                    call.callee,
                    call.file_path.display(),
                    call.start_line
                );
            }
        }
    }

    Ok(())
}

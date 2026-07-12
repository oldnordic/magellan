//! Dead code command implementation
//!
//! Shows symbols unreachable from an entry point (dead code detection).

use anyhow::Result;
use magellan::graph::DeadSymbol;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

use crate::status_cmd::ExecutionTracker;

/// Run the dead-code command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `entry_symbol_id` - Stable symbol ID of the entry point (e.g., main function)
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable list of dead symbols with reasons, or JSON output
pub fn run_dead_code(
    db_path: PathBuf,
    entry_symbol_id: String,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let mut tracker = ExecutionTracker::new(
        vec![
            "dead-code".to_string(),
            "--entry".to_string(),
            entry_symbol_id.clone(),
        ],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;
    let exec_id = tracker.exec_id().to_string();

    let result = (|| -> Result<()> {
        let dead_symbols = graph.dead_symbols(&entry_symbol_id)?;

        if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
            return output_json_mode(&entry_symbol_id, dead_symbols, &exec_id, output_format);
        }

        if dead_symbols.is_empty() {
            println!(
                "No dead code found. All symbols are reachable from \"{}\"",
                entry_symbol_id
            );
        } else {
            println!("Dead code (unreachable from \"{}\"):", entry_symbol_id);
            for dead in &dead_symbols {
                let fqn_display = dead.symbol.fqn.as_deref().unwrap_or("?");
                println!(
                    "  {} ({}) in {} - {}",
                    fqn_display, dead.symbol.kind, dead.symbol.file_path, dead.reason
                );
            }
        }

        Ok(())
    })();

    if let Err(err) = &result {
        tracker.set_error(format!("{err:#}"));
    }
    tracker.finish(&graph)?;
    result
}

/// Response structure for dead-code command
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeadCodeResponse {
    /// Entry point symbol ID
    pub entry_symbol_id: String,
    /// Number of dead symbols found
    pub count: usize,
    /// List of dead symbols with reasons
    pub dead_symbols: Vec<DeadSymbolJson>,
}

/// Dead symbol info for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeadSymbolJson {
    /// Base symbol information
    pub symbol: SymbolInfoJson,
    /// Reason why this symbol is unreachable/dead
    pub reason: String,
}

/// Symbol info for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolInfoJson {
    /// Stable symbol ID (32-char BLAKE3 hash)
    pub symbol_id: Option<String>,
    /// Fully-qualified name
    pub fqn: Option<String>,
    /// File path containing the symbol
    pub file_path: String,
    /// Symbol kind (Function, Method, Class, etc.)
    pub kind: String,
}

impl From<DeadSymbol> for DeadSymbolJson {
    fn from(dead: DeadSymbol) -> Self {
        Self {
            symbol: SymbolInfoJson::from(dead.symbol),
            reason: dead.reason,
        }
    }
}

impl From<magellan::graph::SymbolInfo> for SymbolInfoJson {
    fn from(info: magellan::graph::SymbolInfo) -> Self {
        Self {
            symbol_id: info.symbol_id,
            fqn: info.fqn,
            file_path: info.file_path,
            kind: info.kind,
        }
    }
}

/// Output dead code results in JSON format
fn output_json_mode(
    entry_symbol_id: &str,
    dead_symbols: Vec<DeadSymbol>,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    let dead_symbols_json: Vec<DeadSymbolJson> =
        dead_symbols.into_iter().map(DeadSymbolJson::from).collect();

    let response = DeadCodeResponse {
        entry_symbol_id: entry_symbol_id.to_string(),
        count: dead_symbols_json.len(),
        dead_symbols: dead_symbols_json,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

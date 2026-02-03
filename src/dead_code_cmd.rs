//! Dead code command implementation
//!
//! Shows symbols unreachable from an entry point (dead code detection).

use anyhow::Result;
use magellan::graph::DeadSymbol;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

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
    // Build args for execution tracking
    let args = vec![
        "dead-code".to_string(),
        "--entry".to_string(),
        entry_symbol_id.clone(),
    ];

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = magellan::output::generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    // Query dead symbols
    let dead_symbols = graph.dead_symbols(&entry_symbol_id)?;

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            &entry_symbol_id,
            dead_symbols,
            &exec_id,
            output_format,
        );
    }

    // Human mode
    if dead_symbols.is_empty() {
        println!("No dead code found. All symbols are reachable from \"{}\"", entry_symbol_id);
    } else {
        println!("Dead code (unreachable from \"{}\"):", entry_symbol_id);
        for dead in &dead_symbols {
            let fqn_display = dead.symbol.fqn.as_deref().unwrap_or("?");
            println!(
                "  {} ({}) in {} - {}",
                fqn_display,
                dead.symbol.kind,
                dead.symbol.file_path,
                dead.reason
            );
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
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

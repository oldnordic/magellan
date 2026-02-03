//! Reachable command implementation
//!
//! Shows reachable symbols (forward/reverse reachability) from a starting symbol.

use anyhow::Result;
use magellan::graph::SymbolInfo;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the reachable command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `symbol_id` - Stable symbol ID to start from
/// * `reverse` - If true, show callers (reverse reachability); if false, show callees
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable list of reachable symbols or JSON output
pub fn run_reachable(
    db_path: PathBuf,
    symbol_id: String,
    reverse: bool,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec!["reachable".to_string()];
    args.push("--symbol".to_string());
    args.push(symbol_id.clone());
    if reverse {
        args.push("--reverse".to_string());
    }

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

    // Query reachability
    let symbols = if reverse {
        graph.reverse_reachable_symbols(&symbol_id, None)?
    } else {
        graph.reachable_symbols(&symbol_id, None)?
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            &symbol_id,
            reverse,
            symbols,
            &exec_id,
            output_format,
        );
    }

    // Human mode
    let direction_label = if reverse {
        "that can reach"
    } else {
        "reachable from"
    };

    if symbols.is_empty() {
        println!("No symbols {} \"{}\"", direction_label, symbol_id);
    } else {
        println!("Symbols {} \"{}\":", direction_label, symbol_id);
        for symbol in &symbols {
            let fqn_display = symbol.fqn.as_deref().unwrap_or("?");
            println!(
                "  {} ({}) in {}",
                fqn_display,
                symbol.kind,
                symbol.file_path
            );
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// Response structure for reachable command
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReachableResponse {
    /// Starting symbol ID
    pub symbol_id: String,
    /// Direction: "forward" or "reverse"
    pub direction: String,
    /// Number of reachable symbols found
    pub count: usize,
    /// List of reachable symbols
    pub symbols: Vec<SymbolInfoJson>,
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

impl From<SymbolInfo> for SymbolInfoJson {
    fn from(info: SymbolInfo) -> Self {
        Self {
            symbol_id: info.symbol_id,
            fqn: info.fqn,
            file_path: info.file_path,
            kind: info.kind,
        }
    }
}

/// Output reachable results in JSON format
fn output_json_mode(
    symbol_id: &str,
    reverse: bool,
    symbols: Vec<SymbolInfo>,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    let direction = if reverse { "reverse" } else { "forward" }.to_string();

    let symbols_json: Vec<SymbolInfoJson> =
        symbols.into_iter().map(SymbolInfoJson::from).collect();

    let response = ReachableResponse {
        symbol_id: symbol_id.to_string(),
        direction,
        count: symbols_json.len(),
        symbols: symbols_json,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

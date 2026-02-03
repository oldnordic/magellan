//! Cycles command implementation
//!
//! Shows strongly connected components (cycles) in the call graph.

use anyhow::Result;
use magellan::graph::{Cycle, CycleKind};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the cycles command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `symbol_id` - Optional symbol ID to find cycles containing specific symbol
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable list of cycles or JSON output
pub fn run_cycles(
    db_path: PathBuf,
    symbol_id: Option<String>,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec!["cycles".to_string()];
    if let Some(ref sym) = symbol_id {
        args.push("--symbol".to_string());
        args.push(sym.clone());
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

    // Query cycles
    let cycles = if let Some(ref sym) = symbol_id {
        graph.find_cycles_containing(sym)?
    } else {
        graph.detect_cycles()?.cycles
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            symbol_id.as_deref(),
            cycles,
            &exec_id,
            output_format,
        );
    }

    // Human mode
    if cycles.is_empty() {
        println!("No cycles detected in the call graph.");
    } else {
        println!("Detected {} cycle(s):", cycles.len());
        for (idx, cycle) in cycles.iter().enumerate() {
            println!("  [{}] {}:", idx + 1, cycle_kind_display(&cycle.kind));
            for member in &cycle.members {
                let fqn_display = member.fqn.as_deref().unwrap_or("?");
                println!(
                    "      {} ({}) in {}",
                    fqn_display,
                    member.kind,
                    member.file_path
                );
            }
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

fn cycle_kind_display(kind: &CycleKind) -> &'static str {
    match kind {
        CycleKind::MutualRecursion => "Mutual Recursion",
        CycleKind::SelfLoop => "Self Loop",
    }
}

/// Response structure for cycles command
#[derive(Debug, Clone, serde::Serialize)]
pub struct CyclesResponse {
    /// Optional symbol filter
    pub symbol_id: Option<String>,
    /// Number of cycles found
    pub count: usize,
    /// List of cycles
    pub cycles: Vec<CycleJson>,
}

/// Cycle info for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct CycleJson {
    /// Kind of cycle
    pub kind: String,
    /// Members of the cycle
    pub members: Vec<SymbolInfoJson>,
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

impl From<Cycle> for CycleJson {
    fn from(cycle: Cycle) -> Self {
        Self {
            kind: cycle_kind_json(&cycle.kind),
            members: cycle.members.into_iter().map(SymbolInfoJson::from).collect(),
        }
    }
}

fn cycle_kind_json(kind: &CycleKind) -> String {
    match kind {
        CycleKind::MutualRecursion => "mutual_recursion".to_string(),
        CycleKind::SelfLoop => "self_loop".to_string(),
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

/// Output cycles results in JSON format
fn output_json_mode(
    symbol_id: Option<&str>,
    cycles: Vec<Cycle>,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    let cycles_json: Vec<CycleJson> =
        cycles.into_iter().map(CycleJson::from).collect();

    let response = CyclesResponse {
        symbol_id: symbol_id.map(|s| s.to_string()),
        count: cycles_json.len(),
        cycles: cycles_json,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

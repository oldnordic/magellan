//! Condense command implementation
//!
//! Shows call graph condensation (SCCs collapsed into supernodes).

use anyhow::Result;
use magellan::graph::{CondensationResult, Supernode};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::collections::HashMap;
use std::path::PathBuf;

/// Run the condense command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `show_members` - If true, show all members of each supernode
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable condensation graph or JSON output
pub fn run_condense(
    db_path: PathBuf,
    show_members: bool,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
    let args = vec![
        "condense".to_string(),
        if show_members { "--members".to_string() } else { "--no-members".to_string() },
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

    // Query condensation
    let condensation = graph.condense_call_graph()?;

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            condensation,
            show_members,
            &exec_id,
            output_format,
        );
    }

    // Human mode
    println!("Call Graph Condensation:");
    println!("  Supernodes: {}", condensation.graph.supernodes.len());
    println!("  Edges: {}", condensation.graph.edges.len());
    println!();

    for supernode in &condensation.graph.supernodes {
        let fqn_display = if let Some(first) = supernode.members.first() {
            first.fqn.as_deref().unwrap_or("?")
        } else {
            "?"
        };

        if show_members && supernode.members.len() > 1 {
            println!("  [Supernode {}] {} ({} members):", supernode.id, fqn_display, supernode.members.len());
            for member in &supernode.members {
                let member_fqn = member.fqn.as_deref().unwrap_or("?");
                println!("      - {} ({})", member_fqn, member.kind);
            }
        } else {
            let member_count = if supernode.members.len() > 1 {
                format!(" ({} members)", supernode.members.len())
            } else {
                String::new()
            };
            println!("  [Supernode {}] {}{}", supernode.id, fqn_display, member_count);
        }
    }

    if !condensation.graph.edges.is_empty() {
        println!();
        println!("  Edges:");
        for (from, to) in &condensation.graph.edges {
            println!("    {} -> {}", from, to);
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// Response structure for condense command
#[derive(Debug, Clone, serde::Serialize)]
pub struct CondenseResponse {
    /// Number of supernodes
    pub supernode_count: usize,
    /// Number of edges between supernodes
    pub edge_count: usize,
    /// Supernodes
    pub supernodes: Vec<SupernodeJson>,
    /// Edges between supernodes
    pub edges: Vec<EdgeJson>,
    /// Symbol to supernode mapping
    pub symbol_to_supernode: HashMap<String, i64>,
}

/// Supernode info for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct SupernodeJson {
    /// Supernode ID (stable across condensations)
    pub id: i64,
    /// Number of members in this supernode
    pub member_count: usize,
    /// Members of this supernode
    pub members: Vec<SymbolInfoJson>,
}

/// Edge info for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct EdgeJson {
    /// Source supernode ID
    pub from: i64,
    /// Target supernode ID
    pub to: i64,
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

impl From<CondensationResult> for CondenseResponse {
    fn from(result: CondensationResult) -> Self {
        Self {
            supernode_count: result.graph.supernodes.len(),
            edge_count: result.graph.edges.len(),
            supernodes: result.graph.supernodes.into_iter().map(SupernodeJson::from).collect(),
            edges: result.graph.edges.into_iter().map(|(from, to)| EdgeJson { from, to }).collect(),
            symbol_to_supernode: result.original_to_supernode,
        }
    }
}

impl From<Supernode> for SupernodeJson {
    fn from(supernode: Supernode) -> Self {
        Self {
            id: supernode.id,
            member_count: supernode.members.len(),
            members: supernode.members.into_iter().map(SymbolInfoJson::from).collect(),
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

/// Output condensation results in JSON format
fn output_json_mode(
    condensation: CondensationResult,
    _show_members: bool,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    let response = CondenseResponse::from(condensation);

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

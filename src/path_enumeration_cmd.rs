//! Path enumeration command implementation
//!
//! Enumerates execution paths between symbols using bounded DFS.

use anyhow::Result;
use magellan::graph::{
    ExecutionPath, PathEnumerationResult, PathStatistics, SymbolInfo,
};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Run the paths command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `start_symbol_id` - Starting symbol ID or FQN
/// * `end_symbol_id` - Optional ending symbol ID or FQN
/// * `max_depth` - Maximum path depth
/// * `max_paths` - Maximum number of paths to return
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable list of execution paths with statistics, or JSON output
pub fn run_paths(
    db_path: PathBuf,
    start_symbol_id: String,
    end_symbol_id: Option<String>,
    max_depth: usize,
    max_paths: usize,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
    let mut args = vec![
        "paths".to_string(),
        "--start".to_string(),
        start_symbol_id.clone(),
        "--max-depth".to_string(),
        max_depth.to_string(),
        "--max-paths".to_string(),
        max_paths.to_string(),
    ];
    if let Some(ref end) = end_symbol_id {
        args.push("--end".to_string());
        args.push(end.clone());
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

    // Query paths
    let result = graph.enumerate_paths(
        &start_symbol_id,
        end_symbol_id.as_deref(),
        max_depth,
        max_paths,
    )?;

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            &start_symbol_id,
            end_symbol_id.as_deref(),
            result,
            &exec_id,
            output_format,
        );
    }

    // Human mode
    let end_label = end_symbol_id
        .as_ref()
        .map(|s| format!(" to \"{}\"", s))
        .unwrap_or_default();

    if result.paths.is_empty() {
        println!(
            "No paths found from \"{}\"{}",
            start_symbol_id, end_label
        );
    } else {
        println!(
            "Execution paths from \"{}\"{}:",
            start_symbol_id, end_label
        );
        println!("  Total paths enumerated: {}", result.total_enumerated);
        println!("  Paths returned: {}", result.paths.len());
        if result.bounded_hit {
            println!("  Note: Enumeration hit bounds (max_depth={}, max_paths={})", max_depth, max_paths);
        }

        println!("\nStatistics:");
        println!("  Average length: {:.2}", result.statistics.avg_length);
        println!(
            "  Min length: {}",
            result.statistics.min_length
        );
        println!(
            "  Max length: {}",
            result.statistics.max_length
        );
        println!(
            "  Unique symbols: {}",
            result.statistics.unique_symbols
        );

        println!("\nPaths:");
        for (i, path) in result.paths.iter().enumerate() {
            println!("  [{}] Length: {}", i + 1, path.length);
            for (j, symbol) in path.symbols.iter().enumerate() {
                let fqn_display = symbol.fqn.as_deref().unwrap_or("?");
                println!("    {}. {} ({})", j + 1, fqn_display, symbol.kind);
            }
            if i < result.paths.len().saturating_sub(1) {
                println!();
            }
        }
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// Response structure for paths command
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathsResponse {
    /// Starting symbol ID
    pub start_symbol_id: String,
    /// Ending symbol ID (if specified)
    pub end_symbol_id: Option<String>,
    /// Configuration used
    pub config: PathsConfig,
    /// All discovered paths
    pub paths: Vec<ExecutionPathJson>,
    /// Total number of paths enumerated
    pub total_enumerated: usize,
    /// Whether enumeration hit bounds
    pub bounded_hit: bool,
    /// Statistics about the discovered paths
    pub statistics: PathStatisticsJson,
}

/// Configuration for path enumeration
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathsConfig {
    /// Maximum path depth
    pub max_depth: usize,
    /// Maximum number of paths to return
    pub max_paths: usize,
}

/// Execution path for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionPathJson {
    /// Symbols along the path in order
    pub symbols: Vec<SymbolInfoJson>,
    /// Number of symbols in the path
    pub length: usize,
}

/// Path statistics for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathStatisticsJson {
    /// Average path length
    pub avg_length: f64,
    /// Minimum path length
    pub min_length: usize,
    /// Maximum path length
    pub max_length: usize,
    /// Number of unique symbols across all paths
    pub unique_symbols: usize,
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

impl From<ExecutionPath> for ExecutionPathJson {
    fn from(path: ExecutionPath) -> Self {
        Self {
            symbols: path.symbols.into_iter().map(SymbolInfoJson::from).collect(),
            length: path.length,
        }
    }
}

impl From<PathStatistics> for PathStatisticsJson {
    fn from(stats: PathStatistics) -> Self {
        Self {
            avg_length: stats.avg_length,
            min_length: stats.min_length,
            max_length: stats.max_length,
            unique_symbols: stats.unique_symbols,
        }
    }
}

/// Output paths results in JSON format
fn output_json_mode(
    start_symbol_id: &str,
    end_symbol_id: Option<&str>,
    result: PathEnumerationResult,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    let paths_json: Vec<ExecutionPathJson> =
        result.paths.into_iter().map(ExecutionPathJson::from).collect();

    let response = PathsResponse {
        start_symbol_id: start_symbol_id.to_string(),
        end_symbol_id: end_symbol_id.map(|s| s.to_string()),
        config: PathsConfig {
            max_depth: 0, // Not tracked in result
            max_paths: 0, // Not tracked in result
        },
        paths: paths_json,
        total_enumerated: result.total_enumerated,
        bounded_hit: result.bounded_hit,
        statistics: PathStatisticsJson::from(result.statistics),
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

//! Slice command implementation
//!
//! Shows program slices (backward/forward) for bug isolation and refactoring safety.

use anyhow::Result;
use magellan::backend_router::{BackendType, MagellanBackend};
use magellan::output::command::{SliceResponse, SliceStats, Span, SymbolMatch};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Resolved target information
struct ResolvedTarget {
    /// The numeric ID for geometric backend
    pub numeric_id: u64,
    /// The symbol ID (BLAKE3 hash) for SQLite backend
    pub symbol_id: String,
    /// The FQN of the symbol
    pub fqn: String,
    /// The file path
    pub file_path: String,
    /// The symbol kind
    pub kind: String,
}

/// Slice direction for CLI arguments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliSliceDirection {
    Backward,
    Forward,
}

impl CliSliceDirection {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "backward" => Some(CliSliceDirection::Backward),
            "forward" => Some(CliSliceDirection::Forward),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CliSliceDirection::Backward => "backward",
            CliSliceDirection::Forward => "forward",
        }
    }
}

/// Resolve a target string to a symbol
///
/// First tries to interpret as a symbol ID (BLAKE3 hash or numeric ID).
/// If not found, searches by name/FQN and shows ranked results if multiple matches.
fn resolve_target(
    backend: &MagellanBackend,
    graph: &mut CodeGraph,
    db_path: &PathBuf,
    target: &str,
) -> Result<ResolvedTarget> {
    let backend_type = MagellanBackend::detect_type(db_path);

    match backend_type {
        BackendType::Geometric => {
            // For geometric backend, try numeric ID first, then FQN, then name
            resolve_target_geometric(backend, target)
        }
        BackendType::SQLite | BackendType::NativeV3 => {
            // For SQLite backend, try symbol ID (BLAKE3 hash) first, then FQN
            resolve_target_sqlite(graph, target)
        }
    }
}

/// Resolve target for geometric backend
fn resolve_target_geometric(backend: &MagellanBackend, target: &str) -> Result<ResolvedTarget> {
    // First try numeric ID
    if let Ok(id) = target.parse::<u64>() {
        if let Some(info) = backend.find_symbol_by_id(id) {
            return Ok(ResolvedTarget {
                numeric_id: id,
                symbol_id: id.to_string(),
                fqn: info.fqn.clone(),
                file_path: info.file_path.clone(),
                kind: format!("{:?}", info.kind),
            });
        }
    }

    // Try FQN lookup
    if let Ok(Some(info)) = backend.find_symbol_by_fqn(target) {
        return Ok(ResolvedTarget {
            numeric_id: info.id,
            symbol_id: info.id.to_string(),
            fqn: info.fqn.clone(),
            file_path: info.file_path.clone(),
            kind: format!("{:?}", info.kind),
        });
    }

    // Try name lookup
    let symbols = backend.find_symbols_by_name(target)?;
    if symbols.is_empty() {
        return Err(anyhow::anyhow!(
            "Target symbol '{}' not found (tried ID, FQN, and name)",
            target
        ));
    }

    if symbols.len() == 1 {
        let info = &symbols[0];
        return Ok(ResolvedTarget {
            numeric_id: info.id,
            symbol_id: info.id.to_string(),
            fqn: info.fqn.clone(),
            file_path: info.file_path.clone(),
            kind: format!("{:?}", info.kind),
        });
    }

    // Multiple matches - show ranked list
    eprintln!("Ambiguous target '{}': found {} candidates", target, symbols.len());
    eprintln!();
    eprintln!("Top matches:");

    for (i, info) in symbols.iter().take(10).enumerate() {
        eprintln!(
            "  [{}] {} ({:?}) in {}:{}",
            i + 1,
            info.fqn,
            info.kind,
            info.file_path,
            info.start_line
        );
        eprintln!("      ID: {}", info.id);
    }

    if symbols.len() > 10 {
        eprintln!("  ... and {} more", symbols.len() - 10);
    }

    eprintln!();
    eprintln!("Use the numeric ID for precise lookup (e.g., --target <ID>)");

    Err(anyhow::anyhow!(
        "Target '{}' is ambiguous ({} matches)",
        target,
        symbols.len()
    ))
}

/// Resolve target for SQLite backend
fn resolve_target_sqlite(graph: &mut CodeGraph, target: &str) -> Result<ResolvedTarget> {
    use magellan::graph::query;
    use magellan::ingest::SymbolFact;

    // First try symbol ID (32-char BLAKE3 hash or 64-char hex)
    if target.len() == 64 || target.len() == 32 {
        if let Ok(Some(_symbol)) = query::find_by_symbol_id(graph, target) {
            // Get entity ID for this symbol
            if let Ok(entity_id) = graph.resolve_symbol_entity(target) {
                // Get full symbol info for file path
                if let Ok(info) = graph.symbol_by_entity_id(entity_id) {
                    return Ok(ResolvedTarget {
                        numeric_id: entity_id as u64,
                        symbol_id: target.to_string(),
                        fqn: info.fqn.unwrap_or_else(|| target.to_string()),
                        file_path: info.file_path,
                        kind: info.kind,
                    });
                }
            }
        }
    }

    // Try FQN lookup using resolve_symbol_entity (handles both symbol_id and FQN)
    match graph.resolve_symbol_entity(target) {
        Ok(entity_id) => {
            // Found by FQN or symbol_id, get symbol info
            if let Ok(info) = graph.symbol_by_entity_id(entity_id) {
                return Ok(ResolvedTarget {
                    numeric_id: entity_id as u64,
                    symbol_id: info.symbol_id.unwrap_or_default(),
                    fqn: info.fqn.unwrap_or_else(|| target.to_string()),
                    file_path: info.file_path,
                    kind: info.kind,
                });
            }
        }
        Err(_) => {
            // Not found by FQN either, try name search
        }
    }

    // Try name search across all files
    let file_nodes = graph.all_file_nodes()?;
    let mut matches: Vec<(i64, SymbolFact, Option<String>)> = Vec::new();

    for file_path in file_nodes.keys() {
        let entries = query::symbol_nodes_in_file_with_ids(graph, file_path)?;
        for (node_id, symbol, symbol_id) in entries {
            if let Some(name) = &symbol.name {
                if name == target || name.contains(target) {
                    matches.push((node_id, symbol, symbol_id));
                }
            }
        }
    }

    if matches.is_empty() {
        return Err(anyhow::anyhow!(
            "Target symbol '{}' not found (tried symbol_id, FQN, and name)",
            target
        ));
    }

    if matches.len() == 1 {
        let (node_id, symbol, symbol_id) = &matches[0];
        let fqn = symbol.canonical_fqn.as_ref()
            .or(symbol.display_fqn.as_ref())
            .map(|s| s.as_str())
            .unwrap_or_else(|| symbol.name.as_deref().unwrap_or("<unknown>"));
        return Ok(ResolvedTarget {
            numeric_id: *node_id as u64,
            symbol_id: symbol_id.clone().unwrap_or_default(),
            fqn: fqn.to_string(),
            file_path: symbol.file_path.to_string_lossy().to_string(),
            kind: symbol.kind_normalized.clone(),
        });
    }

    // Multiple matches - show ranked list
    eprintln!("Ambiguous target '{}': found {} candidates", target, matches.len());
    eprintln!();
    eprintln!("Top matches:");

    for (i, (_node_id, symbol, symbol_id)) in matches.iter().take(10).enumerate() {
        let fqn = symbol.canonical_fqn.as_ref()
            .or(symbol.display_fqn.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("<unknown>");
        let sid = symbol_id.as_deref().unwrap_or("<none>");
        let name = symbol.name.as_deref().unwrap_or("<unknown>");

        eprintln!(
            "  [{}] {} ({}) in {}:{}",
            i + 1,
            name,
            symbol.kind_normalized,
            symbol.file_path.display(),
            symbol.start_line
        );
        eprintln!("      Symbol ID: {}", sid);
        eprintln!("      FQN: {}", fqn);
    }

    if matches.len() > 10 {
        eprintln!("  ... and {} more", matches.len() - 10);
    }

    eprintln!();
    eprintln!("Use --target <symbol_id> for precise lookup");

    Err(anyhow::anyhow!(
        "Target '{}' is ambiguous ({} matches)",
        target,
        matches.len()
    ))
}

/// Run the slice command
pub fn run_slice(
    db_path: PathBuf,
    target: String,
    direction: CliSliceDirection,
    verbose: bool,
    output_format: OutputFormat,
) -> Result<()> {
    let mut args = vec![
        "slice".to_string(),
        "--target".to_string(),
        target.clone(),
        "--direction".to_string(),
        direction.as_str().to_string(),
    ];
    if verbose {
        args.push("--verbose".to_string());
    }

    let backend = MagellanBackend::open(&db_path)?;
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = magellan::output::generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    // Resolve target to symbol
    let resolved = match resolve_target(&backend, &mut graph, &db_path, &target) {
        Ok(r) => r,
        Err(e) => {
            backend.finish_execution(&exec_id, "error", Some(&e.to_string()), 0, 0, 0)?;
            return Err(e);
        }
    };

    let backend_type = MagellanBackend::detect_type(&db_path);

    // Compute slice using call-graph reachability
    let included_symbols = match backend_type {
        BackendType::Geometric => {
            // For geometric backend, use numeric IDs
            let included_ids = match direction {
                CliSliceDirection::Backward => backend.reverse_reachable_from(resolved.numeric_id),
                CliSliceDirection::Forward => backend.reachable_from(resolved.numeric_id),
            };

            let mut symbols = Vec::new();
            for id in &included_ids {
                if let Some(info) = backend.find_symbol_by_id(*id) {
                    symbols.push(magellan::graph::SymbolInfo {
                        symbol_id: Some(id.to_string()),
                        fqn: Some(info.fqn.clone()),
                        kind: format!("{:?}", info.kind),
                        file_path: info.file_path.clone(),
                    });
                }
            }
            symbols
        }
        BackendType::SQLite | BackendType::NativeV3 => {
            // For SQLite backend, use symbol-based reachability
            let symbols_result = match direction {
                CliSliceDirection::Backward => graph.reverse_reachable_symbols(&resolved.symbol_id, None),
                CliSliceDirection::Forward => graph.reachable_symbols(&resolved.symbol_id, None),
            };

            match symbols_result {
                Ok(symbols) => symbols,
                Err(e) => {
                    backend.finish_execution(&exec_id, "error", Some(&e.to_string()), 0, 0, 0)?;
                    return Err(e);
                }
            }
        }
    };

    let target_info = magellan::graph::SymbolInfo {
        symbol_id: Some(resolved.symbol_id.clone()),
        fqn: Some(resolved.fqn.clone()),
        kind: resolved.kind.clone(),
        file_path: resolved.file_path.clone(),
    };

    let slice_result = magellan::graph::SliceResult {
        slice: magellan::graph::ProgramSlice {
            direction: match direction {
                CliSliceDirection::Backward => magellan::graph::SliceDirection::Backward,
                CliSliceDirection::Forward => magellan::graph::SliceDirection::Forward,
            },
            target: target_info.clone(),
            included_symbols: included_symbols.clone(),
            symbol_count: included_symbols.len(),
        },
        statistics: magellan::graph::SliceStatistics {
            total_symbols: included_symbols.len(),
            data_dependencies: 0,
            control_dependencies: included_symbols.len().saturating_sub(1),
        },
    };

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(&resolved.fqn, slice_result, verbose, &exec_id, output_format);
    }

    // Human mode
    let direction_label = match direction {
        CliSliceDirection::Backward => "that affect",
        CliSliceDirection::Forward => "affected by",
    };

    println!("Program slice: symbols {} \"{}\"", direction_label, resolved.fqn);
    println!("  Target: {} ({})", resolved.fqn, resolved.kind);
    println!("  File:   {}", resolved.file_path);
    println!("  Total symbols: {}", slice_result.statistics.total_symbols);
    if verbose {
        println!(
            "  Data dependencies: {} (not computed in call-graph fallback)",
            slice_result.statistics.data_dependencies
        );
        println!(
            "  Control dependencies: {}",
            slice_result.statistics.control_dependencies
        );
    }

    if slice_result.slice.included_symbols.is_empty() {
        println!("\n  No symbols in slice.");
    } else {
        println!("\n  Symbols in slice:");
        for symbol in &slice_result.slice.included_symbols {
            let fqn_display = symbol.fqn.as_deref().unwrap_or("?");
            println!(
                "    {} ({}) in {}",
                fqn_display, symbol.kind, symbol.file_path
            );
        }
    }

    if slice_result.statistics.data_dependencies == 0
        && !slice_result.slice.included_symbols.is_empty()
    {
        println!("\n  Note: Current implementation uses call-graph reachability.");
    }

    backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

fn output_json_mode(
    _target: &str,
    slice_result: magellan::graph::SliceResult,
    _verbose: bool,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    // Use placeholder values for line numbers since SymbolInfo doesn't have them
    let target_span = Span::new(
        slice_result.slice.target.file_path.clone(),
        0,
        0,
        1,
        0,
        1,
        0,
    );
    let target_symbol_id = slice_result.slice.target.symbol_id.clone();
    let target_match = SymbolMatch::new(
        slice_result
            .slice
            .target
            .fqn
            .unwrap_or_else(|| "?".to_string()),
        slice_result.slice.target.kind,
        target_span,
        None,
        target_symbol_id,
    );

    let included_symbols: Vec<SymbolMatch> = slice_result
        .slice
        .included_symbols
        .into_iter()
        .map(|sym| {
            let span = Span::new(sym.file_path.clone(), 0, 0, 1, 0, 1, 0);
            SymbolMatch::new(
                sym.fqn.unwrap_or_else(|| "?".to_string()),
                sym.kind,
                span,
                None,
                sym.symbol_id,
            )
        })
        .collect();

    let direction = match slice_result.slice.direction {
        magellan::graph::SliceDirection::Backward => "backward".to_string(),
        magellan::graph::SliceDirection::Forward => "forward".to_string(),
    };

    let statistics = SliceStats {
        total_symbols: slice_result.statistics.total_symbols,
        data_dependencies: slice_result.statistics.data_dependencies,
        control_dependencies: slice_result.statistics.control_dependencies,
    };

    let response = SliceResponse {
        target: target_match,
        direction,
        included_symbols,
        statistics,
    };

    let json_response = JsonResponse::new(response, exec_id);
    output_json(&json_response, output_format)?;

    Ok(())
}

//! Slice command implementation
//!
//! Shows program slices (backward/forward) for bug isolation and refactoring safety.

use anyhow::Result;
use magellan::backend_router::MagellanBackend;
use magellan::output::command::{SliceResponse, SliceStats, Span, SymbolMatch};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use std::path::PathBuf;

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
    let exec_id = magellan::output::generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    backend.start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &args,
        None,
        &db_path_str,
    )?;

    // Parse target ID
    let target_id: u64 = target.parse().unwrap_or(0);

    // Get target symbol info
    let target_info_unified = backend
        .find_symbol_by_id(target_id)
        .ok_or_else(|| anyhow::anyhow!("Target symbol ID '{}' not found", target))?;

    // Convert UnifiedSymbolInfo to SymbolInfo
    let target_info = magellan::graph::SymbolInfo {
        symbol_id: Some(target_id.to_string()),
        fqn: Some(target_info_unified.fqn.clone()),
        kind: format!("{:?}", target_info_unified.kind),
        file_path: target_info_unified.file_path.clone(),
    };

    // Compute slice using call-graph reachability
    let included_ids = match direction {
        CliSliceDirection::Backward => backend.reverse_reachable_from(target_id),
        CliSliceDirection::Forward => backend.reachable_from(target_id),
    };

    // Build included symbols
    let mut included_symbols = Vec::new();
    for id in &included_ids {
        if let Some(info) = backend.find_symbol_by_id(*id) {
            included_symbols.push(magellan::graph::SymbolInfo {
                symbol_id: Some(id.to_string()),
                fqn: Some(info.fqn.clone()),
                kind: format!("{:?}", info.kind),
                file_path: info.file_path.clone(),
            });
        }
    }

    let slice_result = magellan::graph::SliceResult {
        slice: magellan::graph::ProgramSlice {
            direction: match direction {
                CliSliceDirection::Backward => magellan::graph::SliceDirection::Backward,
                CliSliceDirection::Forward => magellan::graph::SliceDirection::Forward,
            },
            target: target_info.clone(),
            included_symbols,
            symbol_count: included_ids.len(),
        },
        statistics: magellan::graph::SliceStatistics {
            total_symbols: included_ids.len(),
            data_dependencies: 0,
            control_dependencies: included_ids.len().saturating_sub(1),
        },
    };

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        backend.finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(&target, slice_result, verbose, &exec_id, output_format);
    }

    // Human mode
    let direction_label = match direction {
        CliSliceDirection::Backward => "that affect",
        CliSliceDirection::Forward => "affected by",
    };

    println!("Program slice: symbols {} \"{}\"", direction_label, target);
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

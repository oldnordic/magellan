//! Slice command implementation
//!
//! Shows program slices (backward/forward) for bug isolation and refactoring safety.

use anyhow::Result;
use magellan::graph::{SliceDirection, SliceResult};
use magellan::output::command::{SliceResponse, SliceStats, Span, SymbolMatch};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::CodeGraph;
use std::path::PathBuf;

/// Slice direction for CLI arguments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliSliceDirection {
    Backward,
    Forward,
}

impl CliSliceDirection {
    /// Convert from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "backward" => Some(CliSliceDirection::Backward),
            "forward" => Some(CliSliceDirection::Forward),
            _ => None,
        }
    }

    /// Convert to SliceDirection
    pub fn to_direction(&self) -> SliceDirection {
        match self {
            CliSliceDirection::Backward => SliceDirection::Backward,
            CliSliceDirection::Forward => SliceDirection::Forward,
        }
    }

    /// As string
    pub fn as_str(&self) -> &'static str {
        match self {
            CliSliceDirection::Backward => "backward",
            CliSliceDirection::Forward => "forward",
        }
    }
}

/// Run the slice command
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `target` - Target symbol ID to slice from
/// * `direction` - Slice direction (backward or forward)
/// * `verbose` - Show detailed statistics
/// * `output_format` - Output format (Human or Json)
///
/// # Displays
/// Human-readable slice results with statistics or JSON output
///
/// # Note
/// Uses call-graph reachability as a fallback for full CFG-based slicing.
pub fn run_slice(
    db_path: PathBuf,
    target: String,
    direction: CliSliceDirection,
    verbose: bool,
    output_format: OutputFormat,
) -> Result<()> {
    // Build args for execution tracking
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

    let graph = CodeGraph::open(&db_path)?;
    let exec_id = magellan::output::generate_execution_id();
    let db_path_str = db_path.to_string_lossy().to_string();

    graph
        .execution_log()
        .start_execution(
            &exec_id,
            env!("CARGO_PKG_VERSION"),
            &args,
            None,
            &db_path_str,
        )?;

    // Compute slice
    let slice_result = match direction.to_direction() {
        SliceDirection::Backward => graph.backward_slice(&target)?,
        SliceDirection::Forward => graph.forward_slice(&target)?,
    };

    // Handle JSON output mode
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        graph
            .execution_log()
            .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
        return output_json_mode(
            &target,
            slice_result,
            verbose,
            &exec_id,
            output_format,
        );
    }

    // Human mode
    let direction_label = match slice_result.slice.direction {
        SliceDirection::Backward => "that affects",
        SliceDirection::Forward => "affected by",
    };

    println!(
        "Program slice: symbols {} \"{}\"",
        direction_label,
        target
    );
    println!(
        "  Total symbols: {}",
        slice_result.statistics.total_symbols
    );
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
                fqn_display,
                symbol.kind,
                symbol.file_path
            );
        }
    }

    // Note about call-graph fallback
    if slice_result.statistics.data_dependencies == 0 && !slice_result.slice.included_symbols.is_empty() {
        println!("\n  Note: Current implementation uses call-graph reachability.");
        println!("  Full CFG-based slicing will be available in a future release.");
    }

    graph
        .execution_log()
        .finish_execution(&exec_id, "success", None, 0, 0, 0)?;
    Ok(())
}

/// Output slice results in JSON format
fn output_json_mode(
    _target: &str,
    slice_result: SliceResult,
    _verbose: bool,
    exec_id: &str,
    output_format: OutputFormat,
) -> Result<()> {
    // Convert target SymbolInfo to SymbolMatch for JSON output
    let target_span = Span::new(
        slice_result.slice.target.file_path.clone(),
        0, // byte_start not available from SymbolInfo
        0, // byte_end not available
        1, // line unknown
        0, // col unknown
        1, // end_line unknown
        0, // end_col unknown
    );
    let target_match = SymbolMatch::new(
        slice_result
            .slice
            .target
            .fqn
            .unwrap_or_else(|| "?".to_string()),
        slice_result.slice.target.kind.clone(),
        target_span,
        None, // parent unknown
        slice_result.slice.target.symbol_id,
    );

    // Convert included symbols to SymbolMatch
    let included_symbols: Vec<SymbolMatch> = slice_result
        .slice
        .included_symbols
        .into_iter()
        .map(|sym| {
            let span = Span::new(
                sym.file_path.clone(),
                0, 0, 1, 0, 1, 0,
            );
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
        SliceDirection::Backward => "backward".to_string(),
        SliceDirection::Forward => "forward".to_string(),
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

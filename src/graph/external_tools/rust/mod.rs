//! Rust MIR CFG extraction via nightly rustc → MIR text
//!
//! This module dumps MIR (Mid-level IR) for Rust source files using the
//! nightly toolchain, then parses the MIR text to extract control-flow graphs.
//!
//! This is an opt-in enrichment path — tree-sitter remains the default indexer.
//! If nightly is not installed, callers fall back to tree-sitter CFG.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

pub use crate::graph::cfg_edges_extract::CfgWithEdges;
pub use crate::graph::external_tools::tool_detector;

pub mod mir_invoker;
pub mod mir_parser;

/// Extract CFG from a Rust source file using MIR.
///
/// This is the main entry point for Rust MIR CFG extraction.
///
/// # Arguments
///
/// * `source_path` - Path to the Rust source file (must be self-contained,
///   no external crate dependencies)
///
/// # Returns
///
/// CfgWithEdges with blocks and edges for all functions in the source file.
///
/// # Errors
///
/// Returns error if:
/// - Nightly rustc is not installed
/// - Compilation fails (e.g. syntax errors in the source)
/// - MIR parsing fails
pub fn extract_cfg_from_rust(source_path: &Path) -> Result<CfgWithEdges> {
    // Step 1: Dump MIR text
    let mir_content =
        mir_invoker::dump_mir(source_path).context("Failed to dump MIR from Rust source")?;

    // Step 2: Parse CFG from MIR text
    let function_cfgs = mir_parser::extract_cfg_from_mir(&mir_content)
        .context("Failed to parse CFG from MIR text")?;

    // Step 3: Merge all function CFGs into one
    let merged_cfg = merge_function_cfgs(function_cfgs);

    Ok(merged_cfg)
}

/// Merge multiple per-function CFGs into a single CfgWithEdges.
///
/// Each function's blocks are appended with offset indices, and edges are
/// remapped to the merged block indices. This matches the pattern used by
/// the C/C++ and Java extractors.
fn merge_function_cfgs(function_cfgs: HashMap<String, CfgWithEdges>) -> CfgWithEdges {
    let mut all_blocks = Vec::new();
    let mut all_edges = Vec::new();
    let mut offset = 0usize;

    for (_name, cfg) in function_cfgs {
        // Remap edge indices with the current offset
        for edge in cfg.edges {
            all_edges.push(crate::graph::cfg_edges_extract::CfgEdge {
                source_idx: edge.source_idx + offset,
                target_idx: edge.target_idx + offset,
                edge_type: edge.edge_type,
            });
        }
        all_blocks.extend(cfg.blocks);
        offset += all_blocks.len();
    }

    CfgWithEdges {
        blocks: all_blocks,
        edges: all_edges,
        function_id: 0,
    }
}

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
use std::sync::OnceLock;

pub use crate::graph::cfg_edges_extract::CfgWithEdges;
pub use crate::graph::external_tools::tool_detector;

pub mod mir_invoker;
pub mod mir_parser;

/// Check if a nightly rustc toolchain is available for MIR dumping.
///
/// The result is cached process-wide: nightly detection shells out to
/// `rustup which`, which must not run once per indexed file.
pub fn is_rustc_nightly_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| tool_detector::find_rustc_nightly().is_ok())
}

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
    let function_cfgs = extract_cfgs_from_rust(source_path)?;
    let merged_cfg = merge_function_cfgs(function_cfgs);

    Ok(merged_cfg)
}

/// Extract per-function CFGs from a Rust source file using MIR.
///
/// Unlike `extract_cfg_from_rust` (which merges everything into one CFG),
/// this keeps each function's CFG separate so callers can attribute blocks
/// and edges to the correct function symbol — the same shape the clang
/// (`extract_cfgs_and_calls_with_flags`) and javac paths produce.
///
/// The file must be self-contained: `rustc -Zunpretty=mir` on a single file
/// fails for anything with `use crate::...`, external crate dependencies, or
/// `mod foo;` declarations. Callers must treat any error as "fall back to
/// tree-sitter for this file", never as an indexing failure.
pub fn extract_cfgs_from_rust(source_path: &Path) -> Result<HashMap<String, CfgWithEdges>> {
    // Step 1: Dump MIR text (15s timeout guards against compiler hangs)
    let mir_content = mir_invoker::dump_mir_with_timeout(source_path, 15)
        .context("Failed to dump MIR from Rust source")?;

    // Step 2: Parse CFG from MIR text
    let function_cfgs = mir_parser::extract_cfg_from_mir(&mir_content)
        .context("Failed to parse CFG from MIR text")?;

    Ok(function_cfgs)
}

/// Merge multiple per-function CFGs into a single CfgWithEdges.
///
/// Each function's blocks are appended with offset indices, and edges are
/// remapped to the merged block indices. This matches the pattern used by
/// the C/C++ and Java extractors.
fn merge_function_cfgs(function_cfgs: HashMap<String, CfgWithEdges>) -> CfgWithEdges {
    let mut all_blocks = Vec::new();
    let mut all_edges = Vec::new();

    for (_name, cfg) in function_cfgs {
        // Capture the offset BEFORE extending: edges of this function index
        // into its own block list, which starts at the current merged length.
        let block_offset = all_blocks.len();

        // Remap edge indices with the current offset
        for edge in cfg.edges {
            all_edges.push(crate::graph::cfg_edges_extract::CfgEdge {
                source_idx: edge.source_idx + block_offset,
                target_idx: edge.target_idx + block_offset,
                edge_type: edge.edge_type,
            });
        }
        all_blocks.extend(cfg.blocks);
    }

    CfgWithEdges {
        blocks: all_blocks,
        edges: all_edges,
        function_id: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType};
    use crate::graph::schema::CfgBlock;

    /// Each block carries its function's name in `kind` so the test can
    /// verify edge endpoints stay inside the owning function regardless of
    /// HashMap iteration order.
    fn func_cfg(name: &str, n_blocks: usize) -> CfgWithEdges {
        let blocks = (0..n_blocks)
            .map(|_| CfgBlock {
                cfg_hash: None,
                statements: None,
                function_id: 0,
                kind: format!("fn:{}", name),
                terminator: "Goto".to_string(),
                byte_start: 0,
                byte_end: 0,
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 0,
                cfg_condition: None,
            })
            .collect::<Vec<_>>();
        // Chain edges bb0->bb1->...->bbN-1 with intra-function indices
        let edges = (0..n_blocks.saturating_sub(1))
            .map(|i| CfgEdge {
                source_idx: i,
                target_idx: i + 1,
                edge_type: CfgEdgeType::Jump,
            })
            .collect();
        CfgWithEdges {
            blocks,
            edges,
            function_id: 0,
        }
    }

    #[test]
    fn test_merge_three_functions_edge_indices() {
        let mut cfgs = HashMap::new();
        cfgs.insert("f1".to_string(), func_cfg("f1", 2));
        cfgs.insert("f2".to_string(), func_cfg("f2", 3));
        cfgs.insert("f3".to_string(), func_cfg("f3", 4));

        let merged = merge_function_cfgs(cfgs);

        assert_eq!(merged.blocks.len(), 2 + 3 + 4);
        assert_eq!(merged.edges.len(), 1 + 2 + 3);

        // Every merged edge must connect two blocks owned by the SAME
        // function. With the pre-fix bug (offset accumulated from the
        // post-extend length), the third merged function's edges were
        // shifted past its own blocks — either out of bounds (the indexing
        // below would panic) or into another function's blocks (kind
        // mismatch).
        for edge in &merged.edges {
            let source = &merged.blocks[edge.source_idx];
            let target = &merged.blocks[edge.target_idx];
            assert_eq!(
                source.kind, target.kind,
                "edge {} -> {} crosses function boundary ({} -> {})",
                edge.source_idx, edge.target_idx, source.kind, target.kind
            );
            // The chain shape must survive the offset remap exactly.
            assert_eq!(
                edge.target_idx,
                edge.source_idx + 1,
                "edge {} -> {} lost its within-function chain shape",
                edge.source_idx,
                edge.target_idx
            );
        }

        // Each function contributes exactly (n_blocks - 1) chain edges.
        let mut per_fn: HashMap<String, usize> = HashMap::new();
        for edge in &merged.edges {
            *per_fn
                .entry(merged.blocks[edge.source_idx].kind.clone())
                .or_insert(0) += 1;
        }
        assert_eq!(per_fn.get("fn:f1"), Some(&1));
        assert_eq!(per_fn.get("fn:f2"), Some(&2));
        assert_eq!(per_fn.get("fn:f3"), Some(&3));
    }
}

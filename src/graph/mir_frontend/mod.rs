//! MIR frontend for Rust CFG extraction (future work)
//!
//! **CURRENT STATUS:** Placeholder for future RUSTC_WRAPPER implementation.
//!
//! The correct approach for MIR-based CFG extraction is the RUSTC_WRAPPER pattern
//! used by Clippy and Miri:
//!
//! ```bash
//! RUSTC_WRAPPER=magellan-mir-extract cargo build
//! ```
//!
//! **Architecture:**
//! 1. Create `magellan-mir-extract` binary (nightly, rustc_private)
//! 2. Wrapper calls `rustc_driver::run_compiler()` with custom `Callbacks`
//! 3. In `after_analysis()` callback: iterate `tcx.hir().items()` → `tcx.optimized_mir(def_id)`
//! 4. Extract CFG blocks/edges from MIR and write to database
//! 5. Compilation continues normally — zero overhead
//!
//! **Why per-file rustc --emit=mir failed:**
//! - Compiling individual source files breaks on external dependencies
//! - Module declarations (`mod foo;`) reference other files
//! - Macros and feature gates require whole-crate compilation
//! - Type resolution needs complete crate context
//!
//! **Future implementation:**
//! - Requires nightly Rust (rustc_private)
//! - ~200-300 LOC wrapper binary
//! - Whole-crate semantics, all deps resolved, macros expanded
//!
//! **Schema kept for future use:**
//! The `MirCfgResult`, `CfgBlock`, and `CfgEdge` types are preserved
//! for the RUSTC_WRAPPER implementation.

use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType};
use crate::graph::schema::CfgBlock;

/// MIR extraction result with CFG blocks and edges
///
/// **NOTE:** This type is kept for future RUSTC_WRAPPER implementation.
/// The current `extract_cfg_from_rust_source` function is removed because
/// per-file compilation is fundamentally broken.
#[derive(Debug, Clone)]
pub struct MirCfgResult {
    /// Function name
    pub function_name: String,
    /// CFG basic blocks
    pub blocks: Vec<CfgBlock>,
    /// CFG edges between blocks
    pub edges: Vec<CfgEdge>,
}

/// Extract CFG using MIR (NOT IMPLEMENTED)
///
/// **REMOVED:** Per-file `rustc --emit=mir` approach is broken.
///
/// **Future:** Implement as RUSTC_WRAPPER with rustc_driver::Callbacks.
pub fn extract_cfg_from_rust_source(
    _function_name: &str,
    _source_code: &str,
) -> anyhow::Result<MirCfgResult> {
    anyhow::bail!(
        "MIR CFG extraction is not implemented. \
        Per-file rustc --emit=mir is broken. \
        Use RUSTC_WRAPPER pattern (see module docs)."
    );
}

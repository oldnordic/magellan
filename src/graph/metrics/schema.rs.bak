//! Metrics data structures for CodeGraph
//!
//! Pre-computed metrics (fan-in, fan-out, LOC, complexity) enable fast debug tool queries
//! without expensive SQL aggregations on graph_entities and graph_edges.

use serde::{Deserialize, Serialize};

/// File-level metrics for fast hotspots and complexity queries
///
/// Computed during file indexing and stored in file_metrics table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetrics {
    /// File path (PRIMARY KEY)
    pub file_path: String,

    /// Number of symbols defined in this file
    pub symbol_count: i64,

    /// Actual line count (newline count + 1)
    pub loc: i64,

    /// Estimated LOC (bytes / 40.0 heuristic)
    pub estimated_loc: f64,

    /// Incoming edges from other files (references + calls into this file)
    pub fan_in: i64,

    /// Outgoing edges to other files (references + calls from this file)
    pub fan_out: i64,

    /// Weighted complexity score: loc*0.1 + fan_in*0.5 + fan_out*0.3
    pub complexity_score: f64,

    /// Unix timestamp (seconds) when metrics were last updated
    pub last_updated: i64,
}

/// Symbol-level metrics for detailed analysis
///
/// Computed during file indexing and stored in symbol_metrics table.
/// cyclomatic_complexity is set to 1 (placeholder) until Phase 35 CFG implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMetrics {
    /// Symbol ID (PRIMARY KEY, FOREIGN KEY to graph_entities)
    pub symbol_id: i64,

    /// Symbol name for display
    pub symbol_name: String,

    /// Symbol kind (Function, Struct, Enum, etc.)
    pub kind: String,

    /// File path containing this symbol
    pub file_path: String,

    /// Lines of code (end_line - start_line + 1)
    pub loc: i64,

    /// Estimated LOC (bytes / 40.0 heuristic)
    pub estimated_loc: f64,

    /// Incoming reference/call edges
    pub fan_in: i64,

    /// Outgoing call edges
    pub fan_out: i64,

    /// Cyclomatic complexity (placeholder = 1, Phase 35 will compute properly)
    pub cyclomatic_complexity: i64,

    /// Unix timestamp (seconds) when metrics were last updated
    pub last_updated: i64,
}

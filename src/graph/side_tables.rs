//! Side table storage abstraction for Magellan
//!
//! This module provides backend-specific side table implementations:
//! - `SqliteSideTables`: Uses SQLite for all side tables
//!
//! # Design Philosophy
//!
//! Each backend is **fully self-contained**:
//! - SQLite backend: Everything in `.db` file
//!   No mixing between backends for optimal performance.

use anyhow::Result;
use std::path::Path;

// Re-export types from existing modules
pub use crate::generation::CodeChunk;
pub use crate::graph::execution_log::ExecutionRecord;
pub use crate::graph::metrics::{FileMetrics, SymbolMetrics};

/// A content search result from FTS5 full-text search over code_chunks.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeContentSearchResult {
    /// Symbol name (may be NULL for non-symbol chunks)
    pub symbol_name: Option<String>,
    /// Symbol kind (fn, struct, etc.)
    pub symbol_kind: Option<String>,
    /// File path
    pub file_path: String,
    /// Byte offset of the chunk start
    pub byte_start: usize,
    /// Byte offset of the chunk end
    pub byte_end: usize,
    /// Computed start line (1-indexed)
    pub start_line: usize,
    /// Computed end line (1-indexed)
    pub end_line: usize,
    /// Text excerpt around the first match (~200 chars)
    pub excerpt: String,
    /// FTS5 rank (lower = better match)
    pub rank: f64,
}

/// Side table operations trait - backend-agnostic interface
///
/// Both backends implement this trait to provide:
/// - Code chunks storage
/// - File/symbol metrics
/// - Execution logging
///
/// This trait is object-safe and can be used with `Box<dyn SideTables>`.
pub trait SideTables: Send + Sync {
    // ===== Execution Log Methods =====

    /// Start a new execution log entry
    fn start_execution(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64>;

    /// Finish an execution log entry
    fn finish_execution(
        &self,
        execution_id: &str,
        outcome: &str,
        error_message: Option<&str>,
        files_indexed: usize,
        symbols_indexed: usize,
        references_indexed: usize,
    ) -> Result<()>;

    /// Get an execution record by execution_id
    fn get_execution(&self, execution_id: &str) -> Result<Option<ExecutionRecord>>;

    /// List all executions, ordered by most recent first
    fn list_executions(&self, limit: Option<usize>) -> Result<Vec<ExecutionRecord>>;

    // ===== File Metrics Methods =====

    /// Store file metrics
    fn store_file_metrics(&self, metrics: &FileMetrics) -> Result<()>;

    /// Get file metrics by file path
    fn get_file_metrics(&self, file_path: &str) -> Result<Option<FileMetrics>>;

    // ===== Symbol Metrics Methods =====

    /// Store symbol metrics
    fn store_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()>;

    /// Get symbol metrics by symbol_id
    fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>>;

    /// Delete all metrics for a file
    fn delete_metrics_for_file(&self, file_path: &str) -> Result<usize>;

    /// Get hotspots (files with highest complexity scores)
    fn get_hotspots(
        &self,
        limit: Option<u32>,
        min_loc: Option<i64>,
        min_fan_in: Option<i64>,
        min_fan_out: Option<i64>,
    ) -> Result<Vec<FileMetrics>>;

    // ===== Code Chunk Methods =====

    /// Store a code chunk
    fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64>;

    /// Get a code chunk by ID
    fn get_chunk(&self, chunk_id: i64) -> Result<Option<CodeChunk>>;

    /// Get a code chunk by file path and byte span
    fn get_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>>;

    /// Get all chunks for a file
    fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>>;

    /// Count chunks for a file
    fn count_chunks_for_file(&self, file_path: &str) -> Result<usize>;

    /// Delete all chunks for a file
    fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize>;

    /// Get chunks by symbol name
    fn get_chunks_by_symbol(&self, file_path: &str, symbol_name: &str) -> Result<Vec<CodeChunk>>;

    /// Get all chunks
    fn get_all_chunks(&self) -> Result<Vec<CodeChunk>>;

    /// Count all chunks
    fn count_chunks(&self) -> Result<usize>;

    /// Search code chunk content via FTS5 full-text search.
    ///
    /// Returns ranked results with symbol metadata, file path, byte span,
    /// and a text excerpt around the first match.
    fn search_code_content(
        &self,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<CodeContentSearchResult>>;

    // ===== AST Node Methods =====

    /// Store an AST node, return node ID
    fn store_ast_node(&self, node: &crate::graph::AstNode, file_id: i64) -> Result<i64>;

    /// Store multiple AST nodes in a batch operation
    ///
    /// This is more efficient than calling `store_ast_node` multiple times,
    /// especially for SQLite which can use transactions for bulk inserts.
    ///
    /// # Arguments
    /// * `nodes` - Vector of tuples containing (AstNode, file_id)
    ///
    /// # Returns
    /// Vector of assigned node IDs in the same order as input nodes
    ///
    /// # Performance
    /// - SQLite: Uses a single transaction for all inserts
    /// - V3: Batches KV operations together
    fn store_ast_nodes_batch(&self, nodes: &[(crate::graph::AstNode, i64)]) -> Result<Vec<i64>>;

    /// Get AST node by ID
    fn get_ast_node(&self, node_id: i64) -> Result<Option<crate::graph::AstNode>>;

    /// Update the parent_id of an AST node
    ///
    /// Used after batch insertion to resolve placeholder parent references.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node to update
    /// * `new_parent_id` - The new parent ID to set
    ///
    /// # Implementation
    /// - SQLite: Uses UPDATE query for efficient in-place update
    /// - V3: Deletes and re-inserts the node (KV stores don't support updates)
    fn update_ast_node_parent(&self, node_id: i64, new_parent_id: i64) -> Result<()>;

    /// Get all AST nodes for a file
    fn get_ast_nodes_by_file(&self, file_id: i64) -> Result<Vec<crate::graph::AstNode>>;

    /// Get all AST nodes (for finding roots)
    fn get_all_ast_nodes(&self) -> Result<Vec<crate::graph::AstNode>>;

    /// Get AST nodes by kind (e.g., "if_expression")
    fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<crate::graph::AstNode>>;

    /// Get children of an AST node
    fn get_ast_children(&self, parent_id: i64) -> Result<Vec<crate::graph::AstNode>>;

    /// Count all AST nodes
    fn count_ast_nodes(&self) -> Result<usize>;

    /// Count AST nodes for a specific file
    fn count_ast_nodes_for_file(&self, file_id: i64) -> Result<usize>;

    /// Delete all AST nodes for a file (returns count deleted)
    fn delete_ast_nodes_for_file(&self, file_id: i64) -> Result<usize>;

    // ===== Cross-File Reference Methods =====

    /// Store a cross-file reference
    ///
    /// # Arguments
    /// * `cref` - The cross-file reference to store
    fn store_cross_file_ref(&self, cref: &crate::graph::schema::CrossFileRef) -> Result<()>;

    /// Get all references to a specific symbol (by target symbol ID)
    ///
    /// # Arguments
    /// * `to_symbol_id` - The target symbol ID
    ///
    /// # Returns
    /// Vector of cross-file references where `to_symbol_id` is the target
    fn get_references_to(
        &self,
        to_symbol_id: &str,
    ) -> Result<Vec<crate::graph::schema::CrossFileRef>>;

    /// Get all references from a specific symbol (by source symbol ID)
    ///
    /// # Arguments
    /// * `from_symbol_id` - The source symbol ID
    ///
    /// # Returns
    /// Vector of cross-file references where `from_symbol_id` is the source
    fn get_references_from(
        &self,
        from_symbol_id: &str,
    ) -> Result<Vec<crate::graph::schema::CrossFileRef>>;

    /// Delete all cross-file references for a file
    ///
    /// # Arguments
    /// * `file_path` - The file path to delete references for
    ///
    /// # Returns
    /// Number of references deleted
    fn delete_cross_file_refs_for_file(&self, file_path: &str) -> Result<usize>;

    /// Count total cross-file references
    fn count_cross_file_refs(&self) -> Result<usize>;

    // ===== Label Methods =====

    /// Add a label to an entity
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID to label
    /// * `label` - The label to add
    fn add_label(&self, entity_id: i64, label: &str) -> Result<()>;

    /// Get all labels for an entity
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID
    ///
    /// # Returns
    /// Vector of labels for the entity
    fn get_labels_for_entity(&self, entity_id: i64) -> Result<Vec<String>>;

    /// Get all entities with a specific label
    ///
    /// # Arguments
    /// * `label` - The label to query
    ///
    /// # Returns
    /// Vector of entity IDs with this label
    fn get_entities_by_label(&self, label: &str) -> Result<Vec<i64>>;

    /// Get all labels in use
    ///
    /// # Returns
    /// Vector of all distinct labels
    fn get_all_labels(&self) -> Result<Vec<String>>;

    /// Convert to Any for downcasting
    ///
    /// This allows downcasting to concrete backend types (e.g., V3SideTables)
    /// for backend-specific operations.
    fn as_any(&self) -> &dyn std::any::Any;
}

// Re-export telemetry types for SideTables integration
pub use crate::graph::telemetry::{TelemetryEvent, TelemetryEventType};

// =============================================================================
// SQLite Implementation
// =============================================================================

#[cfg(feature = "sqlite-backend")]
#[path = "side_tables_sqlite.rs"]
mod side_tables_sqlite;

// =============================================================================
// Re-exports
// =============================================================================

#[cfg(feature = "sqlite-backend")]
pub use side_tables_sqlite::SqliteSideTables;

/// Create appropriate side tables for the selected backend
#[cfg(feature = "sqlite-backend")]
pub fn create_side_tables(db_path: &Path) -> Result<Box<dyn SideTables>> {
    Ok(Box::new(SqliteSideTables::open(db_path)?))
}

/// Build a text excerpt from content around the first match of the FTS query.
///
/// Extracts the first query token, finds it (case-insensitive) in the content,
/// and returns ~200 chars centered on the match. If no token is found, returns
/// the first 200 chars of the content.
fn build_excerpt(content: &str, pattern: &str) -> String {
    let excerpt_len = 200usize;
    let tokens: Vec<&str> = pattern.split_whitespace().filter(|t| t.len() > 1).collect();

    for token in &tokens {
        if let Some(pos) = content.to_lowercase().find(&token.to_lowercase()) {
            let start = pos.saturating_sub(excerpt_len / 2);
            let end = (start + excerpt_len).min(content.len());
            let excerpt = &content[start..end];
            let prefix = if start > 0 { "..." } else { "" };
            let suffix = if end < content.len() { "..." } else { "" };
            return format!("{prefix}{excerpt}{suffix}");
        }
    }

    // No token found — return first N chars
    let end = excerpt_len.min(content.len());
    let excerpt = &content[..end];
    if content.len() > end {
        format!("{excerpt}...")
    } else {
        excerpt.to_string()
    }
}

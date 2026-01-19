//! Graph schema definitions for Magellan
//!
//! Defines node and edge payloads for sqlitegraph persistence.

use serde::{Deserialize, Serialize};

use anyhow::Result;

/// File node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String,
    pub hash: String,
    /// Unix timestamp (seconds since epoch) when this file was last indexed
    pub last_indexed_at: i64,
    /// Unix timestamp (seconds since epoch) of filesystem mtime when indexed
    pub last_modified: i64,
}

/// Symbol node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Stable symbol ID derived from (language, fqn, span_id)
    ///
    /// Generated via SHA-256 hash of language:fqn:span_id.
    /// This ID is deterministic and stable across runs for the same symbol.
    #[serde(default)]
    pub symbol_id: Option<String>,
    pub name: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub kind_normalized: Option<String>,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Reference node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceNode {
    pub file: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
}

/// Call node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode {
    pub file: String,
    pub caller: String,
    pub callee: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
}

/// A single edge endpoint row for orphan detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeEndpoints {
    pub from_id: i64,
    pub to_id: i64,
}

/// Delete edges whose from_id OR to_id is in the provided set of entity IDs.
///
/// Determinism: IDs must be pre-sorted by caller.
pub fn delete_edges_touching_entities(
    conn: &rusqlite::Connection,
    entity_ids_sorted: &[i64],
) -> Result<usize> {
    use rusqlite::params_from_iter;

    if entity_ids_sorted.is_empty() {
        return Ok(0);
    }

    // Build placeholders for IN list.
    let placeholders = std::iter::repeat("?")
        .take(entity_ids_sorted.len())
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "DELETE FROM graph_edges WHERE from_id IN ({}) OR to_id IN ({})",
        placeholders, placeholders
    );

    // Params are duplicated (for from_id and to_id IN lists).
    let params = entity_ids_sorted
        .iter()
        .chain(entity_ids_sorted.iter())
        .map(|id| *id);

    let affected = conn
        .execute(&sql, params_from_iter(params))
        .map_err(|e| anyhow::anyhow!("Failed to delete edges touching entities: {}", e))?;

    Ok(affected)
}

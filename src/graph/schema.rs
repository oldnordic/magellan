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

    /// Fully-qualified name for this symbol
    /// Format varies by language (crate::module::Name for Rust, package.Class.Name for Java)
    /// This is the primary key for symbol lookup, preventing name collisions
    #[serde(default)]
    pub fqn: Option<String>,

    /// Canonical fully-qualified name with file path for unambiguous identity
    /// Format: crate_name::file_path::kind symbol_name
    /// Example: my_crate::src/lib.rs::Function my_function
    #[serde(default)]
    pub canonical_fqn: Option<String>,

    /// Display fully-qualified name for human-readable output
    /// Shortened form without file path when possible
    /// Example: my_crate::my_function
    #[serde(default)]
    pub display_fqn: Option<String>,

    /// Simple symbol name (display name)
    /// For user-facing output only. Not used as a unique identifier.
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
    /// Stable symbol ID of the caller
    #[serde(default)]
    pub caller_symbol_id: Option<String>,
    /// Stable symbol ID of the callee
    #[serde(default)]
    pub callee_symbol_id: Option<String>,
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

/// Control Flow Basic Block node payload stored in database
///
/// Represents a single basic block in a function's control flow graph.
/// Basic blocks are sequences of statements with a single entry point
/// and single exit point (via terminator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgBlock {
    /// Function symbol ID this block belongs to
    pub function_id: i64,

    /// Block kind (entry, conditional, loop, match, return, etc.)
    pub kind: String,

    /// Terminator kind (how control exits this block)
    pub terminator: String,

    /// Byte offset where block starts
    pub byte_start: u64,

    /// Byte offset where block ends
    pub byte_end: u64,

    /// Line where block starts (1-indexed)
    pub start_line: u64,

    /// Column where block starts (0-indexed)
    pub start_col: u64,

    /// Line where block ends (1-indexed)
    pub end_line: u64,

    /// Column where block ends (0-indexed)
    pub end_col: u64,
}

/// Control Flow Edge payload stored in graph_edges table
///
/// Represents edges between basic blocks in a function's CFG.
/// Uses edge_type = "CFG_BLOCK" for graph_edges entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgEdge {
    /// Source block ID
    pub from_block_id: i64,

    /// Target block ID
    pub to_block_id: i64,

    /// Edge kind (unconditional, conditional_true, conditional_false, etc.)
    pub kind: String,
}

/// Import node payload stored in sqlitegraph
///
/// Represents an import/use/from statement in source code.
/// Stores metadata about what was imported and where.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportNode {
    /// File path containing this import
    pub file: String,
    /// Kind of import statement (use_crate, use_super, use_self, etc.)
    pub import_kind: String,
    /// Full import path as components (e.g., ["crate", "foo", "bar"] for crate::foo::bar)
    pub import_path: Vec<String>,
    /// Specific names imported (e.g., ["HashMap", "HashSet"] for use std::collections::* with specific items)
    pub imported_names: Vec<String>,
    /// Whether this is a glob import (e.g., use foo::* or from foo import *)
    pub is_glob: bool,
    /// Byte offset where import starts
    pub byte_start: u64,
    /// Byte offset where import ends
    pub byte_end: u64,
    /// Line where import starts (1-indexed)
    pub start_line: u64,
    /// Column where import starts (0-indexed)
    pub start_col: u64,
    /// Line where import ends (1-indexed)
    pub end_line: u64,
    /// Column where import ends (0-indexed)
    pub end_col: u64,
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

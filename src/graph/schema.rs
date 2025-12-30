//! Graph schema definitions for Magellan
//!
//! Defines node and edge payloads for sqlitegraph persistence.

use serde::{Deserialize, Serialize};

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
    pub name: Option<String>,
    pub kind: String,
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

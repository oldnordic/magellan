//! Graph schema definitions for Magellan
//!
//! Defines node and edge payloads for sqlitegraph persistence.

use serde::{Deserialize, Serialize};

/// File node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String,
    pub hash: String,
}

/// Symbol node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    pub name: Option<String>,
    pub kind: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Reference node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceNode {
    pub file: String,
    pub byte_start: u64,
    pub byte_end: u64,
}

//! Symbol node operations for CodeGraph
//!
//! Handles symbol node CRUD operations and DEFINES edge management.
//!
//! # Symbol ID Generation
//!
//! Symbol IDs are stable identifiers derived from a symbol's defining characteristics:
//! - **Language**: The programming language (e.g., "rust", "python", "javascript")
//! - **Fully-Qualified Name (FQN)**: The complete hierarchical name of the symbol
//! - **Span ID**: The stable identifier of the defining span in the source file
//!
//! The symbol ID format is: `SHA256(language:fqn:span_id)[0..16]` (16 hex characters)
//!
//! ## Stability Guarantees
//!
//! Symbol IDs are **stable** when:
//! - The same symbol is re-indexed after content changes elsewhere in the file
//! - The file path remains the same (span_id depends on file path)
//! - The language detection is consistent
//! - The fully-qualified name doesn't change
//!
//! Symbol IDs **change** when:
//! - The symbol is renamed (FQN changes)
//! - The symbol is moved to a different location (span_id changes)
//! - The file is renamed or moved (span_id depends on file path)
//! - The symbol's defining signature changes (affects FQN in future versions)

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlitegraph::{
    add_label, BackendDirection, EdgeSpec, GraphBackend, NeighborQuery, NodeId, NodeSpec,
    SqliteGraphBackend,
};
use std::rc::Rc;

use crate::detect_language;
use crate::graph::schema::SymbolNode;
use crate::ingest::SymbolFact;

/// Generate a stable symbol ID from (language, fqn, span_id)
///
/// Uses SHA-256 for platform-independent, deterministic symbol IDs.
///
/// # Algorithm
///
/// The hash is computed from: `language + ":" + fqn + ":" + span_id`
/// The first 8 bytes (64 bits) of the hash are formatted as 16 hex characters.
///
/// # Properties
///
/// This ensures symbol IDs are:
/// - **Deterministic**: same inputs always produce the same ID
/// - **Platform-independent**: SHA-256 produces consistent results across architectures
/// - **Collision-resistant**: 64-bit space with good distribution
///
/// # Examples
///
/// ```
/// use magellan::graph::symbols::generate_symbol_id;
///
/// let id1 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
/// let id2 = generate_symbol_id("rust", "my_crate::main", "a1b2c3d4e5f6g7h8");
/// let id3 = generate_symbol_id("python", "my_module.main", "a1b2c3d4e5f6g7h8");
///
/// assert_eq!(id1, id2);  // Same inputs = same ID
/// assert_ne!(id1, id3);  // Different language = different ID
/// assert_eq!(id1.len(), 16);  // Always 16 hex characters
/// ```
pub fn generate_symbol_id(language: &str, fqn: &str, span_id: &str) -> String {
    let mut hasher = Sha256::new();

    // Hash language
    hasher.update(language.as_bytes());

    // Separator to distinguish language from fqn
    hasher.update(b":");

    // Hash fully-qualified name
    hasher.update(fqn.as_bytes());

    // Separator to distinguish fqn from span_id
    hasher.update(b":");

    // Hash span_id
    hasher.update(span_id.as_bytes());

    // Take first 8 bytes (64 bits) and format as hex
    let result = hasher.finalize();
    format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            result[0], result[1], result[2], result[3],
            result[4], result[5], result[6], result[7])
}

/// Symbol operations for CodeGraph
pub struct SymbolOps {
    pub backend: Rc<SqliteGraphBackend>,
}

impl SymbolOps {
    /// Insert a symbol node from SymbolFact
    pub fn insert_symbol_node(&self, fact: &SymbolFact) -> Result<NodeId> {
        let symbol_node = SymbolNode {
            symbol_id: None, // Will be set in Task 4
            name: fact.name.clone(),
            kind: format!("{:?}", fact.kind),
            kind_normalized: Some(fact.kind_normalized.clone()),
            byte_start: fact.byte_start,
            byte_end: fact.byte_end,
            start_line: fact.start_line,
            start_col: fact.start_col,
            end_line: fact.end_line,
            end_col: fact.end_col,
        };

        let name = fact.name.clone().unwrap_or_else(|| {
            // Generate a name for unnamed symbols (like impl blocks)
            format!("<{:?} at {}>", fact.kind, fact.byte_start)
        });

        let node_spec = NodeSpec {
            kind: "Symbol".to_string(),
            name,
            file_path: Some(fact.file_path.to_string_lossy().to_string()),
            data: serde_json::to_value(symbol_node)?,
        };

        let id = self.backend.insert_node(node_spec)?;
        let node_id = NodeId::from(id);

        // Add labels for efficient querying
        let graph = self.backend.graph();

        // Language label (e.g., "rust", "python", "javascript")
        if let Some(language) = detect_language(&fact.file_path) {
            add_label(graph, node_id.as_i64(), language.as_str())?;
        }

        // Symbol kind label (e.g., "fn", "struct", "enum", "method")
        add_label(graph, node_id.as_i64(), &fact.kind_normalized)?;

        Ok(node_id)
    }

    /// Insert DEFINES edge from file to symbol
    pub fn insert_defines_edge(&self, file_id: NodeId, symbol_id: NodeId) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: file_id.as_i64(),
            to: symbol_id.as_i64(),
            edge_type: "DEFINES".to_string(),
            data: serde_json::json!({}),
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    /// Delete all symbols and DEFINES edges for a file
    pub fn delete_file_symbols(&self, file_id: NodeId) -> Result<()> {
        // Find all outgoing DEFINES edges
        let neighbor_ids = self.backend.neighbors(
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        // Delete each symbol node (edges are cascade deleted)
        for symbol_node_id in neighbor_ids {
            self.backend.graph().delete_entity(symbol_node_id)?;
        }

        Ok(())
    }
}

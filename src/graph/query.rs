//! Query operations for CodeGraph
//!
//! Handles symbol and reference queries.

use anyhow::Result;
use sqlitegraph::{BackendDirection, NeighborQuery, GraphBackend};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::ingest::{SymbolFact, SymbolKind};
use crate::references::ReferenceFact;
use crate::graph::schema::SymbolNode;

use super::CodeGraph;

/// Query all symbols defined in a file
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
///
/// # Returns
/// Vector of SymbolFact for all symbols in the file
pub fn symbols_in_file(graph: &mut CodeGraph, path: &str) -> Result<Vec<SymbolFact>> {
    symbols_in_file_with_kind(graph, path, None)
}

/// Query symbols defined in a file, optionally filtered by kind
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
/// * `kind` - Optional symbol kind filter (None returns all symbols)
///
/// # Returns
/// Vector of SymbolFact matching the kind filter
pub fn symbols_in_file_with_kind(
    graph: &mut CodeGraph,
    path: &str,
    kind: Option<SymbolKind>,
) -> Result<Vec<SymbolFact>> {
    let file_id = match graph.files.find_file_node(path)? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };

    let path_buf = PathBuf::from(path);

    // Query neighbors via DEFINES edges
    let neighbor_ids = graph.files.backend.neighbors(
        file_id.as_i64(),
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: Some("DEFINES".to_string()),
        },
    )?;

    // Convert each neighbor node ID to SymbolFact, filtering by kind if specified
    let mut symbols = Vec::new();
    for symbol_node_id in neighbor_ids {
        if let Ok(Some(fact)) = graph.files.symbol_fact_from_node(symbol_node_id, path_buf.clone()) {
            // Apply kind filter if specified
            if let Some(ref filter_kind) = kind {
                if fact.kind == *filter_kind {
                    symbols.push(fact);
                }
            } else {
                symbols.push(fact);
            }
        }
    }

    Ok(symbols)
}

/// Query the node ID of a specific symbol by file path and symbol name
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
/// * `name` - Symbol name
///
/// # Returns
/// Option<i64> - Some(node_id) if found, None if not found
///
/// # Note
/// This is a minimal query helper for testing. It reuses existing graph queries
/// and maintains determinism. No new indexes or caching.
pub fn symbol_id_by_name(graph: &mut CodeGraph, path: &str, name: &str) -> Result<Option<i64>> {
    let file_id = match graph.files.find_file_node(path)? {
        Some(id) => id,
        None => return Ok(None),
    };

    // Query neighbors via DEFINES edges
    let neighbor_ids = graph.files.backend.neighbors(
        file_id.as_i64(),
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: Some("DEFINES".to_string()),
        },
    )?;

    // Find symbol with matching name
    for symbol_node_id in neighbor_ids {
        if let Ok(node) = graph.files.backend.get_node(symbol_node_id) {
            if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(node.data) {
                if symbol_node.name.as_ref().map(|n| n == name).unwrap_or(false) {
                    return Ok(Some(symbol_node_id));
                }
            }
        }
    }

    Ok(None)
}

/// Index references for a file into the graph
///
/// # Behavior
/// 1. Get all symbols for this file
/// 2. Build map of symbol name -> node ID
/// 3. Extract references from source
/// 4. Insert Reference nodes and REFERENCES edges
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
/// * `source` - File contents as bytes
///
/// # Returns
/// Number of references indexed
pub fn index_references(graph: &mut CodeGraph, path: &str, source: &[u8]) -> Result<usize> {
    // Get file node ID
    let file_id = match graph.files.find_file_node(path)? {
        Some(id) => id,
        None => return Ok(0), // No file, no references
    };

    // Get all symbols for this file
    let symbol_ids = graph.files.backend.neighbors(
        file_id.as_i64(),
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: Some("DEFINES".to_string()),
        },
    )?;

    // Build map: symbol name -> node ID
    let mut symbol_name_to_id: HashMap<String, i64> = HashMap::new();
    for symbol_id in symbol_ids {
        if let Ok(node) = graph.files.backend.get_node(symbol_id) {
            if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(node.data.clone()) {
                if let Some(name) = symbol_node.name {
                    symbol_name_to_id.insert(name, symbol_id);
                }
            }
        }
    }

    // Index references using ReferenceOps
    graph.references.index_references(path, source, &symbol_name_to_id)
}

/// Query all references to a specific symbol
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `symbol_id` - Node ID of the target symbol
///
/// # Returns
/// Vector of ReferenceFact for all references to the symbol
pub fn references_to_symbol(graph: &mut CodeGraph, symbol_id: i64) -> Result<Vec<ReferenceFact>> {
    graph.references.references_to_symbol(symbol_id)
}

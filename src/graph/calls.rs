//! Call graph operations for CodeGraph
//!
//! Handles call indexing and query operations for CALLS edges.

use anyhow::Result;
use sqlitegraph::{BackendDirection, GraphBackend, NeighborQuery};
use std::collections::HashMap;

use crate::graph::schema::SymbolNode;
use crate::references::CallFact;

use super::CodeGraph;

/// Index calls for a file into the graph
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path
/// * `source` - File contents as bytes
///
/// # Returns
/// Number of calls indexed
pub fn index_calls(graph: &mut CodeGraph, path: &str, source: &[u8]) -> Result<usize> {
    // Get file node ID
    let file_id = match graph.files.find_file_node(path)? {
        Some(id) => id,
        None => return Ok(0), // No file, no calls
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

    // Index calls using CallOps
    graph.calls.index_calls(path, source, &symbol_name_to_id)
}

/// Query all calls FROM a specific symbol (forward call graph)
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path containing the symbol
/// * `name` - Symbol name
///
/// # Returns
/// Vector of CallFact for all calls from this symbol
pub fn calls_from_symbol(graph: &mut CodeGraph, path: &str, name: &str) -> Result<Vec<CallFact>> {
    let symbol_id = match super::query::symbol_id_by_name(graph, path, name)? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    graph.calls.calls_from_symbol(symbol_id)
}

/// Query all calls TO a specific symbol (reverse call graph)
///
/// # Arguments
/// * `graph` - CodeGraph instance
/// * `path` - File path containing the symbol
/// * `name` - Symbol name
///
/// # Returns
/// Vector of CallFact for all calls to this symbol
pub fn callers_of_symbol(graph: &mut CodeGraph, path: &str, name: &str) -> Result<Vec<CallFact>> {
    let symbol_id = match super::query::symbol_id_by_name(graph, path, name)? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    graph.calls.callers_of_symbol(symbol_id)
}

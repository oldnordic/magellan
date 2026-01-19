//! Call graph operations for CodeGraph
//!
//! Handles call indexing and query operations for CALLS edges.

use anyhow::Result;
use sqlitegraph::GraphBackend;
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
    // Build map: FQN -> node ID from ALL symbols in database.
    // Prefer symbols defined in the current file for duplicate names.
    let mut symbol_fqn_to_id: HashMap<String, (i64, bool)> = HashMap::new();
    let entity_ids = graph.files.backend.entity_ids()?;
    for entity_id in entity_ids {
        if let Ok(node) = graph.files.backend.get_node(entity_id) {
            if node.kind != "Symbol" {
                continue;
            }

            let symbol_node: SymbolNode = match serde_json::from_value(node.data.clone()) {
                Ok(value) => value,
                Err(_) => continue,
            };

            // Use FQN as key, fall back to name
            let fqn = symbol_node
                .fqn
                .or(symbol_node.name)
                .unwrap_or_default();

            if fqn.is_empty() {
                continue;
            }

            let is_current_file = node.file_path.as_deref() == Some(path);
            match symbol_fqn_to_id.get(&fqn) {
                Some((_, existing_is_current)) if *existing_is_current || !is_current_file => {}
                _ => {
                    symbol_fqn_to_id.insert(fqn, (entity_id, is_current_file));
                }
            }
        }
    }

    let symbol_fqn_to_id: HashMap<String, i64> = symbol_fqn_to_id
        .into_iter()
        .map(|(fqn, (id, _))| (fqn, id))
        .collect();

    // Index calls using CallOps
    graph.calls.index_calls(path, source, &symbol_fqn_to_id)
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

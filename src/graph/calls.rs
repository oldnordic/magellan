//! Call graph operations for CodeGraph
//!
//! Handles call indexing and query operations for CALLS edges.

use anyhow::Result;
use std::collections::HashMap;

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
///
/// # Performance
/// Uses in-memory SymbolLookup for O(1) symbol resolution instead of O(n) database scan.
pub fn index_calls(graph: &mut CodeGraph, path: &str, source: &[u8]) -> Result<usize> {
    // Use in-memory lookup for O(1) symbol resolution
    // This replaces the previous O(n) database scan
    let symbol_fqn_to_id_with_file = graph.symbols.lookup.fqn_to_id_with_current_file(path);

    let symbol_fqn_to_id: HashMap<String, i64> = symbol_fqn_to_id_with_file
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

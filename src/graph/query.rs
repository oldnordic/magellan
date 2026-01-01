//! Query operations for CodeGraph
//!
//! Handles symbol and reference queries.

use anyhow::Result;
use rusqlite::params;
use sqlitegraph::{BackendDirection, GraphBackend, NeighborQuery};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::graph::schema::SymbolNode;
use crate::ingest::{SymbolFact, SymbolKind};
use crate::references::ReferenceFact;

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
    let entries = symbol_nodes_in_file(graph, path)?;
    Ok(entries.into_iter().map(|(_, fact)| fact).collect())
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
    let entries = symbol_nodes_in_file(graph, path)?;
    let mut symbols = Vec::new();
    for (_, fact) in entries {
        if let Some(ref filter_kind) = kind {
            if fact.kind == *filter_kind {
                symbols.push(fact);
            }
        } else {
            symbols.push(fact);
        }
    }
    Ok(symbols)
}

/// Query symbols in a file along with their node IDs for deterministic CLI output.
pub fn symbol_nodes_in_file(graph: &mut CodeGraph, path: &str) -> Result<Vec<(i64, SymbolFact)>> {
    let file_id = match graph.files.find_file_node(path)? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };

    let path_buf = PathBuf::from(path);

    let neighbor_ids = graph.files.backend.neighbors(
        file_id.as_i64(),
        NeighborQuery {
            direction: BackendDirection::Outgoing,
            edge_type: Some("DEFINES".to_string()),
        },
    )?;

    let mut entries = Vec::new();
    for symbol_node_id in neighbor_ids {
        if let Ok(Some(fact)) = graph
            .files
            .symbol_fact_from_node(symbol_node_id, path_buf.clone())
        {
            entries.push((symbol_node_id, fact));
        }
    }

    entries.sort_by(|(_, a), (_, b)| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| a.start_col.cmp(&b.start_col))
            .then_with(|| a.byte_start.cmp(&b.byte_start))
    });

    Ok(entries)
}

/// Lookup symbol extents (byte + line range) by name within a file.
pub fn symbol_extents(
    graph: &mut CodeGraph,
    path: &str,
    name: &str,
) -> Result<Vec<(i64, SymbolFact)>> {
    let entries = symbol_nodes_in_file(graph, path)?;
    let mut matches = Vec::new();
    for (node_id, fact) in entries {
        if fact.name.as_deref() == Some(name) {
            matches.push((node_id, fact));
        }
    }
    Ok(matches)
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
                if symbol_node
                    .name
                    .as_ref()
                    .map(|n| n == name)
                    .unwrap_or(false)
                {
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
    graph
        .references
        .index_references(path, source, &symbol_name_to_id)
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

// ============================================================================
// Label-based queries (Phase 2: Label and Property Integration)
// ============================================================================

/// Query result containing symbol metadata
#[derive(Debug, Clone)]
pub struct SymbolQueryResult {
    /// Entity ID in the graph
    pub entity_id: i64,
    /// Symbol name
    pub name: String,
    /// File path containing the symbol
    pub file_path: String,
    /// Symbol kind (fn, struct, enum, etc.)
    pub kind: String,
    /// Byte range
    pub byte_start: usize,
    pub byte_end: usize,
}

impl CodeGraph {
    /// Get all entity IDs that have a specific label
    ///
    /// Uses raw SQL to query the graph_labels table directly.
    pub fn get_entities_by_label(&self, label: &str) -> Result<Vec<i64>> {
        let conn = self.chunks.connect()?;

        let mut stmt = conn
            .prepare_cached("SELECT DISTINCT entity_id FROM graph_labels WHERE label = ?1")
            .map_err(|e| anyhow::anyhow!("Failed to prepare label query: {}", e))?;

        let entity_ids = stmt
            .query_map(params![label], |row: &rusqlite::Row| row.get(0))
            .map_err(|e| anyhow::anyhow!("Failed to execute label query: {}", e))?
            .collect::<Result<Vec<i64>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to collect label results: {}", e))?;

        Ok(entity_ids)
    }

    /// Get all entity IDs that have all of the specified labels (AND semantics)
    pub fn get_entities_by_labels(&self, labels: &[&str]) -> Result<Vec<i64>> {
        if labels.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.chunks.connect()?;

        // Build query with positional placeholders for each label
        let placeholders = std::iter::repeat("?")
            .take(labels.len())
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            "SELECT entity_id FROM graph_labels WHERE label IN ({})
             GROUP BY entity_id HAVING COUNT(DISTINCT label) = ?",
            placeholders
        );

        // Build params: label strings + count as i64
        let label_params: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
        let count_param: i64 = labels.len() as i64;

        let mut stmt = conn
            .prepare_cached(&query)
            .map_err(|e| anyhow::anyhow!("Failed to prepare multi-label query: {}", e))?;

        // Combine label params and count param into a single slice
        let params: Vec<&dyn rusqlite::ToSql> = label_params
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .chain(std::iter::once(&count_param as &dyn rusqlite::ToSql))
            .collect();

        let entity_ids = stmt
            .query_map(&params[..], |row: &rusqlite::Row| row.get(0))
            .map_err(|e| anyhow::anyhow!("Failed to execute multi-label query: {}", e))?
            .collect::<Result<Vec<i64>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to collect multi-label results: {}", e))?;

        Ok(entity_ids)
    }

    /// Get all labels currently in use
    pub fn get_all_labels(&self) -> Result<Vec<String>> {
        let conn = self.chunks.connect()?;

        let mut stmt = conn
            .prepare_cached("SELECT DISTINCT label FROM graph_labels ORDER BY label")
                .map_err(|e| anyhow::anyhow!("Failed to prepare labels query: {}", e))?;

        let labels = stmt
            .query_map([], |row: &rusqlite::Row| row.get::<_, String>(0))
            .map_err(|e| anyhow::anyhow!("Failed to execute labels query: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to collect labels: {}", e))?;

        Ok(labels)
    }

    /// Get count of entities with a specific label
    pub fn count_entities_by_label(&self, label: &str) -> Result<usize> {
        let conn = self.chunks.connect()?;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT entity_id) FROM graph_labels WHERE label = ?1",
                params![label],
                |row: &rusqlite::Row| row.get(0),
            )
            .map_err(|e| anyhow::anyhow!("Failed to count entities by label: {}", e))?;

        Ok(count as usize)
    }

    /// Get symbols by label with full metadata
    pub fn get_symbols_by_label(&self, label: &str) -> Result<Vec<SymbolQueryResult>> {
        let entity_ids = self.get_entities_by_label(label)?;
        let mut results = Vec::new();

        for entity_id in entity_ids {
            if let Ok(node) = self.symbols.backend.get_node(entity_id) {
                let symbol_node: SymbolNode = serde_json::from_value(node.data)
                    .unwrap_or_else(|_| SymbolNode {
                        name: None,
                        kind: "Unknown".to_string(),
                        kind_normalized: None,
                        byte_start: 0,
                        byte_end: 0,
                        start_line: 0,
                        start_col: 0,
                        end_line: 0,
                        end_col: 0,
                    });

                results.push(SymbolQueryResult {
                    entity_id,
                    name: symbol_node.name.unwrap_or_else(|| "<unnamed>".to_string()),
                    file_path: node.file_path.unwrap_or_else(|| "?".to_string()),
                    kind: symbol_node.kind_normalized.unwrap_or(symbol_node.kind),
                    byte_start: symbol_node.byte_start,
                    byte_end: symbol_node.byte_end,
                });
            }
        }

        Ok(results)
    }

    /// Get symbols by multiple labels (AND semantics) with full metadata
    pub fn get_symbols_by_labels(&self, labels: &[&str]) -> Result<Vec<SymbolQueryResult>> {
        let entity_ids = self.get_entities_by_labels(labels)?;
        let mut results = Vec::new();

        for entity_id in entity_ids {
            if let Ok(node) = self.symbols.backend.get_node(entity_id) {
                let symbol_node: SymbolNode = serde_json::from_value(node.data)
                    .unwrap_or_else(|_| SymbolNode {
                        name: None,
                        kind: "Unknown".to_string(),
                        kind_normalized: None,
                        byte_start: 0,
                        byte_end: 0,
                        start_line: 0,
                        start_col: 0,
                        end_line: 0,
                        end_col: 0,
                    });

                results.push(SymbolQueryResult {
                    entity_id,
                    name: symbol_node.name.unwrap_or_else(|| "<unnamed>".to_string()),
                    file_path: node.file_path.unwrap_or_else(|| "?".to_string()),
                    kind: symbol_node.kind_normalized.unwrap_or(symbol_node.kind),
                    byte_start: symbol_node.byte_start,
                    byte_end: symbol_node.byte_end,
                });
            }
        }

        Ok(results)
    }
}

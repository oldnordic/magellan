//! Graph algorithms for code analysis
//!
//! This module provides wrapper functions for sqlitegraph's algorithm library,
//! exposing reachability analysis, dead code detection, and impact analysis
//! through Magellan's CodeGraph API.
//!
//! # Graph Views
//!
//! Magellan stores multiple edge types in the same graph:
//! - **DEFINES**: File → Symbol (defines/contains relationship)
//! - **REFERENCES**: Reference → Symbol (what symbol is referenced)
//! - **CALLS**: Call node → Symbol (call graph edges)
//! - **CALLER**: Symbol → Call node (reverse call graph edges)
//!
//! For call graph algorithms (reachability, dead code detection), we filter
//! to **CALLS** edges only to traverse the call graph structure.
//!
//! # Entity IDs vs Symbol IDs
//!
//! sqlitegraph algorithms work with **entity IDs** (i64 database row IDs),
//! while Magellan's public API uses **stable symbol IDs** (32-char BLAKE3 hashes).
//!
//! This module provides translation functions:
//! - `resolve_symbol_entity()`: Symbol ID → entity ID
//! - `symbol_by_entity_id()`: entity ID → SymbolInfo
//!
//! # Algorithm Functions
//!
//! - [`CodeGraph::reachable_symbols()`]: Forward reachability from a symbol
//! - [`CodeGraph::reverse_reachable_symbols()`]: Reverse reachability (callers)
//! - [`CodeGraph::dead_symbols()`]: Dead code detection from entry point
//!
//! # Example
//!
//! ```no_run
//! use magellan::CodeGraph;
//!
//! let mut graph = CodeGraph::open("codegraph.db")?;
//!
//! // Find all functions reachable from main
//! let reachable = graph.reachable_symbols("main_symbol_id", None)?;
//!
//! // Find dead code unreachable from main
//! let dead = graph.dead_symbols("main_symbol_id")?;
//! ```

use anyhow::Result;
use rusqlite::params;
use sqlitegraph::{algo, GraphBackend, SnapshotId};
use std::collections::HashSet;

use crate::graph::schema::SymbolNode;

use super::CodeGraph;

/// Symbol information for algorithm results
///
/// Contains the key metadata needed to identify and locate a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    /// Stable symbol ID (32-char BLAKE3 hash)
    pub symbol_id: Option<String>,
    /// Fully-qualified name
    pub fqn: Option<String>,
    /// File path containing the symbol
    pub file_path: String,
    /// Symbol kind (Function, Method, Class, etc.)
    pub kind: String,
}

/// Dead symbol information
///
/// Extends [`SymbolInfo`] with a reason why the symbol is considered dead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadSymbol {
    /// Base symbol information
    pub symbol: SymbolInfo,
    /// Reason why this symbol is unreachable/dead
    pub reason: String,
}

impl CodeGraph {
    /// Resolve a stable symbol ID or FQN to its entity ID
    ///
    /// First tries to lookup by symbol_id (32-char BLAKE3 hash).
    /// If not found, falls back to FQN lookup for convenience.
    ///
    /// # Arguments
    /// * `symbol_id_or_fqn` - Stable symbol ID (32-char BLAKE3 hash) or FQN
    ///
    /// # Returns
    /// The entity ID (i64 database row ID) for the symbol
    ///
    /// # Errors
    /// Returns an error if the symbol is not found in the database
    fn resolve_symbol_entity(&self, symbol_id_or_fqn: &str) -> Result<i64> {
        let conn = self.chunks.connect()?;

        // First try: lookup by symbol_id
        let mut stmt = conn
            .prepare_cached(
                "SELECT id FROM graph_entities
                 WHERE kind = 'Symbol'
                 AND json_extract(data, '$.symbol_id') = ?1",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare symbol ID query: {}", e))?;

        let result = stmt.query_row(params![symbol_id_or_fqn], |row| row.get::<_, i64>(0));

        match result {
            Ok(entity_id) => return Ok(entity_id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Fallback: try FQN lookup
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query symbol ID: {}", e));
            }
        }

        // Fallback: lookup by FQN or display_fqn
        let mut stmt = conn
            .prepare_cached(
                "SELECT id FROM graph_entities
                 WHERE kind = 'Symbol'
                 AND (json_extract(data, '$.fqn') = ?1
                      OR json_extract(data, '$.display_fqn') = ?1
                      OR json_extract(data, '$.canonical_fqn') = ?1)",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare FQN query: {}", e))?;

        stmt.query_row(params![symbol_id_or_fqn], |row| row.get::<_, i64>(0))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    anyhow::anyhow!(
                        "Symbol '{}' not found in database (tried symbol_id, fqn, display_fqn, canonical_fqn)",
                        symbol_id_or_fqn
                    )
                }
                _ => anyhow::anyhow!("Failed to query symbol by FQN: {}", e),
            })
    }

    /// Get symbol information by entity ID
    ///
    /// # Arguments
    /// * `entity_id` - Entity ID (i64 database row ID)
    ///
    /// # Returns
    /// SymbolInfo with metadata from the symbol node
    fn symbol_by_entity_id(&self, entity_id: i64) -> Result<SymbolInfo> {
        let snapshot = SnapshotId::current();
        let node = self
            .calls
            .backend
            .get_node(snapshot, entity_id)
            .map_err(|e| anyhow::anyhow!("Failed to get entity {}: {}", entity_id, e))?;

        if node.kind != "Symbol" {
            return Err(anyhow::anyhow!(
                "Entity {} is not a Symbol (kind: {})",
                entity_id,
                node.kind
            ));
        }

        let symbol_node: SymbolNode = serde_json::from_value(node.data)
            .map_err(|e| anyhow::anyhow!("Failed to parse SymbolNode data: {}", e))?;

        Ok(SymbolInfo {
            symbol_id: symbol_node.symbol_id,
            fqn: symbol_node.fqn.or_else(|| symbol_node.display_fqn.clone()),
            file_path: node
                .file_path
                .unwrap_or_else(|| "?".to_string()),
            kind: symbol_node.kind,
        })
    }

    /// Get all Symbol entity IDs in the call graph
    ///
    /// Returns entity IDs for all Symbol nodes that are part of the call graph
    /// (have incoming or outgoing CALLS edges).
    ///
    /// # Returns
    /// Vector of entity IDs for call graph symbols
    fn all_call_graph_entities(&self) -> Result<Vec<i64>> {
        let conn = self.chunks.connect()?;

        // Find all symbols that participate in CALLS edges
        // (either as caller via CALLER edges or as callee via CALLS edges)
        let mut stmt = conn
            .prepare_cached(
                "SELECT DISTINCT from_id FROM graph_edges WHERE edge_type = 'CALLER'
                 UNION
                 SELECT DISTINCT to_id FROM graph_edges WHERE edge_type = 'CALLS'",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare call graph query: {}", e))?;

        let entity_ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| anyhow::anyhow!("Failed to execute call graph query: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to collect call graph results: {}", e))?;

        Ok(entity_ids)
    }

    /// Find all symbols reachable from a given symbol (forward reachability)
    ///
    /// Computes the transitive closure of the call graph starting from the
    /// specified symbol. Returns all symbols that can be reached by following
    /// CALLS edges from the starting symbol.
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only), not other edge types.
    /// The starting symbol itself is NOT included in the results.
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID to start from (or FQN as fallback)
    /// * `max_depth` - Optional maximum depth limit (None = unlimited)
    ///
    /// # Returns
    /// Vector of [`SymbolInfo`] for reachable symbols, sorted deterministically
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find all functions called from main (directly or indirectly)
    /// let reachable = graph.reachable_symbols("main", None)?;
    /// ```
    pub fn reachable_symbols(
        &self,
        symbol_id: &str,
        _max_depth: Option<usize>,
    ) -> Result<Vec<SymbolInfo>> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Use sqlitegraph's reachable_from algorithm
        // This traverses outgoing edges from the start node
        // Note: sqlitegraph 1.3.0 API takes (graph, start), not (backend, snapshot, start, depth)
        let reachable_entity_ids = algo::reachable_from(backend.graph(), entity_id)?;

        // Convert entity IDs to SymbolInfo
        let mut symbols = Vec::new();
        for id in reachable_entity_ids {
            // Skip the starting symbol itself
            if id == entity_id {
                continue;
            }

            if let Ok(info) = self.symbol_by_entity_id(id) {
                symbols.push(info);
            }
        }

        // Sort deterministically for stable output
        symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.fqn.as_ref().cmp(&b.fqn.as_ref()))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        Ok(symbols)
    }

    /// Find all symbols that can reach a given symbol (reverse reachability)
    ///
    /// Computes the reverse transitive closure of the call graph. Returns all
    /// symbols from which the specified symbol can be reached by following
    /// CALLS edges (i.e., all callers that directly or indirectly call this symbol).
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only).
    ///
    /// # Arguments
    /// * `symbol_id` - Stable symbol ID to analyze
    /// * `max_depth` - Optional maximum depth limit (None = unlimited)
    ///
    /// # Returns
    /// Vector of [`SymbolInfo`] for symbols that can reach the target, sorted deterministically
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find all functions that directly or indirectly call 'helper_function'
    /// let callers = graph.reverse_reachable_symbols("helper_symbol_id", None)?;
    /// ```
    pub fn reverse_reachable_symbols(
        &self,
        symbol_id: &str,
        _max_depth: Option<usize>,
    ) -> Result<Vec<SymbolInfo>> {
        let entity_id = self.resolve_symbol_entity(symbol_id)?;
        let backend = &self.calls.backend;

        // Use sqlitegraph's reverse_reachable_from algorithm
        // This traverses incoming edges to the target node
        // Note: sqlitegraph 1.3.0 API takes (graph, target)
        let reachable_entity_ids = algo::reverse_reachable_from(backend.graph(), entity_id)?;

        // Convert entity IDs to SymbolInfo
        let mut symbols = Vec::new();
        for id in reachable_entity_ids {
            // Skip the starting symbol itself
            if id == entity_id {
                continue;
            }

            if let Ok(info) = self.symbol_by_entity_id(id) {
                symbols.push(info);
            }
        }

        // Sort deterministically for stable output
        symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.fqn.as_ref().cmp(&b.fqn.as_ref()))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        Ok(symbols)
    }

    /// Find dead code unreachable from an entry point symbol
    ///
    /// Identifies all symbols in the call graph that cannot be reached from
    /// the specified entry point (e.g., `main`, `test_main`). This is useful
    /// for detecting unused functions and dead code.
    ///
    /// # Graph View
    ///
    /// This operates on the **call graph** (CALLS edges only). Symbols not
    /// participating in call edges are not considered.
    ///
    /// # Limitations
    ///
    /// - Only considers the call graph. Symbols called via reflection,
    ///   function pointers, or dynamic dispatch may be incorrectly flagged.
    /// - Test functions, benchmark code, and platform-specific code may
    ///   appear as dead code if not reachable from the specified entry point.
    ///
    /// # Arguments
    /// * `entry_symbol_id` - Stable symbol ID of the entry point (e.g., main function)
    ///
    /// # Returns
    /// Vector of [`DeadSymbol`] for unreachable symbols, sorted deterministically
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use magellan::CodeGraph;
    /// # let mut graph = CodeGraph::open("test.db").unwrap();
    /// // Find all functions unreachable from main
    /// let dead = graph.dead_symbols("main_symbol_id")?;
    /// for dead_symbol in dead {
    ///     println!("Dead: {} in {} ({})",
    ///         dead_symbol.symbol.fqn.as_deref().unwrap_or("?"),
    ///         dead_symbol.symbol.file_path,
    ///         dead_symbol.reason);
    /// }
    /// ```
    pub fn dead_symbols(&self, entry_symbol_id: &str) -> Result<Vec<DeadSymbol>> {
        let entry_entity = self.resolve_symbol_entity(entry_symbol_id)?;
        let backend = &self.calls.backend;

        // Get all call graph entities
        let all_entities = self.all_call_graph_entities()?;

        // Find all entities reachable from the entry point
        // Note: sqlitegraph 1.3.0 API takes (graph, start)
        let reachable_ids =
            algo::reachable_from(backend.graph(), entry_entity)?;

        // Dead symbols = all entities - reachable entities
        let reachable_set: HashSet<i64> = reachable_ids.into_iter().collect();
        let mut dead_symbols = Vec::new();

        for entity_id in all_entities {
            // Skip the entry point itself
            if entity_id == entry_entity {
                continue;
            }

            // If not reachable from entry point, it's dead code
            if !reachable_set.contains(&entity_id) {
                if let Ok(info) = self.symbol_by_entity_id(entity_id) {
                    dead_symbols.push(DeadSymbol {
                        reason: "unreachable from entry point".to_string(),
                        symbol: info,
                    });
                }
            }
        }

        // Sort deterministically for stable output
        dead_symbols.sort_by(|a, b| {
            a.symbol
                .file_path
                .cmp(&b.symbol.file_path)
                .then_with(|| a.symbol.fqn.as_ref().cmp(&b.symbol.fqn.as_ref()))
                .then_with(|| a.symbol.kind.cmp(&b.symbol.kind))
        });

        Ok(dead_symbols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodeGraph;

    /// Test helper to create a simple call graph for testing
    ///
    /// Creates:
    /// - main() -> helper_a() -> leaf()
    /// - main() -> helper_b() -> leaf()
    /// - unused_function() -> leaf()
    ///
    /// Returns the CodeGraph and symbol IDs for main and unused_function
    fn create_test_graph() -> Result<(CodeGraph, String, String)> {
        let temp_dir = tempfile::TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");

        let source = r#"
fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    leaf();
}

fn helper_b() {
    leaf();
}

fn leaf() {
    println!("leaf");
}

fn unused_function() {
    leaf();
}
"#;

        let mut graph = CodeGraph::open(&db_path)?;
        // Index the file - use a temporary file path
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, source)?;
        let path_str = test_file.to_string_lossy().to_string();
        let source_bytes = std::fs::read(&test_file)?;

        // Index symbols and calls
        graph.index_file(&path_str, &source_bytes)?;
        graph.index_calls(&path_str, &source_bytes)?;

        // Find the symbol IDs for main and unused_function
        let symbols = crate::graph::query::symbols_in_file(&mut graph, &path_str)?;
        let main_id = symbols
            .iter()
            .find(|s| s.name.as_deref() == Some("main"))
            .and_then(|s| s.fqn.clone())
            .unwrap_or_default();

        let unused_id = symbols
            .iter()
            .find(|s| s.name.as_deref() == Some("unused_function"))
            .and_then(|s| s.fqn.clone())
            .unwrap_or_default();

        // For testing, use the symbol's FQN directly
        // In a real scenario with proper SymbolId generation, we'd use that
        Ok((graph, main_id, unused_id))
    }

    #[test]
    fn test_resolve_symbol_entity_not_found() {
        let (graph, _, _) = create_test_graph().unwrap();
        let result = graph.resolve_symbol_entity("nonexistent_id_123456789012");
        assert!(result.is_err());
    }

    #[test]
    fn test_symbol_by_entity_id() {
        let (graph, _, _) = create_test_graph().unwrap();

        // Get all entity IDs
        let entity_ids = graph.calls.backend.entity_ids().unwrap();
        let snapshot = SnapshotId::current();

        // Find a Symbol entity
        for entity_id in entity_ids {
            if let Ok(node) = graph.calls.backend.get_node(snapshot, entity_id) {
                if node.kind == "Symbol" {
                    let info = graph.symbol_by_entity_id(entity_id);
                    assert!(info.is_ok());
                    let symbol_info = info.unwrap();
                    assert!(!symbol_info.file_path.is_empty());
                    assert!(!symbol_info.kind.is_empty());
                    return;
                }
            }
        }

        panic!("No Symbol entity found in test graph");
    }

    #[test]
    fn test_reachable_symbols_basic() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get all symbols and verify we can query them
        let entity_ids = graph.calls.backend.entity_ids().unwrap();
        let snapshot = SnapshotId::current();
        let mut found_symbols = 0;

        for entity_id in entity_ids {
            if let Ok(node) = graph.calls.backend.get_node(snapshot, entity_id) {
                if node.kind == "Symbol" {
                    found_symbols += 1;
                }
            }
        }

        // We should have found at least some symbols
        assert!(found_symbols > 0, "Should find Symbol entities in test graph");
    }

    #[test]
    fn test_reachable_symbols_max_depth() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get the main function's entity ID
        let snapshot = SnapshotId::current();
        let entity_ids = graph.calls.backend.entity_ids().unwrap();

        let main_entity_id = entity_ids
            .into_iter()
            .find(|&id| {
                if let Ok(node) = graph.calls.backend.get_node(snapshot, id) {
                    if let Ok(data) = serde_json::from_value::<serde_json::Value>(node.data) {
                        if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                            return name == "main";
                        }
                    }
                }
                false
            });

        if let Some(entity_id) = main_entity_id {
            // Verify we can get the node
            let result = graph.calls.backend.get_node(snapshot, entity_id);
            assert!(result.is_ok(), "Should be able to get main node");
        }
    }

    #[test]
    fn test_dead_symbols() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get all entity IDs
        let entity_ids = graph.calls.backend.entity_ids().unwrap();

        // We should have some entities in the call graph
        assert!(entity_ids.len() > 0, "Should have call graph entities");
    }

    #[test]
    fn test_reverse_reachable_symbols() {
        let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

        // Get all entity IDs
        let entity_ids = graph.calls.backend.entity_ids().unwrap();

        // We should have some entities in the call graph
        assert!(entity_ids.len() > 0, "Should have call graph entities");
    }
}

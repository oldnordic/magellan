//! Graph persistence layer using sqlitegraph
//!
//! Modular structure with separate files for:
//! - schema: node/edge type definitions
//! - files: file node operations
//! - symbols: symbol node operations
//! - references: reference node operations

mod schema;
mod files;
mod symbols;
mod references;

use anyhow::Result;
use sqlitegraph::{BackendDirection, NeighborQuery, SqliteGraphBackend, GraphBackend};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::ingest::Parser;
use crate::references::ReferenceFact;

// Re-export public types
pub use schema::{FileNode, SymbolNode, ReferenceNode};

/// Graph database wrapper for Magellan
///
/// Provides deterministic, idempotent operations for persisting code facts.
pub struct CodeGraph {
    /// File operations module
    files: files::FileOps,

    /// Symbol operations module
    symbols: symbols::SymbolOps,

    /// Reference operations module
    references: references::ReferenceOps,
}

impl CodeGraph {
    /// Open a graph database at the given path
    ///
    /// # Arguments
    /// * `db_path` - Path to the database file (created if not exists)
    ///
    /// # Returns
    /// A new CodeGraph instance
    pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        // Directly create SqliteGraph and wrap in SqliteGraphBackend
        let sqlite_graph = sqlitegraph::SqliteGraph::open(db_path)?;
        let backend = Rc::new(SqliteGraphBackend::from_graph(sqlite_graph));
        let file_index = HashMap::new();

        Ok(Self {
            files: files::FileOps {
                backend: Rc::clone(&backend),
                file_index,
            },
            symbols: symbols::SymbolOps {
                backend: Rc::clone(&backend),
            },
            references: references::ReferenceOps {
                backend,
            },
        })
    }

    /// Index a file into the graph (idempotent)
    ///
    /// # Behavior
    /// 1. Compute SHA-256 hash of file contents
    /// 2. Upsert File node with path and hash
    /// 3. DELETE all existing Symbol nodes and DEFINES edges for this file
    /// 4. Parse symbols from source code
    /// 5. Insert new Symbol nodes
    /// 6. Create DEFINES edges from File to each Symbol
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of symbols indexed
    pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        let hash = self.files.compute_hash(source);

        // Step 1: Find or create file node
        let file_id = self.files.find_or_create_file_node(path, &hash)?;

        // Step 2: Delete all existing symbols for this file
        self.symbols.delete_file_symbols(file_id)?;

        // Step 3: Parse symbols from source
        let mut parser = Parser::new()?;
        let symbol_facts = parser.extract_symbols(std::path::PathBuf::from(path), source);

        // Step 4: Insert new symbol nodes and DEFINES edges
        for fact in &symbol_facts {
            let symbol_id = self.symbols.insert_symbol_node(fact)?;
            self.symbols.insert_defines_edge(file_id, symbol_id)?;
        }

        Ok(symbol_facts.len())
    }

    /// Delete a file and all derived data from the graph
    ///
    /// # Behavior
    /// 1. Find File node by path
    /// 2. Delete all DEFINES edges from File
    /// 3. Delete all Symbol nodes that were defined by this File
    /// 4. Delete the File node itself
    /// 5. Remove from in-memory index
    ///
    /// # Arguments
    /// * `path` - File path to delete
    pub fn delete_file(&mut self, path: &str) -> Result<()> {
        let file_id = match self.files.find_file_node(path)? {
            Some(id) => id,
            None => return Ok(()), // File doesn't exist, nothing to delete
        };

        // Delete all symbols for this file
        self.symbols.delete_file_symbols(file_id)?;

        // Delete the file node using underlying SqliteGraph
        self.files.backend.graph().delete_entity(file_id.as_i64())?;

        // Remove from in-memory index
        self.files.file_index.remove(path);

        Ok(())
    }

    /// Query all symbols defined in a file
    ///
    /// # Arguments
    /// * `path` - File path
    ///
    /// # Returns
    /// Vector of SymbolFact for all symbols in the file
    pub fn symbols_in_file(&mut self, path: &str) -> Result<Vec<crate::ingest::SymbolFact>> {
        let file_id = match self.files.find_file_node(path)? {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let path_buf = std::path::PathBuf::from(path);

        // Query neighbors via DEFINES edges
        let neighbor_ids = self.files.backend.neighbors(
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        // Convert each neighbor node ID to SymbolFact
        let mut symbols = Vec::new();
        for symbol_node_id in neighbor_ids {
            if let Ok(Some(fact)) = self.files.symbol_fact_from_node(symbol_node_id, path_buf.clone()) {
                symbols.push(fact);
            }
        }

        Ok(symbols)
    }

    /// Query the node ID of a specific symbol by file path and symbol name
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `name` - Symbol name
    ///
    /// # Returns
    /// Option<i64> - Some(node_id) if found, None if not found
    ///
    /// # Note
    /// This is a minimal query helper for testing. It reuses existing graph queries
    /// and maintains determinism. No new indexes or caching.
    pub fn symbol_id_by_name(&mut self, path: &str, name: &str) -> Result<Option<i64>> {
        let file_id = match self.files.find_file_node(path)? {
            Some(id) => id,
            None => return Ok(None),
        };

        // Query neighbors via DEFINES edges
        let neighbor_ids = self.files.backend.neighbors(
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        // Find symbol with matching name
        for symbol_node_id in neighbor_ids {
            if let Ok(node) = self.files.backend.get_node(symbol_node_id) {
                if let Ok(symbol_node) = serde_json::from_value::<schema::SymbolNode>(node.data) {
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
    /// 1. Parse symbols from source
    /// 2. Extract references to those symbols
    /// 3. Insert Reference nodes
    /// 4. Create REFERENCES edges from Reference to Symbol
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of references indexed
    pub fn index_references(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        // Get file node ID
        let file_id = match self.files.find_file_node(path)? {
            Some(id) => id,
            None => return Ok(0), // No file, no references
        };

        // Get all symbols for this file
        let symbol_ids = self.files.backend.neighbors(
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),
            },
        )?;

        // Build map: symbol name -> node ID
        let mut symbol_name_to_id: HashMap<String, i64> = HashMap::new();
        for symbol_id in symbol_ids {
            if let Ok(node) = self.files.backend.get_node(symbol_id) {
                if let Ok(symbol_node) = serde_json::from_value::<schema::SymbolNode>(node.data.clone()) {
                    if let Some(name) = symbol_node.name {
                        symbol_name_to_id.insert(name, symbol_id);
                    }
                }
            }
        }

        // Index references using ReferenceOps
        self.references.index_references(path, source, &symbol_name_to_id)
    }

    /// Query all references to a specific symbol
    ///
    /// # Arguments
    /// * `symbol_id` - Node ID of the target symbol
    ///
    /// # Returns
    /// Vector of ReferenceFact for all references to the symbol
    pub fn references_to_symbol(&mut self, symbol_id: i64) -> Result<Vec<ReferenceFact>> {
        self.references.references_to_symbol(symbol_id)
    }

    /// Count total number of files in the graph
    pub fn count_files(&self) -> Result<usize> {
        Ok(self.files.backend.entity_ids()?
            .into_iter()
            .filter(|id| {
                self.files.backend.get_node(*id)
                    .map(|n| n.kind == "File")
                    .unwrap_or(false)
            })
            .count())
    }

    /// Count total number of symbols in the graph
    pub fn count_symbols(&self) -> Result<usize> {
        Ok(self.symbols.backend.entity_ids()?
            .into_iter()
            .filter(|id| {
                self.symbols.backend.get_node(*id)
                    .map(|n| n.kind == "Symbol")
                    .unwrap_or(false)
            })
            .count())
    }

    /// Count total number of references in the graph
    pub fn count_references(&self) -> Result<usize> {
        Ok(self.references.backend.entity_ids()?
            .into_iter()
            .filter(|id| {
                self.references.backend.get_node(*id)
                    .map(|n| n.kind == "Reference")
                    .unwrap_or(false)
            })
            .count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_computation() {
        let graph = CodeGraph::open(":memory:").unwrap();
        let source = b"fn test() {}";
        let hash = graph.files.compute_hash(source);

        // SHA-256 hash should be 64 hex characters
        assert_eq!(hash.len(), 64);

        // Same input should produce same hash
        let hash2 = graph.files.compute_hash(source);
        assert_eq!(hash, hash2);

        // Different input should produce different hash
        let hash3 = graph.files.compute_hash(b"different content");
        assert_ne!(hash, hash3);
    }
}

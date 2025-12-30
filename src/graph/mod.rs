//! Graph persistence layer using sqlitegraph
mod schema; mod files; mod symbols; mod references;
mod call_ops; mod calls; mod count; mod ops; mod scan; mod query; mod export;
mod freshness;
#[cfg(test)] mod tests;

use anyhow::Result;
use sqlitegraph::SqliteGraphBackend;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::references::{ReferenceFact, CallFact};

// Re-export public types
pub use schema::{FileNode, SymbolNode, ReferenceNode, CallNode};
pub use freshness::{FreshnessStatus, check_freshness, STALE_THRESHOLD_SECS};

/// Progress callback for scan_directory
///
/// Receives (current_count, total_count) as scanning progresses
pub type ScanProgress = dyn Fn(usize, usize) + Send + Sync;

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

    /// Call operations module
    calls: call_ops::CallOps,
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

        // Build initial file_index from database (lazy initialization)
        let file_index = HashMap::new();
        let mut files = files::FileOps {
            backend: Rc::clone(&backend),
            file_index,
        };

        // Populate file_index with existing File nodes from database
        files.rebuild_file_index()?;

        Ok(Self {
            files,
            symbols: symbols::SymbolOps {
                backend: Rc::clone(&backend),
            },
            references: references::ReferenceOps {
                backend: Rc::clone(&backend),
            },
            calls: call_ops::CallOps {
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
    /// 7. Index calls (CALLS edges)
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of symbols indexed
    pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        ops::index_file(self, path, source)
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
        ops::delete_file(self, path)
    }

    /// Query all symbols defined in a file
    ///
    /// # Arguments
    /// * `path` - File path
    ///
    /// # Returns
    /// Vector of SymbolFact for all symbols in the file
    pub fn symbols_in_file(&mut self, path: &str) -> Result<Vec<crate::ingest::SymbolFact>> {
        query::symbols_in_file(self, path)
    }

    /// Query symbols defined in a file, optionally filtered by kind
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `kind` - Optional symbol kind filter (None returns all symbols)
    ///
    /// # Returns
    /// Vector of SymbolFact matching the kind filter
    pub fn symbols_in_file_with_kind(
        &mut self,
        path: &str,
        kind: Option<crate::ingest::SymbolKind>,
    ) -> Result<Vec<crate::ingest::SymbolFact>> {
        query::symbols_in_file_with_kind(self, path, kind)
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
        query::symbol_id_by_name(self, path, name)
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
        query::index_references(self, path, source)
    }

    /// Query all references to a specific symbol
    ///
    /// # Arguments
    /// * `symbol_id` - Node ID of the target symbol
    ///
    /// # Returns
    /// Vector of ReferenceFact for all references to the symbol
    pub fn references_to_symbol(&mut self, symbol_id: i64) -> Result<Vec<ReferenceFact>> {
        query::references_to_symbol(self, symbol_id)
    }

    /// Index calls for a file into the graph
    ///
    /// # Behavior
    /// 1. Get file node ID
    /// 2. Get all symbols for this file
    /// 3. Extract calls from source
    /// 4. Insert Call nodes and CALLS edges
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of calls indexed
    pub fn index_calls(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        calls::index_calls(self, path, source)
    }

    /// Query all calls FROM a specific symbol (forward call graph)
    ///
    /// # Arguments
    /// * `path` - File path containing the symbol
    /// * `name` - Symbol name
    ///
    /// # Returns
    /// Vector of CallFact for all calls from this symbol
    pub fn calls_from_symbol(&mut self, path: &str, name: &str) -> Result<Vec<CallFact>> {
        calls::calls_from_symbol(self, path, name)
    }

    /// Query all calls TO a specific symbol (reverse call graph)
    ///
    /// # Arguments
    /// * `path` - File path containing the symbol
    /// * `name` - Symbol name
    ///
    /// # Returns
    /// Vector of CallFact for all calls to this symbol
    pub fn callers_of_symbol(&mut self, path: &str, name: &str) -> Result<Vec<CallFact>> {
        calls::callers_of_symbol(self, path, name)
    }

    /// Count total number of files in the graph
    pub fn count_files(&self) -> Result<usize> {
        count::count_files(self)
    }

    /// Count total number of symbols in the graph
    pub fn count_symbols(&self) -> Result<usize> {
        count::count_symbols(self)
    }

    /// Count total number of references in the graph
    pub fn count_references(&self) -> Result<usize> {
        count::count_references(self)
    }

    /// Scan a directory and index all Rust files found
    ///
    /// # Behavior
    /// 1. Walk directory recursively
    /// 2. Find all .rs files
    /// 3. Index each file (symbols + references)
    /// 4. Report progress via callback
    ///
    /// # Arguments
    /// * `dir_path` - Directory to scan
    /// * `progress` - Optional callback for progress reporting (current, total)
    ///
    /// # Returns
    /// Number of files indexed
    ///
    /// # Guarantees
    /// - Only .rs files are processed
    /// - Files are indexed in sorted order for determinism
    /// - Non-.rs files are silently skipped
    pub fn scan_directory(&mut self, dir_path: &Path, progress: Option<&ScanProgress>) -> Result<usize> {
        scan::scan_directory(self, dir_path, progress)
    }

    /// Export all graph data to JSON format
    ///
    /// # Returns
    /// JSON string containing all files, symbols, references, and calls
    pub fn export_json(&mut self) -> Result<String> {
        export::export_json(self)
    }

    /// Get the FileNode for a given file path
    ///
    /// # Arguments
    /// * `path` - File path to query
    ///
    /// # Returns
    /// Option<FileNode> with file metadata including timestamps, or None if not found
    pub fn get_file_node(&mut self, path: &str) -> Result<Option<FileNode>> {
        self.files.get_file_node(path)
    }

    /// Get all FileNodes from the database
    ///
    /// # Returns
    /// HashMap of file path -> FileNode for all files in the database
    pub fn all_file_nodes(&mut self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.files.all_file_nodes()
    }
}

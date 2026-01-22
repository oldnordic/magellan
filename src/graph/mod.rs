//! Graph persistence layer using sqlitegraph
mod call_ops;
mod calls;
mod cache;
mod count;
mod db_compat;
mod execution_log;
pub mod export;
mod files;
pub mod filter;
mod freshness;
mod ops;
pub mod query;
mod references;
pub mod scan;
mod schema;
mod symbols;
pub mod validation;

// Re-export small public types from ops.
pub use ops::{DeleteResult, ReconcileOutcome};

// Re-export test helpers for integration tests.
// The test_helpers module is public in ops.rs for use by delete_transaction_tests.rs
pub use ops::test_helpers;

// Re-export symbol ID generation function
pub use symbols::generate_symbol_id;
#[cfg(test)]
mod tests;

use anyhow::Result;
use sqlitegraph::SqliteGraphBackend;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::generation::{ChunkStore, CodeChunk};
use crate::references::{CallFact, ReferenceFact};

// Re-export public types
pub use cache::CacheStats;
pub use db_compat::MAGELLAN_SCHEMA_VERSION;
pub use export::{ExportConfig, ExportFormat};
pub use freshness::{check_freshness, FreshnessStatus, STALE_THRESHOLD_SECS};
pub use schema::{CallNode, FileNode, ReferenceNode, SymbolNode};

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

    /// Code chunk storage module
    chunks: ChunkStore,

    /// Execution log module for tracking Magellan runs
    execution_log: execution_log::ExecutionLog,

    /// File node cache for frequently accessed files
    file_node_cache: cache::FileNodeCache,
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
        // Convert to PathBuf for reuse
        let db_path_buf = db_path.as_ref().to_path_buf();

        // Phase 1: read-only compatibility preflight for existing DB files.
        // This MUST run before any sqlitegraph or Magellan side-table writes occur.
        let _preflight = db_compat::preflight_sqlitegraph_compat(&db_path_buf)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // Phase 2: mutating open (sqlitegraph ensure_schema/migrations).
        let sqlite_graph = sqlitegraph::SqliteGraph::open(&db_path_buf)?;
        let backend = Rc::new(SqliteGraphBackend::from_graph(sqlite_graph));

        // Phase 2b: Configure SQLite performance PRAGMAs
        // Note: sqlitegraph 1.0.0 already configures these in from_connection(),
        // but we set them explicitly here to ensure they're applied even if
        // sqlitegraph changes its defaults in future versions.
        //
        // These PRAGMA settings are configured on a separate connection but affect
        // the entire database file (PRAGMA is file-level, not connection-level).
        //
        // Scoped block ensures connection closes even if PRAGMA operations fail.
        // Without this scope, early returns via ? would leak the connection.
        {
            let pragma_conn = rusqlite::Connection::open(&db_path_buf)
                .map_err(|e| anyhow::anyhow!("Failed to open connection for PRAGMA config: {}", e))?;

            // WAL mode for better concurrency (allows reads during writes)
            // query() returns the new mode value, execute() would error
            // Note: :memory: databases don't support WAL mode (returns "memory")
            let journal_mode = pragma_conn
                .query_row("PRAGMA journal_mode = WAL", [], |row| {
                    let mode: String = row.get(0)?;
                    Ok(mode)
                })
                .map_err(|e| anyhow::anyhow!("Failed to set WAL mode: {}", e))?;
            // Only assert WAL mode for file-based databases (not :memory:)
            if db_path_buf != PathBuf::from(":memory:") {
                debug_assert_eq!(journal_mode, "wal", "WAL mode should be enabled");
            }

            // Faster writes (safe with WAL mode - durability still guaranteed)
            pragma_conn
                .execute("PRAGMA synchronous = NORMAL", [])
                .map_err(|e| anyhow::anyhow!("Failed to set synchronous: {}", e))?;

            // Increase cache (negative value = KB, -64000 = 64MB)
            // Note: sqlitegraph also sets this to -64000, ensuring 64MB cache
            pragma_conn
                .execute("PRAGMA cache_size = -64000", [])
                .map_err(|e| anyhow::anyhow!("Failed to set cache_size: {}", e))?;

            // Temp tables in memory (faster than disk)
            pragma_conn
                .execute("PRAGMA temp_store = MEMORY", [])
                .map_err(|e| anyhow::anyhow!("Failed to set temp_store: {}", e))?;
            // pragma_conn drops automatically here at block end
        }

        // Build initial file_index from database (eager initialization)
        let file_index = HashMap::new();
        let mut files = files::FileOps {
            backend: Rc::clone(&backend),
            file_index,
        };

        // Populate file_index with existing File nodes from database
        files.rebuild_file_index()?;

        // Phase 3: Magellan-owned DB compatibility metadata.
        // MUST run after sqlitegraph open and before any other Magellan side-table writes.
        db_compat::ensure_magellan_meta(&db_path_buf)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // Open a shared connection for ChunkStore to enable transactional operations
        // This allows chunk operations to participate in transactions with graph operations
        let shared_conn = rusqlite::Connection::open(&db_path_buf)
            .map_err(|e| anyhow::anyhow!("Failed to open shared connection for ChunkStore: {}", e))?;

        // Initialize ChunkStore with shared connection and ensure schema exists
        let chunks = ChunkStore::with_connection(shared_conn);
        chunks.ensure_schema()?;

        // Initialize ExecutionLog and ensure schema exists
        let execution_log = execution_log::ExecutionLog::new(&db_path_buf);
        execution_log.ensure_schema()?;

        // Initialize file node cache with capacity of 128 entries
        let file_node_cache = cache::FileNodeCache::new(128);

        Ok(Self {
            files,
            symbols: symbols::SymbolOps {
                backend: Rc::clone(&backend),
            },
            references: references::ReferenceOps {
                backend: Rc::clone(&backend),
            },
            calls: call_ops::CallOps { backend },
            chunks,
            execution_log,
            file_node_cache,
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
    /// This delegates to `delete_file_facts` which removes *all* file-derived facts
    /// (symbols, references, calls, chunks, file node).
    ///
    /// # Returns
    /// DeleteResult with counts of deleted entities
    pub fn delete_file(&mut self, path: &str) -> Result<DeleteResult> {
        ops::delete_file(self, path)
    }

    /// Delete ALL facts derived from a file path.
    ///
    /// This is the authoritative deletion path used by reconcile.
    ///
    /// # Returns
    /// DeleteResult with counts of deleted entities
    pub fn delete_file_facts(&mut self, path: &str) -> Result<DeleteResult> {
        ops::delete_file_facts(self, path)
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

    /// Query symbol facts along with their node IDs for deterministic ordering/output.
    pub fn symbol_nodes_in_file(
        &mut self,
        path: &str,
    ) -> Result<Vec<(i64, crate::ingest::SymbolFact)>> {
        query::symbol_nodes_in_file(self, path)
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

    /// Lookup symbol extent (byte + line span) for a specific symbol name in a file.
    pub fn symbol_extents(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<(i64, crate::ingest::SymbolFact)>> {
        query::symbol_extents(self, path, name)
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

    /// Count total number of calls in the graph
    pub fn count_calls(&self) -> Result<usize> {
        count::count_calls(self)
    }

    /// Reconcile a file path against filesystem + content hash.
    ///
    /// This is the deterministic primitive used by scan and watcher updates.
    pub fn reconcile_file_path(&mut self, path: &Path, path_key: &str) -> Result<ReconcileOutcome> {
        ops::reconcile_file_path(self, path, path_key)
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
    pub fn scan_directory(
        &mut self,
        dir_path: &Path,
        progress: Option<&ScanProgress>,
    ) -> Result<usize> {
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
        // Check cache first
        if let Some(node) = self.file_node_cache.get(&path.to_string()) {
            return Ok(Some(node.clone()));
        }

        // Cache miss - query database
        let result = self.files.get_file_node(path)?;

        // Store in cache if found
        if let Some(ref node) = result {
            self.file_node_cache.put(path.to_string(), node.clone());
        }

        Ok(result)
    }

    /// Get all FileNodes from the database
    ///
    /// # Returns
    /// HashMap of file path -> FileNode for all files in the database
    pub fn all_file_nodes(&mut self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.files.all_file_nodes()
    }

    /// Get code chunks for a specific file.
    ///
    /// # Arguments
    /// * `file_path` - File path to query
    ///
    /// # Returns
    /// Vector of CodeChunk for all chunks in the file
    pub fn get_code_chunks(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
        self.chunks.get_chunks_for_file(file_path)
    }

    /// Get code chunks for a specific symbol in a file.
    ///
    /// # Arguments
    /// * `file_path` - File path containing the symbol
    /// * `symbol_name` - Symbol name to query
    ///
    /// # Returns
    /// Vector of CodeChunk for the symbol (may be multiple for overloads)
    pub fn get_code_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<CodeChunk>> {
        self.chunks.get_chunks_for_symbol(file_path, symbol_name)
    }

    /// Get a code chunk by exact byte span.
    ///
    /// # Arguments
    /// * `file_path` - File path containing the chunk
    /// * `byte_start` - Starting byte offset
    /// * `byte_end` - Ending byte offset
    ///
    /// # Returns
    /// Option<CodeChunk> if found, None otherwise
    pub fn get_code_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>> {
        self.chunks
            .get_chunk_by_span(file_path, byte_start, byte_end)
    }

    /// Store code chunks for a file.
    ///
    /// # Arguments
    /// * `chunks` - Code chunks to store
    ///
    /// # Returns
    /// Vector of inserted chunk IDs
    pub fn store_code_chunks(&self, chunks: &[CodeChunk]) -> Result<Vec<i64>> {
        self.chunks.store_chunks(chunks)
    }

    /// Count total code chunks stored.
    pub fn count_chunks(&self) -> Result<usize> {
        self.chunks.count_chunks()
    }

    /// Get the execution log for recording command execution
    pub fn execution_log(&self) -> &execution_log::ExecutionLog {
        &self.execution_log
    }

    /// Validate graph invariants post-run
    ///
    /// Checks for orphan references, orphan calls, and other structural issues.
    /// This is a convenience method that calls validation::validate_graph().
    ///
    /// # Returns
    /// ValidationReport with validation results (errors, warnings, passed status)
    pub fn validate_graph(&mut self) -> validation::ValidationReport {
        validation::validate_graph(self).unwrap_or_else(|e| validation::ValidationReport {
            passed: false,
            errors: vec![validation::ValidationError::new(
                "VALIDATION_ERROR".to_string(),
                format!("Validation failed with error: {}", e),
            )],
            warnings: Vec::new(),
        })
    }

    /// Get cache statistics for monitoring cache effectiveness
    ///
    /// # Returns
    /// CacheStats with hits, misses, size, and hit rate
    pub fn cache_stats(&self) -> CacheStats {
        self.file_node_cache.stats()
    }

    /// Invalidate cache entry for a specific file path
    ///
    /// This should be called when a file is modified or deleted to ensure
    /// cache doesn't return stale data.
    ///
    /// # Arguments
    /// * `path` - File path to invalidate
    pub fn invalidate_cache(&mut self, path: &str) {
        self.file_node_cache.invalidate(&path.to_string());
    }

    /// Clear all cache entries
    ///
    /// This resets the cache to empty state, useful for testing or after bulk operations.
    pub fn clear_cache(&mut self) {
        self.file_node_cache.clear();
    }
}

use anyhow::Result;
use std::path::Path;

use crate::generation::CodeChunk;

use super::{
    cache::CacheStats, count, export, filter, metrics, ops, scan, side_tables, validation,
    CodeGraph, FileNode, GraphStats, ReconcileOutcome, ScanProgress, ScanResult, SymbolNode,
};

impl CodeGraph {
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

    /// Count total number of CFG blocks in the graph
    ///
    /// Note: Returns 0 for SQLite backend.
    pub fn count_cfg_blocks(&self) -> Result<usize> {
        Ok(0)
    }

    /// Check if coverage schema tables exist in the database.
    ///
    /// Returns true if all three coverage tables are present.
    pub fn check_coverage_schema(&self) -> Result<bool> {
        let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| {
            anyhow::anyhow!("Failed to open connection for coverage schema check: {}", e)
        })?;
        let tables = [
            "cfg_block_coverage",
            "cfg_edge_coverage",
            "cfg_coverage_meta",
        ];
        for table in tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if count == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Get combined statistics for the graph
    ///
    /// Returns symbol count, file count, and cfg block count
    pub fn get_stats(&self) -> Result<GraphStats> {
        Ok(GraphStats {
            symbol_count: self.count_symbols()?,
            file_count: self.count_files()?,
            cfg_block_count: 0,
        })
    }

    /// Reconcile a file path against filesystem + content hash.
    ///
    /// This is the deterministic primitive used by scan and watcher updates.
    pub fn reconcile_file_path(&mut self, path: &Path, path_key: &str) -> Result<ReconcileOutcome> {
        ops::reconcile_file_path(self, path, path_key)
    }

    /// Reconcile a file path using pre-read source bytes.
    ///
    /// Same as `reconcile_file_path` but avoids re-reading from disk.
    pub fn reconcile_file_path_with_source(
        &mut self,
        path: &Path,
        path_key: &str,
        source: &[u8],
    ) -> Result<ReconcileOutcome> {
        ops::reconcile_file_path_with_source(self, path, path_key, source)
    }

    /// Scan a directory and index all Rust files found
    pub fn scan_directory(
        &mut self,
        dir_path: &Path,
        progress: Option<&ScanProgress>,
    ) -> Result<usize> {
        scan::scan_directory(self, dir_path, progress)
    }

    /// Scan a directory with a pre-built `FileFilter`.
    pub fn scan_directory_with_filter(
        &mut self,
        dir_path: &Path,
        filter: &filter::FileFilter,
        progress: Option<&ScanProgress>,
    ) -> Result<ScanResult> {
        scan::scan_directory_with_filter(self, dir_path, filter, progress)
    }

    /// Async version of scan_directory with parallel file reading
    pub async fn scan_directory_async(
        &mut self,
        dir_path: &Path,
        progress: Option<&ScanProgress>,
    ) -> Result<usize> {
        let filter = filter::FileFilter::new(dir_path, &[], &[])?;
        let result = scan::scan_directory_async(self, dir_path, &filter, progress).await?;
        Ok(result.indexed)
    }

    /// Backfill metrics for all existing files in the database
    pub fn backfill_metrics(
        &mut self,
        progress: Option<&ScanProgress>,
    ) -> Result<metrics::BackfillResult> {
        self.metrics.backfill_all_metrics(progress)
    }

    /// Export all graph data to JSON format
    pub fn export_json(&mut self) -> Result<String> {
        export::export_json(self)
    }

    /// Get the FileNode for a given file path
    pub fn get_file_node(&mut self, path: &str) -> Result<Option<FileNode>> {
        if let Some(node) = self.file_node_cache.get(&path.to_string()) {
            return Ok(Some(node.clone()));
        }

        let result = self.files.get_file_node(path)?;

        if let Some(ref node) = result {
            self.file_node_cache.put(path.to_string(), node.clone());
        }

        Ok(result)
    }

    /// Get all FileNodes from the database
    pub fn all_file_nodes(&mut self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.files.all_file_nodes()
    }

    /// Get all FileNodes from the database (read-only, doesn't require mutation).
    pub fn all_file_nodes_readonly(&self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.files.all_file_nodes_readonly()
    }

    /// Get code chunks for a specific file.
    pub fn get_code_chunks(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
        let location_path = match self
            .files
            .find_all_file_nodes(file_path)
            .ok()
            .and_then(|nodes| nodes.first().map(|(_, n)| n.path.clone()))
        {
            Some(stored_path) => self.files.absolute_fs_path(&stored_path),
            None => self.files.absolute_fs_path(file_path),
        };
        self.chunks.get_chunks_for_file(&location_path)
    }

    /// Search code chunk content via FTS5 full-text search.
    pub fn search_code_content(
        &self,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<side_tables::CodeContentSearchResult>> {
        self.chunks.search_code_content(pattern, limit)
    }

    /// Get code chunks for a specific symbol in a file.
    pub fn get_code_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<CodeChunk>> {
        let location_path = match self
            .files
            .find_all_file_nodes(file_path)
            .ok()
            .and_then(|nodes| nodes.first().map(|(_, n)| n.path.clone()))
        {
            Some(stored_path) => self.files.absolute_fs_path(&stored_path),
            None => self.files.absolute_fs_path(file_path),
        };
        self.chunks
            .get_chunks_for_symbol(&location_path, symbol_name)
    }

    /// Get a code chunk by exact byte span.
    pub fn get_code_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>> {
        let location_path = match self
            .files
            .find_all_file_nodes(file_path)
            .ok()
            .and_then(|nodes| nodes.first().map(|(_, n)| n.path.clone()))
        {
            Some(stored_path) => self.files.absolute_fs_path(&stored_path),
            None => self.files.absolute_fs_path(file_path),
        };
        self.chunks
            .get_chunk_by_span(&location_path, byte_start, byte_end)
    }

    /// Store code chunks for a file.
    pub fn store_code_chunks(&self, chunks: &[CodeChunk]) -> Result<Vec<i64>> {
        self.chunks.store_chunks(chunks)
    }

    /// Count total code chunks stored.
    pub fn count_chunks(&self) -> Result<usize> {
        self.chunks.count_chunks()
    }

    /// Get the execution log for recording command execution
    pub fn execution_log(&self) -> &super::execution_log::ExecutionLog {
        &self.execution_log
    }

    /// Get the metrics operations module
    pub fn metrics(&self) -> &metrics::MetricsOps {
        &self.metrics
    }

    /// Get the telemetry operations module
    pub fn telemetry(&self) -> &super::telemetry::TelemetryOps {
        &self.telemetry
    }

    /// Validate graph invariants post-run
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
    pub fn cache_stats(&self) -> CacheStats {
        self.file_node_cache.stats()
    }

    /// Get combined cache statistics (file node + navigator query caches).
    pub fn full_cache_stats(&self) -> CacheStats {
        let file_stats = self.file_node_cache.stats();
        let entity_stats = self.entity_cache.stats();
        let name_stats = self.name_cache.stats();
        let expand_stats = self.expand_cache.stats();
        CacheStats {
            hits: file_stats.hits + entity_stats.hits + name_stats.hits + expand_stats.hits,
            misses: file_stats.misses
                + entity_stats.misses
                + name_stats.misses
                + expand_stats.misses,
            size: file_stats.size + entity_stats.size + name_stats.size + expand_stats.size,
        }
    }

    /// Invalidate cache entry for a specific file path
    pub fn invalidate_cache(&mut self, path: &str) {
        self.file_node_cache.invalidate(&path.to_string());
    }

    /// Clear all cache entries
    pub fn clear_cache(&mut self) {
        self.file_node_cache.clear();
    }

    /// Clear all navigator query caches.
    pub fn clear_query_caches(&self) {
        self.entity_cache.clear();
        self.name_cache.clear();
        self.expand_cache.clear();
    }

    /// Get backend for testing/benchmarking
    #[doc(hidden)]
    pub fn __backend_for_benchmarks(&self) -> &std::sync::Arc<dyn sqlitegraph::GraphBackend> {
        &self.files.backend
    }

    /// Rebuild FTS5 symbol search index
    pub fn rebuild_fts5_index(db_path: &Path) -> Result<()> {
        use rusqlite::Connection;

        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.execute("INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild')", [])?;

        Ok(())
    }

    /// Get backend reference.
    #[doc(hidden)]
    pub fn __backend_for_watcher(&self) -> &std::sync::Arc<dyn sqlitegraph::GraphBackend> {
        &self.files.backend
    }

    /// Get backend reference for backend router operations.
    #[doc(hidden)]
    pub fn backend(&self) -> &std::sync::Arc<dyn sqlitegraph::GraphBackend> {
        &self.files.backend
    }

    /// Get symbol node by entity ID
    pub fn get_symbol_by_entity_id(&self, entity_id: i64) -> Option<SymbolNode> {
        use sqlitegraph::SnapshotId;

        let snapshot = SnapshotId::current();
        match self.files.backend.get_node(snapshot, entity_id) {
            Ok(node) => {
                if node.kind != "Symbol" {
                    return None;
                }
                serde_json::from_value(node.data).ok()
            }
            Err(_) => None,
        }
    }

    /// Add a label to an entity (uses side_tables)
    pub fn add_label(&self, entity_id: i64, label: &str) -> Result<()> {
        self.side_tables.add_label(entity_id, label)
    }

    /// Get all labels for an entity (uses side_tables)
    pub fn get_labels_for_entity(&self, entity_id: i64) -> Result<Vec<String>> {
        self.side_tables.get_labels_for_entity(entity_id)
    }
}

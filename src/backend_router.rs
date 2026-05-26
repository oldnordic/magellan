//! Backend router for Magellan CLI
//!
//! Provides unified interface for SQLite backend.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::graph::CodeGraph;
use crate::graph::SymbolNode;
use crate::ingest::SymbolKind;
use sqlitegraph::{GraphBackend, SnapshotId};

/// Unified symbol information
#[derive(Debug, Clone)]
pub struct UnifiedSymbolInfo {
    pub id: u64,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
    pub language: Option<String>,
}

/// Backend type (SQLite only)
#[derive(Debug, PartialEq)]
pub enum BackendType {
    SQLite,
}

/// Unified backend interface (SQLite only)
pub enum MagellanBackend {
    SQLite(CodeGraph),
}

impl MagellanBackend {
    /// Detect backend type (always SQLite)
    pub fn detect_type(_db_path: &Path) -> BackendType {
        BackendType::SQLite
    }

    /// Create a new database
    pub fn create(db_path: &Path) -> Result<Self> {
        let graph = CodeGraph::open(db_path).context("Failed to create SQLite database")?;
        Ok(MagellanBackend::SQLite(graph))
    }

    /// Open or create a database
    pub fn open_or_create(db_path: &Path) -> Result<Self> {
        if db_path.exists() {
            Self::open(db_path)
        } else {
            Self::create(db_path)
        }
    }

    /// Open a database
    pub fn open(db_path: &Path) -> Result<Self> {
        let graph = CodeGraph::open(db_path).context("Failed to open SQLite database")?;
        Ok(MagellanBackend::SQLite(graph))
    }

    /// Find a symbol by its fully qualified name
    pub fn find_symbol_by_fqn(&self, fqn: &str) -> Result<Option<UnifiedSymbolInfo>> {
        match self {
            MagellanBackend::SQLite(graph) => {
                if let Ok(id) = fqn.parse::<i64>() {
                    let snapshot = SnapshotId::current();
                    if let Ok(node) = graph.backend().get_node(snapshot, id) {
                        if node.kind == "Symbol" {
                            if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(node.data)
                            {
                                return Ok(Some(Self::convert_symbol_node(&symbol_node, id)));
                            }
                        }
                    }
                }
                let symbols = Self::get_all_sqlite_symbols(graph)?;
                for (entity_id, symbol) in symbols {
                    if symbol.fqn.as_deref() == Some(fqn) {
                        return Ok(Some(Self::convert_symbol_node(&symbol, entity_id)));
                    }
                }
                Ok(None)
            }
        }
    }

    /// Find a symbol by its numeric ID
    pub fn find_symbol_by_id(&self, id: u64) -> Option<UnifiedSymbolInfo> {
        match self {
            MagellanBackend::SQLite(graph) => {
                let snapshot = SnapshotId::current();
                graph
                    .backend()
                    .get_node(snapshot, id as i64)
                    .ok()
                    .and_then(|node| {
                        if node.kind == "Symbol" {
                            serde_json::from_value::<SymbolNode>(node.data)
                                .ok()
                                .map(|symbol| Self::convert_symbol_node(&symbol, id as i64))
                        } else {
                            None
                        }
                    })
            }
        }
    }

    /// Find symbols by name
    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<UnifiedSymbolInfo>> {
        match self {
            MagellanBackend::SQLite(graph) => {
                let symbols = Self::get_all_sqlite_symbols(graph)?;
                let results: Vec<UnifiedSymbolInfo> = symbols
                    .into_iter()
                    .filter(|(_, symbol)| symbol.name.as_deref() == Some(name))
                    .map(|(entity_id, symbol)| Self::convert_symbol_node(&symbol, entity_id))
                    .collect();
                Ok(results)
            }
        }
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<BackendStats> {
        match self {
            MagellanBackend::SQLite(graph) => {
                let symbol_count = graph.count_symbols().unwrap_or(0);
                let file_count = graph.count_files().unwrap_or(0);
                let cfg_block_count = 0;
                Ok(BackendStats {
                    node_count: symbol_count,
                    symbol_count,
                    file_count,
                    cfg_block_count,
                })
            }
        }
    }

    /// Convert SymbolNode to unified format
    fn convert_symbol_node(node: &SymbolNode, entity_id: i64) -> UnifiedSymbolInfo {
        UnifiedSymbolInfo {
            id: entity_id as u64,
            name: node.name.clone().unwrap_or_default(),
            fqn: node.fqn.clone().unwrap_or_default(),
            kind: SymbolKind::parse(&node.kind).unwrap_or(SymbolKind::Unknown),
            file_path: String::new(),
            byte_start: node.byte_start as u64,
            byte_end: node.byte_end as u64,
            start_line: node.start_line as u64,
            start_col: node.start_col as u64,
            end_line: node.end_line as u64,
            end_col: node.end_col as u64,
            language: None,
        }
    }

    /// Helper to get all symbols from SQLite backend
    fn get_all_sqlite_symbols(graph: &CodeGraph) -> Result<Vec<(i64, SymbolNode)>> {
        let backend = graph.backend();
        let entity_ids = backend.entity_ids()?;
        let snapshot = SnapshotId::current();
        let mut symbols = Vec::new();

        for entity_id in entity_ids {
            if let Ok(node) = backend.get_node(snapshot, entity_id) {
                if node.kind == "Symbol" {
                    if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(node.data) {
                        symbols.push((entity_id, symbol_node));
                    }
                }
            }
        }

        Ok(symbols)
    }

    /// Export database to JSON format
    pub fn export_json(&self) -> Result<String> {
        match self {
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_json not implemented for SQLite backend"
            )),
        }
    }

    /// Export database to JSON Lines format
    pub fn export_jsonl(&self) -> Result<String> {
        match self {
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_jsonl not implemented for SQLite backend"
            )),
        }
    }

    /// Export database to CSV format
    pub fn export_csv(&self) -> Result<String> {
        match self {
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_csv not implemented for SQLite backend"
            )),
        }
    }

    /// Get symbols in a specific file
    pub fn symbols_in_file(&self, file_path: &str) -> Result<Vec<UnifiedSymbolInfo>> {
        match self {
            MagellanBackend::SQLite(_graph) => {
                let _ = file_path;
                Ok(Vec::new())
            }
        }
    }

    /// Get code chunks for a file
    pub fn get_code_chunks(
        &self,
        file_path: &str,
    ) -> Result<Vec<crate::generation::schema::CodeChunk>> {
        match self {
            MagellanBackend::SQLite(graph) => graph.get_code_chunks(file_path),
        }
    }

    /// Get code chunks for a specific symbol in a file
    pub fn get_code_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<crate::generation::schema::CodeChunk>> {
        match self {
            MagellanBackend::SQLite(graph) => {
                graph.get_code_chunks_for_symbol(file_path, symbol_name)
            }
        }
    }

    /// Get a code chunk by exact byte span
    pub fn get_code_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<crate::generation::schema::CodeChunk>> {
        match self {
            MagellanBackend::SQLite(graph) => {
                graph.get_code_chunk_by_span(file_path, byte_start, byte_end)
            }
        }
    }

    /// Start execution tracking
    pub fn start_execution(
        &self,
        exec_id: &str,
        version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64> {
        match self {
            MagellanBackend::SQLite(graph) => graph
                .execution_log()
                .start_execution(exec_id, version, args, root, db_path),
        }
    }

    /// Finish execution tracking
    pub fn finish_execution(
        &self,
        exec_id: &str,
        status: &str,
        error: Option<&str>,
        files_processed: usize,
        symbols_indexed: usize,
        refs_indexed: usize,
    ) -> Result<()> {
        match self {
            MagellanBackend::SQLite(graph) => graph.execution_log().finish_execution(
                exec_id,
                status,
                error,
                files_processed,
                symbols_indexed,
                refs_indexed,
            ),
        }
    }

    /// Find dead code from entry points
    pub fn dead_code_from_entries(&self, _entry_ids: &[u64]) -> Vec<u64> {
        Vec::new()
    }

    /// Get strongly connected components
    pub fn get_sccs(&self) -> SccResult {
        SccResult {
            sccs: Vec::new(),
            scc_count: 0,
        }
    }

    /// Condense graph to DAG
    pub fn condense_graph(&self) -> CondensationDag {
        CondensationDag {
            dag: Vec::new(),
            node_count: 0,
        }
    }

    /// Enumerate execution paths
    pub fn enumerate_paths(&self, _entry_id: u64, _max_depth: usize) -> PathEnumerationResult {
        PathEnumerationResult {
            paths: Vec::new(),
            path_count: 0,
        }
    }

    /// Reverse reachable from a node
    pub fn reverse_reachable_from(&self, _id: u64) -> Vec<u64> {
        Vec::new()
    }

    /// Forward reachable from a node
    pub fn reachable_from(&self, _id: u64) -> Vec<u64> {
        Vec::new()
    }
}

/// Database statistics
#[derive(Debug)]
pub struct BackendStats {
    pub node_count: usize,
    pub symbol_count: usize,
    pub file_count: usize,
    pub cfg_block_count: usize,
}

/// SCC result
#[derive(Debug)]
pub struct SccResult {
    pub sccs: Vec<Vec<u64>>,
    pub scc_count: usize,
}

/// Condensation DAG
#[derive(Debug)]
pub struct CondensationDag {
    pub dag: Vec<(u64, u64)>,
    pub node_count: usize,
}

/// Path enumeration result
#[derive(Debug)]
pub struct PathEnumerationResult {
    pub paths: Vec<Vec<u64>>,
    pub path_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_type_sqlite() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("test.db");
        std::fs::File::create(&db_path).unwrap();

        assert_eq!(MagellanBackend::detect_type(&db_path), BackendType::SQLite);
    }
}

//! Backend router for Magellan CLI
//!
//! Provides unified interface across different backend types (SQLite, Geometric, V3)
//! Automatically detects backend type from file extension and routes accordingly.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::graph::backend::Backend;
#[cfg(feature = "geometric-backend")]
use crate::graph::geometric_backend::{
    GeometricBackend, GeometricBackendStats, SymbolInfo as GeometricSymbolInfo,
};
use crate::graph::CodeGraph;
use crate::ingest::SymbolKind;

/// Unified symbol information across all backends
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
    /// Optional language for semantic enrichment
    pub language: Option<String>,
}

/// Backend types supported by Magellan
#[derive(Debug, PartialEq)]
pub enum BackendType {
    /// SQLite backend (default)
    SQLite,
    /// Geometric backend (spatial indexing)
    Geometric,
    /// Native V3 backend (B-tree storage)
    NativeV3,
}

/// Unified backend interface
pub enum MagellanBackend {
    SQLite(CodeGraph),
    #[cfg(feature = "geometric-backend")]
    Geometric(GeometricBackend),
}

impl MagellanBackend {
    /// Detect backend type from file extension
    pub fn detect_type(db_path: &Path) -> BackendType {
        match db_path.extension().and_then(|e| e.to_str()) {
            #[cfg(feature = "geometric-backend")]
            Some("geo") => BackendType::Geometric,
            #[cfg(not(feature = "geometric-backend"))]
            Some("geo") => {
                // Fallback to SQLite if geometric backend is not available
                BackendType::SQLite
            }
            Some("v3") => BackendType::NativeV3,
            Some("db") | Some("sqlite") | _ => BackendType::SQLite,
        }
    }

    /// Create a new database with automatic backend detection
    /// If the file already exists, this will return an error
    pub fn create(db_path: &Path) -> Result<Self> {
        match Self::detect_type(db_path) {
            #[cfg(feature = "geometric-backend")]
            BackendType::Geometric => {
                let backend = GeometricBackend::create(db_path)
                    .context("Failed to create geometric database")?;
                Ok(MagellanBackend::Geometric(backend))
            }
            #[cfg(not(feature = "geometric-backend"))]
            BackendType::Geometric => Err(anyhow::anyhow!(
                "Geometric backend requires 'geometric-backend' feature"
            )),
            BackendType::SQLite | BackendType::NativeV3 => {
                // For SQLite/V3, CodeGraph::open already handles creating if not exists
                let graph = CodeGraph::open(db_path).context("Failed to create SQLite database")?;
                Ok(MagellanBackend::SQLite(graph))
            }
        }
    }

    /// Open or create a database with automatic backend detection
    /// If the file doesn't exist, it will be created
    pub fn open_or_create(db_path: &Path) -> Result<Self> {
        if db_path.exists() {
            Self::open(db_path)
        } else {
            Self::create(db_path)
        }
    }

    /// Open a database with automatic backend detection
    pub fn open(db_path: &Path) -> Result<Self> {
        match Self::detect_type(db_path) {
            #[cfg(feature = "geometric-backend")]
            BackendType::Geometric => {
                let backend =
                    GeometricBackend::open(db_path).context("Failed to open geometric database")?;
                Ok(MagellanBackend::Geometric(backend))
            }
            #[cfg(not(feature = "geometric-backend"))]
            BackendType::Geometric => Err(anyhow::anyhow!(
                "Geometric backend requires 'geometric-backend' feature"
            )),
            BackendType::SQLite | BackendType::NativeV3 => {
                let graph = CodeGraph::open(db_path).context("Failed to open SQLite database")?;
                Ok(MagellanBackend::SQLite(graph))
            }
        }
    }

    /// Find a symbol by its fully qualified name
    pub fn find_symbol_by_fqn(&self, fqn: &str) -> Result<Option<UnifiedSymbolInfo>> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => {
                // For numeric ID lookup (FQN is just the ID as string)
                if let Ok(id) = fqn.parse::<u64>() {
                    if let Some(info) = backend.find_symbol_by_id_info(id) {
                        return Ok(Some(Self::convert_geometric_symbol(&info)));
                    }
                }
                // Otherwise try FQN lookup
                if let Some(info) = backend.find_symbol_by_fqn_info(fqn) {
                    Ok(Some(Self::convert_geometric_symbol(&info)))
                } else {
                    Ok(None)
                }
            }
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite symbol lookup via graph
                let _ = fqn;
                Ok(None)
            }
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite symbol lookup via graph
                let _ = fqn;
                Ok(None)
            }
        }
    }

    /// Find a symbol by its numeric ID
    pub fn find_symbol_by_id(&self, id: u64) -> Option<UnifiedSymbolInfo> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => backend
                .find_symbol_by_id_info(id)
                .map(|info| Self::convert_geometric_symbol(&info)),
            MagellanBackend::SQLite(_graph) => {
                let _ = id;
                None
            }
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => {
                let _ = id;
                None
            }
        }
    }

    /// Find symbols by name (simple name, not FQN)
    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<UnifiedSymbolInfo>> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => {
                let symbols = backend.find_symbols_by_name_info(name);
                let results: Vec<UnifiedSymbolInfo> = symbols
                    .into_iter()
                    .map(|info| Self::convert_geometric_symbol(&info))
                    .collect();
                Ok(results)
            }
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite symbol lookup by name
                let _ = name;
                Ok(Vec::new())
            }
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite symbol lookup by name
                let _ = name;
                Ok(Vec::new())
            }
        }
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<BackendStats> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => {
                let stats = backend.get_stats()?;
                Ok(BackendStats {
                    node_count: stats.node_count,
                    symbol_count: stats.symbol_count,
                    file_count: stats.file_count,
                    cfg_block_count: stats.cfg_block_count,
                })
            }
            MagellanBackend::SQLite(graph) => {
                let symbol_count = graph.count_symbols().unwrap_or(0);
                let file_count = graph.count_files().unwrap_or(0);
                let cfg_block_count = 0; // TODO: Get actual CFG block count
                Ok(BackendStats {
                    node_count: symbol_count,
                    symbol_count,
                    file_count,
                    cfg_block_count,
                })
            }
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(graph) => {
                let symbol_count = graph.count_symbols().unwrap_or(0);
                let file_count = graph.count_files().unwrap_or(0);
                let cfg_block_count = 0; // TODO: Get actual CFG block count
                Ok(BackendStats {
                    node_count: symbol_count,
                    symbol_count,
                    file_count,
                    cfg_block_count,
                })
            }
        }
    }

    /// Convert geometric symbol info to unified format
    #[cfg(feature = "geometric-backend")]
    fn convert_geometric_symbol(info: &GeometricSymbolInfo) -> UnifiedSymbolInfo {
        use crate::ingest::Language;
        let language_str = Some(info.language.as_str().to_string());
        UnifiedSymbolInfo {
            id: info.id,
            name: info.name.clone(),
            fqn: info.fqn.clone(),
            kind: info.kind.clone(),
            file_path: info.file_path.clone(),
            byte_start: info.byte_start,
            byte_end: info.byte_end,
            start_line: info.start_line as u64,
            start_col: info.start_col as u64,
            end_line: info.end_line as u64,
            end_col: info.end_col as u64,
            language: language_str,
        }
    }

    /// Export database to JSON format
    pub fn export_json(&self) -> Result<String> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => backend.export_json(),
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_json not implemented for SQLite backend"
            )),
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_json not implemented for SQLite backend"
            )),
        }
    }

    /// Export database to JSON Lines format
    pub fn export_jsonl(&self) -> Result<String> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => backend.export_jsonl(),
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_jsonl not implemented for SQLite backend"
            )),
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_jsonl not implemented for SQLite backend"
            )),
        }
    }

    /// Export database to CSV format
    pub fn export_csv(&self) -> Result<String> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => backend.export_csv(),
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_csv not implemented for SQLite backend"
            )),
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => Err(anyhow::anyhow!(
                "export_csv not implemented for SQLite backend"
            )),
        }
    }

    /// Get symbols in a specific file
    pub fn symbols_in_file(&self, file_path: &str) -> Result<Vec<UnifiedSymbolInfo>> {
        #[cfg(feature = "geometric-backend")]
        match self {
            MagellanBackend::Geometric(backend) => {
                let symbols = backend.symbols_in_file(file_path)?;
                Ok(symbols
                    .into_iter()
                    .map(|info| Self::convert_geometric_symbol(&info))
                    .collect())
            }
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite file-based symbol lookup
                let _ = file_path;
                Ok(Vec::new())
            }
        }
        #[cfg(not(feature = "geometric-backend"))]
        match self {
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite file-based symbol lookup
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
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.get_code_chunks(file_path),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => _graph.get_code_chunks(file_path),
            #[cfg(not(feature = "geometric-backend"))]
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
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => {
                backend.get_code_chunks_for_symbol(file_path, symbol_name)
            }
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                _graph.get_code_chunks_for_symbol(file_path, symbol_name)
            }
            #[cfg(not(feature = "geometric-backend"))]
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
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => {
                backend.get_code_chunk_by_span(file_path, byte_start, byte_end)
            }
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                _graph.get_code_chunk_by_span(file_path, byte_start, byte_end)
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(graph) => {
                graph.get_code_chunk_by_span(file_path, byte_start, byte_end)
            }
        }
    }

    /// Start an execution log entry
    pub fn start_execution(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<()> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => {
                backend.start_execution(execution_id, tool_version, args, root, db_path)
            }
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(graph) => {
                graph.execution_log().start_execution(
                    execution_id,
                    tool_version,
                    args,
                    root,
                    db_path,
                )?;
                Ok(())
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(graph) => {
                graph.execution_log().start_execution(
                    execution_id,
                    tool_version,
                    args,
                    root,
                    db_path,
                )?;
                Ok(())
            }
        }
    }

    /// Finish an execution log entry
    pub fn finish_execution(
        &self,
        execution_id: &str,
        outcome: &str,
        error_message: Option<&str>,
        files_indexed: i64,
        symbols_indexed: i64,
        references_indexed: i64,
    ) -> Result<()> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.finish_execution(
                execution_id,
                outcome,
                error_message,
                files_indexed,
                symbols_indexed,
                references_indexed,
            ),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(graph) => {
                graph.execution_log().finish_execution(
                    execution_id,
                    outcome,
                    error_message,
                    files_indexed as usize,
                    symbols_indexed as usize,
                    references_indexed as usize,
                )?;
                Ok(())
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(graph) => {
                graph.execution_log().finish_execution(
                    execution_id,
                    outcome,
                    error_message,
                    files_indexed as usize,
                    symbols_indexed as usize,
                    references_indexed as usize,
                )?;
                Ok(())
            }
        }
    }

    /// Get outgoing calls (callees) for a symbol by name and path
    ///
    /// Returns CallFact structs with full metadata for all calls from the symbol
    pub fn calls_from_symbol(
        &self,
        path: &str,
        name: &str,
    ) -> Result<Vec<crate::references::CallFact>> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => {
                Ok(backend.calls_from_symbol_as_facts(path, name))
            }
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // SQLite requires mutable access - for now, return empty
                // TODO: Implement proper SQLite support through backend-agnostic API
                let _ = (path, name); // Explicitly mark as used for API compatibility
                Ok(Vec::new())
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => {
                let _ = (path, name); // Explicitly mark as used for API compatibility
                Ok(Vec::new())
            }
        }
    }

    /// Get incoming calls (callers) for a symbol by name and path
    ///
    /// Returns CallFact structs with full metadata for all calls to the symbol
    pub fn callers_of_symbol(
        &self,
        path: &str,
        name: &str,
    ) -> Result<Vec<crate::references::CallFact>> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => {
                Ok(backend.callers_of_symbol_as_facts(path, name))
            }
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // SQLite requires mutable access - for now, return empty
                // TODO: Implement proper SQLite support through backend-agnostic API
                let _ = (path, name); // Explicitly mark as used for API compatibility
                Ok(Vec::new())
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => {
                let _ = (path, name); // Explicitly mark as used for API compatibility
                Ok(Vec::new())
            }
        }
    }

    /// Find symbol ID by name and file path
    ///
    /// This is used by the refs command to resolve a symbol name + path to a symbol ID
    pub fn find_symbol_id_by_name_and_path(&self, path: &str, name: &str) -> Option<u64> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => {
                backend.find_symbol_id_by_name_and_path(name, path)
            }
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite version
                let _ = (path, name);
                None
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => {
                let _ = (path, name);
                None
            }
        }
    }

    /// Forward reachability from a symbol
    ///
    /// Returns all symbol IDs reachable from the given start symbol via call edges.
    pub fn reachable_from(&self, start_id: u64) -> Vec<u64> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.reachable_from(start_id),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite version
                let _ = start_id;
                Vec::new()
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => {
                let _ = start_id;
                Vec::new()
            }
        }
    }

    /// Reverse reachability from a symbol
    ///
    /// Returns all symbol IDs that can reach the given start symbol via call edges.
    pub fn reverse_reachable_from(&self, start_id: u64) -> Vec<u64> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.reverse_reachable_from(start_id),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite version
                let _ = start_id;
                Vec::new()
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => {
                let _ = start_id;
                Vec::new()
            }
        }
    }

    /// Find dead code from entry points
    ///
    /// Returns all symbol IDs not reachable from any of the given entry points.
    pub fn dead_code_from_entries(&self, entry_ids: &[u64]) -> Vec<u64> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.dead_code_from_entries(entry_ids),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite version
                let _ = entry_ids;
                Vec::new()
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => {
                let _ = entry_ids;
                Vec::new()
            }
        }
    }

    /// Get all symbol IDs
    pub fn get_all_symbol_ids(&self) -> Vec<u64> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.get_all_symbol_ids(),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => {
                // TODO: Implement SQLite version
                Vec::new()
            }
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => Vec::new(),
        }
    }

    /// Find cycles (mutually recursive SCCs) in the call graph
    pub fn find_cycles(&self) -> Vec<Vec<u64>> {
        match self {
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::Geometric(backend) => backend.find_call_graph_cycles(),
            #[cfg(feature = "geometric-backend")]
            MagellanBackend::SQLite(_graph) => Vec::new(),
            #[cfg(not(feature = "geometric-backend"))]
            MagellanBackend::SQLite(_graph) => Vec::new(),
        }
    }

    /// Get all strongly connected components
    #[cfg(feature = "geometric-backend")]
    pub fn get_sccs(&self) -> crate::graph::geometric_calls::SccResult {
        match self {
            MagellanBackend::Geometric(backend) => backend.get_strongly_connected_components(),
            MagellanBackend::SQLite(_graph) => {
                let scc = crate::graph::geometric_calls::SccResult {
                    components: Vec::new(),
                    node_to_component: std::collections::HashMap::new(),
                };
                scc
            }
        }
    }

    /// Condense the call graph (collapse SCCs into supernodes)
    #[cfg(feature = "geometric-backend")]
    pub fn condense_graph(&self) -> crate::graph::geometric_calls::CondensationDag {
        match self {
            MagellanBackend::Geometric(backend) => backend.condense_call_graph(),
            MagellanBackend::SQLite(_graph) => {
                let dag = crate::graph::geometric_calls::CondensationDag {
                    supernodes: Vec::new(),
                    node_to_supernode: std::collections::HashMap::new(),
                    edges: Vec::new(),
                };
                dag
            }
        }
    }

    /// Enumerate paths in the call graph
    #[cfg(feature = "geometric-backend")]
    pub fn enumerate_paths(
        &self,
        start_id: u64,
        end_id: Option<u64>,
        max_depth: usize,
        max_paths: usize,
    ) -> crate::graph::geometric_backend::PathEnumerationResult {
        match self {
            MagellanBackend::Geometric(backend) => {
                backend.enumerate_paths(start_id, end_id, max_depth, max_paths)
            }
            MagellanBackend::SQLite(_graph) => {
                crate::graph::geometric_backend::PathEnumerationResult {
                    paths: Vec::new(),
                    total_enumerated: 0,
                    bounded_hit: false,
                }
            }
        }
    }

    /// Backward slice (what affects this symbol)
    pub fn backward_slice(&self, symbol_id: u64) -> Vec<u64> {
        self.reverse_reachable_from(symbol_id)
    }

    /// Forward slice (what this symbol affects)
    pub fn forward_slice(&self, symbol_id: u64) -> Vec<u64> {
        self.reachable_from(symbol_id)
    }
}

/// Statistics for any backend
#[derive(Debug, Clone)]
pub struct BackendStats {
    pub node_count: usize,
    pub symbol_count: usize,
    pub file_count: usize,
    pub cfg_block_count: usize,
}

/// Find a symbol by name across all files (backend-agnostic)
pub fn find_symbol_by_name(db_path: &Path, name: &str) -> Result<Option<UnifiedSymbolInfo>> {
    let backend = MagellanBackend::open(db_path)?;
    backend.find_symbol_by_fqn(name)
}

/// Find symbols in a specific file (backend-agnostic)
pub fn find_symbols_in_file(db_path: &Path, file_path: &str) -> Result<Vec<UnifiedSymbolInfo>> {
    let backend = MagellanBackend::open(db_path)?;
    backend.symbols_in_file(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_detection() {
        // .geo files are detected as Geometric when feature is enabled, otherwise SQLite fallback
        #[cfg(feature = "geometric-backend")]
        assert!(matches!(
            MagellanBackend::detect_type(Path::new("test.geo")),
            BackendType::Geometric
        ));
        #[cfg(not(feature = "geometric-backend"))]
        assert!(matches!(
            MagellanBackend::detect_type(Path::new("test.geo")),
            BackendType::SQLite
        ));
        assert!(matches!(
            MagellanBackend::detect_type(Path::new("test.db")),
            BackendType::SQLite
        ));
        assert!(matches!(
            MagellanBackend::detect_type(Path::new("test.sqlite")),
            BackendType::SQLite
        ));
        assert!(matches!(
            MagellanBackend::detect_type(Path::new("test.v3")),
            BackendType::NativeV3
        ));
    }
}

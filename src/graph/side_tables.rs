//! Side table storage abstraction for Magellan
//!
//! This module provides backend-specific side table implementations:
//! - `SqliteSideTables`: Uses SQLite for all side tables
//! - `V3SideTables`: Uses V3 KV store (no SQLite dependency)
//!
//! # Design Philosophy
//!
//! Each backend is **fully self-contained**:
//! - SQLite backend: Everything in `.db` file
//! - V3 backend: Everything in `.v3` file (graph + KV)
//!
//! No mixing between backends for optimal performance.

use anyhow::Result;
use std::path::Path;
#[cfg(feature = "native-v3")]
use std::sync::Arc;

// Re-export types from existing modules
pub use crate::generation::CodeChunk;
pub use crate::graph::execution_log::ExecutionRecord;
pub use crate::graph::metrics::{FileMetrics, SymbolMetrics};

/// Side table operations trait - backend-agnostic interface
///
/// Both backends implement this trait to provide:
/// - Code chunks storage
/// - File/symbol metrics
/// - Execution logging
/// 
/// This trait is object-safe and can be used with `Box<dyn SideTables>`.
pub trait SideTables: Send + Sync {
    // ===== Execution Log Methods =====
    
    /// Start a new execution log entry
    fn start_execution(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64>;
    
    /// Finish an execution log entry
    fn finish_execution(
        &self,
        execution_id: &str,
        outcome: &str,
        error_message: Option<&str>,
        files_indexed: usize,
        symbols_indexed: usize,
        references_indexed: usize,
    ) -> Result<()>;
    
    /// Get an execution record by execution_id
    fn get_execution(&self, execution_id: &str) -> Result<Option<ExecutionRecord>>;
    
    /// List all executions, ordered by most recent first
    fn list_executions(&self, limit: Option<usize>) -> Result<Vec<ExecutionRecord>>;
    
    // ===== File Metrics Methods =====
    
    /// Store file metrics
    fn store_file_metrics(&self, metrics: &FileMetrics) -> Result<()>;
    
    /// Get file metrics by file path
    fn get_file_metrics(&self, file_path: &str) -> Result<Option<FileMetrics>>;
    
    // ===== Symbol Metrics Methods =====
    
    /// Store symbol metrics
    fn store_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()>;
    
    /// Get symbol metrics by symbol_id
    fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>>;
    
    /// Delete all metrics for a file
    fn delete_metrics_for_file(&self, file_path: &str) -> Result<usize>;
    
    /// Get hotspots (files with highest complexity scores)
    fn get_hotspots(
        &self,
        limit: Option<u32>,
        min_loc: Option<i64>,
        min_fan_in: Option<i64>,
        min_fan_out: Option<i64>,
    ) -> Result<Vec<FileMetrics>>;
    
    // ===== Code Chunk Methods =====
    
    /// Store a code chunk
    fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64>;
    
    /// Get a code chunk by ID
    fn get_chunk(&self, chunk_id: i64) -> Result<Option<CodeChunk>>;
    
    /// Get a code chunk by file path and byte span
    fn get_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>>;
    
    /// Get all chunks for a file
    fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>>;
    
    /// Count chunks for a file
    fn count_chunks_for_file(&self, file_path: &str) -> Result<usize>;
    
    /// Delete all chunks for a file
    fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize>;
    
    /// Get chunks by symbol name
    fn get_chunks_by_symbol(&self, file_path: &str, symbol_name: &str) -> Result<Vec<CodeChunk>>;
    
    /// Get all chunks
    fn get_all_chunks(&self) -> Result<Vec<CodeChunk>>;
    
    /// Count all chunks
    fn count_chunks(&self) -> Result<usize>;
    
    // ===== AST Node Methods =====

    /// Store an AST node, return node ID
    fn store_ast_node(&self, node: &crate::graph::AstNode, file_id: i64) -> Result<i64>;

    /// Store multiple AST nodes in a batch operation
    ///
    /// This is more efficient than calling `store_ast_node` multiple times,
    /// especially for SQLite which can use transactions for bulk inserts.
    ///
    /// # Arguments
    /// * `nodes` - Vector of tuples containing (AstNode, file_id)
    ///
    /// # Returns
    /// Vector of assigned node IDs in the same order as input nodes
    ///
    /// # Performance
    /// - SQLite: Uses a single transaction for all inserts
    /// - V3: Batches KV operations together
    fn store_ast_nodes_batch(&self, nodes: &[(crate::graph::AstNode, i64)]) -> Result<Vec<i64>>;

    /// Get AST node by ID
    fn get_ast_node(&self, node_id: i64) -> Result<Option<crate::graph::AstNode>>;

    /// Update the parent_id of an AST node
    ///
    /// Used after batch insertion to resolve placeholder parent references.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node to update
    /// * `new_parent_id` - The new parent ID to set
    ///
    /// # Implementation
    /// - SQLite: Uses UPDATE query for efficient in-place update
    /// - V3: Deletes and re-inserts the node (KV stores don't support updates)
    fn update_ast_node_parent(&self, node_id: i64, new_parent_id: i64) -> Result<()>;

    /// Get all AST nodes for a file
    fn get_ast_nodes_by_file(&self, file_id: i64) -> Result<Vec<crate::graph::AstNode>>;
    
    /// Get all AST nodes (for finding roots)
    fn get_all_ast_nodes(&self) -> Result<Vec<crate::graph::AstNode>>;
    
    /// Get AST nodes by kind (e.g., "if_expression")
    fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<crate::graph::AstNode>>;
    
    /// Get children of an AST node
    fn get_ast_children(&self, parent_id: i64) -> Result<Vec<crate::graph::AstNode>>;
    
    /// Count all AST nodes
    fn count_ast_nodes(&self) -> Result<usize>;
    
    /// Count AST nodes for a specific file
    fn count_ast_nodes_for_file(&self, file_id: i64) -> Result<usize>;
    
    /// Delete all AST nodes for a file (returns count deleted)
    fn delete_ast_nodes_for_file(&self, file_id: i64) -> Result<usize>;

    // ===== Cross-File Reference Methods =====

    /// Store a cross-file reference
    ///
    /// # Arguments
    /// * `cref` - The cross-file reference to store
    fn store_cross_file_ref(&self, cref: &crate::graph::schema::CrossFileRef) -> Result<()>;

    /// Get all references to a specific symbol (by target symbol ID)
    ///
    /// # Arguments
    /// * `to_symbol_id` - The target symbol ID
    ///
    /// # Returns
    /// Vector of cross-file references where `to_symbol_id` is the target
    fn get_references_to(&self, to_symbol_id: &str) -> Result<Vec<crate::graph::schema::CrossFileRef>>;

    /// Get all references from a specific symbol (by source symbol ID)
    ///
    /// # Arguments
    /// * `from_symbol_id` - The source symbol ID
    ///
    /// # Returns
    /// Vector of cross-file references where `from_symbol_id` is the source
    fn get_references_from(&self, from_symbol_id: &str) -> Result<Vec<crate::graph::schema::CrossFileRef>>;

    /// Delete all cross-file references for a file
    ///
    /// # Arguments
    /// * `file_path` - The file path to delete references for
    ///
    /// # Returns
    /// Number of references deleted
    fn delete_cross_file_refs_for_file(&self, file_path: &str) -> Result<usize>;

    /// Count total cross-file references
    fn count_cross_file_refs(&self) -> Result<usize>;
    
    /// Convert to Any for downcasting
    ///
    /// This allows downcasting to concrete backend types (e.g., V3SideTables)
    /// for backend-specific operations.
    fn as_any(&self) -> &dyn std::any::Any;
}

// =============================================================================
// SQLite Implementation
// =============================================================================

#[cfg(feature = "sqlite-backend")]
pub mod sqlite_impl {
    use super::*;
    use rusqlite::{Connection, OptionalExtension, params};
    use std::sync::Mutex;

    /// SQLite-based side tables implementation
    pub struct SqliteSideTables {
        conn: Mutex<Connection>,
    }

    impl SqliteSideTables {
        /// Open or create side tables in SQLite database
        pub fn open(db_path: &Path) -> Result<Self> {
            let conn = Connection::open(db_path)?;
            let tables = Self { conn: Mutex::new(conn) };
            tables.ensure_schema()?;
            Ok(tables)
        }

        fn ensure_schema(&self) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            
            // File metrics table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS file_metrics (
                    file_path TEXT PRIMARY KEY,
                    symbol_count INTEGER DEFAULT 0,
                    loc INTEGER DEFAULT 0,
                    estimated_loc REAL DEFAULT 0,
                    fan_in INTEGER DEFAULT 0,
                    fan_out INTEGER DEFAULT 0,
                    complexity_score REAL DEFAULT 0,
                    last_updated INTEGER NOT NULL
                )",
                [],
            )?;

            // Symbol metrics table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS symbol_metrics (
                    symbol_id INTEGER PRIMARY KEY,
                    symbol_name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    loc INTEGER DEFAULT 0,
                    estimated_loc REAL DEFAULT 0,
                    fan_in INTEGER DEFAULT 0,
                    fan_out INTEGER DEFAULT 0,
                    cyclomatic_complexity INTEGER DEFAULT 0,
                    last_updated INTEGER NOT NULL
                )",
                [],
            )?;

            // Execution log table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS execution_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    execution_id TEXT NOT NULL UNIQUE,
                    tool_version TEXT NOT NULL,
                    args TEXT NOT NULL,
                    root TEXT,
                    db_path TEXT NOT NULL,
                    started_at INTEGER NOT NULL,
                    finished_at INTEGER,
                    duration_ms INTEGER,
                    outcome TEXT NOT NULL,
                    error_message TEXT,
                    files_indexed INTEGER DEFAULT 0,
                    symbols_indexed INTEGER DEFAULT 0,
                    references_indexed INTEGER DEFAULT 0
                )",
                [],
            )?;

            // Indexes
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_execution_log_started_at ON execution_log(started_at DESC)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_execution_log_execution_id ON execution_log(execution_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_execution_log_outcome ON execution_log(outcome)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_file_path ON symbol_metrics(file_path)",
                [],
            )?;

            // Cross-file references table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS cross_file_refs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    from_symbol_id TEXT NOT NULL,
                    to_symbol_id TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    line_number INTEGER NOT NULL,
                    byte_start INTEGER NOT NULL,
                    byte_end INTEGER NOT NULL
                )",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cross_file_refs_to ON cross_file_refs(to_symbol_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cross_file_refs_from ON cross_file_refs(from_symbol_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cross_file_refs_file ON cross_file_refs(file_path)",
                [],
            )?;

            Ok(())
        }
    }

    impl SideTables for SqliteSideTables {
        fn start_execution(
            &self,
            execution_id: &str,
            tool_version: &str,
            args: &[String],
            root: Option<&str>,
            db_path: &str,
        ) -> Result<i64> {
            let conn = self.conn.lock().unwrap();
            let args_json = serde_json::to_string(args)?;
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO execution_log
                    (execution_id, tool_version, args, root, db_path, started_at, outcome)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running')",
                params![execution_id, tool_version, args_json, root, db_path, started_at],
            )?;

            Ok(conn.last_insert_rowid())
        }

        fn finish_execution(
            &self,
            execution_id: &str,
            outcome: &str,
            error_message: Option<&str>,
            files_indexed: usize,
            symbols_indexed: usize,
            references_indexed: usize,
        ) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            let finished_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            // Get started_at to compute duration
            let started_at: i64 = conn
                .query_row(
                    "SELECT started_at FROM execution_log WHERE execution_id = ?1",
                    params![execution_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(finished_at);

            let duration_ms = (finished_at - started_at) * 1000;

            conn.execute(
                "UPDATE execution_log
                    SET finished_at = ?1, outcome = ?2, error_message = ?3,
                        duration_ms = ?4, files_indexed = ?5, symbols_indexed = ?6,
                        references_indexed = ?7
                    WHERE execution_id = ?8",
                params![
                    finished_at,
                    outcome,
                    error_message,
                    duration_ms,
                    files_indexed as i64,
                    symbols_indexed as i64,
                    references_indexed as i64,
                    execution_id,
                ],
            )?;

            Ok(())
        }

        fn get_execution(&self, execution_id: &str) -> Result<Option<ExecutionRecord>> {
            let conn = self.conn.lock().unwrap();

            let result = conn
                .query_row(
                    "SELECT id, execution_id, tool_version, args, root, db_path,
                            started_at, finished_at, duration_ms, outcome, error_message,
                            files_indexed, symbols_indexed, references_indexed
                     FROM execution_log
                     WHERE execution_id = ?1",
                    params![execution_id],
                    |row| {
                        Ok(ExecutionRecord {
                            id: row.get(0)?,
                            execution_id: row.get(1)?,
                            tool_version: row.get(2)?,
                            args: row.get(3)?,
                            root: row.get(4)?,
                            db_path: row.get(5)?,
                            started_at: row.get(6)?,
                            finished_at: row.get(7)?,
                            duration_ms: row.get(8)?,
                            outcome: row.get(9)?,
                            error_message: row.get(10)?,
                            files_indexed: row.get(11)?,
                            symbols_indexed: row.get(12)?,
                            references_indexed: row.get(13)?,
                        })
                    },
                )
                .optional()?;

            Ok(result)
        }

        fn list_executions(&self, limit: Option<usize>) -> Result<Vec<ExecutionRecord>> {
            let conn = self.conn.lock().unwrap();

            let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();
            let sql = format!(
                "SELECT id, execution_id, tool_version, args, root, db_path,
                        started_at, finished_at, duration_ms, outcome, error_message,
                        files_indexed, symbols_indexed, references_indexed
                 FROM execution_log
                 ORDER BY started_at DESC{}",
                limit_clause
            );

            let mut stmt = conn.prepare(&sql)?;
            let records = stmt
                .query_map([], |row| {
                    Ok(ExecutionRecord {
                        id: row.get(0)?,
                        execution_id: row.get(1)?,
                        tool_version: row.get(2)?,
                        args: row.get(3)?,
                        root: row.get(4)?,
                        db_path: row.get(5)?,
                        started_at: row.get(6)?,
                        finished_at: row.get(7)?,
                        duration_ms: row.get(8)?,
                        outcome: row.get(9)?,
                        error_message: row.get(10)?,
                        files_indexed: row.get(11)?,
                        symbols_indexed: row.get(12)?,
                        references_indexed: row.get(13)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(records)
        }

        fn store_file_metrics(&self, metrics: &FileMetrics) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO file_metrics (
                    file_path, symbol_count, loc, estimated_loc,
                    fan_in, fan_out, complexity_score, last_updated
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &metrics.file_path,
                    metrics.symbol_count,
                    metrics.loc,
                    metrics.estimated_loc,
                    metrics.fan_in,
                    metrics.fan_out,
                    metrics.complexity_score,
                    metrics.last_updated,
                ],
            )?;
            Ok(())
        }

        fn get_file_metrics(&self, file_path: &str) -> Result<Option<FileMetrics>> {
            let conn = self.conn.lock().unwrap();
            let result = conn
                .query_row(
                    "SELECT file_path, symbol_count, loc, estimated_loc,
                            fan_in, fan_out, complexity_score, last_updated
                     FROM file_metrics
                     WHERE file_path = ?1",
                    params![file_path],
                    |row| {
                        Ok(FileMetrics {
                            file_path: row.get(0)?,
                            symbol_count: row.get(1)?,
                            loc: row.get(2)?,
                            estimated_loc: row.get(3)?,
                            fan_in: row.get(4)?,
                            fan_out: row.get(5)?,
                            complexity_score: row.get(6)?,
                            last_updated: row.get(7)?,
                        })
                    },
                )
                .optional()?;

            Ok(result)
        }

        fn store_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO symbol_metrics (
                    symbol_id, symbol_name, kind, file_path,
                    loc, estimated_loc, fan_in, fan_out,
                    cyclomatic_complexity, last_updated
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    metrics.symbol_id,
                    &metrics.symbol_name,
                    &metrics.kind,
                    &metrics.file_path,
                    metrics.loc,
                    metrics.estimated_loc,
                    metrics.fan_in,
                    metrics.fan_out,
                    metrics.cyclomatic_complexity,
                    metrics.last_updated,
                ],
            )?;
            Ok(())
        }

        fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>> {
            let conn = self.conn.lock().unwrap();
            let result = conn
                .query_row(
                    "SELECT symbol_id, symbol_name, kind, file_path,
                            loc, estimated_loc, fan_in, fan_out,
                            cyclomatic_complexity, last_updated
                     FROM symbol_metrics
                     WHERE symbol_id = ?1",
                    params![symbol_id],
                    |row| {
                        Ok(SymbolMetrics {
                            symbol_id: row.get(0)?,
                            symbol_name: row.get(1)?,
                            kind: row.get(2)?,
                            file_path: row.get(3)?,
                            loc: row.get(4)?,
                            estimated_loc: row.get(5)?,
                            fan_in: row.get(6)?,
                            fan_out: row.get(7)?,
                            cyclomatic_complexity: row.get(8)?,
                            last_updated: row.get(9)?,
                        })
                    },
                )
                .optional()?;

            Ok(result)
        }

        fn delete_metrics_for_file(&self, file_path: &str) -> Result<usize> {
            let conn = self.conn.lock().unwrap();

            // Delete symbol metrics for this file first
            let symbol_count = conn.execute(
                "DELETE FROM symbol_metrics WHERE file_path = ?1",
                params![file_path],
            )?;

            // Delete file metrics
            conn.execute(
                "DELETE FROM file_metrics WHERE file_path = ?1",
                params![file_path],
            )?;

            Ok(symbol_count)
        }

        fn get_hotspots(
            &self,
            limit: Option<u32>,
            min_loc: Option<i64>,
            min_fan_in: Option<i64>,
            min_fan_out: Option<i64>,
        ) -> Result<Vec<FileMetrics>> {
            let conn = self.conn.lock().unwrap();

            // Build query with optional filters
            let mut query = String::from(
                "SELECT file_path, symbol_count, loc, estimated_loc,
                        fan_in, fan_out, complexity_score, last_updated
                 FROM file_metrics
                 WHERE 1=1",
            );
            let mut param_count = 0;

            if min_loc.is_some() {
                param_count += 1;
                query.push_str(&format!(" AND loc >= ?{param_count}"));
            }
            if min_fan_in.is_some() {
                param_count += 1;
                query.push_str(&format!(" AND fan_in >= ?{param_count}"));
            }
            if min_fan_out.is_some() {
                param_count += 1;
                query.push_str(&format!(" AND fan_out >= ?{param_count}"));
            }

            param_count += 1;
            query.push_str(&format!(" ORDER BY complexity_score DESC LIMIT ?{param_count}"));

            let mut stmt = conn.prepare(&query)?;

            // Build params based on which filters are active
            let mut query_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(min_loc) = min_loc {
                query_params.push(Box::new(min_loc));
            }
            if let Some(min_fi) = min_fan_in {
                query_params.push(Box::new(min_fi));
            }
            if let Some(min_fo) = min_fan_out {
                query_params.push(Box::new(min_fo));
            }
            query_params.push(Box::new(limit.unwrap_or(20) as i64));

            // Convert to references for query
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                query_params.iter().map(|p| p.as_ref()).collect();

            let mut rows = stmt.query(&*param_refs)?;

            let mut results = Vec::new();
            while let Some(row) = rows.next()? {
                results.push(FileMetrics {
                    file_path: row.get(0)?,
                    symbol_count: row.get(1)?,
                    loc: row.get(2)?,
                    estimated_loc: row.get(3)?,
                    fan_in: row.get(4)?,
                    fan_out: row.get(5)?,
                    complexity_score: row.get(6)?,
                    last_updated: row.get(7)?,
                });
            }

            Ok(results)
        }
        
        // ===== Code Chunk Methods =====
        
        fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO code_chunks
                    (file_path, byte_start, byte_end, content, content_hash, symbol_name, symbol_kind, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    chunk.file_path,
                    chunk.byte_start as i64,
                    chunk.byte_end as i64,
                    chunk.content,
                    chunk.content_hash,
                    chunk.symbol_name,
                    chunk.symbol_kind,
                    chunk.created_at,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
        
        fn get_chunk(&self, chunk_id: i64) -> Result<Option<CodeChunk>> {
            let conn = self.conn.lock().unwrap();
            let result = conn
                .query_row(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks WHERE id = ?1",
                    params![chunk_id],
                    |row| {
                        Ok(CodeChunk {
                            id: Some(row.get(0)?),
                            file_path: row.get(1)?,
                            byte_start: row.get::<_, i64>(2)? as usize,
                            byte_end: row.get::<_, i64>(3)? as usize,
                            content: row.get(4)?,
                            content_hash: row.get(5)?,
                            symbol_name: row.get(6)?,
                            symbol_kind: row.get(7)?,
                            created_at: row.get(8)?,
                        })
                    },
                )
                .optional()?;
            Ok(result)
        }
        
        fn get_chunk_by_span(
            &self,
            file_path: &str,
            byte_start: usize,
            byte_end: usize,
        ) -> Result<Option<CodeChunk>> {
            let conn = self.conn.lock().unwrap();
            let result = conn
                .query_row(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks WHERE file_path = ?1 AND byte_start = ?2 AND byte_end = ?3",
                    params![file_path, byte_start as i64, byte_end as i64],
                    |row| {
                        Ok(CodeChunk {
                            id: Some(row.get(0)?),
                            file_path: row.get(1)?,
                            byte_start: row.get::<_, i64>(2)? as usize,
                            byte_end: row.get::<_, i64>(3)? as usize,
                            content: row.get(4)?,
                            content_hash: row.get(5)?,
                            symbol_name: row.get(6)?,
                            symbol_kind: row.get(7)?,
                            created_at: row.get(8)?,
                        })
                    },
                )
                .optional()?;
            Ok(result)
        }
        
        fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks WHERE file_path = ?1 ORDER BY byte_start",
            )?;
            let chunks = stmt
                .query_map(params![file_path], |row| {
                    Ok(CodeChunk {
                        id: Some(row.get(0)?),
                        file_path: row.get(1)?,
                        byte_start: row.get::<_, i64>(2)? as usize,
                        byte_end: row.get::<_, i64>(3)? as usize,
                        content: row.get(4)?,
                        content_hash: row.get(5)?,
                        symbol_name: row.get(6)?,
                        symbol_kind: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(chunks)
        }
        
        fn count_chunks_for_file(&self, file_path: &str) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM code_chunks WHERE file_path = ?1",
                    params![file_path],
                    |row| row.get(0),
                )?;
            Ok(count as usize)
        }
        
        fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let affected = conn.execute(
                "DELETE FROM code_chunks WHERE file_path = ?1",
                params![file_path],
            )?;
            Ok(affected)
        }
        
        fn get_chunks_by_symbol(&self, file_path: &str, symbol_name: &str) -> Result<Vec<CodeChunk>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks 
                 WHERE file_path = ?1 AND symbol_name = ?2
                 ORDER BY byte_start",
            )?;
            let chunks = stmt
                .query_map(params![file_path, symbol_name], |row| {
                    Ok(CodeChunk {
                        id: Some(row.get(0)?),
                        file_path: row.get(1)?,
                        byte_start: row.get::<_, i64>(2)? as usize,
                        byte_end: row.get::<_, i64>(3)? as usize,
                        content: row.get(4)?,
                        content_hash: row.get(5)?,
                        symbol_name: row.get(6)?,
                        symbol_kind: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(chunks)
        }
        
        fn get_all_chunks(&self) -> Result<Vec<CodeChunk>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks ORDER BY file_path, byte_start",
            )?;
            let chunks = stmt
                .query_map([], |row| {
                    Ok(CodeChunk {
                        id: Some(row.get(0)?),
                        file_path: row.get(1)?,
                        byte_start: row.get::<_, i64>(2)? as usize,
                        byte_end: row.get::<_, i64>(3)? as usize,
                        content: row.get(4)?,
                        content_hash: row.get(5)?,
                        symbol_name: row.get(6)?,
                        symbol_kind: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(chunks)
        }
        
        fn count_chunks(&self) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM code_chunks",
                [],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        }

        // ===== AST Node Methods =====

        fn store_ast_node(&self, node: &crate::graph::AstNode, file_id: i64) -> Result<i64> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end, file_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    node.parent_id,
                    node.kind,
                    node.byte_start as i64,
                    node.byte_end as i64,
                    file_id,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }

        fn store_ast_nodes_batch(&self, nodes: &[(crate::graph::AstNode, i64)]) -> Result<Vec<i64>> {
            if nodes.is_empty() {
                return Ok(Vec::new());
            }

            let mut conn = self.conn.lock().unwrap();

            // Use transaction for better performance
            let tx = conn.transaction()?;

            let mut ids = Vec::with_capacity(nodes.len());
            for (node, file_id) in nodes {
                tx.execute(
                    "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end, file_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        node.parent_id,
                        node.kind,
                        node.byte_start as i64,
                        node.byte_end as i64,
                        file_id,
                    ],
                )?;
                ids.push(tx.last_insert_rowid());
            }

            tx.commit()?;
            Ok(ids)
        }

        fn get_ast_node(&self, node_id: i64) -> Result<Option<crate::graph::AstNode>> {
            let conn = self.conn.lock().unwrap();
            let result = conn
                .query_row(
                    "SELECT id, parent_id, kind, byte_start, byte_end
                     FROM ast_nodes WHERE id = ?1",
                    params![node_id],
                    |row| {
                        Ok(crate::graph::AstNode {
                            id: Some(row.get(0)?),
                            parent_id: row.get(1)?,
                            kind: row.get(2)?,
                            byte_start: row.get::<_, i64>(3)? as usize,
                            byte_end: row.get::<_, i64>(4)? as usize,
                        })
                    },
                )
                .optional()?;
            Ok(result)
        }

        fn update_ast_node_parent(&self, node_id: i64, new_parent_id: i64) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE ast_nodes SET parent_id = ?1 WHERE id = ?2",
                params![new_parent_id, node_id],
            )?;
            Ok(())
        }

        fn get_ast_nodes_by_file(&self, file_id: i64) -> Result<Vec<crate::graph::AstNode>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes WHERE file_id = ?1 ORDER BY byte_start",
            )?;
            let nodes = stmt
                .query_map(params![file_id], |row| {
                    Ok(crate::graph::AstNode {
                        id: Some(row.get(0)?),
                        parent_id: row.get(1)?,
                        kind: row.get(2)?,
                        byte_start: row.get::<_, i64>(3)? as usize,
                        byte_end: row.get::<_, i64>(4)? as usize,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(nodes)
        }
        
        fn get_all_ast_nodes(&self) -> Result<Vec<crate::graph::AstNode>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes ORDER BY byte_start",
            )?;
            let nodes = stmt
                .query_map([], |row| {
                    Ok(crate::graph::AstNode {
                        id: Some(row.get(0)?),
                        parent_id: row.get(1)?,
                        kind: row.get(2)?,
                        byte_start: row.get::<_, i64>(3)? as usize,
                        byte_end: row.get::<_, i64>(4)? as usize,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(nodes)
        }
        
        fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<crate::graph::AstNode>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes WHERE kind = ?1 ORDER BY byte_start",
            )?;
            let nodes = stmt
                .query_map(params![kind], |row| {
                    Ok(crate::graph::AstNode {
                        id: Some(row.get(0)?),
                        parent_id: row.get(1)?,
                        kind: row.get(2)?,
                        byte_start: row.get::<_, i64>(3)? as usize,
                        byte_end: row.get::<_, i64>(4)? as usize,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(nodes)
        }
        
        fn get_ast_children(&self, parent_id: i64) -> Result<Vec<crate::graph::AstNode>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes WHERE parent_id = ?1 ORDER BY byte_start",
            )?;
            let nodes = stmt
                .query_map(params![parent_id], |row| {
                    Ok(crate::graph::AstNode {
                        id: Some(row.get(0)?),
                        parent_id: row.get(1)?,
                        kind: row.get(2)?,
                        byte_start: row.get::<_, i64>(3)? as usize,
                        byte_end: row.get::<_, i64>(4)? as usize,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(nodes)
        }
        
        fn count_ast_nodes(&self) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM ast_nodes", [], |row| row.get(0))?;
            Ok(count as usize)
        }
        
        fn count_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM ast_nodes WHERE file_id = ?1",
                    params![file_id],
                    |row| row.get(0),
                )?;
            Ok(count as usize)
        }
        
        fn delete_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let affected = conn.execute(
                "DELETE FROM ast_nodes WHERE file_id = ?1",
                params![file_id],
            )?;
            Ok(affected)
        }

        // ===== Cross-File Reference Methods =====

        fn store_cross_file_ref(&self, cref: &crate::graph::schema::CrossFileRef) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO cross_file_refs
                    (from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    cref.from_symbol_id,
                    cref.to_symbol_id,
                    cref.file_path,
                    cref.line_number as i64,
                    cref.byte_start as i64,
                    cref.byte_end as i64,
                ],
            )?;
            Ok(())
        }

        fn get_references_to(&self, to_symbol_id: &str) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end
                 FROM cross_file_refs WHERE to_symbol_id = ?1"
            )?;
            let rows = stmt.query_map(params![to_symbol_id], |row| {
                Ok(crate::graph::schema::CrossFileRef {
                    from_symbol_id: row.get(0)?,
                    to_symbol_id: row.get(1)?,
                    file_path: row.get(2)?,
                    line_number: row.get::<_, i64>(3)? as usize,
                    byte_start: row.get::<_, i64>(4)? as usize,
                    byte_end: row.get::<_, i64>(5)? as usize,
                })
            })?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        }

        fn get_references_from(&self, from_symbol_id: &str) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end
                 FROM cross_file_refs WHERE from_symbol_id = ?1"
            )?;
            let rows = stmt.query_map(params![from_symbol_id], |row| {
                Ok(crate::graph::schema::CrossFileRef {
                    from_symbol_id: row.get(0)?,
                    to_symbol_id: row.get(1)?,
                    file_path: row.get(2)?,
                    line_number: row.get::<_, i64>(3)? as usize,
                    byte_start: row.get::<_, i64>(4)? as usize,
                    byte_end: row.get::<_, i64>(5)? as usize,
                })
            })?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        }

        fn delete_cross_file_refs_for_file(&self, file_path: &str) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let affected = conn.execute(
                "DELETE FROM cross_file_refs WHERE file_path = ?1",
                params![file_path],
            )?;
            Ok(affected)
        }

        fn count_cross_file_refs(&self) -> Result<usize> {
            let conn = self.conn.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM cross_file_refs",
                [],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
}

// =============================================================================
// V3 KV Implementation
// =============================================================================

#[cfg(feature = "native-v3")]
pub mod v3_impl {
    use super::*;
    use sqlitegraph::backend::native::v3::{V3Backend, KvValue};
    use sqlitegraph::SnapshotId;

    /// V3 KV-based side tables implementation (NO SQLITE!)
    pub struct V3SideTables {
        backend: Arc<V3Backend>,
    }

    impl V3SideTables {
        /// Create V3 side tables using KV store
        pub fn new(backend: Arc<V3Backend>) -> Self {
            Self { backend }
        }

        /// Key format: exec:{execution_id}
        fn execution_key(execution_id: &str) -> Vec<u8> {
            format!("exec:{}", execution_id).into_bytes()
        }

        /// Key format: file_metrics:{file_path}
        fn file_metrics_key(file_path: &str) -> Vec<u8> {
            format!("file_metrics:{}", file_path).into_bytes()
        }

        /// Key format: symbol_metrics:{symbol_id}
        fn symbol_metrics_key(symbol_id: i64) -> Vec<u8> {
            format!("symbol_metrics:{}", symbol_id).into_bytes()
        }
        
        /// Key format: chunk:{chunk_id}
        fn chunk_key(chunk_id: i64) -> Vec<u8> {
            format!("chunk:{}", chunk_id).into_bytes()
        }

        // ===== V3-Exclusive KV Operations =====
        // These methods are NOT part of the SideTables trait but are
        // available only on V3SideTables for llmgrep integration.

        /// KV prefix scan - returns all keys starting with prefix
        /// 
        /// # Arguments
        /// * `prefix` - Key prefix to search for
        /// 
        /// # Returns
        /// Vector of (key, value) tuples where key starts with prefix
        pub fn kv_prefix_scan(&self, prefix: &[u8]) -> Vec<(Vec<u8>, KvValue)> {
            let snapshot = SnapshotId::current();
            self.backend.kv_prefix_scan_v3(snapshot, prefix)
        }

        /// KV get - retrieve value by exact key
        /// 
        /// # Arguments
        /// * `key` - Exact key to lookup
        /// 
        /// # Returns
        /// Some(value) if key exists, None otherwise
        pub fn kv_get(&self, key: &[u8]) -> Option<KvValue> {
            let snapshot = SnapshotId::current();
            self.backend.kv_get_v3(snapshot, key)
        }

        /// Symbol lookup by FQN using KV store
        /// 
        /// Key format: `sym:fqn:{fqn}`
        /// 
        /// # Arguments
        /// * `fqn` - Fully qualified name to lookup
        /// 
        /// # Returns
        /// Some(symbol_id) if found, None otherwise
        pub fn lookup_symbol_by_fqn(&self, fqn: &str) -> Option<i64> {
            let key = format!("sym:fqn:{}", fqn);
            match self.kv_get(key.as_bytes()) {
                Some(KvValue::Integer(id)) => Some(id),
                Some(KvValue::Bytes(bytes)) => {
                    // Try to parse as i64 from bytes
                    if bytes.len() == 8 {
                        Some(i64::from_le_bytes([
                            bytes[0], bytes[1], bytes[2], bytes[3],
                            bytes[4], bytes[5], bytes[6], bytes[7],
                        ]))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }

        /// Get all symbols with a given label
        /// 
        /// Key format: `label:{label}:{symbol_id}`
        /// 
        /// # Arguments
        /// * `label` - Label name (e.g., "test", "entry_point")
        /// 
        /// # Returns
        /// Vector of symbol IDs that have this label
        pub fn get_symbols_by_label(&self, label: &str) -> Vec<i64> {
            let prefix = format!("label:{label}:");
            let results = self.kv_prefix_scan(prefix.as_bytes());
            
            results
                .into_iter()
                .filter_map(|(_, value)| {
                    // Parse symbol_id from the end of the key or value
                    match value {
                        KvValue::Integer(id) => Some(id),
                        _ => None,
                    }
                })
                .collect()
        }

        /// FQN completion - get all FQNs starting with prefix
        /// 
        /// Key format: `sym:fqn:{fqn}`
        /// 
        /// # Arguments
        /// * `prefix` - FQN prefix to complete
        /// * `limit` - Maximum number of results
        /// 
        /// # Returns
        /// Vector of matching FQNs
        pub fn complete_fqn(&self, prefix: &str, limit: usize) -> Vec<String> {
            let key_prefix = format!("sym:fqn:{}", prefix);
            let results = self.kv_prefix_scan(key_prefix.as_bytes());
            
            results
                .into_iter()
                .take(limit)
                .filter_map(|(key, _)| {
                    // Strip "sym:fqn:" prefix from key
                    String::from_utf8(key)
                        .ok()
                        .map(|s| s.strip_prefix("sym:fqn:").map(String::from))
                        .flatten()
                })
                .collect()
        }
    }

    impl SideTables for V3SideTables {
        fn start_execution(
            &self,
            execution_id: &str,
            tool_version: &str,
            args: &[String],
            root: Option<&str>,
            db_path: &str,
        ) -> Result<i64> {
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            let record = ExecutionRecord {
                id: 0, // Will be assigned by storage
                execution_id: execution_id.to_string(),
                tool_version: tool_version.to_string(),
                args: serde_json::to_string(args)?,
                root: root.map(|s| s.to_string()),
                db_path: db_path.to_string(),
                started_at,
                finished_at: None,
                duration_ms: None,
                outcome: "running".to_string(),
                error_message: None,
                files_indexed: 0,
                symbols_indexed: 0,
                references_indexed: 0,
            };

            let data = serde_json::to_vec(&record)?;
            let key = Self::execution_key(execution_id);
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(started_at) // Return timestamp as ID
        }

        fn finish_execution(
            &self,
            execution_id: &str,
            outcome: &str,
            error_message: Option<&str>,
            files_indexed: usize,
            symbols_indexed: usize,
            references_indexed: usize,
        ) -> Result<()> {
            let key = Self::execution_key(execution_id);
            let snapshot = SnapshotId::current();

            // Get existing record
            let mut record = match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    serde_json::from_slice::<ExecutionRecord>(&data)?
                }
                _ => {
                    // No existing record, create a minimal one
                    ExecutionRecord {
                        id: 0,
                        execution_id: execution_id.to_string(),
                        tool_version: String::new(),
                        args: String::new(),
                        root: None,
                        db_path: String::new(),
                        started_at: 0,
                        finished_at: None,
                        duration_ms: None,
                        outcome: String::new(),
                        error_message: None,
                        files_indexed: 0,
                        symbols_indexed: 0,
                        references_indexed: 0,
                    }
                }
            };

            // Update fields
            let finished_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            let started_at = record.started_at;
            let duration_ms = if started_at > 0 {
                (finished_at - started_at) * 1000
            } else {
                0
            };

            record.finished_at = Some(finished_at);
            record.outcome = outcome.to_string();
            record.error_message = error_message.map(|s| s.to_string());
            record.duration_ms = Some(duration_ms);
            record.files_indexed = files_indexed as i64;
            record.symbols_indexed = symbols_indexed as i64;
            record.references_indexed = references_indexed as i64;

            // Store updated record
            let data = serde_json::to_vec(&record)?;
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(())
        }

        fn get_execution(&self, execution_id: &str) -> Result<Option<ExecutionRecord>> {
            let key = Self::execution_key(execution_id);
            let snapshot = SnapshotId::current();

            match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    let record: ExecutionRecord = serde_json::from_slice(&data)?;
                    Ok(Some(record))
                }
                _ => Ok(None),
            }
        }

        fn list_executions(&self, limit: Option<usize>) -> Result<Vec<ExecutionRecord>> {
            let snapshot = SnapshotId::current();
            let prefix = b"exec:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            let mut executions: Vec<ExecutionRecord> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<ExecutionRecord>(&data).ok()
                    } else {
                        None
                    }
                })
                .collect();
            
            // Sort by started_at descending (most recent first)
            executions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
            
            if let Some(limit) = limit {
                executions.truncate(limit);
            }
            
            Ok(executions)
        }

        fn store_file_metrics(&self, metrics: &FileMetrics) -> Result<()> {
            let data = serde_json::to_vec(metrics)?;
            let key = Self::file_metrics_key(&metrics.file_path);
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(())
        }

        fn get_file_metrics(&self, file_path: &str) -> Result<Option<FileMetrics>> {
            let key = Self::file_metrics_key(file_path);
            let snapshot = SnapshotId::current();

            match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    let metrics: FileMetrics = serde_json::from_slice(&data)?;
                    Ok(Some(metrics))
                }
                _ => Ok(None),
            }
        }

        fn store_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()> {
            let data = serde_json::to_vec(metrics)?;
            let key = Self::symbol_metrics_key(metrics.symbol_id);
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(())
        }

        fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>> {
            let key = Self::symbol_metrics_key(symbol_id);
            let snapshot = SnapshotId::current();

            match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    let metrics: SymbolMetrics = serde_json::from_slice(&data)?;
                    Ok(Some(metrics))
                }
                _ => Ok(None),
            }
        }

        fn delete_metrics_for_file(&self, file_path: &str) -> Result<usize> {
            // Delete file metrics
            let file_key = Self::file_metrics_key(file_path);
            self.backend.kv_delete_v3(&file_key);
            
            // Delete symbol metrics for this file using prefix scan
            let snapshot = SnapshotId::current();
            let prefix = format!("symbol_metrics:file:{}", file_path).into_bytes();
            let symbols = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            
            let mut count = 0;
            for (key, _) in symbols {
                self.backend.kv_delete_v3(&key);
                count += 1;
            }
            
            Ok(count)
        }

        fn get_hotspots(
            &self,
            limit: Option<u32>,
            min_loc: Option<i64>,
            min_fan_in: Option<i64>,
            min_fan_out: Option<i64>,
        ) -> Result<Vec<FileMetrics>> {
            let snapshot = SnapshotId::current();
            let prefix = b"file_metrics:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            let mut metrics: Vec<FileMetrics> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<FileMetrics>(&data).ok()
                    } else {
                        None
                    }
                })
                .filter(|m| {
                    min_loc.map_or(true, |min| m.loc >= min)
                        && min_fan_in.map_or(true, |min| m.fan_in >= min)
                        && min_fan_out.map_or(true, |min| m.fan_out >= min)
                })
                .collect();
            
            // Sort by complexity score descending
            metrics.sort_by(|a, b| {
                b.complexity_score
                    .partial_cmp(&a.complexity_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            
            if let Some(limit) = limit {
                metrics.truncate(limit as usize);
            }
            
            Ok(metrics)
        }
        
        // ===== Code Chunk Methods =====
        
        fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64> {
            let chunk_id = chunk.id.unwrap_or_else(|| {
                // Generate ID from timestamp + random component
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                let random = std::process::id() as i64;
                (now << 16) | (random & 0xFFFF)
            });
            
            let key = Self::chunk_key(chunk_id);
            let data = serde_json::to_vec(chunk)?;
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(chunk_id)
        }
        
        fn get_chunk(&self, chunk_id: i64) -> Result<Option<CodeChunk>> {
            let key = Self::chunk_key(chunk_id);
            let snapshot = SnapshotId::current();
            
            match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    let chunk: CodeChunk = serde_json::from_slice(&data)?;
                    Ok(Some(chunk))
                }
                _ => Ok(None),
            }
        }
        
        fn get_chunk_by_span(
            &self,
            file_path: &str,
            byte_start: usize,
            byte_end: usize,
        ) -> Result<Option<CodeChunk>> {
            // For V3, we need to scan all chunks to find matching span
            // This is inefficient but works for now
            let all_chunks = self.get_all_chunks()?;
            Ok(all_chunks.into_iter().find(|c| {
                c.file_path == file_path && c.byte_start == byte_start && c.byte_end == byte_end
            }))
        }
        
        fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
            let all_chunks = self.get_all_chunks()?;
            Ok(all_chunks.into_iter().filter(|c| c.file_path == file_path).collect())
        }
        
        fn count_chunks_for_file(&self, file_path: &str) -> Result<usize> {
            self.get_chunks_for_file(file_path).map(|v| v.len())
        }
        
        fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize> {
            let chunks = self.get_chunks_for_file(file_path)?;
            let count = chunks.len();
            
            for chunk in chunks {
                if let Some(id) = chunk.id {
                    let key = Self::chunk_key(id);
                    self.backend.kv_delete_v3(&key);
                }
            }
            
            Ok(count)
        }
        
        fn get_chunks_by_symbol(&self, file_path: &str, symbol_name: &str) -> Result<Vec<CodeChunk>> {
            let all_chunks = self.get_all_chunks()?;
            Ok(all_chunks
                .into_iter()
                .filter(|c| {
                    c.file_path == file_path && 
                    c.symbol_name.as_ref() == Some(&symbol_name.to_string())
                })
                .collect())
        }
        
        fn get_all_chunks(&self) -> Result<Vec<CodeChunk>> {
            let snapshot = SnapshotId::current();
            let prefix = b"chunk:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            let chunks: Vec<CodeChunk> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<CodeChunk>(&data).ok()
                    } else {
                        None
                    }
                })
                .collect();
            
            Ok(chunks)
        }
        
        fn count_chunks(&self) -> Result<usize> {
            // Use prefix scan to count all chunk keys
            let snapshot = SnapshotId::current();
            let prefix = b"chunk:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            Ok(results.len())
        }
        
        // ===== AST Node Methods =====

        fn store_ast_node(&self, node: &crate::graph::AstNode, file_id: i64) -> Result<i64> {
            let node_id = node.id.unwrap_or_else(|| {
                // Generate unique ID
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                let random = std::process::id() as i64;
                (now << 16) | (random & 0xFFFF)
            });

            // Store with file_id prefix for file-based queries
            let file_key = format!("ast:file:{}:{}", file_id, node_id);
            let kind_key = format!("ast:kind:{}:{}", node.kind, node_id);

            let data = serde_json::to_vec(node)?;

            self.backend.kv_set_v3(file_key.into_bytes(), KvValue::Bytes(data.clone()), None);
            self.backend.kv_set_v3(kind_key.into_bytes(), KvValue::Bytes(data), None);

            Ok(node_id)
        }

        fn store_ast_nodes_batch(&self, nodes: &[(crate::graph::AstNode, i64)]) -> Result<Vec<i64>> {
            if nodes.is_empty() {
                return Ok(Vec::new());
            }

            let mut ids = Vec::with_capacity(nodes.len());

            for (node, file_id) in nodes {
                let node_id = node.id.unwrap_or_else(|| {
                    // Generate unique ID
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;
                    let random = std::process::id() as i64;
                    (now << 16) | (random & 0xFFFF)
                });

                // Store with file_id prefix for file-based queries
                let file_key = format!("ast:file:{}:{}", file_id, node_id);
                let kind_key = format!("ast:kind:{}:{}", node.kind, node_id);

                let data = serde_json::to_vec(node)?;

                self.backend.kv_set_v3(file_key.into_bytes(), KvValue::Bytes(data.clone()), None);
                self.backend.kv_set_v3(kind_key.into_bytes(), KvValue::Bytes(data), None);

                ids.push(node_id);
            }

            Ok(ids)
        }

        fn get_ast_node(&self, node_id: i64) -> Result<Option<crate::graph::AstNode>> {
            // Need to scan all AST nodes to find matching ID
            let snapshot = SnapshotId::current();
            let prefix = b"ast:file:";

            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            for (_, value) in results {
                if let KvValue::Bytes(data) = value {
                    if let Ok(node) = serde_json::from_slice::<crate::graph::AstNode>(&data) {
                        if node.id == Some(node_id) {
                            return Ok(Some(node));
                        }
                    }
                }
            }

            Ok(None)
        }

        fn update_ast_node_parent(&self, node_id: i64, new_parent_id: i64) -> Result<()> {
            // V3 doesn't support in-place updates, so we need to:
            // 1. Find the node and its file_id
            // 2. Delete old KV entries
            // 3. Update parent_id
            // 4. Re-insert with updated data

            let snapshot = SnapshotId::current();
            let prefix = b"ast:file:";

            // Scan to find the node and extract file_id from the key
            let mut found_node: Option<crate::graph::AstNode> = None;
            let mut file_id: Option<i64> = None;
            let mut kind: Option<String> = None;

            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            for (key, value) in results {
                if let KvValue::Bytes(data) = value {
                    if let Ok(node) = serde_json::from_slice::<crate::graph::AstNode>(&data) {
                        if node.id == Some(node_id) {
                            // Extract file_id from key format: "ast:file:{file_id}:{node_id}"
                            let key_str = String::from_utf8_lossy(&key);
                            if let Some(rest) = key_str.strip_prefix("ast:file:") {
                                if let Some(node_id_str) = rest.split(':').nth(1) {
                                    if node_id_str.parse::<i64>() == Ok(node_id) {
                                        if let Some(file_str) = rest.split(':').next() {
                                            file_id = file_str.parse::<i64>().ok();
                                            kind = node.kind.clone().into();
                                        }
                                    }
                                }
                            }
                            found_node = Some(node);
                            break;
                        }
                    }
                }
            }

            if let (Some(mut node), Some(fid), Some(kind_str)) = (found_node, file_id, kind) {
                // Delete old entries
                let old_file_key = format!("ast:file:{}:{}", fid, node_id);
                let old_kind_key = format!("ast:kind:{}:{}", kind_str, node_id);
                self.backend.kv_delete_v3(old_file_key.into_bytes());
                self.backend.kv_delete_v3(old_kind_key.into_bytes());

                // Update parent_id and re-insert
                node.parent_id = Some(new_parent_id);

                let file_key = format!("ast:file:{}:{}", fid, node_id);
                let kind_key = format!("ast:kind:{}:{}", kind_str, node_id);
                let data = serde_json::to_vec(&node)?;

                self.backend.kv_set_v3(file_key.into_bytes(), KvValue::Bytes(data.clone()), None);
                self.backend.kv_set_v3(kind_key.into_bytes(), KvValue::Bytes(data), None);

                Ok(())
            } else {
                Err(anyhow::anyhow!("Node {} not found for parent update", node_id))
            }
        }

        fn get_ast_nodes_by_file(&self, file_id: i64) -> Result<Vec<crate::graph::AstNode>> {
            let snapshot = SnapshotId::current();
            let prefix = format!("ast:file:{}", file_id).into_bytes();
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            let mut nodes: Vec<crate::graph::AstNode> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<crate::graph::AstNode>(&data).ok()
                    } else {
                        None
                    }
                })
                .collect();
            
            // Sort by byte_start for consistent ordering
            nodes.sort_by_key(|n| n.byte_start);
            
            Ok(nodes)
        }
        
        fn get_all_ast_nodes(&self) -> Result<Vec<crate::graph::AstNode>> {
            let snapshot = SnapshotId::current();
            let prefix = b"ast:file:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            let mut nodes: Vec<crate::graph::AstNode> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<crate::graph::AstNode>(&data).ok()
                    } else {
                        None
                    }
                })
                .collect();
            
            // Sort by byte_start for consistent ordering
            nodes.sort_by_key(|n| n.byte_start);
            
            Ok(nodes)
        }
        
        fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<crate::graph::AstNode>> {
            let snapshot = SnapshotId::current();
            let prefix = format!("ast:kind:{}", kind).into_bytes();
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            let mut nodes: Vec<crate::graph::AstNode> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<crate::graph::AstNode>(&data).ok()
                    } else {
                        None
                    }
                })
                .collect();
            
            // Sort by byte_start for consistent ordering
            nodes.sort_by_key(|n| n.byte_start);
            
            Ok(nodes)
        }
        
        fn get_ast_children(&self, parent_id: i64) -> Result<Vec<crate::graph::AstNode>> {
            // Need to scan all AST nodes and filter by parent_id
            let snapshot = SnapshotId::current();
            let prefix = b"ast:file:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            let mut nodes: Vec<crate::graph::AstNode> = results
                .into_iter()
                .filter_map(|(_, value)| {
                    if let KvValue::Bytes(data) = value {
                        serde_json::from_slice::<crate::graph::AstNode>(&data).ok()
                    } else {
                        None
                    }
                })
                .filter(|n| n.parent_id == Some(parent_id))
                .collect();
            
            // Sort by byte_start for consistent ordering
            nodes.sort_by_key(|n| n.byte_start);
            
            Ok(nodes)
        }
        
        fn count_ast_nodes(&self) -> Result<usize> {
            let snapshot = SnapshotId::current();
            let prefix = b"ast:file:";
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            Ok(results.len())
        }
        
        fn count_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
            let snapshot = SnapshotId::current();
            let prefix = format!("ast:file:{}", file_id).into_bytes();
            
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            Ok(results.len())
        }
        
        fn delete_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
            let snapshot = SnapshotId::current();
            let prefix = format!("ast:file:{}", file_id).into_bytes();
            
            // Find all nodes for this file
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            let mut count = 0;
            
            for (key, value) in results {
                // Delete the file-indexed entry
                self.backend.kv_delete_v3(&key);
                
                // Also delete the kind-indexed entry if we can parse the node
                if let KvValue::Bytes(data) = value {
                    if let Ok(node) = serde_json::from_slice::<crate::graph::AstNode>(&data) {
                        let kind_key = format!("ast:kind:{}:{}", node.kind, node.id.unwrap_or(0)).into_bytes();
                        self.backend.kv_delete_v3(&kind_key);
                    }
                }
                count += 1;
            }
            
            Ok(count)
        }

        // ===== Cross-File Reference Methods =====

        fn store_cross_file_ref(&self, cref: &crate::graph::schema::CrossFileRef) -> Result<()> {
            // Key format: cref:to:{to_symbol_id}:{from_symbol_id}
            let key = format!("cref:to:{}:{}", cref.to_symbol_id, cref.from_symbol_id).into_bytes();
            let value = serde_json::to_vec(cref)?;
            self.backend.kv_set_v3(key, KvValue::Bytes(value), None);
            
            // Also store reverse lookup: cref:from:{from_symbol_id}:{to_symbol_id}
            let reverse_key = format!("cref:from:{}:{}", cref.from_symbol_id, cref.to_symbol_id).into_bytes();
            self.backend.kv_set_v3(reverse_key, KvValue::Bytes(vec![]), None);
            
            // Store file index: cref:file:{file_path}:{to_symbol_id}:{from_symbol_id}
            let file_key = format!("cref:file:{}:{}:{}", cref.file_path, cref.to_symbol_id, cref.from_symbol_id).into_bytes();
            self.backend.kv_set_v3(file_key, KvValue::Bytes(vec![]), None);
            
            Ok(())
        }

        fn get_references_to(&self, to_symbol_id: &str) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
            let snapshot = SnapshotId::current();
            let prefix = format!("cref:to:{to_symbol_id}:").into_bytes();
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            
            let mut refs = Vec::new();
            for (_, value) in results {
                if let KvValue::Bytes(data) = value {
                    if let Ok(cref) = serde_json::from_slice::<crate::graph::schema::CrossFileRef>(&data) {
                        refs.push(cref);
                    }
                }
            }
            Ok(refs)
        }

        fn get_references_from(&self, from_symbol_id: &str) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
            let snapshot = SnapshotId::current();
            let prefix = format!("cref:from:{from_symbol_id}:").into_bytes();
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            
            let mut refs = Vec::new();
            for (key, _) in results {
                // Extract to_symbol_id from key: cref:from:{from_symbol_id}:{to_symbol_id}
                let key_str = String::from_utf8_lossy(&key);
                if let Some(to_symbol_id) = key_str.split(':').nth(3) {
                    // Now lookup the full reference using to: lookup
                    let to_refs = self.get_references_to(to_symbol_id)?;
                    for cref in to_refs {
                        if cref.from_symbol_id == from_symbol_id {
                            refs.push(cref);
                        }
                    }
                }
            }
            Ok(refs)
        }

        fn delete_cross_file_refs_for_file(&self, file_path: &str) -> Result<usize> {
            let snapshot = SnapshotId::current();
            let prefix = format!("cref:file:{file_path}:").into_bytes();
            let results = self.backend.kv_prefix_scan_v3(snapshot, &prefix);
            
            let mut count = 0;
            for (file_key, _) in results {
                // Parse file key: cref:file:{file_path}:{to_symbol_id}:{from_symbol_id}
                let key_str = String::from_utf8_lossy(&file_key);
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() >= 5 {
                    let to_symbol_id = parts[3];
                    let from_symbol_id = parts[4];
                    
                    // Delete the to: entry (contains full data)
                    let to_key = format!("cref:to:{to_symbol_id}:{from_symbol_id}").into_bytes();
                    self.backend.kv_delete_v3(&to_key);
                    
                    // Delete the from: entry
                    let from_key = format!("cref:from:{from_symbol_id}:{to_symbol_id}").into_bytes();
                    self.backend.kv_delete_v3(&from_key);
                    
                    // Delete the file: entry
                    self.backend.kv_delete_v3(&file_key);
                    
                    count += 1;
                }
            }
            Ok(count)
        }

        fn count_cross_file_refs(&self) -> Result<usize> {
            let snapshot = SnapshotId::current();
            let prefix = b"cref:to:";
            let results = self.backend.kv_prefix_scan_v3(snapshot, prefix);
            Ok(results.len())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
}

// =============================================================================
// Re-exports
// =============================================================================

#[cfg(feature = "sqlite-backend")]
pub use sqlite_impl::SqliteSideTables;

#[cfg(feature = "native-v3")]
pub use v3_impl::V3SideTables;

/// Create appropriate side tables for the selected backend
#[cfg(feature = "sqlite-backend")]
pub fn create_side_tables(db_path: &Path) -> Result<Box<dyn SideTables>> {
    Ok(Box::new(SqliteSideTables::open(db_path)?))
}

/// Create V3 side tables (uses KV store, no SQLite!)
#[cfg(feature = "native-v3")]
pub fn create_side_tables_v3(backend: Arc<sqlitegraph::backend::native::v3::V3Backend>) -> Result<Box<dyn SideTables>> {
    Ok(Box::new(V3SideTables::new(backend)))
}

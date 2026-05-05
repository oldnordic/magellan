//! Side table storage abstraction for Magellan
//!
//! This module provides backend-specific side table implementations:
//! - `SqliteSideTables`: Uses SQLite for all side tables
//!
//! # Design Philosophy
//!
//! Each backend is **fully self-contained**:
//! - SQLite backend: Everything in `.db` file
//! No mixing between backends for optimal performance.

use anyhow::Result;
use std::path::Path;

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
    fn get_references_to(
        &self,
        to_symbol_id: &str,
    ) -> Result<Vec<crate::graph::schema::CrossFileRef>>;

    /// Get all references from a specific symbol (by source symbol ID)
    ///
    /// # Arguments
    /// * `from_symbol_id` - The source symbol ID
    ///
    /// # Returns
    /// Vector of cross-file references where `from_symbol_id` is the source
    fn get_references_from(
        &self,
        from_symbol_id: &str,
    ) -> Result<Vec<crate::graph::schema::CrossFileRef>>;

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

    // ===== Label Methods =====

    /// Add a label to an entity
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID to label
    /// * `label` - The label to add
    fn add_label(&self, entity_id: i64, label: &str) -> Result<()>;

    /// Get all labels for an entity
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID
    ///
    /// # Returns
    /// Vector of labels for the entity
    fn get_labels_for_entity(&self, entity_id: i64) -> Result<Vec<String>>;

    /// Get all entities with a specific label
    ///
    /// # Arguments
    /// * `label` - The label to query
    ///
    /// # Returns
    /// Vector of entity IDs with this label
    fn get_entities_by_label(&self, label: &str) -> Result<Vec<i64>>;

    /// Get all labels in use
    ///
    /// # Returns
    /// Vector of all distinct labels
    fn get_all_labels(&self) -> Result<Vec<String>>;

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
    use rusqlite::{params, Connection, OptionalExtension};
    use std::sync::{Arc, Mutex};

    /// SQLite-based side tables implementation
    pub struct SqliteSideTables {
        conn: Arc<Mutex<Connection>>,
    }

    impl SqliteSideTables {
        /// Lock the shared connection, recovering from poison if necessary.
        fn lock_conn(&self) -> std::sync::MutexGuard<'_, Connection> {
            match self.conn.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            }
        }

        /// Open or create side tables in SQLite database
        pub fn open(db_path: &Path) -> Result<Self> {
            let conn = Connection::open(db_path)?;
            Self::with_mutex(Arc::new(Mutex::new(conn)))
        }

        /// Create from an existing connection reference-counted mutex.
        ///
        /// This allows `CodeGraph` to share its `side_conn` with `SqliteSideTables`,
        /// eliminating redundant connection opens.
        pub fn with_mutex(conn: Arc<Mutex<Connection>>) -> Result<Self> {
            let tables = Self { conn };
            tables.ensure_schema()?;
            Ok(tables)
        }

        fn ensure_schema(&self) -> Result<()> {
            let conn = self.lock_conn();

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
            let conn = self.lock_conn();
            let args_json = serde_json::to_string(args)?;
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO execution_log
                    (execution_id, tool_version, args, root, db_path, started_at, outcome)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running')",
                params![
                    execution_id,
                    tool_version,
                    args_json,
                    root,
                    db_path,
                    started_at
                ],
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
            let conn = self.lock_conn();
            let finished_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
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
            let conn = self.lock_conn();

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
            let conn = self.lock_conn();

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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();

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
            let conn = self.lock_conn();

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
            query.push_str(&format!(
                " ORDER BY complexity_score DESC LIMIT ?{param_count}"
            ));

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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM code_chunks WHERE file_path = ?1",
                params![file_path],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        }

        fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize> {
            let conn = self.lock_conn();
            let affected = conn.execute(
                "DELETE FROM code_chunks WHERE file_path = ?1",
                params![file_path],
            )?;
            Ok(affected)
        }

        fn get_chunks_by_symbol(
            &self,
            file_path: &str,
            symbol_name: &str,
        ) -> Result<Vec<CodeChunk>> {
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM code_chunks", [], |row| row.get(0))?;
            Ok(count as usize)
        }

        // ===== AST Node Methods =====

        fn store_ast_node(&self, node: &crate::graph::AstNode, file_id: i64) -> Result<i64> {
            let conn = self.lock_conn();
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

        fn store_ast_nodes_batch(
            &self,
            nodes: &[(crate::graph::AstNode, i64)],
        ) -> Result<Vec<i64>> {
            if nodes.is_empty() {
                return Ok(Vec::new());
            }

            let mut conn = self.lock_conn();

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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
            conn.execute(
                "UPDATE ast_nodes SET parent_id = ?1 WHERE id = ?2",
                params![new_parent_id, node_id],
            )?;
            Ok(())
        }

        fn get_ast_nodes_by_file(&self, file_id: i64) -> Result<Vec<crate::graph::AstNode>> {
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
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
            let conn = self.lock_conn();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM ast_nodes", [], |row| row.get(0))?;
            Ok(count as usize)
        }

        fn count_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
            let conn = self.lock_conn();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM ast_nodes WHERE file_id = ?1",
                params![file_id],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        }

        fn delete_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
            let conn = self.lock_conn();
            let affected =
                conn.execute("DELETE FROM ast_nodes WHERE file_id = ?1", params![file_id])?;
            Ok(affected)
        }

        // ===== Cross-File Reference Methods =====

        fn store_cross_file_ref(&self, cref: &crate::graph::schema::CrossFileRef) -> Result<()> {
            let conn = self.lock_conn();
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

        fn get_references_to(
            &self,
            to_symbol_id: &str,
        ) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
            let conn = self.lock_conn();
            let mut stmt = conn.prepare(
                "SELECT from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end
                 FROM cross_file_refs WHERE to_symbol_id = ?1",
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

        fn get_references_from(
            &self,
            from_symbol_id: &str,
        ) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
            let conn = self.lock_conn();
            let mut stmt = conn.prepare(
                "SELECT from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end
                 FROM cross_file_refs WHERE from_symbol_id = ?1",
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
            let conn = self.lock_conn();
            let affected = conn.execute(
                "DELETE FROM cross_file_refs WHERE file_path = ?1",
                params![file_path],
            )?;
            Ok(affected)
        }

        fn count_cross_file_refs(&self) -> Result<usize> {
            let conn = self.lock_conn();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM cross_file_refs", [], |row| row.get(0))?;
            Ok(count as usize)
        }

        // ===== Label Methods =====

        fn add_label(&self, entity_id: i64, label: &str) -> Result<()> {
            let conn = self.lock_conn();
            conn.execute(
                "INSERT OR IGNORE INTO graph_labels(entity_id, label) VALUES(?1, ?2)",
                params![entity_id, label],
            )?;
            Ok(())
        }

        fn get_labels_for_entity(&self, entity_id: i64) -> Result<Vec<String>> {
            let conn = self.lock_conn();
            let mut stmt =
                conn.prepare("SELECT label FROM graph_labels WHERE entity_id = ?1 ORDER BY label")?;
            let labels = stmt
                .query_map(params![entity_id], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(labels)
        }

        fn get_entities_by_label(&self, label: &str) -> Result<Vec<i64>> {
            let conn = self.lock_conn();
            let mut stmt = conn.prepare(
                "SELECT entity_id FROM graph_labels WHERE label = ?1 ORDER BY entity_id",
            )?;
            let entities = stmt
                .query_map(params![label], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(entities)
        }

        fn get_all_labels(&self) -> Result<Vec<String>> {
            let conn = self.lock_conn();
            let mut stmt =
                conn.prepare("SELECT DISTINCT label FROM graph_labels ORDER BY label")?;
            let labels = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(labels)
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

/// Create appropriate side tables for the selected backend
#[cfg(feature = "sqlite-backend")]
pub fn create_side_tables(db_path: &Path) -> Result<Box<dyn SideTables>> {
    Ok(Box::new(SqliteSideTables::open(db_path)?))
}

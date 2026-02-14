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

        fn list_executions(&self, _limit: Option<usize>) -> Result<Vec<ExecutionRecord>> {
            // V3 doesn't have kv_scan, so we can't efficiently list all executions
            // For now, return empty (would need to implement prefix scan in sqlitegraph)
            Ok(vec![])
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
            
            // Note: Symbol metrics for this file would need a scan to find
            // For now, we just return 0 since V3 doesn't have prefix scan
            Ok(0)
        }

        fn get_hotspots(
            &self,
            _limit: Option<u32>,
            _min_loc: Option<i64>,
            _min_fan_in: Option<i64>,
            _min_fan_out: Option<i64>,
        ) -> Result<Vec<FileMetrics>> {
            // V3 doesn't have kv_scan, so we can't efficiently list all files
            // For now, return empty (would need to implement prefix scan in sqlitegraph)
            Ok(vec![])
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

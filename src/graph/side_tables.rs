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
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Side table operations trait - backend-agnostic interface
///
/// Both backends implement this trait to provide:
/// - Code chunks storage
/// - File/symbol metrics
/// - Execution logging
/// - AST nodes (future)
/// - CFG blocks (future)
pub trait SideTables: Send + Sync {
    /// Store a code chunk
    fn store_chunk(&self, file_id: i64, chunk_id: &str, content: &str, start_line: i32, end_line: i32) -> Result<()>;
    
    /// Retrieve a code chunk
    fn get_chunk(&self, file_id: i64, chunk_id: &str) -> Result<Option<CodeChunk>>;
    
    /// Store file metrics
    fn store_file_metrics(&self, file_id: i64, metrics: &FileMetrics) -> Result<()>;
    
    /// Get file metrics
    fn get_file_metrics(&self, file_id: i64) -> Result<Option<FileMetrics>>;
    
    /// Store symbol metrics
    fn store_symbol_metrics(&self, symbol_id: i64, metrics: &SymbolMetrics) -> Result<()>;
    
    /// Get symbol metrics
    fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>>;
    
    /// Log execution
    fn log_execution(&self, command: &str, duration_ms: i64, success: bool) -> Result<()>;
    
    /// Get recent execution logs
    fn get_recent_logs(&self, limit: usize) -> Result<Vec<ExecutionLogEntry>>;
    
    /// Flush any pending writes
    fn flush(&self) -> Result<()>;
}

/// Code chunk data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub file_id: i64,
    pub chunk_id: String,
    pub content: String,
    pub start_line: i32,
    pub end_line: i32,
}

/// File-level metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileMetrics {
    pub line_count: i32,
    pub code_lines: i32,
    pub comment_lines: i32,
    pub blank_lines: i32,
    pub symbol_count: i32,
    pub complexity: i32,
}

/// Symbol-level metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolMetrics {
    pub line_count: i32,
    pub cyclomatic_complexity: i32,
    pub parameter_count: i32,
    pub call_count: i32,
    pub caller_count: i32,
}

/// Execution log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLogEntry {
    pub timestamp: i64,
    pub command: String,
    pub duration_ms: i64,
    pub success: bool,
}

// =============================================================================
// SQLite Implementation
// =============================================================================

#[cfg(feature = "sqlite-backend")]
pub mod sqlite_impl {
    use super::*;
    use rusqlite::{Connection, params};
    use std::path::PathBuf;
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
            
            // Code chunks table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS code_chunks (
                    file_id INTEGER NOT NULL,
                    chunk_id TEXT NOT NULL,
                    content TEXT NOT NULL,
                    start_line INTEGER NOT NULL,
                    end_line INTEGER NOT NULL,
                    PRIMARY KEY (file_id, chunk_id)
                )",
                [],
            )?;

            // File metrics table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS file_metrics (
                    file_id INTEGER PRIMARY KEY,
                    line_count INTEGER DEFAULT 0,
                    code_lines INTEGER DEFAULT 0,
                    comment_lines INTEGER DEFAULT 0,
                    blank_lines INTEGER DEFAULT 0,
                    symbol_count INTEGER DEFAULT 0,
                    complexity INTEGER DEFAULT 0
                )",
                [],
            )?;

            // Symbol metrics table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS symbol_metrics (
                    symbol_id INTEGER PRIMARY KEY,
                    line_count INTEGER DEFAULT 0,
                    cyclomatic_complexity INTEGER DEFAULT 0,
                    parameter_count INTEGER DEFAULT 0,
                    call_count INTEGER DEFAULT 0,
                    caller_count INTEGER DEFAULT 0
                )",
                [],
            )?;

            // Execution log table
            conn.execute(
                "CREATE TABLE IF NOT EXISTS execution_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp INTEGER NOT NULL,
                    command TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    success INTEGER NOT NULL
                )",
                [],
            )?;

            // Indexes
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_file ON code_chunks(file_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_log_timestamp ON execution_log(timestamp)",
                [],
            )?;

            Ok(())
        }
    }

    impl SideTables for SqliteSideTables {
        fn store_chunk(&self, file_id: i64, chunk_id: &str, content: &str, start_line: i32, end_line: i32) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO code_chunks (file_id, chunk_id, content, start_line, end_line)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![file_id, chunk_id, content, start_line, end_line],
            )?;
            Ok(())
        }

        fn get_chunk(&self, file_id: i64, chunk_id: &str) -> Result<Option<CodeChunk>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT file_id, chunk_id, content, start_line, end_line 
                 FROM code_chunks WHERE file_id = ?1 AND chunk_id = ?2"
            )?;
            
            let chunk = stmt.query_row(params![file_id, chunk_id], |row| {
                Ok(CodeChunk {
                    file_id: row.get(0)?,
                    chunk_id: row.get(1)?,
                    content: row.get(2)?,
                    start_line: row.get(3)?,
                    end_line: row.get(4)?,
                })
            }).ok();
            
            Ok(chunk)
        }

        fn store_file_metrics(&self, file_id: i64, metrics: &FileMetrics) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO file_metrics 
                 (file_id, line_count, code_lines, comment_lines, blank_lines, symbol_count, complexity)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    file_id, metrics.line_count, metrics.code_lines, 
                    metrics.comment_lines, metrics.blank_lines, 
                    metrics.symbol_count, metrics.complexity
                ],
            )?;
            Ok(())
        }

        fn get_file_metrics(&self, file_id: i64) -> Result<Option<FileMetrics>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT line_count, code_lines, comment_lines, blank_lines, symbol_count, complexity
                 FROM file_metrics WHERE file_id = ?1"
            )?;
            
            let metrics = stmt.query_row(params![file_id], |row| {
                Ok(FileMetrics {
                    line_count: row.get(0)?,
                    code_lines: row.get(1)?,
                    comment_lines: row.get(2)?,
                    blank_lines: row.get(3)?,
                    symbol_count: row.get(4)?,
                    complexity: row.get(5)?,
                })
            }).ok();
            
            Ok(metrics)
        }

        fn store_symbol_metrics(&self, symbol_id: i64, metrics: &SymbolMetrics) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO symbol_metrics 
                 (symbol_id, line_count, cyclomatic_complexity, parameter_count, call_count, caller_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    symbol_id, metrics.line_count, metrics.cyclomatic_complexity,
                    metrics.parameter_count, metrics.call_count, metrics.caller_count
                ],
            )?;
            Ok(())
        }

        fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT line_count, cyclomatic_complexity, parameter_count, call_count, caller_count
                 FROM symbol_metrics WHERE symbol_id = ?1"
            )?;
            
            let metrics = stmt.query_row(params![symbol_id], |row| {
                Ok(SymbolMetrics {
                    line_count: row.get(0)?,
                    cyclomatic_complexity: row.get(1)?,
                    parameter_count: row.get(2)?,
                    call_count: row.get(3)?,
                    caller_count: row.get(4)?,
                })
            }).ok();
            
            Ok(metrics)
        }

        fn log_execution(&self, command: &str, duration_ms: i64, success: bool) -> Result<()> {
            let conn = self.conn.lock().unwrap();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            
            conn.execute(
                "INSERT INTO execution_log (timestamp, command, duration_ms, success)
                 VALUES (?1, ?2, ?3, ?4)",
                params![timestamp, command, duration_ms, success as i32],
            )?;
            Ok(())
        }

        fn get_recent_logs(&self, limit: usize) -> Result<Vec<ExecutionLogEntry>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT timestamp, command, duration_ms, success
                 FROM execution_log
                 ORDER BY timestamp DESC
                 LIMIT ?1"
            )?;
            
            let logs = stmt.query_map(params![limit as i64], |row| {
                Ok(ExecutionLogEntry {
                    timestamp: row.get(0)?,
                    command: row.get(1)?,
                    duration_ms: row.get(2)?,
                    success: row.get::<_, i32>(3)? != 0,
                })
            })?.collect::<Result<Vec<_>, _>>()?;
            
            Ok(logs)
        }

        fn flush(&self) -> Result<()> {
            // SQLite handles this automatically
            Ok(())
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

        /// Key format: {prefix}:{id}:{sub_id}
        fn chunk_key(file_id: i64, chunk_id: &str) -> Vec<u8> {
            format!("chunk:{}:{}", file_id, chunk_id).into_bytes()
        }

        fn file_metrics_key(file_id: i64) -> Vec<u8> {
            format!("metrics:file:{}", file_id).into_bytes()
        }

        fn symbol_metrics_key(symbol_id: i64) -> Vec<u8> {
            format!("metrics:symbol:{}", symbol_id).into_bytes()
        }

        fn log_key(timestamp: i64, seq: u64) -> Vec<u8> {
            format!("log:{}:{}", timestamp, seq).into_bytes()
        }
    }

    impl SideTables for V3SideTables {
        fn store_chunk(&self, file_id: i64, chunk_id: &str, content: &str, start_line: i32, end_line: i32) -> Result<()> {
            let chunk = CodeChunk {
                file_id,
                chunk_id: chunk_id.to_string(),
                content: content.to_string(),
                start_line,
                end_line,
            };
            
            let data = serde_json::to_vec(&chunk)?;
            let key = Self::chunk_key(file_id, chunk_id);
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(())
        }

        fn get_chunk(&self, file_id: i64, chunk_id: &str) -> Result<Option<CodeChunk>> {
            let key = Self::chunk_key(file_id, chunk_id);
            let snapshot = SnapshotId::current();
            
            match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    let chunk: CodeChunk = serde_json::from_slice(&data)?;
                    Ok(Some(chunk))
                }
                _ => Ok(None),
            }
        }

        fn store_file_metrics(&self, file_id: i64, metrics: &FileMetrics) -> Result<()> {
            let data = serde_json::to_vec(metrics)?;
            let key = Self::file_metrics_key(file_id);
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(())
        }

        fn get_file_metrics(&self, file_id: i64) -> Result<Option<FileMetrics>> {
            let key = Self::file_metrics_key(file_id);
            let snapshot = SnapshotId::current();
            
            match self.backend.kv_get_v3(snapshot, &key) {
                Some(KvValue::Bytes(data)) => {
                    let metrics: FileMetrics = serde_json::from_slice(&data)?;
                    Ok(Some(metrics))
                }
                _ => Ok(None),
            }
        }

        fn store_symbol_metrics(&self, symbol_id: i64, metrics: &SymbolMetrics) -> Result<()> {
            let data = serde_json::to_vec(metrics)?;
            let key = Self::symbol_metrics_key(symbol_id);
            
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

        fn log_execution(&self, command: &str, duration_ms: i64, success: bool) -> Result<()> {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            
            let entry = ExecutionLogEntry {
                timestamp,
                command: command.to_string(),
                duration_ms,
                success,
            };
            
            let data = serde_json::to_vec(&entry)?;
            // Use timestamp + random suffix for unique key
            let seq = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos() as u64;
            let key = Self::log_key(timestamp, seq);
            
            self.backend.kv_set_v3(key, KvValue::Bytes(data), None);
            Ok(())
        }

        fn get_recent_logs(&self, limit: usize) -> Result<Vec<ExecutionLogEntry>> {
            // V3 doesn't have kv_scan, so we can't efficiently list all logs
            // For now, return empty (would need to implement prefix scan in sqlitegraph)
            // TODO: Implement kv_scan in sqlitegraph V3 or use different approach
            let _ = limit;
            Ok(vec![])
        }

        fn flush(&self) -> Result<()> {
            // V3 backend handles this automatically
            Ok(())
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

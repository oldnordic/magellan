//! Metrics operations for CodeGraph
//!
//! Pre-computed metrics (fan-in, fan-out, LOC, complexity) enable fast debug tool queries.
//!
//! # Thread Safety
//!
//! **This module is NOT thread-safe.**
//!
//! `MetricsOps` is designed for single-threaded use only:
//! - All methods require `&mut self` (exclusive access)
//! - Uses separate rusqlite connection to same database file
//! - No `Send` or `Sync` impls
//!
//! # Usage Pattern
//!
//! `MetricsOps` is accessed exclusively through `CodeGraph`, which
//! enforces single-threaded access. The parent `CodeGraph` instance
//! must not be shared across threads.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "native-v2")]
use sqlitegraph::backend::KvValue;
#[cfg(feature = "native-v2")]
use sqlitegraph::SnapshotId;

pub mod backfill;
pub mod compute;
pub mod schema;

pub use backfill::BackfillResult;
pub use schema::{FileMetrics, SymbolMetrics};

/// Metrics operations for CodeGraph
///
/// Uses separate rusqlite connection to same database file (SQLite mode).
/// Uses KV store for metrics persistence (native-v2 mode).
/// Follows ExecutionLog pattern for side-table management.
pub struct MetricsOps {
    db_path: std::path::PathBuf,
    #[cfg(feature = "native-v2")]
    kv_backend: Option<Rc<dyn sqlitegraph::GraphBackend>>,
}

impl MetricsOps {
    /// Create a new MetricsOps with the given database path
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_path_buf(),
            #[cfg(feature = "native-v2")]
            kv_backend: None,
        }
    }

    /// Create a disabled MetricsOps for native-v2 mode.
    ///
    /// Native V2 doesn't use SQLite-based metrics.
    /// Creates a stub with :memory: path (operations become no-ops).
    #[cfg(feature = "native-v2")]
    pub fn disabled() -> Self {
        Self {
            db_path: std::path::PathBuf::from(":memory:"),
            kv_backend: None,
        }
    }

    /// Create a KV-backed MetricsOps for native-v2 mode.
    ///
    /// Uses the Native V2 backend's KV store for metrics persistence.
    /// Metrics stored in KV survive across magellan runs.
    ///
    /// # Arguments
    /// * `backend` - Graph backend (must be Native V2 for KV operations)
    #[cfg(feature = "native-v2")]
    pub fn with_kv_backend(backend: Rc<dyn sqlitegraph::GraphBackend>) -> Self {
        Self {
            db_path: std::path::PathBuf::from(":memory:"),
            kv_backend: Some(backend),
        }
    }

    /// Ensure metrics tables exist (creates if new DB)
    pub fn ensure_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        // Delegate to db_compat module which has the schema definition
        crate::graph::db_compat::ensure_metrics_schema(&conn)
            .map_err(|e| anyhow::anyhow!("Failed to ensure metrics schema: {}", e))
    }

    /// Open a connection to the database
    fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        rusqlite::Connection::open(&self.db_path)
    }

    /// Get current Unix timestamp in seconds
    ///
    /// Reserved for future timestamp tracking in metrics operations.
    #[allow(dead_code)]
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Upsert file metrics (insert or replace)
    pub fn upsert_file_metrics(&self, metrics: &FileMetrics) -> Result<()> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                // Use KV storage in native-v2 mode
                use crate::kv::encoding::encode_json;
                use crate::kv::keys::file_metrics_key;

                let key = file_metrics_key(&metrics.file_path);
                let json = encode_json(metrics)?;
                backend.kv_set(key, KvValue::Bytes(json), None)
                    .map_err(|e| anyhow::anyhow!("Failed to upsert file metrics (KV): {}", e))?;
                return Ok(());
            }
        }

        // Fall back to SQLite for non-KV mode
        let conn = self.connect()?;
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
        )
        .map_err(|e| anyhow::anyhow!("Failed to upsert file metrics: {}", e))?;
        Ok(())
    }

    /// Upsert symbol metrics (insert or replace)
    pub fn upsert_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                // Use KV storage in native-v2 mode
                use crate::kv::encoding::encode_json;
                use crate::kv::keys::symbol_metrics_key;

                let key = symbol_metrics_key(metrics.symbol_id);
                let json = encode_json(metrics)?;
                backend.kv_set(key, KvValue::Bytes(json), None)
                    .map_err(|e| anyhow::anyhow!("Failed to upsert symbol metrics (KV): {}", e))?;
                return Ok(());
            }
        }

        // Fall back to SQLite for non-KV mode
        let conn = self.connect()?;
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
        )
        .map_err(|e| anyhow::anyhow!("Failed to upsert symbol metrics: {}", e))?;
        Ok(())
    }

    /// Delete all metrics for a file (both file_metrics and symbol_metrics rows)
    pub fn delete_file_metrics(&self, file_path: &str) -> Result<usize> {
        let conn = self.connect()?;

        // Delete symbol metrics for this file first (foreign key dependency)
        let symbol_count = conn
            .execute(
                "DELETE FROM symbol_metrics WHERE file_path = ?1",
                params![file_path],
            )
            .map_err(|e| anyhow::anyhow!("Failed to delete symbol metrics: {}", e))?;

        // Delete file metrics
        conn.execute(
            "DELETE FROM file_metrics WHERE file_path = ?1",
            params![file_path],
        )
        .map_err(|e| anyhow::anyhow!("Failed to delete file metrics: {}", e))?;

        // Return total rows deleted
        Ok(symbol_count)
    }

    /// Get file metrics by path
    pub fn get_file_metrics(&self, file_path: &str) -> Result<Option<FileMetrics>> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                // Use KV storage in native-v2 mode
                use crate::kv::encoding::decode_json;
                use crate::kv::keys::file_metrics_key;

                let key = file_metrics_key(file_path);
                let snapshot = SnapshotId::current();
                if let Some(KvValue::Bytes(json)) = backend.kv_get(snapshot, &key)? {
                    let metrics: FileMetrics = decode_json(&json)?;
                    return Ok(Some(metrics));
                } else {
                    return Ok(None);
                }
            }
        }

        // Fall back to SQLite for non-KV mode
        let conn = self.connect()?;
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
            .optional()
            .map_err(|e| anyhow::anyhow!("Failed to get file metrics: {}", e))?;

        Ok(result)
    }

    /// Get symbol metrics by symbol_id
    pub fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                // Use KV storage in native-v2 mode
                use crate::kv::encoding::decode_json;
                use crate::kv::keys::symbol_metrics_key;

                let key = symbol_metrics_key(symbol_id);
                let snapshot = SnapshotId::current();
                if let Some(KvValue::Bytes(json)) = backend.kv_get(snapshot, &key)? {
                    let metrics: SymbolMetrics = decode_json(&json)?;
                    return Ok(Some(metrics));
                } else {
                    return Ok(None);
                }
            }
        }

        // Fall back to SQLite for non-KV mode
        let conn = self.connect()?;
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
            .optional()
            .map_err(|e| anyhow::anyhow!("Failed to get symbol metrics: {}", e))?;

        Ok(result)
    }

    /// Get hotspots (files with highest complexity scores)
    ///
    /// Returns files ordered by complexity_score DESC, optionally filtered by thresholds.
    pub fn get_hotspots(
        &self,
        limit: Option<u32>,
        min_loc: Option<i64>,
        min_fan_in: Option<i64>,
        min_fan_out: Option<i64>,
    ) -> Result<Vec<FileMetrics>> {
        let conn = self.connect()?;

        // Build query with optional filters
        let mut query = String::from(
            "SELECT file_path, symbol_count, loc, estimated_loc,
                    fan_in, fan_out, complexity_score, last_updated
             FROM file_metrics
             WHERE 1=1",
        );
        let mut param_count = 0;

        if let Some(_) = min_loc {
            param_count += 1;
            query.push_str(&format!(" AND loc >= ?{param_count}"));
        }
        if let Some(_) = min_fan_in {
            param_count += 1;
            query.push_str(&format!(" AND fan_in >= ?{param_count}"));
        }
        if let Some(_) = min_fan_out {
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

pub mod query {
    //! Public query functions for metrics

    use anyhow::Result;
    use super::MetricsOps;
    use super::schema::{FileMetrics, SymbolMetrics};

    /// Get file metrics by path (public wrapper)
    pub fn get_file_metrics(metrics: &MetricsOps, file_path: &str) -> Result<Option<FileMetrics>> {
        metrics.get_file_metrics(file_path)
    }

    /// Get symbol metrics by symbol_id (public wrapper)
    pub fn get_symbol_metrics(metrics: &MetricsOps, symbol_id: i64) -> Result<Option<SymbolMetrics>> {
        metrics.get_symbol_metrics(symbol_id)
    }

    /// Get hotspots with optional filters (public wrapper)
    pub fn get_hotspots(
        metrics: &MetricsOps,
        limit: Option<u32>,
        min_loc: Option<i64>,
        min_fan_in: Option<i64>,
        min_fan_out: Option<i64>,
    ) -> Result<Vec<FileMetrics>> {
        metrics.get_hotspots(limit, min_loc, min_fan_in, min_fan_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "native-v2")]
    fn test_metrics_file_kv_roundtrip() {
        use sqlitegraph::NativeGraphBackend;
        use tempfile::TempDir;
        use std::rc::Rc;

        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = NativeGraphBackend::open(&db_path).unwrap();
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(backend);

        let metrics_ops = MetricsOps::with_kv_backend(backend);

        // Create test file metrics
        let file_metrics = FileMetrics {
            file_path: "src/test.rs".to_string(),
            symbol_count: 5,
            loc: 100,
            estimated_loc: 100.0,
            fan_in: 2,
            fan_out: 3,
            complexity_score: 15.5,
            last_updated: 1234567890,
        };

        // Upsert metrics
        metrics_ops.upsert_file_metrics(&file_metrics).unwrap();

        // Retrieve metrics
        let retrieved = metrics_ops.get_file_metrics("src/test.rs").unwrap();

        // Verify all fields match
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.file_path, file_metrics.file_path);
        assert_eq!(retrieved.symbol_count, file_metrics.symbol_count);
        assert_eq!(retrieved.loc, file_metrics.loc);
        assert_eq!(retrieved.estimated_loc, file_metrics.estimated_loc);
        assert_eq!(retrieved.fan_in, file_metrics.fan_in);
        assert_eq!(retrieved.fan_out, file_metrics.fan_out);
        assert_eq!(retrieved.complexity_score, file_metrics.complexity_score);
        assert_eq!(retrieved.last_updated, file_metrics.last_updated);
    }

    #[test]
    #[cfg(feature = "native-v2")]
    fn test_metrics_symbol_kv_roundtrip() {
        use sqlitegraph::NativeGraphBackend;
        use tempfile::TempDir;
        use std::rc::Rc;

        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = NativeGraphBackend::open(&db_path).unwrap();
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(backend);

        let metrics_ops = MetricsOps::with_kv_backend(backend);

        // Create test symbol metrics
        let symbol_metrics = SymbolMetrics {
            symbol_id: 12345,
            symbol_name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_path: "src/test.rs".to_string(),
            loc: 50,
            estimated_loc: 50.0,
            fan_in: 1,
            fan_out: 2,
            cyclomatic_complexity: 3,
            last_updated: 1234567890,
        };

        // Upsert metrics
        metrics_ops.upsert_symbol_metrics(&symbol_metrics).unwrap();

        // Retrieve metrics
        let retrieved = metrics_ops.get_symbol_metrics(12345).unwrap();

        // Verify all fields match
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.symbol_id, symbol_metrics.symbol_id);
        assert_eq!(retrieved.symbol_name, symbol_metrics.symbol_name);
        assert_eq!(retrieved.kind, symbol_metrics.kind);
        assert_eq!(retrieved.file_path, symbol_metrics.file_path);
        assert_eq!(retrieved.loc, symbol_metrics.loc);
        assert_eq!(retrieved.estimated_loc, symbol_metrics.estimated_loc);
        assert_eq!(retrieved.fan_in, symbol_metrics.fan_in);
        assert_eq!(retrieved.fan_out, symbol_metrics.fan_out);
        assert_eq!(retrieved.cyclomatic_complexity, symbol_metrics.cyclomatic_complexity);
        assert_eq!(retrieved.last_updated, symbol_metrics.last_updated);
    }

    #[test]
    #[cfg(feature = "native-v2")]
    fn test_metrics_kv_persistence() {
        use sqlitegraph::NativeGraphBackend;
        use tempfile::TempDir;
        use std::rc::Rc;

        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = NativeGraphBackend::open(&db_path).unwrap();
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(backend);

        // Create first MetricsOps instance and upsert metrics
        {
            let metrics_ops = MetricsOps::with_kv_backend(backend.clone());
            let file_metrics = FileMetrics {
                file_path: "src/persist.rs".to_string(),
                symbol_count: 10,
                loc: 200,
                estimated_loc: 200.0,
                fan_in: 5,
                fan_out: 7,
                complexity_score: 25.0,
                last_updated: 9876543210,
            };
            metrics_ops.upsert_file_metrics(&file_metrics).unwrap();
        } // metrics_ops dropped here

        // Create new MetricsOps instance with same backend
        let metrics_ops = MetricsOps::with_kv_backend(backend);

        // Verify metrics are retrievable (persistence works)
        let retrieved = metrics_ops.get_file_metrics("src/persist.rs").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.file_path, "src/persist.rs");
        assert_eq!(retrieved.symbol_count, 10);
        assert_eq!(retrieved.loc, 200);
        assert_eq!(retrieved.complexity_score, 25.0);
        assert_eq!(retrieved.last_updated, 9876543210);
    }

    #[test]
    #[cfg(feature = "native-v2")]
    fn test_metrics_kv_update() {
        use sqlitegraph::NativeGraphBackend;
        use tempfile::TempDir;
        use std::rc::Rc;

        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = NativeGraphBackend::open(&db_path).unwrap();
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(backend);

        let metrics_ops = MetricsOps::with_kv_backend(backend);

        // Upsert initial metrics
        let initial_metrics = FileMetrics {
            file_path: "src/update.rs".to_string(),
            symbol_count: 3,
            loc: 50,
            estimated_loc: 50.0,
            fan_in: 1,
            fan_out: 1,
            complexity_score: 5.0,
            last_updated: 1111111111,
        };
        metrics_ops.upsert_file_metrics(&initial_metrics).unwrap();

        // Verify initial values
        let retrieved = metrics_ops.get_file_metrics("src/update.rs").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.symbol_count, 3);
        assert_eq!(retrieved.complexity_score, 5.0);

        // Upsert updated metrics with different values
        let updated_metrics = FileMetrics {
            file_path: "src/update.rs".to_string(),
            symbol_count: 8,
            loc: 150,
            estimated_loc: 150.0,
            fan_in: 4,
            fan_out: 6,
            complexity_score: 20.0,
            last_updated: 2222222222,
        };
        metrics_ops.upsert_file_metrics(&updated_metrics).unwrap();

        // Verify latest values are returned
        let retrieved = metrics_ops.get_file_metrics("src/update.rs").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.symbol_count, 8);
        assert_eq!(retrieved.loc, 150);
        assert_eq!(retrieved.complexity_score, 20.0);
        assert_eq!(retrieved.last_updated, 2222222222);
    }

    #[test]
    #[cfg(feature = "native-v2")]
    fn test_metrics_kv_missing_key_returns_none() {
        use sqlitegraph::NativeGraphBackend;
        use tempfile::TempDir;
        use std::rc::Rc;

        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = NativeGraphBackend::open(&db_path).unwrap();
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(backend);

        let metrics_ops = MetricsOps::with_kv_backend(backend);

        // Try to get metrics for a file that doesn't exist
        let retrieved = metrics_ops.get_file_metrics("src/nonexistent.rs").unwrap();
        assert!(retrieved.is_none());

        // Try to get metrics for a symbol that doesn't exist
        let retrieved = metrics_ops.get_symbol_metrics(99999).unwrap();
        assert!(retrieved.is_none());
    }
}

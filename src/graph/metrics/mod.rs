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
use std::time::{SystemTime, UNIX_EPOCH};

pub mod backfill;
pub mod compute;
pub mod schema;

pub use backfill::BackfillResult;
pub use schema::{FileMetrics, SymbolMetrics};

/// Metrics operations for CodeGraph
///
/// Uses separate rusqlite connection to same database file.
/// Follows ExecutionLog pattern for side-table management.
pub struct MetricsOps {
    db_path: std::path::PathBuf,
}

impl MetricsOps {
    /// Create a new MetricsOps with the given database path
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_path_buf(),
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

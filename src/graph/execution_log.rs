//! Execution log for tracking Magellan runs
//!
//! Records every CLI command execution with execution_id, timestamps,
//! arguments, and outcome. Provides audit trail for correlating outputs.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::path::Path;

/// Execution log entry
#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    pub id: i64,
    pub execution_id: String,
    pub tool_version: String,
    pub args: String, // JSON array
    pub root: Option<String>,
    pub db_path: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub outcome: String, // "success", "error", "partial"
    pub error_message: Option<String>,
    pub files_indexed: i64,
    pub symbols_indexed: i64,
    pub references_indexed: i64,
}

/// Execution log storage
///
/// Uses separate rusqlite connection to same database file.
/// Follows ChunkStore pattern for side-table management.
pub struct ExecutionLog {
    db_path: std::path::PathBuf,
}

impl ExecutionLog {
    pub fn new(db_path: &Path) -> Self {
        Self { db_path: db_path.to_path_buf() }
    }

    pub fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        rusqlite::Connection::open(&self.db_path)
    }

    pub fn ensure_schema(&self) -> Result<()> {
        let conn = self.connect()?;

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
        )
        .map_err(|e| anyhow::anyhow!("Failed to create execution_log table: {}", e))?;

        // Indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_execution_log_started_at
                    ON execution_log(started_at DESC)",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create started_at index: {}", e))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_execution_log_execution_id
                    ON execution_log(execution_id)",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create execution_id index: {}", e))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_execution_log_outcome
                    ON execution_log(outcome)",
            [],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create outcome index: {}", e))?;

        Ok(())
    }

    pub fn start_execution(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64> {
        let conn = self.connect()?;
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
        )
        .map_err(|e| anyhow::anyhow!("Failed to insert execution log: {}", e))?;

        Ok(conn.last_insert_rowid())
    }

    pub fn finish_execution(
        &self,
        execution_id: &str,
        outcome: &str,
        error_message: Option<&str>,
        files_indexed: usize,
        symbols_indexed: usize,
        references_indexed: usize,
    ) -> Result<()> {
        let conn = self.connect()?;
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
        )
        .map_err(|e| anyhow::anyhow!("Failed to update execution log: {}", e))?;

        Ok(())
    }

    /// Get an execution record by execution_id
    pub fn get_by_execution_id(&self, execution_id: &str) -> Result<Option<ExecutionRecord>> {
        let conn = self.connect()?;

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
            .optional()
            .map_err(|e| anyhow::anyhow!("Failed to query execution log: {}", e))?;

        Ok(result)
    }

    /// Get all execution records, ordered by most recent first
    pub fn list_all(&self, limit: Option<usize>) -> Result<Vec<ExecutionRecord>> {
        let conn = self.connect()?;

        let limit_clause = limit
            .map(|l| format!(" LIMIT {}", l))
            .unwrap_or_default();
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
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to collect execution records: {}", e))?;

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_execution_log_schema() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let log = ExecutionLog::new(&db_path);

        // ensure_schema should create table without error
        log.ensure_schema().unwrap();

        // Verify table exists by querying it
        let conn = log.connect().unwrap();
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='execution_log'",
                [],
                |_| Ok(true),
            )
            .optional()
            .unwrap()
            .unwrap_or(false);

        assert!(table_exists, "execution_log table should exist");
    }

    #[test]
    fn test_execution_log_insert_and_update() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let log = ExecutionLog::new(&db_path);

        log.ensure_schema().unwrap();

        // Start an execution
        let row_id = log
            .start_execution(
                "test-exec-001",
                "0.1.0",
                &["magellan".to_string(), "index".to_string()],
                Some("/project"),
                "/project/magellan.db",
            )
            .unwrap();

        assert!(row_id > 0, "Row ID should be positive");

        // Verify initial state
        let record = log.get_by_execution_id("test-exec-001").unwrap();
        assert!(record.is_some());
        let rec = record.unwrap();
        assert_eq!(rec.execution_id, "test-exec-001");
        assert_eq!(rec.tool_version, "0.1.0");
        assert_eq!(rec.outcome, "running");
        assert!(rec.finished_at.is_none());
        assert!(rec.duration_ms.is_none());

        // Finish the execution
        log.finish_execution("test-exec-001", "success", None, 42, 100, 50)
            .unwrap();

        // Verify updated state
        let record = log.get_by_execution_id("test-exec-001").unwrap();
        assert!(record.is_some());
        let rec = record.unwrap();
        assert_eq!(rec.outcome, "success");
        assert!(rec.finished_at.is_some());
        assert!(rec.duration_ms.is_some());
        assert_eq!(rec.files_indexed, 42);
        assert_eq!(rec.symbols_indexed, 100);
        assert_eq!(rec.references_indexed, 50);
    }

    #[test]
    fn test_execution_log_duplicate_id() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let log = ExecutionLog::new(&db_path);

        log.ensure_schema().unwrap();

        // Start first execution
        log.start_execution("dup-exec", "0.1.0", &[], None, "/db")
            .unwrap();

        // Attempt to start second with same ID - should fail
        let result = log.start_execution("dup-exec", "0.1.0", &[], None, "/db");

        assert!(result.is_err(), "Duplicate execution_id should fail");
    }

    #[test]
    fn test_execution_outcome_values() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let log = ExecutionLog::new(&db_path);

        log.ensure_schema().unwrap();

        // Test success outcome
        log.start_execution("exec-success", "0.1.0", &[], None, "/db")
            .unwrap();
        log.finish_execution("exec-success", "success", None, 1, 0, 0)
            .unwrap();

        let rec = log.get_by_execution_id("exec-success").unwrap().unwrap();
        assert_eq!(rec.outcome, "success");

        // Test error outcome
        log.start_execution("exec-error", "0.1.0", &[], None, "/db")
            .unwrap();
        log.finish_execution("exec-error", "error", Some("test error"), 0, 0, 0)
            .unwrap();

        let rec = log.get_by_execution_id("exec-error").unwrap().unwrap();
        assert_eq!(rec.outcome, "error");
        assert_eq!(rec.error_message, Some("test error".to_string()));

        // Test partial outcome
        log.start_execution("exec-partial", "0.1.0", &[], None, "/db")
            .unwrap();
        log.finish_execution("exec-partial", "partial", None, 5, 0, 0)
            .unwrap();

        let rec = log.get_by_execution_id("exec-partial").unwrap().unwrap();
        assert_eq!(rec.outcome, "partial");
    }

    #[test]
    fn test_list_all_executions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let log = ExecutionLog::new(&db_path);

        log.ensure_schema().unwrap();

        // Create multiple executions
        for i in 0..5 {
            let id = format!("exec-{:03}", i);
            log.start_execution(&id, "0.1.0", &[], None, "/db").unwrap();
            log.finish_execution(&id, "success", None, i, 0, 0)
                .unwrap();
        }

        // List all
        let all = log.list_all(None).unwrap();
        assert_eq!(all.len(), 5);

        // List with limit
        let limited = log.list_all(Some(3)).unwrap();
        assert_eq!(limited.len(), 3);

        // Verify order (most recent first)
        // Due to timing, we just verify the IDs exist
        let ids: Vec<_> = all.iter().map(|r| r.execution_id.as_str()).collect();
        assert!(ids.contains(&"exec-000"));
        assert!(ids.contains(&"exec-004"));
    }

    #[test]
    fn test_duration_calculation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let log = ExecutionLog::new(&db_path);

        log.ensure_schema().unwrap();

        log.start_execution("exec-duration", "0.1.0", &[], None, "/db")
            .unwrap();

        // Small delay to ensure positive duration
        std::thread::sleep(std::time::Duration::from_millis(50));

        log.finish_execution("exec-duration", "success", None, 0, 0, 0)
            .unwrap();

        let rec = log.get_by_execution_id("exec-duration").unwrap().unwrap();
        assert!(rec.duration_ms.is_some());
        assert!(rec.duration_ms.unwrap() >= 0); // Duration should be non-negative
        assert!(rec.duration_ms.unwrap() < 1000); // Should be less than 1 second
    }
}

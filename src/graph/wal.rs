//! WAL checkpoint utilities for SQLite databases.
//!
//! Provides helpers to force WAL checkpoints after large write transactions,
//! preventing unbounded WAL growth that can lead to "database disk image is malformed".

use rusqlite::Connection;
use std::path::Path;

/// Force a WAL checkpoint on the given database file.
///
/// This should be called after large write transactions (bulk inserts, coverage ingestion,
/// watcher batch processing) to prevent unbounded WAL growth and reduce corruption risk.
///
/// # Errors
/// Returns `DatabaseBusy` if the checkpoint is blocked by active readers.
pub fn checkpoint_wal(db_path: &Path) -> Result<(), rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    checkpoint_conn(&conn)
}

/// Checkpoint using an existing connection (avoids opening another connection).
///
/// # Errors
/// Returns `DatabaseBusy` if the checkpoint is blocked by active readers.
pub fn checkpoint_conn(conn: &Connection) -> Result<(), rusqlite::Error> {
    let busy: i32 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
    if busy != 0 {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseBusy,
                extended_code: 5,
            },
            Some("WAL checkpoint blocked by active readers".to_string()),
        ));
    }
    Ok(())
}

/// Checkpoint with retry logic for `DatabaseBusy` scenarios.
///
/// Retries up to `max_retries` times with exponential backoff.
pub fn checkpoint_conn_with_retry(
    conn: &Connection,
    max_retries: u32,
) -> Result<(), rusqlite::Error> {
    let mut last_err = None;
    for attempt in 0..max_retries {
        match checkpoint_conn(conn) {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                let delay_ms = 2_u64.pow(attempt) * 10;
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                last_err = Some(err);
            }
            Err(e) => return Err(e),
        }
    }
    Err(rusqlite::Error::SqliteFailure(
        last_err.unwrap_or(rusqlite::ffi::Error {
            code: rusqlite::ErrorCode::DatabaseBusy,
            extended_code: 5,
        }),
        Some("WAL checkpoint failed after max retries".to_string()),
    ))
}

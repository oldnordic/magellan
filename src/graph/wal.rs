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

//! Code generation and storage module.
//!
//! This module provides functionality for storing and retrieving source code chunks
//! with their byte spans. This enables token-efficient queries by storing code
//! fragments in the database rather than re-reading entire files.

pub mod schema;

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub use schema::CodeChunk;

/// Connection source for ChunkStore.
///
/// Allows ChunkStore to either open its own connections (legacy behavior)
/// or use a shared connection provided by CodeGraph (for transactional operations).
enum ChunkStoreConnection {
    /// Owned connection source - ChunkStore opens connections as needed
    Owned(std::path::PathBuf),
    /// Shared connection - provided by CodeGraph for transactional operations
    /// Thread-safe: uses Arc<Mutex<>> instead of Rc<RefCell<>>
    Shared(Arc<Mutex<rusqlite::Connection>>),
}

/// Code chunk storage operations.
///
/// Can use either its own connections (legacy) or a shared connection provided
/// by CodeGraph for transactional operations.
pub struct ChunkStore {
    /// Connection source - either owned path or shared connection
    conn_source: ChunkStoreConnection,
}

impl ChunkStore {
    /// Create a new ChunkStore with the given database path.
    ///
    /// This is the legacy constructor that opens its own connections.
    pub fn new(db_path: &Path) -> Self {
        Self {
            conn_source: ChunkStoreConnection::Owned(db_path.to_path_buf()),
        }
    }

    /// Create a ChunkStore with a shared connection.
    ///
    /// This constructor enables transactional operations by using a connection
    /// shared with CodeGraph. All operations will use this shared connection.
    ///
    /// # Arguments
    /// * `conn` - Shared SQLite connection wrapped in Rc<RefCell<>> for interior mutability
    pub fn with_connection(conn: rusqlite::Connection) -> Self {
        Self {
            conn_source: ChunkStoreConnection::Shared(Rc::new(std::cell::RefCell::new(conn))),
        }
    }

    /// Get a connection to the database.
    ///
    /// For owned connections, opens a new connection.
    /// For shared connections, also opens a new connection (to the same database).
    ///
    /// Note: This method always opens a NEW connection, even when using shared mode.
    /// This is needed for operations that require raw access to the connection,
    /// such as delete_edges_touching_entities which operates on sqlitegraph tables.
    pub fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        match &self.conn_source {
            ChunkStoreConnection::Owned(path) => rusqlite::Connection::open(path),
            ChunkStoreConnection::Shared(rc) => {
                // Open a new connection to the same database.
                // We need to extract the path from the existing connection.
                let conn = rc.try_borrow()
                    .map_err(|_| rusqlite::Error::InvalidParameterName(
                        "Shared connection already borrowed".to_string()
                    ))?;
                // Get the database path from the existing connection
                let path = conn.path().ok_or_else(|| {
                    rusqlite::Error::InvalidParameterName("Cannot get database path".to_string())
                })?;
                // Open a new connection to the same database
                rusqlite::Connection::open(path)
            }
        }
    }

    /// Execute an operation with a connection.
    ///
    /// This helper method abstracts over owned vs shared connection sources,
    /// allowing all ChunkStore methods to work with both modes.
    fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<R>,
    {
        match &self.conn_source {
            ChunkStoreConnection::Owned(path) => {
                let conn = rusqlite::Connection::open(path)?;
                let result = f(&conn)?;
                Ok(result)
            }
            ChunkStoreConnection::Shared(rc) => {
                let conn = rc.try_borrow()
                    .map_err(|_| anyhow::anyhow!("Shared connection already borrowed"))?;
                let result = f(&conn)?;
                Ok(result)
            }
        }
    }

    /// Execute a mutable operation with a connection.
    ///
    /// This helper method is for operations that need mutable access to the connection.
    fn with_connection_mut<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R>,
    {
        match &self.conn_source {
            ChunkStoreConnection::Owned(path) => {
                let mut conn = rusqlite::Connection::open(path)?;
                let result = f(&mut conn)?;
                Ok(result)
            }
            ChunkStoreConnection::Shared(rc) => {
                let mut conn = rc.try_borrow_mut()
                    .map_err(|_| anyhow::anyhow!("Shared connection already borrowed"))?;
                let result = f(&mut conn)?;
                Ok(result)
            }
        }
    }

    /// Ensure the code_chunks table exists.
    pub fn ensure_schema(&self) -> Result<()> {
        self.with_connection_mut(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS code_chunks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_path TEXT NOT NULL,
                    byte_start INTEGER NOT NULL,
                    byte_end INTEGER NOT NULL,
                    content TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    symbol_name TEXT,
                    symbol_kind TEXT,
                    created_at INTEGER NOT NULL,
                    UNIQUE(file_path, byte_start, byte_end)
                )",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create code_chunks table: {}", e))?;

            // Create indexes for common queries
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON code_chunks(file_path)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create file_path index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_symbol_name ON code_chunks(symbol_name)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create symbol_name index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON code_chunks(content_hash)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create content_hash index: {}", e))?;

            Ok(())
        })
    }

    /// Store a code chunk in the database.
    ///
    /// Uses INSERT OR REPLACE to handle duplicates based on (file_path, byte_start, byte_end).
    pub fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64> {
        self.with_connection_mut(|conn| {
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
            )
            .map_err(|e| anyhow::anyhow!("Failed to store code chunk: {}", e))?;

            Ok(conn.last_insert_rowid())
        })
    }

    /// Store multiple code chunks in a transaction.
    pub fn store_chunks(&self, chunks: &[CodeChunk]) -> Result<Vec<i64>> {
        self.with_connection_mut(|conn| {
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| anyhow::anyhow!("Failed to start transaction: {}", e))?;

            let mut ids = Vec::new();

            for chunk in chunks {
                tx.execute(
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
                )
                .map_err(|e| anyhow::anyhow!("Failed to store code chunk: {}", e))?;

                ids.push(tx.last_insert_rowid());
            }

            tx.commit()
                .map_err(|e| anyhow::anyhow!("Failed to commit transaction: {}", e))?;

            Ok(ids)
        })
    }

    /// Get a code chunk by file path and byte span.
    pub fn get_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks
                     WHERE file_path = ?1 AND byte_start = ?2 AND byte_end = ?3",
                )
                .map_err(|e| anyhow::anyhow!("Failed to prepare query: {}", e))?;

            let result = stmt
                .query_row(
                    params![file_path, byte_start as i64, byte_end as i64],
                    |row: &rusqlite::Row| {
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
                .optional()
                .map_err(|e| anyhow::anyhow!("Failed to query code chunk: {}", e))?;

            Ok(result)
        })
    }

    /// Get all code chunks for a specific file.
    pub fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks
                     WHERE file_path = ?1
                     ORDER BY byte_start",
                )
                .map_err(|e| anyhow::anyhow!("Failed to prepare query: {}", e))?;

            let chunks = stmt
                .query_map(params![file_path], |row: &rusqlite::Row| {
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
                })
                .map_err(|e| anyhow::anyhow!("Failed to query code chunks: {}", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("Failed to collect chunks: {}", e))?;

            Ok(chunks)
        })
    }

    /// Get code chunks for a specific symbol in a file.
    pub fn get_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<CodeChunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks
                     WHERE file_path = ?1 AND symbol_name = ?2
                     ORDER BY byte_start",
                )
                .map_err(|e| anyhow::anyhow!("Failed to prepare query: {}", e))?;

            let chunks = stmt
                .query_map(params![file_path, symbol_name], |row: &rusqlite::Row| {
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
                })
                .map_err(|e| anyhow::anyhow!("Failed to query code chunks: {}", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("Failed to collect chunks: {}", e))?;

            Ok(chunks)
        })
    }

    /// Delete all code chunks for a specific file.
    pub fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize> {
        self.with_connection_mut(|conn| {
            let affected = conn
                .execute(
                    "DELETE FROM code_chunks WHERE file_path = ?1",
                    params![file_path],
                )
                .map_err(|e| anyhow::anyhow!("Failed to delete code chunks: {}", e))?;

            Ok(affected)
        })
    }

    /// Count total code chunks stored.
    pub fn count_chunks(&self) -> Result<usize> {
        self.with_conn(|conn| {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM code_chunks", [], |row: &rusqlite::Row| {
                    row.get(0)
                })
                .map_err(|e| anyhow::anyhow!("Failed to count chunks: {}", e))?;

            Ok(count as usize)
        })
    }

    /// Count code chunks for a specific file.
    ///
    /// Used by delete operations to verify deletion completeness.
    pub fn count_chunks_for_file(&self, file_path: &str) -> Result<usize> {
        self.with_conn(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM code_chunks WHERE file_path = ?1",
                    params![file_path],
                    |row: &rusqlite::Row| {
                        row.get(0)
                    },
                )
                .map_err(|e| anyhow::anyhow!("Failed to count chunks for file: {}", e))?;

            Ok(count as usize)
        })
    }

    /// Get chunks by symbol kind (e.g., "fn", "struct").
    pub fn get_chunks_by_kind(&self, symbol_kind: &str) -> Result<Vec<CodeChunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks
                     WHERE symbol_kind = ?1
                     ORDER BY file_path, byte_start",
                )
                .map_err(|e| anyhow::anyhow!("Failed to prepare query: {}", e))?;

            let chunks = stmt
                .query_map(params![symbol_kind], |row: &rusqlite::Row| {
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
                })
                .map_err(|e| anyhow::anyhow!("Failed to query code chunks: {}", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("Failed to collect chunks: {}", e))?;

            Ok(chunks)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These are unit tests for the schema.
    // Integration tests with a real database are in tests/.

    #[test]
    fn test_code_chunk_creation() {
        let chunk = CodeChunk::new(
            "test.rs".to_string(),
            0,
            10,
            "fn main() {}".to_string(),
            Some("main".to_string()),
            Some("fn".to_string()),
        );

        assert_eq!(chunk.file_path, "test.rs");
        assert_eq!(chunk.byte_start, 0);
        assert_eq!(chunk.byte_end, 10);
        assert_eq!(chunk.content, "fn main() {}");
        assert_eq!(chunk.symbol_name, Some("main".to_string()));
        assert_eq!(chunk.symbol_kind, Some("fn".to_string()));
        assert!(!chunk.content_hash.is_empty());
        assert!(chunk.id.is_none());
    }
}

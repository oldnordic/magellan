//! Code generation and storage module.
//!
//! This module provides functionality for storing and retrieving source code chunks
//! with their byte spans. This enables token-efficient queries by storing code
//! fragments in the database rather than re-reading entire files.
//!
//! # :memory: Database Path Retrieval
//!
//! ChunkStore uses SQLite Shared connections (via `Arc<parking_lot::Mutex<Connection>>`), which
//! don't work with `:memory:` databases. Each thread would get its own separate
//! in-memory database, breaking the shared state assumption.
//!
//! Additionally, operations that retrieve the database file path (e.g., the `connect()`
//! method's shared connection branch) will fail for `:memory:` databases because
//! in-memory databases have no file path.
//!
//! **Workaround:** Use file-based databases for ChunkStore operations.
//! See [MANUAL.md](../../MANUAL.md#known-limitations) for details.

pub mod schema;

use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use std::sync::Arc;

pub use schema::CodeChunk;

pub(crate) fn is_code_chunks_fts_corruption(err: &rusqlite::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("virtual table is corrupt") || msg.contains("database disk image is malformed")
}

pub(crate) fn rebuild_code_chunks_fts(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO code_chunks_fts(code_chunks_fts) VALUES('rebuild')",
        [],
    )
    .map_err(|e| anyhow!("Failed to rebuild code_chunks_fts: {}", e))?;
    Ok(())
}

/// Storage backend for ChunkStore.
///
/// Supports three modes:
/// - Owned: ChunkStore opens its own SQLite connections (legacy)
/// - Shared: Uses a shared SQLite connection for transactions
/// - SideTables: Uses the SideTables trait abstraction (V3 backend)
enum ChunkStoreBackend {
    /// Owned connection source - ChunkStore opens connections as needed
    Owned(std::path::PathBuf),
    /// Shared connection - provided by CodeGraph for transactional operations
    /// Thread-safe: uses parking_lot::Mutex for fast uncontended locking
    Shared(Arc<parking_lot::Mutex<rusqlite::Connection>>),
    /// SideTables abstraction - for V3 backend (no SQLite dependency)
    SideTables(Arc<dyn crate::graph::side_tables::SideTables>),
}

/// Code chunk storage operations.
///
/// Can use either its own connections (legacy), a shared connection provided
/// by CodeGraph for transactional operations, or the SideTables abstraction
/// for backend-agnostic storage (V3).
pub struct ChunkStore {
    /// Backend - either SQLite connection or SideTables trait
    backend: ChunkStoreBackend,
}

impl Clone for ChunkStore {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
        }
    }
}

impl Clone for ChunkStoreBackend {
    fn clone(&self) -> Self {
        match self {
            ChunkStoreBackend::Shared(arc) => ChunkStoreBackend::Shared(Arc::clone(arc)),
            ChunkStoreBackend::SideTables(st) => ChunkStoreBackend::SideTables(Arc::clone(st)),
            ChunkStoreBackend::Owned(path) => ChunkStoreBackend::Owned(path.clone()),
        }
    }
}

impl ChunkStore {
    /// Create a new ChunkStore with the given database path.
    ///
    /// This is the legacy constructor that opens its own connections.
    pub fn new(db_path: &Path) -> Self {
        Self {
            backend: ChunkStoreBackend::Owned(db_path.to_path_buf()),
        }
    }

    /// Create a ChunkStore with a shared connection.
    ///
    /// This constructor enables transactional operations by using a connection
    /// shared with CodeGraph. All operations will use this shared connection.
    ///
    /// # Arguments
    /// * `conn` - Shared SQLite connection wrapped in Arc<parking_lot::Mutex<>> for thread-safe interior mutability
    pub fn with_connection(conn: rusqlite::Connection) -> Self {
        Self {
            backend: ChunkStoreBackend::Shared(Arc::new(parking_lot::Mutex::new(conn))),
        }
    }

    /// Create a ChunkStore using the SideTables abstraction.
    ///
    /// This constructor is used for V3 backend where we want to avoid SQLite
    /// entirely for side tables.
    ///
    /// # Arguments
    /// * `side_tables` - `Arc<dyn SideTables>` implementation
    pub fn with_side_tables(side_tables: Arc<dyn crate::graph::side_tables::SideTables>) -> Self {
        Self {
            backend: ChunkStoreBackend::SideTables(side_tables),
        }
    }

    /// Create a stub ChunkStore using a temporary file (for testing).
    ///
    /// Uses a temporary file so that new connections can access the same data.
    #[cfg(test)]
    pub fn in_memory() -> Self {
        // Create: Create a unique temporary file for each call
        // This prevents conflicts when multiple tests run concurrently
        let temp_dir = std::env::temp_dir();
        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("SystemTime before UNIX_EPOCH — this should not happen")
                .as_nanos()
        );
        let db_path = temp_dir.join(format!("magellan_chunkstore_stub_{}.db", unique_id));

        let conn = rusqlite::Connection::open(&db_path)
            .expect("Failed to create temporary database for ChunkStore stub");

        // Create the code_chunks table with full schema for compatibility
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
        .expect("Failed to create code_chunks table in ChunkStore stub");

        // Create the ast_nodes table for AST storage
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ast_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id INTEGER,
                kind TEXT NOT NULL,
                byte_start INTEGER NOT NULL,
                byte_end INTEGER NOT NULL,
                file_id INTEGER
            )",
            [],
        )
        .expect("Failed to create ast_nodes table in ChunkStore stub");

        // Create the cfg_blocks table for CFG storage
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cfg_blocks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                function_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                terminator TEXT NOT NULL,
                byte_start INTEGER NOT NULL,
                byte_end INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                cfg_hash TEXT,
                statements TEXT,
                cfg_condition TEXT
            )",
            [],
        )
        .expect("Failed to create cfg_blocks table in ChunkStore stub");

        // Create indexes (use IF NOT EXISTS to avoid conflicts on reconnect)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON code_chunks(file_path)",
            [],
        )
        .expect("Failed to create file_path index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_symbol_name ON code_chunks(symbol_name)",
            [],
        )
        .expect("Failed to create symbol_name index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON code_chunks(content_hash)",
            [],
        )
        .expect("Failed to create content_hash index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent ON ast_nodes(parent_id)",
            [],
        )
        .expect("Failed to create ast_nodes parent index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span ON ast_nodes(byte_start, byte_end)",
            [],
        )
        .expect("Failed to create ast_nodes span index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_file_id ON ast_nodes(file_id)",
            [],
        )
        .expect("Failed to create ast_nodes file_id index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function ON cfg_blocks(function_id)",
            [],
        )
        .expect("Failed to create cfg_blocks function index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_span ON cfg_blocks(byte_start, byte_end)",
            [],
        )
        .expect("Failed to create cfg_blocks span index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_hash ON cfg_blocks(cfg_hash)",
            [],
        )
        .expect("Failed to create cfg_blocks hash index");

        conn.execute(
            "CREATE TABLE IF NOT EXISTS cfg_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                function_id INTEGER NOT NULL,
                source_idx INTEGER NOT NULL,
                target_idx INTEGER NOT NULL,
                edge_type TEXT NOT NULL
            )",
            [],
        )
        .expect("Failed to create cfg_edges table in ChunkStore");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_edges_function ON cfg_edges(function_id)",
            [],
        )
        .expect("Failed to create cfg_edges function index");

        Self {
            backend: ChunkStoreBackend::Owned(db_path),
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
    ///
    /// # Panics
    /// Panics if called when using SideTables backend (not applicable).
    pub fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        match &self.backend {
            ChunkStoreBackend::Owned(path) => rusqlite::Connection::open(path),
            ChunkStoreBackend::Shared(arc) => {
                // Open a new connection to the same database.
                // We need to extract the path from the existing connection.
                let conn = arc.lock();
                // Get the database path from the existing connection
                let path = conn.path().ok_or_else(|| {
                    rusqlite::Error::InvalidParameterName(
                        "Cannot get database path. :memory: databases have no file path. \
                        Use a file-based database (e.g., --db magellan.db) instead. \
                        See MANUAL.md for details."
                            .to_string(),
                    )
                })?;
                // Open a new connection to the same database
                rusqlite::Connection::open(path)
            }
            ChunkStoreBackend::SideTables(_) => Err(rusqlite::Error::InvalidParameterName(
                "SQLite connection not available with V3 backend".to_string(),
            )),
        }
    }

    /// Execute an operation with a connection.
    ///
    /// This helper method abstracts over owned vs shared connection sources,
    /// allowing all ChunkStore methods to work with both modes.
    pub(crate) fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<R>,
    {
        match &self.backend {
            ChunkStoreBackend::Owned(path) => {
                let conn = rusqlite::Connection::open(path)?;
                let result = f(&conn)?;
                Ok(result)
            }
            ChunkStoreBackend::Shared(arc) => {
                let conn = arc.lock();
                let result = f(&conn)?;
                Ok(result)
            }
            ChunkStoreBackend::SideTables(_) => Err(anyhow::anyhow!(
                "SQLite operations not available with V3 backend. Use SideTables trait methods."
            )),
        }
    }

    /// Execute a mutable operation with a connection.
    ///
    /// This helper method is for operations that need mutable access to the connection.
    pub(crate) fn with_connection_mut<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R>,
    {
        match &self.backend {
            ChunkStoreBackend::Owned(path) => {
                let mut conn = rusqlite::Connection::open(path)?;
                let result = f(&mut conn)?;
                Ok(result)
            }
            ChunkStoreBackend::Shared(arc) => {
                let mut conn = arc.lock();
                let result = f(&mut conn)?;
                Ok(result)
            }
            ChunkStoreBackend::SideTables(_) => Err(anyhow::anyhow!(
                "SQLite operations not available with V3 backend. Use SideTables trait methods."
            )),
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
            let insert_chunk = |conn: &rusqlite::Connection| {
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
            };

            match insert_chunk(conn) {
                Ok(_) => {}
                Err(err) if is_code_chunks_fts_corruption(&err) => {
                    rebuild_code_chunks_fts(conn)?;
                    insert_chunk(conn)
                        .map_err(|e| anyhow!("Failed to store code chunk after FTS repair: {}", e))?;
                }
                Err(err) => return Err(anyhow!("Failed to store code chunk: {}", err)),
            }

            Ok(conn.last_insert_rowid())
        })
    }

    /// Store multiple code chunks in a transaction.
    pub fn store_chunks(&self, chunks: &[CodeChunk]) -> Result<Vec<i64>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => {
                // V3 backend: use SideTables trait method
                let mut ids = Vec::new();
                for chunk in chunks {
                    let id = tables.store_chunk(chunk)?;
                    ids.push(id);
                }
                Ok(ids)
            }
            _ => {
                // SQLite backend: use direct connection
                self.with_connection_mut(|conn| {
                    let run_batch =
                        |conn: &mut rusqlite::Connection| -> std::result::Result<Vec<i64>, rusqlite::Error> {
                            let tx = conn.unchecked_transaction()?;
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
                                )?;

                                ids.push(tx.last_insert_rowid());
                            }

                            tx.commit()?;
                            Ok(ids)
                        };

                    match run_batch(conn) {
                        Ok(ids) => Ok(ids),
                        Err(err) if is_code_chunks_fts_corruption(&err) => {
                            rebuild_code_chunks_fts(conn)?;
                            run_batch(conn).map_err(|e| {
                                anyhow!("Failed to store code chunks after FTS repair: {}", e)
                            })
                        }
                        Err(err) => Err(anyhow!("Failed to store code chunks: {}", err)),
                    }
                })
            }
        }
    }

    /// Get a code chunk by file path and byte span.
    pub fn get_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => {
                tables.get_chunk_by_span(file_path, byte_start, byte_end)
            }
            _ => self.with_conn(|conn| {
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
            }),
        }
    }

    /// Get all code chunks for a specific file.
    pub fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => tables.get_chunks_for_file(file_path),
            _ => self.with_conn(|conn| {
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
            }),
        }
    }

    /// Search code chunk content via FTS5 full-text search.
    pub fn search_code_content(
        &self,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<crate::graph::side_tables::CodeContentSearchResult>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => tables.search_code_content(pattern, limit),
            _ => self.with_conn(|conn| {
                use crate::graph::side_tables::CodeContentSearchResult;

                let mut stmt = conn
                    .prepare_cached(
                        "SELECT cc.symbol_name, cc.symbol_kind, cc.file_path,
                                cc.byte_start, cc.byte_end, cc.content, fts.rank
                         FROM code_chunks_fts fts
                         JOIN code_chunks cc ON fts.rowid = cc.id
                         WHERE code_chunks_fts MATCH ?1
                         ORDER BY fts.rank
                         LIMIT ?2",
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to prepare content search: {}", e))?;

                let results = stmt
                    .query_map(params![pattern, limit as i64], |row| {
                        let symbol_name: Option<String> = row.get(0)?;
                        let symbol_kind: Option<String> = row.get(1)?;
                        let file_path: String = row.get(2)?;
                        let byte_start: i64 = row.get(3)?;
                        let byte_end: i64 = row.get(4)?;
                        let content: String = row.get(5)?;
                        let rank: f64 = row.get(6)?;

                        let start_line = content
                            .char_indices()
                            .take_while(|(i, _)| *i < byte_start as usize)
                            .filter(|(_, c)| *c == '\n')
                            .count()
                            + 1;
                        let end_line = content
                            .char_indices()
                            .take_while(|(i, _)| *i < byte_end as usize)
                            .filter(|(_, c)| *c == '\n')
                            .count()
                            + 1;

                        // Build excerpt from first matching token
                        let excerpt = build_search_excerpt(&content, pattern);

                        Ok(CodeContentSearchResult {
                            symbol_name,
                            symbol_kind,
                            file_path,
                            byte_start: byte_start as usize,
                            byte_end: byte_end as usize,
                            start_line,
                            end_line,
                            excerpt,
                            rank,
                        })
                    })
                    .map_err(|e| anyhow::anyhow!("Content search query failed: {}", e))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| anyhow::anyhow!("Content search row parse failed: {}", e))?;

                Ok(results)
            }),
        }
    }

    /// Get code chunks for a specific symbol in a file.
    pub fn get_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<CodeChunk>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => {
                tables.get_chunks_by_symbol(file_path, symbol_name)
            }
            _ => self.with_conn(|conn| {
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
            }),
        }
    }

    /// Delete all code chunks for a specific file.
    pub fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => tables.delete_chunks_for_file(file_path),
            _ => self.with_connection_mut(|conn| {
                let delete_chunks = |conn: &rusqlite::Connection| {
                    conn.execute(
                        "DELETE FROM code_chunks WHERE file_path = ?1",
                        params![file_path],
                    )
                };

                let affected = match delete_chunks(conn) {
                    Ok(affected) => affected,
                    Err(err) if is_code_chunks_fts_corruption(&err) => {
                        rebuild_code_chunks_fts(conn)?;
                        delete_chunks(conn).map_err(|e| {
                            anyhow!("Failed to delete code chunks after FTS repair: {}", e)
                        })?
                    }
                    Err(err) => return Err(anyhow!("Failed to delete code chunks: {}", err)),
                };

                Ok(affected)
            }),
        }
    }

    /// Count total code chunks stored.
    pub fn count_chunks(&self) -> Result<usize> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => tables.count_chunks(),
            _ => self.with_conn(|conn| {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM code_chunks",
                        [],
                        |row: &rusqlite::Row| row.get(0),
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to count chunks: {}", e))?;

                Ok(count as usize)
            }),
        }
    }

    /// Count code chunks for a specific file.
    pub fn count_chunks_for_file(&self, file_path: &str) -> Result<usize> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => tables.count_chunks_for_file(file_path),
            _ => self.with_conn(|conn| {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM code_chunks WHERE file_path = ?1",
                        params![file_path],
                        |row: &rusqlite::Row| row.get(0),
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to count chunks for file: {}", e))?;

                Ok(count as usize)
            }),
        }
    }

    /// Get all code chunks from storage.
    ///
    /// For SQLite, queries the code_chunks table.
    /// For V3, uses SideTables trait method.
    pub fn get_all_chunks(&self) -> Result<Vec<CodeChunk>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(tables) => tables.get_all_chunks(),
            _ => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare_cached(
                        "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                                symbol_name, symbol_kind, created_at
                         FROM code_chunks
                         ORDER BY file_path, byte_start",
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to prepare query: {}", e))?;

                let chunks = stmt
                    .query_map([], |row: &rusqlite::Row| {
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
            }),
        }
    }

    /// Check if this ChunkStore is using KV backend
    ///
    /// This method always returns false since the KV backend was removed.
    pub fn has_kv_backend(&self) -> bool {
        false
    }

    /// Get chunks by symbol kind (e.g., "fn", "struct").
    pub fn get_chunks_by_kind(&self, symbol_kind: &str) -> Result<Vec<CodeChunk>> {
        match &self.backend {
            ChunkStoreBackend::SideTables(_) => {
                // V3 backend: filter from all chunks
                let all_chunks = self.get_all_chunks()?;
                Ok(all_chunks
                    .into_iter()
                    .filter(|c| c.symbol_kind.as_ref() == Some(&symbol_kind.to_string()))
                    .collect())
            }
            _ => self.with_conn(|conn| {
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
            }),
        }
    }
}

/// Build a text excerpt from content around the first match of the FTS query.
fn build_search_excerpt(content: &str, pattern: &str) -> String {
    let excerpt_len = 200usize;
    let tokens: Vec<&str> = pattern.split_whitespace().filter(|t| t.len() > 1).collect();

    for token in &tokens {
        if let Some(pos) = content.to_lowercase().find(&token.to_lowercase()) {
            let start = pos.saturating_sub(excerpt_len / 2);
            let end = (start + excerpt_len).min(content.len());
            let excerpt = &content[start..end];
            let prefix = if start > 0 { "..." } else { "" };
            let suffix = if end < content.len() { "..." } else { "" };
            return format!("{prefix}{excerpt}{suffix}");
        }
    }

    let end = excerpt_len.min(content.len());
    let excerpt = &content[..end];
    if content.len() > end {
        format!("{excerpt}...")
    } else {
        excerpt.to_string()
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

//! Code generation and storage module.
//!
//! This module provides functionality for storing and retrieving source code chunks
//! with their byte spans. This enables token-efficient queries by storing code
//! fragments in the database rather than re-reading entire files.
//!
//! # :memory: Database Path Retrieval
//!
//! ChunkStore uses SQLite Shared connections (via `Arc<Mutex<Connection>>`), which
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

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[cfg(feature = "native-v2")]
use sqlitegraph::backend::KvValue;

#[cfg(feature = "native-v2")]
use sqlitegraph::SnapshotId;

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
///
/// In native-v2 mode, can also use KV store for persistent chunk storage.
pub struct ChunkStore {
    /// Connection source - either owned path or shared connection
    conn_source: ChunkStoreConnection,

    /// KV backend for native-v2 mode (optional)
    #[cfg(feature = "native-v2")]
    kv_backend: Option<Rc<dyn sqlitegraph::GraphBackend>>,
}

impl ChunkStore {
    /// Create a new ChunkStore with the given database path.
    ///
    /// This is the legacy constructor that opens its own connections.
    pub fn new(db_path: &Path) -> Self {
        Self {
            conn_source: ChunkStoreConnection::Owned(db_path.to_path_buf()),
            #[cfg(feature = "native-v2")]
            kv_backend: None,
        }
    }

    /// Create a ChunkStore with a shared connection.
    ///
    /// This constructor enables transactional operations by using a connection
    /// shared with CodeGraph. All operations will use this shared connection.
    ///
    /// # Arguments
    /// * `conn` - Shared SQLite connection wrapped in Arc<Mutex<>> for thread-safe interior mutability
    pub fn with_connection(conn: rusqlite::Connection) -> Self {
        Self {
            conn_source: ChunkStoreConnection::Shared(Arc::new(Mutex::new(conn))),
            #[cfg(feature = "native-v2")]
            kv_backend: None,
        }
    }

    /// Create a ChunkStore with a KV backend for native-v2 mode.
    ///
    /// This constructor enables persistent chunk storage using the Native V2 backend's
    /// KV store. Chunks stored via this ChunkStore will persist across process restarts
    /// and can be retrieved by any ChunkStore instance using the same backend.
    ///
    /// # Arguments
    /// * `backend` - Graph backend (must be Native V2 for KV operations)
    ///
    /// # Example
    /// ```ignore
    /// let backend = sqlitegraph::NativeGraphBackend::in_memory();
    /// let chunk_store = ChunkStore::with_kv_backend(Rc::new(backend));
    /// ```
    #[cfg(feature = "native-v2")]
    pub fn with_kv_backend(backend: Rc<dyn sqlitegraph::GraphBackend>) -> Self {
        Self {
            conn_source: ChunkStoreConnection::Owned(PathBuf::from(":memory:")),
            kv_backend: Some(backend),
        }
    }

    /// Create a stub ChunkStore using a temporary file (for native-v2 mode).
    ///
    /// This is a compatibility shim for native-v2 mode where ChunkStore
    /// can use either KV storage (if backend provided) or a temporary file (fallback).
    ///
    /// Uses a temporary file so that new connections can access the same data.
    #[cfg(feature = "native-v2")]
    pub fn in_memory(kv_backend: Option<Rc<dyn sqlitegraph::GraphBackend>>) -> Self {
        // If KV backend is provided, use it instead of temp file
        if kv_backend.is_some() {
            return Self {
                conn_source: ChunkStoreConnection::Owned(PathBuf::from(":memory:")),
                kv_backend,
            };
        }

        // Fallback: Create a unique temporary file for each call
        // This prevents conflicts when multiple tests run concurrently
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
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
        ).expect("Failed to create code_chunks table in ChunkStore stub");

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
        ).expect("Failed to create ast_nodes table in ChunkStore stub");

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
                end_col INTEGER NOT NULL
            )",
            [],
        ).expect("Failed to create cfg_blocks table in ChunkStore stub");

        // Create indexes (use IF NOT EXISTS to avoid conflicts on reconnect)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON code_chunks(file_path)",
            [],
        ).expect("Failed to create file_path index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_symbol_name ON code_chunks(symbol_name)",
            [],
        ).expect("Failed to create symbol_name index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON code_chunks(content_hash)",
            [],
        ).expect("Failed to create content_hash index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent ON ast_nodes(parent_id)",
            [],
        ).expect("Failed to create ast_nodes parent index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span ON ast_nodes(byte_start, byte_end)",
            [],
        ).expect("Failed to create ast_nodes span index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_file_id ON ast_nodes(file_id)",
            [],
        ).expect("Failed to create ast_nodes file_id index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function ON cfg_blocks(function_id)",
            [],
        ).expect("Failed to create cfg_blocks function index");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_span ON cfg_blocks(byte_start, byte_end)",
            [],
        ).expect("Failed to create cfg_blocks span index");

        Self {
            conn_source: ChunkStoreConnection::Owned(db_path),
            kv_backend: None,
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
            ChunkStoreConnection::Shared(arc) => {
                // Open a new connection to the same database.
                // We need to extract the path from the existing connection.
                let conn = arc.lock().map_err(|_| {
                    rusqlite::Error::InvalidParameterName(
                        "Shared connection lock failed".to_string(),
                    )
                })?;
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
            ChunkStoreConnection::Shared(arc) => {
                let conn = arc
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Shared connection lock poisoned"))?;
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
            ChunkStoreConnection::Shared(arc) => {
                let mut conn = arc
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Shared connection lock poisoned"))?;
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
    ///
    /// In native-v2 mode with a KV backend, stores chunks in KV store for persistence.
    pub fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64> {
        #[cfg(feature = "native-v2")]
        {
            // Use KV backend if available
            if let Some(ref backend) = self.kv_backend {
                use crate::kv::keys::chunk_key;

                let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
                let json_value = serde_json::to_string(chunk)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize chunk: {}", e))?;

                backend.kv_set(key, KvValue::Json(serde_json::from_str(&json_value)?), None)?;

                // Return a dummy ID (KV doesn't have auto-increment IDs)
                return Ok(1);
            }
        }

        // Fallback to SQLite
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
    ///
    /// In native-v2 mode with a KV backend, stores chunks in KV store.
    /// Falls back to SQLite transaction for SQLite backend.
    pub fn store_chunks(&self, chunks: &[CodeChunk]) -> Result<Vec<i64>> {
        #[cfg(feature = "native-v2")]
        {
            // Use KV backend if available
            if let Some(ref backend) = self.kv_backend {
                use crate::kv::keys::chunk_key;

                let mut ids = Vec::new();
                for chunk in chunks {
                    let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
                    let json_value = serde_json::to_string(chunk)
                        .map_err(|e| anyhow::anyhow!("Failed to serialize chunk: {}", e))?;
                    backend.kv_set(key, KvValue::Json(serde_json::from_str(&json_value)?), None)?;
                    ids.push(1); // Dummy ID for KV mode
                }
                return Ok(ids);
            }
        }

        // Fallback to SQLite transaction
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
        #[cfg(feature = "native-v2")]
        {
            // Use KV backend if available
            if let Some(ref backend) = self.kv_backend {
                use crate::kv::keys::chunk_key;
                use crate::kv::encoding::decode_json;

                let key = chunk_key(file_path, byte_start, byte_end);
                let snapshot = SnapshotId::current();

                if let Ok(Some(KvValue::Json(json_value))) = backend.kv_get(snapshot, &key) {
                    let json_str = serde_json::to_string(&json_value)
                        .map_err(|e| anyhow::anyhow!("Failed to convert JSON value: {}", e))?;
                    let chunk: CodeChunk = decode_json(json_str.as_bytes())?;
                    return Ok(Some(chunk));
                }
                return Ok(None);
            }
        }

        // Fallback to SQLite
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
        #[cfg(feature = "native-v2")]
        {
            // Use KV backend if available
            if let Some(ref backend) = self.kv_backend {
                use crate::kv::encoding::decode_json;
                use sqlitegraph::{SnapshotId, backend::KvValue};

                let snapshot = SnapshotId::current();

                // Prefix scan for all chunks in the file
                // Key format: chunk:{file_path}:{start}:{end}
                // Need to escape colons in file_path
                let escaped_path = file_path.replace(':', "::");
                let prefix = format!("chunk:{}:", escaped_path);
                let entries = backend.kv_prefix_scan(snapshot, prefix.as_bytes())?;

                let mut chunks = Vec::new();
                for (_key, value) in entries {
                    if let KvValue::Json(json_value) = value {
                        let json_str = serde_json::to_string(&json_value)
                            .map_err(|e| anyhow::anyhow!("Failed to convert JSON value: {}", e))?;
                        let chunk: CodeChunk = decode_json(json_str.as_bytes())?;
                        // Filter by symbol_name
                        if chunk.symbol_name.as_deref() == Some(symbol_name) {
                            chunks.push(chunk);
                        }
                    }
                }
                return Ok(chunks);
            }
        }

        // Fallback to SQLite (original implementation)
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
        #[cfg(feature = "native-v2")]
        {
            // Use KV backend if available
            if let Some(ref backend) = self.kv_backend {
                use sqlitegraph::SnapshotId;

                // Prefix scan for all chunk:* keys
                let prefix = b"chunk:".to_vec();
                let snapshot = SnapshotId::current();

                let entries = backend.kv_prefix_scan(snapshot, &prefix)
                    .map_err(|e| anyhow::anyhow!("Failed to scan chunks: {}", e))?;

                return Ok(entries.len());
            }
        }

        // Fallback to SQLite
        self.with_conn(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM code_chunks",
                    [],
                    |row: &rusqlite::Row| row.get(0),
                )
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
                    |row: &rusqlite::Row| row.get(0),
                )
                .map_err(|e| anyhow::anyhow!("Failed to count chunks for file: {}", e))?;

            Ok(count as usize)
        })
    }

    /// Get all code chunks from storage.
    ///
    /// For Native-V2 with KV backend, uses prefix scan on chunk: keys.
    /// For SQLite, queries the code_chunks table.
    #[cfg(feature = "native-v2")]
    pub fn get_all_chunks(&self) -> Result<Vec<CodeChunk>> {
        use sqlitegraph::{SnapshotId, backend::KvValue};
        use crate::kv::encoding::decode_json;

        if let Some(ref backend) = self.kv_backend {
            let snapshot = SnapshotId::current();
            let entries = backend.kv_prefix_scan(snapshot, b"chunk:")?;

            let mut chunks = Vec::new();
            for (_key, value) in entries {
                if let KvValue::Json(json_value) = value {
                    let json_str = serde_json::to_string(&json_value)
                        .map_err(|e| anyhow::anyhow!("Failed to convert JSON value: {}", e))?;
                    let chunk: CodeChunk = decode_json(json_str.as_bytes())?;
                    chunks.push(chunk);
                }
            }
            Ok(chunks)
        } else {
            // SQLite fallback - query all chunks
            self.with_conn(|conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks
                     ORDER BY file_path, byte_start"
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
            })
        }
    }

    /// Get all code chunks from storage.
    ///
    /// For SQLite, queries the code_chunks table.
    #[cfg(not(feature = "native-v2"))]
    pub fn get_all_chunks(&self) -> Result<Vec<CodeChunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks
                 ORDER BY file_path, byte_start"
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
        })
    }

    /// Check if this ChunkStore is using KV backend (Native-V2)
    ///
    /// This method allows AST operations to check at runtime whether they should
    /// use KV store (Native-V2) or SQL queries (SQLite).
    pub fn has_kv_backend(&self) -> bool {
        #[cfg(feature = "native-v2")]
        {
            self.kv_backend.is_some()
        }
        #[cfg(not(feature = "native-v2"))]
        {
            false
        }
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

    /// Migrate code chunks from SQLite to KV store.
    ///
    /// This static method reads all chunks from a SQLite database and stores them
    /// in the KV store. Used during backend migration to preserve chunk data.
    ///
    /// # Arguments
    /// * `sqlite_db_path` - Path to the SQLite database containing code_chunks table
    /// * `kv_backend` - KV backend to store chunks in
    ///
    /// # Returns
    /// Number of chunks migrated
    ///
    /// # Errors
    /// - Cannot open SQLite database
    /// - Query fails
    /// - KV storage fails
    ///
    /// # Example
    /// ```ignore
    /// let count = ChunkStore::migrate_chunks_to_kv(&db_path, &backend)?;
    /// println!("Migrated {} chunks", count);
    /// ```
    #[cfg(feature = "native-v2")]
    pub fn migrate_chunks_to_kv(
        sqlite_db_path: &Path,
        kv_backend: Rc<dyn sqlitegraph::GraphBackend>,
    ) -> Result<usize> {
        use crate::kv::keys::chunk_key;
        use sqlitegraph::backend::KvValue;

        // Open SQLite connection
        let conn = rusqlite::Connection::open(sqlite_db_path)?;

        // Query all chunks
        let mut stmt = conn.prepare(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash, symbol_name, symbol_kind, created_at
             FROM code_chunks"
        )?;

        let chunks = stmt.query_map([], |row| {
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
        })?;

        let mut count = 0;
        for chunk_result in chunks {
            let chunk = chunk_result.map_err(|e| anyhow::anyhow!("Failed to read chunk: {}", e))?;

            // Store in KV using encode_json
            let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
            let json = serde_json::to_vec(&chunk)
                .map_err(|e| anyhow::anyhow!("Failed to serialize chunk: {}", e))?;
            kv_backend.kv_set(key, KvValue::Bytes(json), None)?;
            count += 1;
        }

        Ok(count)
    }

    /// Store CFG blocks for a function in KV store (native-v2 mode only).
    ///
    /// This method stores CFG blocks using the KV backend if available.
    /// Falls back gracefully if KV backend is not configured.
    ///
    /// # Arguments
    /// * `function_id` - Database ID of the function
    /// * `blocks` - Slice of CFG blocks to store
    ///
    /// # Returns
    /// Result<()> indicating success or failure
    #[cfg(feature = "native-v2")]
    pub fn store_cfg_blocks(
        &self,
        function_id: i64,
        blocks: &[crate::graph::CfgBlock],
    ) -> Result<()> {
        if let Some(ref backend) = self.kv_backend {
            crate::graph::store_cfg_blocks_kv(
                std::rc::Rc::clone(backend),
                function_id,
                blocks,
            )
        } else {
            // No KV backend available - silently skip
            Ok(())
        }
    }

    /// Retrieve CFG blocks for a function from KV store (native-v2 mode only).
    ///
    /// This method retrieves CFG blocks using the KV backend if available.
    /// Returns empty vector if KV backend is not configured.
    ///
    /// # Arguments
    /// * `function_id` - Database ID of the function
    ///
    /// # Returns
    /// Result<Vec<CfgBlock>> containing the retrieved blocks
    #[cfg(feature = "native-v2")]
    pub fn get_cfg_blocks(
        &self,
        function_id: i64,
    ) -> Result<Vec<crate::graph::CfgBlock>> {
        if let Some(ref backend) = self.kv_backend {
            crate::graph::get_cfg_blocks_kv(backend.as_ref(), function_id)
        } else {
            // No KV backend available - return empty vector
            Ok(vec![])
        }
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

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_chunk_store_kv_roundtrip() {
        use sqlitegraph::NativeGraphBackend;

        // Create a test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(NativeGraphBackend::new(&db_path).unwrap());
        let chunk_store = ChunkStore::with_kv_backend(Rc::clone(&backend));

        // Create a test chunk
        let chunk = CodeChunk::new(
            "src/test.rs".to_string(),
            100,
            200,
            "fn test_function() {}".to_string(),
            Some("test_function".to_string()),
            Some("fn".to_string()),
        );

        // Store the chunk via KV backend
        let result = chunk_store.store_chunk(&chunk);
        assert!(result.is_ok(), "store_chunk should succeed");

        // Retrieve the chunk by span
        let retrieved = chunk_store.get_chunk_by_span("src/test.rs", 100, 200);
        assert!(retrieved.is_ok(), "get_chunk_by_span should succeed");

        let retrieved_chunk = retrieved.unwrap();
        assert!(retrieved_chunk.is_some(), "chunk should exist");
        let retrieved_chunk = retrieved_chunk.unwrap();

        // Verify content matches
        assert_eq!(retrieved_chunk.file_path, "src/test.rs");
        assert_eq!(retrieved_chunk.byte_start, 100);
        assert_eq!(retrieved_chunk.byte_end, 200);
        assert_eq!(retrieved_chunk.content, "fn test_function() {}");
        assert_eq!(retrieved_chunk.symbol_name, Some("test_function".to_string()));
        assert_eq!(retrieved_chunk.symbol_kind, Some("fn".to_string()));
        assert_eq!(retrieved_chunk.content_hash, chunk.content_hash);
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_chunk_store_kv_persistence() {
        use sqlitegraph::NativeGraphBackend;

        // Create a test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(NativeGraphBackend::new(&db_path).unwrap());

        // Create first ChunkStore instance
        let chunk_store1 = ChunkStore::with_kv_backend(Rc::clone(&backend));

        // Create and store a chunk
        let chunk = CodeChunk::new(
            "src/persist.rs".to_string(),
            0,
            50,
            "fn persistent() {}".to_string(),
            Some("persistent".to_string()),
            Some("fn".to_string()),
        );

        chunk_store1.store_chunk(&chunk).unwrap();

        // Drop first ChunkStore
        drop(chunk_store1);

        // Create second ChunkStore instance with same backend
        let chunk_store2 = ChunkStore::with_kv_backend(Rc::clone(&backend));

        // Verify chunk is still retrievable
        let retrieved = chunk_store2.get_chunk_by_span("src/persist.rs", 0, 50).unwrap();
        assert!(retrieved.is_some(), "chunk should persist across ChunkStore instances");

        let retrieved_chunk = retrieved.unwrap();
        assert_eq!(retrieved_chunk.content, "fn persistent() {}");
        assert_eq!(retrieved_chunk.symbol_name, Some("persistent".to_string()));
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_chunk_store_kv_by_symbol() {
        use sqlitegraph::NativeGraphBackend;

        // Create a test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(NativeGraphBackend::new(&db_path).unwrap());
        let chunk_store = ChunkStore::with_kv_backend(Rc::clone(&backend));

        // Store multiple chunks for the same file
        let chunk1 = CodeChunk::new(
            "src/multi.rs".to_string(),
            0,
            30,
            "fn func1() {}".to_string(),
            Some("func1".to_string()),
            Some("fn".to_string()),
        );

        let chunk2 = CodeChunk::new(
            "src/multi.rs".to_string(),
            30,
            60,
            "fn func2() {}".to_string(),
            Some("func2".to_string()),
            Some("fn".to_string()),
        );

        let chunk3 = CodeChunk::new(
            "src/multi.rs".to_string(),
            60,
            90,
            "struct MyStruct {}".to_string(),
            Some("MyStruct".to_string()),
            Some("struct".to_string()),
        );

        chunk_store.store_chunk(&chunk1).unwrap();
        chunk_store.store_chunk(&chunk2).unwrap();
        chunk_store.store_chunk(&chunk3).unwrap();

        // Note: get_chunks_for_symbol uses SQLite fallback, not KV
        // This test verifies individual chunk retrieval works
        let retrieved1 = chunk_store.get_chunk_by_span("src/multi.rs", 0, 30).unwrap();
        assert!(retrieved1.is_some());
        assert_eq!(retrieved1.unwrap().symbol_name, Some("func1".to_string()));

        let retrieved2 = chunk_store.get_chunk_by_span("src/multi.rs", 30, 60).unwrap();
        assert!(retrieved2.is_some());
        assert_eq!(retrieved2.unwrap().symbol_name, Some("func2".to_string()));

        let retrieved3 = chunk_store.get_chunk_by_span("src/multi.rs", 60, 90).unwrap();
        assert!(retrieved3.is_some());
        assert_eq!(retrieved3.unwrap().symbol_name, Some("MyStruct".to_string()));
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_cfg_integration() {
        use crate::graph::CfgBlock;
        use sqlitegraph::NativeGraphBackend;

        // Create a test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: Rc<dyn sqlitegraph::GraphBackend> = Rc::new(NativeGraphBackend::new(&db_path).unwrap());
        let chunk_store = ChunkStore::with_kv_backend(Rc::clone(&backend));

        // Create sample CFG blocks
        let blocks = vec![
            CfgBlock {
                function_id: 1,
                kind: "entry".to_string(),
                terminator: "fallthrough".to_string(),
                byte_start: 0,
                byte_end: 100,
                start_line: 1,
                start_col: 0,
                end_line: 5,
                end_col: 0,
            },
            CfgBlock {
                function_id: 1,
                kind: "if".to_string(),
                terminator: "conditional".to_string(),
                byte_start: 100,
                byte_end: 200,
                start_line: 5,
                start_col: 0,
                end_line: 10,
                end_col: 0,
            },
        ];

        // Store CFG blocks via ChunkStore
        let result = chunk_store.store_cfg_blocks(1, &blocks);
        assert!(result.is_ok(), "store_cfg_blocks should succeed");

        // Retrieve CFG blocks via ChunkStore
        let retrieved = chunk_store.get_cfg_blocks(1);
        assert!(retrieved.is_ok(), "get_cfg_blocks should succeed");

        let retrieved_blocks = retrieved.unwrap();
        assert_eq!(retrieved_blocks.len(), blocks.len());
        assert_eq!(retrieved_blocks[0].kind, "entry");
        assert_eq!(retrieved_blocks[1].kind, "if");
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_cfg_integration_no_backend() {
        use crate::graph::CfgBlock;

        // Create ChunkStore without KV backend
        let chunk_store = ChunkStore::new(Path::new(":memory:"));

        // Create sample CFG blocks
        let blocks = vec![
            CfgBlock {
                function_id: 1,
                kind: "entry".to_string(),
                terminator: "fallthrough".to_string(),
                byte_start: 0,
                byte_end: 100,
                start_line: 1,
                start_col: 0,
                end_line: 5,
                end_col: 0,
            },
        ];

        // Store should succeed (graceful fallback)
        let result = chunk_store.store_cfg_blocks(1, &blocks);
        assert!(result.is_ok(), "store_cfg_blocks should succeed without backend");

        // Retrieve should return empty vector
        let retrieved = chunk_store.get_cfg_blocks(1);
        assert!(retrieved.is_ok(), "get_cfg_blocks should succeed without backend");

        let retrieved_blocks = retrieved.unwrap();
        assert_eq!(retrieved_blocks.len(), 0, "should return empty vector without backend");
    }
}

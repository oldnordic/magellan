//! Cross-backend database migration command implementation
//!
//! Handles migration between SQLite backends (copy with side table migration).
//!
//! ## Architecture
//!
//! This module uses sqlitegraph's `GraphBackend::snapshot_export()` and
//! `snapshot_import()` methods for graph data migration. The wrapper only
//! handles directory creation and returns Magellan-specific metadata.
//!
//! ## Side Tables
//!
//! GraphBackend does NOT handle Magellan-specific side tables:
//! - `code_chunks` - Code snippets stored in generation module
//! - `file_metrics`, `symbol_metrics` - Pre-computed metrics
//! - `execution_log` - Command execution tracking
//! - `ast_nodes` - AST hierarchy storage
//! - `cfg_blocks` - Control flow graph data
//!
//! These must be migrated separately via direct SQL.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use sqlitegraph::GraphBackend;



/// Database backend format
///
/// Currently only SQLite is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendFormat {
    /// SQLite database
    Sqlite,
}

/// Errors that can occur during backend format detection
///
/// These errors provide specific feedback for migration failures,
/// helping users understand what went wrong.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// Database file does not exist at the specified path
    #[error("Database not found: {path}")]
    DatabaseNotFound {
        path: PathBuf,
    },

    /// Cannot open the database file (permission denied, locked, etc.)
    #[error("Cannot open database '{path}': {source}")]
    CannotOpenDatabase {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// In-memory databases cannot be migrated (no file path)
    #[error("In-memory databases (':memory:') are not supported for migration. Use a file-based database.")]
    InMemoryDatabaseNotSupported,

    /// File is not a valid SQLite database
    #[error("Not a valid SQLite database: {path}. Details: {details}")]
    InvalidDatabase {
        path: PathBuf,
        details: String,
    },
}

/// Detect the backend format of a database file
///
/// Currently only SQLite format is supported.
///
/// # Arguments
/// * `db_path` - Path to the database file to detect
///
/// # Returns
/// `Ok(BackendFormat::Sqlite)` if valid SQLite, or `Err(MigrationError)` if detection fails
pub fn detect_backend_format(db_path: &Path) -> Result<BackendFormat, MigrationError> {
    // Check for in-memory database path
    if db_path.to_str() == Some(":memory:") {
        return Err(MigrationError::InMemoryDatabaseNotSupported);
    }

    // Check if file exists
    if !db_path.exists() {
        return Err(MigrationError::DatabaseNotFound {
            path: db_path.to_path_buf(),
        });
    }

    // Try to open as SQLite database (read-only to avoid creating new file)
    rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map(|_| BackendFormat::Sqlite)
    .map_err(|e| MigrationError::InvalidDatabase {
        path: db_path.to_path_buf(),
        details: e.to_string(),
    })
}

/// Snapshot export metadata returned by export_snapshot
///
/// Contains entity/edge counts for verification and export directory info.
#[derive(Debug, Clone)]
pub struct SnapshotExportMetadata {
    /// Number of entities in the exported snapshot
    pub entity_count: i64,

    /// Number of edges in the exported snapshot
    pub edge_count: i64,

    /// Directory where snapshot files were written
    pub export_dir: PathBuf,

    /// Unix timestamp of export
    pub export_timestamp: i64,

    /// Size of the snapshot file in bytes (from backend)
    pub size_bytes: u64,
}

/// Snapshot import metadata returned by import_snapshot
///
/// Contains entity/edge counts for verification and source directory info.
#[derive(Debug, Clone)]
pub struct SnapshotImportMetadata {
    /// Number of entities imported from the snapshot
    pub entities_imported: i64,

    /// Number of edges imported from the snapshot
    pub edges_imported: i64,

    /// Directory where snapshot files were read from
    pub source_dir: PathBuf,

    /// Unix timestamp of import
    pub import_timestamp: i64,
}

/// Export graph data from a backend to a snapshot directory
///
/// This function wraps `GraphBackend::snapshot_export()` to provide
/// Magellan-specific metadata. The actual serialization is handled
/// by the backend (SQLite → JSON format).
///
/// # Arguments
/// * `backend` - Graph backend to export from
/// * `export_dir` - Directory where snapshot files will be written
///
/// # Returns
/// SnapshotExportMetadata with counts, size, and timestamp
pub fn export_snapshot(
    backend: &Rc<dyn GraphBackend>,
    export_dir: &Path,
) -> Result<SnapshotExportMetadata> {
    // Create export directory if it doesn't exist
    fs::create_dir_all(export_dir)?;

    // Delegate to backend-specific export implementation
    let snapshot_meta = backend.snapshot_export(export_dir)?;

    // Get current timestamp
    let export_timestamp = chrono::Utc::now().timestamp();

    Ok(SnapshotExportMetadata {
        entity_count: snapshot_meta.entity_count as i64,
        edge_count: snapshot_meta.edge_count as i64,
        export_dir: export_dir.to_path_buf(),
        export_timestamp,
        size_bytes: snapshot_meta.size_bytes,
    })
}

/// Import graph data from a snapshot directory into a backend
///
/// This function imports a previously exported snapshot into the target backend.
/// The import creates or overwrites graph data based on the snapshot contents.
///
/// # Arguments
/// * `backend` - The graph backend to import into
/// * `snapshot_dir` - Directory containing the snapshot files to import
pub fn import_snapshot(
    backend: &Rc<dyn GraphBackend>,
    snapshot_dir: &Path,
) -> Result<SnapshotImportMetadata> {
    // Verify snapshot_dir exists
    if !snapshot_dir.exists() {
        return Err(anyhow::anyhow!(
            "Snapshot directory '{}' does not exist",
            snapshot_dir.display()
        ));
    }

    // Verify snapshot_dir is a directory
    let metadata = fs::metadata(snapshot_dir).map_err(|e| {
        anyhow::anyhow!("Cannot access snapshot directory '{}': {}", snapshot_dir.display(), e)
    })?;
    if !metadata.is_dir() {
        return Err(anyhow::anyhow!(
            "Snapshot path '{}' is not a directory",
            snapshot_dir.display()
        ));
    }

    // Delegate to backend's snapshot_import implementation
    let import_meta = backend.snapshot_import(snapshot_dir)?;

    // Get current timestamp
    let import_timestamp = chrono::Utc::now().timestamp();

    Ok(SnapshotImportMetadata {
        entities_imported: import_meta.entities_imported as i64,
        edges_imported: import_meta.edges_imported as i64,
        source_dir: snapshot_dir.to_path_buf(),
        import_timestamp,
    })
}

/// Verify that imported counts match exported counts
pub fn verify_import_counts(
    export_meta: &SnapshotExportMetadata,
    import_meta: &SnapshotImportMetadata,
) -> Result<()> {
    if export_meta.entity_count != import_meta.entities_imported {
        return Err(anyhow::anyhow!(
            "Entity count mismatch: export had {} entities, but import loaded {} entities",
            export_meta.entity_count,
            import_meta.entities_imported
        ));
    }

    if export_meta.edge_count != import_meta.edges_imported {
        return Err(anyhow::anyhow!(
            "Edge count mismatch: export had {} edges, but import loaded {} edges",
            export_meta.edge_count,
            import_meta.edges_imported
        ));
    }

    Ok(())
}

/// Get entity and edge counts from a graph backend
pub fn get_graph_counts(_backend: &Rc<dyn GraphBackend>) -> Result<(i64, i64)> {
    // Placeholder - actual counts available from snapshot_export() return value
    Ok((0, 0))
}

/// Result of a backend migration operation
#[derive(Debug, Clone)]
pub struct BackendMigrationResult {
    /// Whether the migration completed successfully
    pub success: bool,
    /// Format of the source database
    pub source_format: BackendFormat,
    /// Format of the target database
    pub target_format: BackendFormat,
    /// Number of entities migrated
    pub entities_migrated: i64,
    /// Number of edges migrated
    pub edges_migrated: i64,
    /// Whether side tables were migrated
    pub side_tables_migrated: bool,
    /// Human-readable status message
    pub message: String,
}

impl std::fmt::Display for BackendMigrationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if self.success {
            write!(
                f,
                "\nFormat: {:?} -> {:?}\nEntities: {}\nEdges: {}",
                self.source_format, self.target_format, self.entities_migrated, self.edges_migrated
            )?;
            if self.side_tables_migrated {
                write!(f, "\nSide tables: migrated")?;
            }
        }
        Ok(())
    }
}

/// Run a complete backend migration from source to target database
///
/// This function orchestrates the full migration pipeline:
/// 1. Open source SQLite database
/// 2. Export graph data to snapshot format  
/// 3. Create new target SQLite database
/// 4. Import snapshot into target database
/// 5. Migrate Magellan-specific side tables
pub fn run_migrate_backend(
    input_db: PathBuf,
    output_db: PathBuf,
    export_dir: Option<PathBuf>,
    dry_run: bool,
) -> Result<BackendMigrationResult> {
    // Validate input_db exists
    if !input_db.exists() {
        return Ok(BackendMigrationResult {
            success: false,
            source_format: BackendFormat::Sqlite,
            target_format: BackendFormat::Sqlite,
            entities_migrated: 0,
            edges_migrated: 0,
            side_tables_migrated: false,
            message: format!("Input database not found: {}", input_db.display()),
        });
    }

    // Open source backend
    let source_backend: Rc<dyn GraphBackend> = {
        use sqlitegraph::{SqliteGraph, SqliteGraphBackend};
        let sqlite_graph = SqliteGraph::open(&input_db).map_err(|e| {
            anyhow::anyhow!(
                "Failed to open SQLite database '{}': {}",
                input_db.display(),
                e
            )
        })?;
        Rc::new(SqliteGraphBackend::from_graph(sqlite_graph))
    };

    // Dry run: just return success
    if dry_run {
        return Ok(BackendMigrationResult {
            success: true,
            source_format: BackendFormat::Sqlite,
            target_format: BackendFormat::Sqlite,
            entities_migrated: 0,
            edges_migrated: 0,
            side_tables_migrated: false,
            message: format!(
                "Would migrate from {:?} to {:?} (dry run)",
                input_db.display(),
                output_db.display()
            ),
        });
    }

    // Default export_dir to temp directory if not provided
    let export_dir = export_dir.unwrap_or_else(|| {
        let timestamp_val = chrono::Utc::now().timestamp();
        std::env::temp_dir().join(format!("magellan_migration_{}", timestamp_val))
    });

    // Create export directory
    fs::create_dir_all(&export_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create export directory '{}': {}",
            export_dir.display(),
            e
        )
    })?;

    // Export snapshot from source backend
    let export_meta = export_snapshot(&source_backend, &export_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to export snapshot from '{}': {}",
            input_db.display(),
            e
        )
    })?;

    // Create target backend (SQLite)
    let target_backend: Rc<dyn GraphBackend> = {
        use sqlitegraph::{SqliteGraph, SqliteGraphBackend};
        let sqlite_graph = SqliteGraph::open(&output_db).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create SQLite database '{}': {}",
                output_db.display(),
                e
            )
        })?;
        Rc::new(SqliteGraphBackend::from_graph(sqlite_graph))
    };

    // Import snapshot into target backend
    let import_meta = import_snapshot(&target_backend, &export_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to import snapshot into '{}': {}",
            output_db.display(),
            e
        )
    })?;

    // Verify data integrity (counts should match)
    verify_import_counts(&export_meta, &import_meta).map_err(|e| {
        anyhow::anyhow!(
            "Data integrity verification failed: {}. Target database may be incomplete.",
            e
        )
    })?;

    // Drop backend connections before migrating side tables
    drop(source_backend);
    drop(target_backend);

    // Migrate Magellan-specific side tables
    let side_tables_migrated =
        migrate_side_tables(&input_db, &output_db).map_err(|e| {
            anyhow::anyhow!(
                "Failed to migrate side tables from '{}' to '{}': {}",
                input_db.display(),
                output_db.display(),
                e
            )
        })?;

    // Clean up export directory (optional - keeping for now for debugging)
    // let _ = fs::remove_dir_all(&export_dir);

    Ok(BackendMigrationResult {
        success: true,
        source_format: BackendFormat::Sqlite,
        target_format: BackendFormat::Sqlite,
        entities_migrated: import_meta.entities_imported,
        edges_migrated: import_meta.edges_imported,
        side_tables_migrated,
        message: format!(
            "Migration complete: {} -> {}",
            input_db.display(),
            output_db.display()
        ),
    })
}

/// Migrate Magellan-specific side tables from source to target database
pub fn migrate_side_tables(source_db: &Path, target_db: &Path) -> Result<bool> {
    use rusqlite::Connection;

    // Open source connection
    let source_conn = Connection::open(source_db).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open source database '{}': {}",
            source_db.display(),
            e
        )
    })?;

    // Open target connection
    let target_conn = Connection::open(target_db).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open target database '{}': {}",
            target_db.display(),
            e
        )
    })?;

    // Define all side tables to migrate
    let side_tables = [
        "code_chunks",
        "file_metrics",
        "symbol_metrics",
        "execution_log",
        "ast_nodes",
        "cfg_blocks",
    ];

    let mut any_migrated = false;

    // Start transaction on target for atomicity
    let tx = target_conn.unchecked_transaction().map_err(|e| {
        anyhow::anyhow!("Failed to start transaction on target database: {}", e)
    })?;

    // For each table, copy data from source to target
    for table_name in &side_tables {
        // Check if table exists in source database
        let table_exists: bool = source_conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=? LIMIT 1",
                [table_name],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !table_exists {
            continue;
        }

        // Get row count
        let row_count: i64 = source_conn
            .query_row(&format!("SELECT COUNT(*) FROM {}", table_name), [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if row_count == 0 {
            continue;
        }

        // Ensure schema exists in target database
        ensure_table_schema(&tx, table_name).map_err(|e| {
            anyhow::anyhow!(
                "Failed to ensure schema for table '{}': {}",
                table_name,
                e
            )
        })?;

        // Get column names
        let mut columns: Vec<String> = Vec::new();
        {
            let mut stmt = source_conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(1)?;
                Ok(name)
            })?;
            for row in rows {
                columns.push(row?);
            }
        }

        if columns.is_empty() {
            continue;
        }

        let column_list = columns.join(", ");

        // Read rows from source and insert into target
        let mut select_stmt = source_conn.prepare(&format!("SELECT * FROM {}", table_name))?;

        let rows = select_stmt.query_map([], |row| {
            let mut values = Vec::new();
            for i in 0..columns.len() {
                let value = row.get::<_, rusqlite::types::Value>(i).ok();
                values.push(value);
            }
            Ok(values)
        })?;

        let mut copied = 0;

        for row_result in rows {
            let row_values = row_result.map_err(|e| {
                anyhow::anyhow!("Failed to read row from table '{}': {}", table_name, e)
            })?;

            let placeholders = (0..row_values.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");

            let insert_sql = format!(
                "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                table_name, column_list, placeholders
            );

            let tosql_refs: Vec<&dyn rusqlite::ToSql> = row_values
                .iter()
                .map(|v| match v {
                    Some(val) => val as &dyn rusqlite::ToSql,
                    None => &rusqlite::types::Value::Null as &dyn rusqlite::ToSql,
                })
                .collect();

            tx.execute(&insert_sql, tosql_refs.as_slice()).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to insert row into table '{}': {}",
                    table_name,
                    e
                )
            })?;
            copied += 1;
        }

        if copied > 0 {
            any_migrated = true;
        }
    }

    // Commit transaction
    tx.commit().map_err(|e| {
        anyhow::anyhow!(
            "Failed to commit side table migration transaction: {}",
            e
        )
    })?;

    Ok(any_migrated)
}

/// Ensure table schema exists in the target database
fn ensure_table_schema(conn: &rusqlite::Connection, table_name: &str) -> Result<()> {
    match table_name {
        "code_chunks" => {
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

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON code_chunks(file_path)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_symbol ON code_chunks(symbol_name, symbol_kind)",
                [],
            )?;
            Ok(())
        }
        "file_metrics" => {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS file_metrics (
                    file_path TEXT PRIMARY KEY,
                    symbol_count INTEGER NOT NULL,
                    loc INTEGER NOT NULL,
                    estimated_loc REAL NOT NULL,
                    fan_in INTEGER NOT NULL DEFAULT 0,
                    fan_out INTEGER NOT NULL DEFAULT 0,
                    complexity_score REAL NOT NULL DEFAULT 0.0,
                    last_updated INTEGER NOT NULL
                )",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create file_metrics table: {}", e))?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_file_metrics_complexity ON file_metrics(complexity_score DESC)",
                [],
            )?;
            Ok(())
        }
        "symbol_metrics" => {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS symbol_metrics (
                    symbol_id INTEGER PRIMARY KEY,
                    symbol_name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    loc INTEGER NOT NULL,
                    estimated_loc REAL NOT NULL,
                    fan_in INTEGER NOT NULL DEFAULT 0,
                    fan_out INTEGER NOT NULL DEFAULT 0,
                    cyclomatic_complexity INTEGER NOT NULL DEFAULT 1,
                    last_updated INTEGER NOT NULL,
                    FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE
                )",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create symbol_metrics table: {}", e))?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_in ON symbol_metrics(fan_in DESC)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_out ON symbol_metrics(fan_out DESC)",
                [],
            )?;
            Ok(())
        }
        "execution_log" => {
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
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_execution_log_started_at ON execution_log(started_at DESC)",
                [],
            )?;
            Ok(())
        }
        "ast_nodes" => {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS ast_nodes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_id INTEGER,
                    parent_id INTEGER,
                    kind TEXT NOT NULL,
                    byte_start INTEGER NOT NULL,
                    byte_end INTEGER NOT NULL
                )",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create ast_nodes table: {}", e))?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent ON ast_nodes(parent_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span ON ast_nodes(byte_start, byte_end)",
                [],
            )?;
            Ok(())
        }
        "cfg_blocks" => {
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
            )
            .map_err(|e| anyhow::anyhow!("Failed to create cfg_blocks table: {}", e))?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function ON cfg_blocks(function_id)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_span ON cfg_blocks(byte_start, byte_end)",
                [],
            )?;
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_terminator ON cfg_blocks(terminator)",
                [],
            )?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Returns current Unix timestamp
fn timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_backend_format_sqlite() {
        let temp_dir = TempDir::new().unwrap();
        let sqlite_db = temp_dir.path().join("test.db");

        // Create a SQLite database
        let conn = rusqlite::Connection::open(&sqlite_db).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER)", []).unwrap();
        drop(conn);

        let format = detect_backend_format(&sqlite_db).unwrap();
        assert_eq!(format, BackendFormat::Sqlite);
    }

    #[test]
    fn test_detect_backend_format_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent.db");

        let result = detect_backend_format(&nonexistent);
        assert!(matches!(result, Err(MigrationError::DatabaseNotFound { .. })));
    }

    #[test]
    fn test_detect_backend_format_in_memory() {
        let result = detect_backend_format(Path::new(":memory:"));
        assert!(matches!(result, Err(MigrationError::InMemoryDatabaseNotSupported)));
    }

    #[test]
    fn test_backend_format_equality() {
        assert_eq!(BackendFormat::Sqlite, BackendFormat::Sqlite);
    }

    #[test]
    fn test_backend_format_debug() {
        assert!(!format!("{:?}", BackendFormat::Sqlite).is_empty());
    }
}

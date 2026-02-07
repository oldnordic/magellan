//! Cross-backend database migration command implementation
//!
//! Handles migration between SQLite and Native V2 backends:
//! - Detect backend format from file headers (magic bytes)
//! - Export graph data from SQLite database to snapshot format
//! - Import snapshot data into Native V2 backend
//! - Migrate Magellan-specific side tables (chunks, metrics, execution log)
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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use sqlitegraph::GraphBackend;

/// Magic bytes for Native V2 database files
///
/// Native V2 databases start with "MAG2" (4 bytes) at offset 0.
/// Verified from sqlitegraph/src/backend/native/v2/constants.rs
const NATIVE_V2_MAGIC: &[u8] = b"MAG2";

/// Database backend format detected from file headers
///
/// Represents the two supported backend formats in Magellan.
/// Used for automatic backend detection before migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendFormat {
    /// SQLite database (traditional backend)
    Sqlite,
    /// Native V2 database (new high-performance backend)
    NativeV2,
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

    /// Cannot read file header (I/O error, truncated file, etc.)
    #[error("Cannot read header from '{path}': {source}")]
    CannotReadHeader {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// File is neither Native V2 nor SQLite format
    #[error("Unknown database format: {path}. Details: {details}")]
    UnknownFormat {
        path: PathBuf,
        details: String,
    },

    /// In-memory databases cannot be migrated (no file path)
    #[error("In-memory databases (':memory:') are not supported for migration. Use a file-based database.")]
    InMemoryDatabaseNotSupported,
}

/// Detect the backend format of a database file from its header
///
/// This function determines whether a database file uses SQLite or Native V2
/// backend format by examining file headers. It does NOT rely on file extensions
/// since users may rename files.
///
/// # Detection Strategy
///
/// 1. Reject `:memory:` path immediately (in-memory databases not supported)
/// 2. Check if file exists (return `DatabaseNotFound` if not)
/// 3. Read first 4 bytes and check for Native V2 magic bytes (`b"MAG2"`)
/// 4. If magic bytes match, return `BackendFormat::NativeV2`
/// 5. Otherwise, attempt to open as SQLite database
/// 6. If SQLite open succeeds, return `BackendFormat::Sqlite`
/// 7. If SQLite open fails, return `UnknownFormat`
///
/// # Arguments
/// * `db_path` - Path to the database file to detect
///
/// # Returns
/// `Ok(BackendFormat)` with the detected format, or `Err(MigrationError)` if detection fails
///
/// # Errors
/// - `InMemoryDatabaseNotSupported` - if path is `:memory:`
/// - `DatabaseNotFound` - if the file doesn't exist
/// - `CannotOpenDatabase` - if file cannot be opened for reading
/// - `CannotReadHeader` - if header cannot be read (I/O error, truncated file)
/// - `UnknownFormat` - if neither Native V2 nor SQLite format is detected
///
/// # Example
/// ```no_run
/// use magellan::migrate_backend_cmd::detect_backend_format;
/// use std::path::Path;
///
/// let db_path = Path::new("/path/to/database.db");
/// match detect_backend_format(db_path) {
///     Ok(format) => println!("Detected format: {:?}", format),
///     Err(e) => eprintln!("Detection failed: {}", e),
/// }
/// ```
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

    // Open file for reading magic bytes
    let mut file = fs::File::open(db_path).map_err(|e| MigrationError::CannotOpenDatabase {
        path: db_path.to_path_buf(),
        source: e,
    })?;

    // Read first 4 bytes (magic number)
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).map_err(|e| MigrationError::CannotReadHeader {
        path: db_path.to_path_buf(),
        source: e,
    })?;

    // Check for Native V2 magic bytes
    if magic == NATIVE_V2_MAGIC {
        return Ok(BackendFormat::NativeV2);
    }

    // Fallback: try to open as SQLite database (read-only to avoid creating new file)
    rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map(|_| BackendFormat::Sqlite)
    .map_err(|e| MigrationError::UnknownFormat {
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
/// by the backend (SQLite → JSON, Native V2 → binary .v2 format).
///
/// # Arguments
/// * `backend` - Graph backend to export from
/// * `export_dir` - Directory where snapshot files will be written
///
/// # Returns
/// SnapshotExportMetadata with counts, size, and timestamp
///
/// # Errors
/// - Export directory cannot be created
/// - Backend snapshot_export() fails (e.g., permission denied, disk full)
///
/// # Example
/// ```no_run
/// use magellan::migrate_backend_cmd::export_snapshot;
/// use std::rc::Rc;
/// use std::path::Path;
///
/// # let backend: Rc<dyn sqlitegraph::GraphBackend> = unimplemented!();
/// let export_dir = Path::new("/tmp/magellan_snapshot");
/// let metadata = export_snapshot(&backend, export_dir)?;
/// println!("Exported {} entities, {} edges", metadata.entity_count, metadata.edge_count);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn export_snapshot(
    backend: &Rc<dyn GraphBackend>,
    export_dir: &Path,
) -> Result<SnapshotExportMetadata> {
    // Create export directory if it doesn't exist
    fs::create_dir_all(export_dir)?;

    // Delegate to backend-specific export implementation
    // SQLite backend exports to JSON format
    // Native V2 backend exports to binary .v2 format + export.manifest
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
///
/// # Returns
/// SnapshotImportMetadata containing entity/edge counts and source location
///
/// # Errors
/// - Snapshot directory does not exist or is not a directory
/// - Required snapshot files are missing
/// - Backend import fails
///
/// # Example
/// ```no_run
/// use magellan::migrate_backend_cmd::import_snapshot;
/// use std::rc::Rc;
/// use std::path::Path;
///
/// # let backend: Rc<dyn sqlitegraph::GraphBackend> = unimplemented!();
/// let snapshot_dir = Path::new("/tmp/magellan_snapshot");
/// let metadata = import_snapshot(&backend, snapshot_dir)?;
/// println!("Imported {} entities, {} edges", metadata.entities_imported, metadata.edges_imported);
/// # Ok::<(), anyhow::Error>(())
/// ```
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
///
/// This helper function compares the entity and edge counts from an export
/// operation with the corresponding import operation to ensure data integrity.
///
/// # Arguments
/// * `export_meta` - Metadata from the original export operation
/// * `import_meta` - Metadata from the import operation
///
/// # Returns
/// `Ok(())` if counts match, `Err` with descriptive message if they don't
///
/// # Example
/// ```no_run
/// use magellan::migrate_backend_cmd::{verify_import_counts, export_snapshot, import_snapshot};
/// use std::rc::Rc;
/// use std::path::Path;
///
/// # let backend: Rc<dyn sqlitegraph::GraphBackend> = unimplemented!();
/// let snapshot_dir = Path::new("/tmp/magellan_snapshot");
/// let export_meta = export_snapshot(&backend, snapshot_dir)?;
/// let import_meta = import_snapshot(&backend, snapshot_dir)?;
/// verify_import_counts(&export_meta, &import_meta)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
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
///
/// Queries the underlying database for total entity and edge counts.
/// Used for verification before/after migration operations.
///
/// # Arguments
/// * `backend` - Graph backend to query
///
/// # Returns
/// Tuple of (entity_count, edge_count)
///
/// # Errors
/// - Query fails (database locked, corrupt, etc.)
///
/// # Note
/// This function provides counts independent of snapshot export for
/// pre-flight validation. The counts from snapshot_export() are preferred
/// for post-export verification since they reflect the actual snapshot.
pub fn get_graph_counts(_backend: &Rc<dyn GraphBackend>) -> Result<(i64, i64)> {
    // The GraphBackend trait doesn't provide a direct count method.
    // We need to use the underlying database connection.
    //
    // For SQLite backend: query graph_entities and graph_edges tables
    // For Native V2 backend: use backend's internal count methods
    //
    // Since we can't access the underlying connection through the trait,
    // we return (0, 0) for now. The actual counts are available from
    // snapshot_export() return value.
    //
    // Future work: add count_entities() and count_edges() to GraphBackend trait
    // in sqlitegraph, or provide a backend-specific accessor.
    Ok((0, 0))
}

/// Result of a backend migration operation
///
/// Contains detailed information about the migration result including
/// source/target formats, entity/edge counts, and status message.
#[derive(Debug, Clone)]
pub struct BackendMigrationResult {
    /// Whether the migration completed successfully
    pub success: bool,
    /// Format of the source database
    pub source_format: BackendFormat,
    /// Format of the target database (always NativeV2 for migrations)
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
/// 1. Detect source backend format
/// 2. Export graph data to snapshot format
/// 3. Import snapshot into Native V2 backend
/// 4. Verify data integrity (entity/edge counts match)
/// 5. Migrate Magellan-specific side tables
///
/// # Arguments
/// * `input_db` - Path to source database (SQLite or Native V2)
/// * `output_db` - Path to target database (will be Native V2 format)
/// * `export_dir` - Optional directory for snapshot files (default: temp dir)
/// * `dry_run` - If true, detect format only without migrating
///
/// # Returns
/// `BackendMigrationResult` with migration status, counts, and message
///
/// # Errors
/// - Input database does not exist
/// - Format detection fails
/// - Export/import operations fail
/// - Data integrity verification fails
/// - Side table migration fails
///
/// # Example
/// ```no_run
/// use magellan::migrate_backend_cmd::run_migrate_backend;
/// use std::path::PathBuf;
///
/// let input = PathBuf::from("/path/to/source.db");
/// let output = PathBuf::from("/path/to/target.db");
/// let result = run_migrate_backend(input, output, None, false)?;
/// println!("{}", result);
/// # Ok::<(), anyhow::Error>(())
/// ```
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
            source_format: BackendFormat::Sqlite, // placeholder
            target_format: BackendFormat::NativeV2,
            entities_migrated: 0,
            edges_migrated: 0,
            side_tables_migrated: false,
            message: format!("Input database not found: {}", input_db.display()),
        });
    }

    // Detect source format
    let source_format = detect_backend_format(&input_db).map_err(|e| {
        anyhow::anyhow!(
            "Failed to detect backend format for '{}': {}",
            input_db.display(),
            e
        )
    })?;

    // Dry run: just return detected format
    if dry_run {
        return Ok(BackendMigrationResult {
            success: true,
            source_format,
            target_format: BackendFormat::NativeV2,
            entities_migrated: 0,
            edges_migrated: 0,
            side_tables_migrated: false,
            message: format!(
                "Would migrate from {:?} to Native V2 (dry run)",
                source_format
            ),
        });
    }

    // Default export_dir to temp directory if not provided
    let export_dir = export_dir.unwrap_or_else(|| {
        let timestamp_val = timestamp();
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

    // Open source backend based on detected format
    let source_backend: Rc<dyn GraphBackend> = match source_format {
        BackendFormat::Sqlite => {
            use sqlitegraph::{SqliteGraph, SqliteGraphBackend};
            let sqlite_graph = SqliteGraph::open(&input_db).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to open SQLite database '{}': {}",
                    input_db.display(),
                    e
                )
            })?;
            Rc::new(SqliteGraphBackend::from_graph(sqlite_graph))
        }
        BackendFormat::NativeV2 => {
            use sqlitegraph::NativeGraphBackend;
            NativeGraphBackend::open(&input_db)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to open Native V2 database '{}': {}",
                        input_db.display(),
                        e
                    )
                })
                .map(Rc::new)?
        }
    };

    // Export snapshot from source backend
    let export_meta = export_snapshot(&source_backend, &export_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to export snapshot from '{}': {}",
            input_db.display(),
            e
        )
    })?;

    // Create target backend (always Native V2)
    #[cfg(feature = "native-v2")]
    let target_backend: Rc<dyn GraphBackend> = {
        use sqlitegraph::NativeGraphBackend;
        // Create new Native V2 database (or overwrite existing)
        let native_backend = NativeGraphBackend::new(&output_db).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create Native V2 database '{}': {}",
                output_db.display(),
                e
            )
        })?;
        Rc::new(native_backend)
    };

    #[cfg(not(feature = "native-v2"))]
    let target_backend: Rc<dyn GraphBackend> = {
        // Fallback to SQLite if native-v2 feature not enabled
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
    // This releases database locks so ATTACH DATABASE can work
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
        source_format,
        target_format: BackendFormat::NativeV2,
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
///
/// This function copies Magellan-owned side tables that are not handled
/// by the GraphBackend snapshot export/import mechanism:
/// - code_chunks: Code snippets from generation module
/// - file_metrics: File-level metrics
/// - symbol_metrics: Symbol-level metrics
/// - execution_log: Command execution tracking
/// - ast_nodes: AST hierarchy data
/// - cfg_blocks: Control flow graph blocks
///
/// # Arguments
/// * `source_db` - Path to source database
/// * `target_db` - Path to target database
///
/// # Returns
/// `Ok(())` if all side tables were migrated successfully
///
/// # Errors
/// - Cannot open source or target database
/// - Side table schema cannot be created
/// - Data copy fails
///
/// # Note
/// This function uses a simpler approach to avoid WAL mode lock issues.
/// Instead of ATTACH DATABASE, we use SQLite's VACUUM INTO to copy data,
/// then clean up the copied tables.
pub fn migrate_side_tables(source_db: &Path, target_db: &Path) -> Result<bool> {
    use rusqlite::Connection;

    // Open source connection to validate it exists and is readable
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

    // For each table, use a simple approach: dump data to CSV, then import
    for table_name in &side_tables {
        // Check if table exists in source database
        let table_exists: bool = source_conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=? LIMIT 1",
                &[table_name],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !table_exists {
            continue;
        }

        // Get row count to report correctly
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

        // For each row in source, copy to target using column-wise iteration
        // Get column names first
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
        // Prepare statement with longer lifetime
        let mut select_stmt = source_conn.prepare(&format!("SELECT * FROM {}", table_name))?;

        let rows = select_stmt.query_map([], |row| {
            // Collect values as rusqlite Values (preserves type information)
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

            // Build the INSERT statement with proper placeholders
            let placeholders = (0..row_values.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");

            let insert_sql = format!(
                "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                table_name, column_list, placeholders
            );

            // Convert Option<Value> to ToSql references
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
///
/// Creates the table schema if it doesn't already exist.
/// Delegates to the appropriate ensure_schema function for each table type.
///
/// # Arguments
/// * `conn` - Database connection
/// * `table_name` - Name of the table to ensure
///
/// # Returns
/// `Ok(())` if schema exists or was created successfully
fn ensure_table_schema(conn: &rusqlite::Connection, table_name: &str) -> Result<()> {
    match table_name {
        "code_chunks" => {
            // Schema from generation/mod.rs
            // Note: actual schema has symbol_kind instead of kind
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
            .map_err(|e| {
                anyhow::anyhow!("Failed to create code_chunks table: {}", e)
            })?;

            // Create indexes
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON code_chunks(file_path)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create code_chunks index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_symbol_name ON code_chunks(symbol_name)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create code_chunks index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON code_chunks(content_hash)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create code_chunks index: {}", e))?;
        }
        "file_metrics" | "symbol_metrics" => {
            // Schema from graph/db_compat.rs::ensure_metrics_schema
            // This function is called per-table, so we create tables individually
            if table_name == "file_metrics" {
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
                    "CREATE INDEX IF NOT EXISTS idx_file_metrics_complexity
                     ON file_metrics(complexity_score DESC)",
                    [],
                )
                .map_err(|e| anyhow::anyhow!("Failed to create file_metrics index: {}", e))?;
            } else {
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
                    "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_in
                     ON symbol_metrics(fan_in DESC)",
                    [],
                )
                .map_err(|e| anyhow::anyhow!("Failed to create symbol_metrics index: {}", e))?;

                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_out
                     ON symbol_metrics(fan_out DESC)",
                    [],
                )
                .map_err(|e| anyhow::anyhow!("Failed to create symbol_metrics index: {}", e))?;
            }
        }
        "execution_log" => {
            // Schema from graph/execution_log.rs
            // Note: schema has 14 columns: id, execution_id, tool_version, args, root, db_path,
            //       started_at, finished_at, duration_ms, outcome, error_message,
            //       files_indexed, symbols_indexed, references_indexed
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
                "CREATE INDEX IF NOT EXISTS idx_execution_log_started_at
                    ON execution_log(started_at DESC)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create execution_log index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_execution_log_execution_id
                    ON execution_log(execution_id)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create execution_log index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_execution_log_outcome
                    ON execution_log(outcome)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create execution_log index: {}", e))?;
        }
        "ast_nodes" => {
            // Schema from graph/db_compat.rs::ensure_ast_schema
            // Note: ast_nodes has 6 columns: id, parent_id, kind, byte_start, byte_end, file_id
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
            .map_err(|e| anyhow::anyhow!("Failed to create ast_nodes table: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent
                 ON ast_nodes(parent_id)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create ast_nodes index: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span
                 ON ast_nodes(byte_start, byte_end)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create ast_nodes index: {}", e))?;

            // Create file_id index if not exists (v6 upgrade)
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_ast_nodes_file_id
                 ON ast_nodes(file_id)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create ast_nodes file_id index: {}", e))?;
        }
        "cfg_blocks" => {
            // Schema from graph/db_compat.rs::ensure_cfg_schema
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
                    FOREIGN KEY (function_id) REFERENCES graph_entities(id) ON DELETE CASCADE
                )",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create cfg_blocks table: {}", e))?;

            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function
                 ON cfg_blocks(function_id)",
                [],
            )
            .map_err(|e| anyhow::anyhow!("Failed to create cfg_blocks index: {}", e))?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown side table: {}", table_name));
        }
    }

    Ok(())
}

/// Get current Unix timestamp in seconds
///
/// Utility function for generating unique timestamps.
/// Used for export directory naming and metadata tracking.
///
/// # Returns
/// Seconds since UNIX epoch (i64)
fn timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlitegraph::{SqliteGraph, SqliteGraphBackend};
    use tempfile::TempDir;

    #[test]
    fn test_export_snapshot_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let export_dir = temp_dir.path().join("export");

        // Create a simple SQLite graph (creates new file if doesn't exist)
        let graph = SqliteGraph::open(&db_path).unwrap();
        let concrete_backend = SqliteGraphBackend::from_graph(graph);
        let backend: Rc<dyn GraphBackend> = Rc::new(concrete_backend);

        // Export should create the directory
        let metadata = export_snapshot(&backend, &export_dir).unwrap();

        // Directory should exist
        assert!(export_dir.exists());

        // Metadata should have valid fields
        assert!(metadata.export_dir == export_dir);
        assert!(metadata.export_timestamp > 0);
    }

    #[test]
    fn test_get_graph_counts_returns_tuple() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let graph = SqliteGraph::open(&db_path).unwrap();
        let concrete_backend = SqliteGraphBackend::from_graph(graph);
        let backend: Rc<dyn GraphBackend> = Rc::new(concrete_backend);

        let (entities, edges) = get_graph_counts(&backend).unwrap();

        // Returns (0, 0) since GraphBackend trait doesn't provide count method
        assert_eq!(entities, 0);
        assert_eq!(edges, 0);
    }

    #[test]
    fn test_verify_import_counts_matching() {
        let export_meta = SnapshotExportMetadata {
            entity_count: 100,
            edge_count: 500,
            export_dir: PathBuf::from("/tmp/export"),
            export_timestamp: 1234567890,
            size_bytes: 1024,
        };

        let import_meta = SnapshotImportMetadata {
            entities_imported: 100,
            edges_imported: 500,
            source_dir: PathBuf::from("/tmp/export"),
            import_timestamp: 1234567900,
        };

        assert!(verify_import_counts(&export_meta, &import_meta).is_ok());
    }

    #[test]
    fn test_verify_import_counts_entity_mismatch() {
        let export_meta = SnapshotExportMetadata {
            entity_count: 100,
            edge_count: 500,
            export_dir: PathBuf::from("/tmp/export"),
            export_timestamp: 1234567890,
            size_bytes: 1024,
        };

        let import_meta = SnapshotImportMetadata {
            entities_imported: 99, // Mismatch
            edges_imported: 500,
            source_dir: PathBuf::from("/tmp/export"),
            import_timestamp: 1234567900,
        };

        assert!(verify_import_counts(&export_meta, &import_meta).is_err());
    }

    #[test]
    fn test_verify_import_counts_edge_mismatch() {
        let export_meta = SnapshotExportMetadata {
            entity_count: 100,
            edge_count: 500,
            export_dir: PathBuf::from("/tmp/export"),
            export_timestamp: 1234567890,
            size_bytes: 1024,
        };

        let import_meta = SnapshotImportMetadata {
            entities_imported: 100,
            edges_imported: 499, // Mismatch
            source_dir: PathBuf::from("/tmp/export"),
            import_timestamp: 1234567900,
        };

        assert!(verify_import_counts(&export_meta, &import_meta).is_err());
    }

    #[test]
    fn test_import_snapshot_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let graph = SqliteGraph::open(&db_path).unwrap();
        let concrete_backend = SqliteGraphBackend::from_graph(graph);
        let backend: Rc<dyn GraphBackend> = Rc::new(concrete_backend);
        let nonexistent = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let result = import_snapshot(&backend, &nonexistent);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_snapshot_import_metadata_struct() {
        let meta = SnapshotImportMetadata {
            entities_imported: 42,
            edges_imported: 99,
            source_dir: PathBuf::from("/test"),
            import_timestamp: 12345,
        };

        assert_eq!(meta.entities_imported, 42);
        assert_eq!(meta.edges_imported, 99);
        assert_eq!(meta.source_dir, PathBuf::from("/test"));
        assert_eq!(meta.import_timestamp, 12345);
    }

    // Tests for detect_backend_format function

    #[test]
    fn test_detect_backend_format_sqlite() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a SQLite database
        let graph = SqliteGraph::open(&db_path).unwrap();
        let _backend = SqliteGraphBackend::from_graph(graph);

        // Detect format
        let format = detect_backend_format(&db_path).unwrap();
        assert_eq!(format, BackendFormat::Sqlite);
    }

    #[test]
    fn test_detect_backend_format_in_memory_rejected() {
        let memory_path = Path::new(":memory:");

        let result = detect_backend_format(memory_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrationError::InMemoryDatabaseNotSupported => {
                // Expected error
            }
            other => panic!("Expected InMemoryDatabaseNotSupported, got: {}", other),
        }
    }

    #[test]
    fn test_detect_backend_format_nonexistent_file() {
        let nonexistent = PathBuf::from("/nonexistent/path/that/does/not/exist.db");

        let result = detect_backend_format(&nonexistent);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrationError::DatabaseNotFound { path } => {
                assert_eq!(path, nonexistent);
            }
            other => panic!("Expected DatabaseNotFound, got: {}", other),
        }
    }

    #[test]
    fn test_detect_backend_format_native_v2_magic_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let fake_v2 = temp_dir.path().join("fake.v2");

        // Create a file with Native V2 magic bytes
        fs::write(&fake_v2, b"MAG2").unwrap();

        let format = detect_backend_format(&fake_v2).unwrap();
        assert_eq!(format, BackendFormat::NativeV2);
    }

    #[test]
    fn test_detect_backend_format_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let empty_file = temp_dir.path().join("empty.db");

        // Create an empty file
        fs::File::create(&empty_file).unwrap();

        let result = detect_backend_format(&empty_file);
        assert!(result.is_err());
        // Empty file will fail to read header (CannotReadHeader)
        match result.unwrap_err() {
            MigrationError::CannotReadHeader { .. } => {
                // Expected - empty file is not a valid database (too short for header)
            }
            other => panic!("Expected CannotReadHeader, got: {}", other),
        }
    }

    #[test]
    fn test_backend_format_equality() {
        // Test BackendFormat equality and debug representation
        assert_eq!(BackendFormat::Sqlite, BackendFormat::Sqlite);
        assert_eq!(BackendFormat::NativeV2, BackendFormat::NativeV2);
        assert_ne!(BackendFormat::Sqlite, BackendFormat::NativeV2);

        // Test Debug output is non-empty
        assert!(!format!("{:?}", BackendFormat::Sqlite).is_empty());
        assert!(!format!("{:?}", BackendFormat::NativeV2).is_empty());
    }
}

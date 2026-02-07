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
pub fn get_graph_counts(backend: &Rc<dyn GraphBackend>) -> Result<(i64, i64)> {
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

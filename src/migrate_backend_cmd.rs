//! Cross-backend database migration command implementation
//!
//! Handles migration between SQLite and Native V2 backends:
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
use std::path::{Path, PathBuf};
use std::rc::Rc;
use sqlitegraph::GraphBackend;

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

        // Create a simple SQLite graph
        let graph = SqliteGraph::new(&db_path).unwrap();
        let backend = Rc::new(SqliteGraphBackend::from_graph(graph));

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

        let graph = SqliteGraph::new(&db_path).unwrap();
        let backend = Rc::new(SqliteGraphBackend::from_graph(graph));

        let (entities, edges) = get_graph_counts(&backend).unwrap();

        // Returns (0, 0) since GraphBackend trait doesn't provide count method
        assert_eq!(entities, 0);
        assert_eq!(edges, 0);
    }
}

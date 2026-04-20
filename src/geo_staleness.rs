//! GeoStalenessChecker — Detects when a .geo index is stale
//!
//! A .geo index is stale when:
//! 1. The .geo file doesn't exist
//! 2. The .db file is newer than the .geo file
//! 3. The SQLite schema version changed since the last build
//! 4. Row counts in SQLite differ from what was recorded

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::graph::schema::GeoIndexMeta;

/// Check if a .geo file is stale relative to its source .db
///
/// Returns true if the .geo index needs to be rebuilt.
pub fn is_geo_stale(db_path: &Path, geo_path: &Path) -> Result<bool> {
    // Rule 1: .geo doesn't exist
    if !geo_path.exists() {
        return Ok(true);
    }

    // Rule 2: .db is newer than .geo
    let db_modified = std::fs::metadata(db_path)?.modified()?;
    let geo_modified = std::fs::metadata(geo_path)?.modified()?;
    if db_modified > geo_modified {
        return Ok(true);
    }

    // Rules 3 & 4: Check metadata
    let conn = rusqlite::Connection::open(db_path)?;

    let meta = match GeoIndexMeta::get_geo_index_meta(&conn)? {
        Some(m) => m,
        None => return Ok(true), // No metadata recorded = stale
    };

    // Rule 3: Schema version mismatch
    let current_schema = crate::migrate_cmd::MAGELLAN_SCHEMA_VERSION;
    if meta.schema_version != current_schema {
        return Ok(true);
    }

    // Rule 4: Row count mismatch
    let (symbol_count, call_count, cfg_block_count) = get_current_counts(&conn)?;
    if meta.symbol_count != symbol_count
        || meta.call_count != call_count
        || meta.cfg_block_count != cfg_block_count
    {
        return Ok(true);
    }

    Ok(false)
}

/// Ensure a .geo index exists and is fresh. Returns the path to the .geo file.
///
/// If the .geo is stale or missing, rebuilds it from the SQLite database.
#[cfg(feature = "geometric-backend")]
pub fn ensure_geo_index(db_path: &Path) -> Result<PathBuf> {
    let geo_path = db_path.with_extension("geo");

    if is_geo_stale(db_path, &geo_path)? {
        eprintln!("Building geometric index from SQLite...");
        let stats = crate::geo_builder::build_geo_index(db_path, &geo_path)?;
        eprintln!(
            "Geometric index built: {} symbols, {} calls",
            stats.symbol_count, stats.call_count
        );
    }

    Ok(geo_path)
}

/// Stub when geometric-backend is disabled
#[cfg(not(feature = "geometric-backend"))]
pub fn ensure_geo_index(_db_path: &Path) -> Result<PathBuf> {
    anyhow::bail!(
        "Geometric index requires the 'geometric-backend' feature. \
         Install with: cargo install magellan --features geometric-backend"
    )
}

/// Force rebuild the .geo index regardless of staleness
#[cfg(feature = "geometric-backend")]
pub fn rebuild_geo_index(db_path: &Path) -> Result<PathBuf> {
    let geo_path = db_path.with_extension("geo");
    if geo_path.exists() {
        std::fs::remove_file(&geo_path)?;
    }
    ensure_geo_index(db_path)
}

/// Stub when geometric-backend is disabled
#[cfg(not(feature = "geometric-backend"))]
pub fn rebuild_geo_index(db_path: &Path) -> Result<PathBuf> {
    let _ = db_path;
    anyhow::bail!(
        "Geometric index requires the 'geometric-backend' feature. \
         Install with: cargo install magellan --features geometric-backend"
    )
}

fn get_current_counts(conn: &rusqlite::Connection) -> Result<(i64, i64, i64)> {
    let symbol_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM graph_entities WHERE kind = 'Symbol'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Call nodes are stored as graph_entities with kind = 'Call'
    let call_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM graph_entities WHERE kind = 'Call'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let cfg_block_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cfg_blocks", [], |row| row.get(0))
        .unwrap_or(0);

    Ok((symbol_count, call_count, cfg_block_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_geo_staleness_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let geo_path = temp_dir.path().join("test.geo");

        // Create empty db
        let _ = crate::CodeGraph::open(&db_path).unwrap();

        // .geo doesn't exist = stale
        assert!(is_geo_stale(&db_path, &geo_path).unwrap());
    }

    #[test]
    fn test_geo_staleness_fresh_build() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let geo_path = temp_dir.path().join("test.geo");

        // Create db and geo file
        let _ = crate::CodeGraph::open(&db_path).unwrap();

        // Create a .geo file that is newer than the db
        std::fs::write(&geo_path, b"geo").unwrap();

        // Without metadata, should still be stale
        assert!(is_geo_stale(&db_path, &geo_path).unwrap());
    }
}

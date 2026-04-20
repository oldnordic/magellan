//! Tests for geo_index_meta table and helpers

use magellan::graph::schema::GeoIndexMeta;
use rusqlite::Connection;
use tempfile::TempDir;

/// Create an empty database with the v9 schema (including geo_index_meta table)
fn create_test_db() -> (TempDir, Connection) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let conn = Connection::open(&db_path).unwrap();

    // Create magellan_meta table
    conn.execute(
        "CREATE TABLE magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();

    // Insert current version
    conn.execute(
        "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
         VALUES (1, 9, 1, 1234567890)",
        [],
    )
    .unwrap();

    // Create geo_index_meta table (as the v9 migration would)
    conn.execute(
        "CREATE TABLE geo_index_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            geo_path TEXT NOT NULL,
            built_at INTEGER NOT NULL,
            schema_version INTEGER NOT NULL,
            symbol_count INTEGER NOT NULL,
            call_count INTEGER NOT NULL,
            cfg_block_count INTEGER NOT NULL,
            checksum TEXT NOT NULL
        )",
        [],
    )
    .unwrap();

    (temp_dir, conn)
}

#[test]
fn test_geo_index_meta_roundtrip() {
    let (_temp, conn) = create_test_db();

    // Record a build
    GeoIndexMeta::record_geo_index_built(&conn, "test.geo", 100, 50, 25, "abc123").unwrap();

    // Read back
    let meta = GeoIndexMeta::get_geo_index_meta(&conn).unwrap().unwrap();
    assert_eq!(meta.geo_path, "test.geo");
    assert_eq!(meta.symbol_count, 100);
    assert_eq!(meta.call_count, 50);
    assert_eq!(meta.cfg_block_count, 25);
    assert_eq!(meta.checksum, "abc123");
}

#[test]
fn test_geo_index_meta_update() {
    let (_temp, conn) = create_test_db();

    // Initial record
    GeoIndexMeta::record_geo_index_built(&conn, "test.geo", 100, 50, 25, "abc123").unwrap();

    // Update
    GeoIndexMeta::record_geo_index_built(&conn, "test.geo", 200, 100, 50, "def456").unwrap();

    // Verify update
    let meta = GeoIndexMeta::get_geo_index_meta(&conn).unwrap().unwrap();
    assert_eq!(meta.symbol_count, 200);
    assert_eq!(meta.call_count, 100);
    assert_eq!(meta.cfg_block_count, 50);
    assert_eq!(meta.checksum, "def456");
}

#[test]
fn test_geo_index_meta_none_when_missing() {
    let (_temp, conn) = create_test_db();

    // No record inserted yet
    let meta = GeoIndexMeta::get_geo_index_meta(&conn).unwrap();
    assert!(meta.is_none());
}

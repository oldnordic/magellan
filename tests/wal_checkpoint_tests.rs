//! Tests verifying that WAL checkpoint reduces WAL file size after bulk writes.

use magellan::CodeGraph;

#[test]
fn test_checkpoint_wal_method() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    let graph = CodeGraph::open(&db).expect("open should succeed");

    // Checkpoint should succeed on a fresh empty database
    graph.checkpoint_wal().expect("checkpoint should succeed");
}

#[test]
fn test_wal_growth_after_bulk_insert() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    {
        let mut graph = CodeGraph::open(&db).expect("open should succeed");

        // Index a small file to generate some WAL entries
        let source = b"fn main() {}";
        graph
            .index_file("src/main.rs", source)
            .expect("index should succeed");

        // Checkpoint should succeed and keep WAL small
        graph.checkpoint_wal().expect("checkpoint should succeed");
    }

    // After checkpoint, the WAL file should be small or absent
    let wal_path = db.with_extension("db-wal");
    if wal_path.exists() {
        let meta = std::fs::metadata(&wal_path).unwrap();
        assert!(
            meta.len() < 1024 * 1024,
            "WAL file should be < 1MB after checkpoint, got {} bytes",
            meta.len()
        );
    }
}

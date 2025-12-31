//! Tests for timestamp tracking in FileNode
//!
//! TDD Task 1: Timestamp Tracking
//!
//! Tests verify that:
//! - FileNode includes last_indexed_at timestamp
//! - FileNode includes last_modified timestamp (filesystem mtime)
//! - Timestamps update on re-index

use magellan::CodeGraph;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// Helper to get current Unix timestamp
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn test_file_node_includes_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a test file
    let source = b"fn test() {}";

    let before_index = now_secs();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source).unwrap();

    let after_index = now_secs();

    // Query the file node and verify timestamps exist
    let file_node = graph
        .get_file_node("test.rs")
        .unwrap()
        .expect("File node should exist");

    // Verify last_indexed_at is set and within expected range
    assert!(
        file_node.last_indexed_at >= before_index,
        "last_indexed_at should be >= before_index ({} >= {})",
        file_node.last_indexed_at,
        before_index
    );
    assert!(
        file_node.last_indexed_at <= after_index,
        "last_indexed_at should be <= after_index ({} <= {})",
        file_node.last_indexed_at,
        after_index
    );

    // Verify last_modified is set (will be 0 if file doesn't exist on filesystem)
    // For files indexed from memory (not on disk), last_modified is 0
    if file_node.last_modified > 0 {
        assert!(
            file_node.last_modified >= before_index,
            "last_modified should be >= before_index"
        );
        assert!(
            file_node.last_modified <= after_index + 1, // Allow 1 second tolerance
            "last_modified should be <= after_index + 1"
        );
    }
}

#[test]
fn test_timestamps_update_on_reindex() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create initial file
    let source1 = b"fn test() {}";

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source1).unwrap();

    let first_index_time = graph
        .get_file_node("test.rs")
        .unwrap()
        .expect("File node should exist")
        .last_indexed_at;

    // Wait a bit (at least 1 second)
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Re-index with different content
    let source2 = b"fn test() { println!(\"changed\"); }";
    graph.index_file("test.rs", source2).unwrap();

    let second_index_time = graph
        .get_file_node("test.rs")
        .unwrap()
        .expect("File node should exist")
        .last_indexed_at;

    // Verify timestamp was updated
    assert!(
        second_index_time > first_index_time,
        "last_indexed_at should increase on re-index ({} > {})",
        second_index_time,
        first_index_time
    );
}

#[test]
fn test_last_modified_captured_from_filesystem() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create an actual file on disk
    let file_path = temp_dir.path().join("actual.rs");
    let source = b"fn actual() {}";
    fs::write(&file_path, source).unwrap();

    // Get the file's mtime before indexing
    let path_str = file_path.to_string_lossy().to_string();
    let metadata_before = fs::metadata(&file_path).unwrap();
    let mtime_before = metadata_before
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, source).unwrap();

    let file_node = graph
        .get_file_node(&path_str)
        .unwrap()
        .expect("File node should exist");

    // Verify last_modified matches filesystem mtime
    assert!(
        file_node.last_modified == mtime_before,
        "last_modified should match filesystem mtime ({} == {})",
        file_node.last_modified,
        mtime_before
    );
}

#[test]
fn test_file_node_json_serialization() {
    // Test that FileNode with timestamps serializes/deserializes correctly
    let file_node = magellan::graph::FileNode {
        path: "test.rs".to_string(),
        hash: "abc123".to_string(),
        last_indexed_at: 1234567890,
        last_modified: 1234567888,
    };

    let serialized = serde_json::to_string(&file_node).unwrap();
    let deserialized: magellan::graph::FileNode = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.path, "test.rs");
    assert_eq!(deserialized.hash, "abc123");
    assert_eq!(deserialized.last_indexed_at, 1234567890);
    assert_eq!(deserialized.last_modified, 1234567888);
}

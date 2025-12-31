//! Tests for database freshness checking
//!
//! TDD Task 3: Pre-query staleness warning
//!
//! Tests verify that:
//! - fresh database produces no warning
//! - stale database produces warning
//! - empty database produces no warning
//! - warning includes time difference

use magellan::CodeGraph;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test file with content
fn create_test_file(dir: &PathBuf, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_fresh_database_no_warning() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create and index a file (recent timestamp)
    let file_path = create_test_file(&root_path, "fresh.rs", "fn fresh() {}");
    let source = fs::read(&file_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, &source).unwrap();

    // Freshness check should pass (no warning needed)
    let result = magellan::graph::check_freshness(&graph);
    assert!(result.is_ok(), "Freshness check should succeed");

    let status = result.unwrap();
    assert!(!status.is_stale(), "Fresh database should not be stale");
    assert!(
        status.minutes_since_index() < 1,
        "Fresh database should be < 1 minute old"
    );
}

#[test]
fn test_stale_database_produces_warning() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create and index a file
    let file_path = create_test_file(&root_path, "stale.rs", "fn stale() {}");
    let source = fs::read(&file_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, &source).unwrap();

    // Manually set the last_indexed_at to be old (more than 5 minutes ago)
    let old_timestamp = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64)
        - 400; // 400 seconds ago (more than 5 minutes)

    {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path).unwrap();

        // Update the data field of the File node
        let mut file_node = graph.get_file_node(&path_str).unwrap().unwrap();
        file_node.last_indexed_at = old_timestamp;
        let new_data = serde_json::to_string(&file_node).unwrap();

        conn.execute(
            "UPDATE graph_entities SET data = ?1 WHERE kind = 'File' AND name = ?2",
            [&new_data, &path_str],
        )
        .unwrap();
    }

    // Freshness check should detect staleness
    let result = magellan::graph::check_freshness(&graph);
    assert!(result.is_ok(), "Freshness check should succeed");

    let status = result.unwrap();
    assert!(
        status.is_stale(),
        "Stale database should be detected as stale"
    );
    assert!(
        status.minutes_since_index() >= 5,
        "Stale database should be >= 5 minutes old"
    );
}

#[test]
fn test_empty_database_no_warning() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create empty database
    let graph = CodeGraph::open(&db_path).unwrap();

    // Freshness check should pass (empty DB is not stale)
    let result = magellan::graph::check_freshness(&graph);
    assert!(result.is_ok(), "Freshness check should succeed on empty DB");

    let status = result.unwrap();
    assert!(
        !status.is_stale(),
        "Empty database should not be considered stale"
    );
}

#[test]
fn test_warning_includes_time_difference() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create and index a file
    let file_path = create_test_file(&root_path, "test.rs", "fn test() {}");
    let source = fs::read(&file_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, &source).unwrap();

    // Set timestamp to 10 minutes ago
    let old_timestamp = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64)
        - 600; // 600 seconds = 10 minutes

    {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path).unwrap();

        let mut file_node = graph.get_file_node(&path_str).unwrap().unwrap();
        file_node.last_indexed_at = old_timestamp;
        let new_data = serde_json::to_string(&file_node).unwrap();

        conn.execute(
            "UPDATE graph_entities SET data = ?1 WHERE kind = 'File' AND name = ?2",
            [&new_data, &path_str],
        )
        .unwrap();
    }

    // Check freshness and verify time difference
    let result = magellan::graph::check_freshness(&graph);
    assert!(result.is_ok());

    let status = result.unwrap();
    let minutes = status.minutes_since_index();

    assert!(
        minutes >= 9 && minutes <= 11,
        "Should be ~10 minutes old, got {}",
        minutes
    );

    // Warning message should include the time difference
    let warning = status.warning_message("/path/to/db".into(), "/path/to/root".into());
    assert!(
        warning.contains("10 minutes"),
        "Warning should mention '10 minutes'"
    );
}

#[test]
fn test_freshness_threshold_constant() {
    // Verify the stale threshold is 300 seconds (5 minutes)
    use magellan::graph::STALE_THRESHOLD_SECS;
    assert_eq!(
        STALE_THRESHOLD_SECS, 300,
        "Stale threshold should be 300 seconds"
    );
}

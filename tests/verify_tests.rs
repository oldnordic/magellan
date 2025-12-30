//! Tests for magellan verify command
//!
//! TDD Task 2: magellan verify command
//!
//! Tests verify that:
//! - verify with no differences produces clean report
//! - verify detects deleted files (in DB but not on filesystem)
//! - verify detects new files (on filesystem but not in DB)
//! - verify detects modified files (hash differs from DB)
//! - verify detects stale files (timestamp old)

use std::fs;
use std::path::PathBuf;
use magellan::CodeGraph;
use tempfile::TempDir;

/// Helper to create a test file with content
fn create_test_file(dir: &PathBuf, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_verify_clean_database() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create and index a file
    let file_path = create_test_file(&root_path, "test.rs", "fn test() {}");
    let source = fs::read(&file_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, &source).unwrap();

    // Verify should report no differences
    let report = magellan::verify::verify_graph(&mut graph, &root_path).unwrap();

    assert_eq!(report.missing.len(), 0, "Should have no missing files");
    assert_eq!(report.new.len(), 0, "Should have no new files");
    assert_eq!(report.modified.len(), 0, "Should have no modified files");
    assert_eq!(report.stale.len(), 0, "Should have no stale files");
}

#[test]
fn test_verify_detects_deleted_files() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create and index a file
    let file_path = create_test_file(&root_path, "deleted.rs", "fn deleted() {}");
    let source = fs::read(&file_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, &source).unwrap();

    // Delete the file
    fs::remove_file(&file_path).unwrap();

    // Verify should detect missing file
    let report = magellan::verify::verify_graph(&mut graph, &root_path).unwrap();

    assert_eq!(report.missing.len(), 1, "Should detect 1 missing file");
    assert!(report.missing[0].ends_with("deleted.rs"), "Missing file should be deleted.rs");
}

#[test]
fn test_verify_detects_new_files() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create empty database
    let _graph = CodeGraph::open(&db_path).unwrap();

    // Create a file that's not in the database
    create_test_file(&root_path, "new.rs", "fn new() {}");

    // Verify should detect new file
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let report = magellan::verify::verify_graph(&mut graph, &root_path).unwrap();

    assert_eq!(report.new.len(), 1, "Should detect 1 new file");
    assert!(report.new[0].ends_with("new.rs"), "New file should be new.rs");
}

#[test]
fn test_verify_detects_modified_files() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create and index a file
    let file_path = create_test_file(&root_path, "modified.rs", "fn original() {}");
    let source = fs::read(&file_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file(&path_str, &source).unwrap();

    // Modify the file
    create_test_file(&root_path, "modified.rs", "fn modified() {}");

    // Verify should detect modified file
    let report = magellan::verify::verify_graph(&mut graph, &root_path).unwrap();

    assert_eq!(report.modified.len(), 1, "Should detect 1 modified file");
    assert!(report.modified[0].ends_with("modified.rs"), "Modified file should be modified.rs");
}

#[test]
fn test_verify_detects_stale_files() {
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
    // by directly manipulating the database via SQL
    let old_timestamp = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64) - 400; // 400 seconds ago (more than 5 minutes)

    // Directly update the database via SQL
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
        ).unwrap();
    }

    // Verify should detect stale file
    let report = magellan::verify::verify_graph(&mut graph, &root_path).unwrap();

    assert_eq!(report.stale.len(), 1, "Should detect 1 stale file");
    assert!(report.stale[0].ends_with("stale.rs"), "Stale file should be stale.rs");
}

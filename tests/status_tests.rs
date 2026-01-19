//! Status flag tests for Magellan binary
//!
//! Tests that magellan watch --status prints correct counts.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_status_flag_prints_counts() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with 2 functions and 1 reference
    fs::write(&file_path, b"fn foo() {}\nfn bar() { foo(); }").unwrap();

    // Index the file by opening CodeGraph directly
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
        graph.index_references(&path_str, &source).unwrap();
    }

    // Run magellan status --db
    let output = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("Failed to execute magellan status");

    // Verify output contains counts
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Process should exit successfully");

    // Should have exactly 1 file, 2 symbols (foo, bar), 1 reference (bar -> foo)
    assert!(
        stdout.contains("files: 1"),
        "Expected 'files: 1' in output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("symbols: 2"),
        "Expected 'symbols: 2' in output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("references: 1"),
        "Expected 'references: 1' in output, got: {}",
        stdout
    );
}

#[test]
fn test_status_json_output_structure() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with content
    fs::write(&file_path, b"fn foo() {}\nfn bar() { foo(); }").unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
        graph.index_references(&path_str, &source).unwrap();
    }

    // Run magellan status --output json --db
    let output = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute magellan status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "Process should exit successfully: {}", stdout);

    // Parse as JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");

    // Verify structure
    assert_eq!(json["schema_version"], "1.0.0");
    assert!(json["execution_id"].is_string());
    assert!(json["data"].is_object());

    // Verify data fields
    let data = &json["data"];
    assert_eq!(data["files"], 1);
    assert_eq!(data["symbols"], 2);
    assert!(data["references"].is_number());
    assert!(data["calls"].is_number());
    assert!(data["code_chunks"].is_number());
}

#[test]
fn test_status_deterministic_ordering() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with content
    fs::write(&file_path, b"fn foo() {}\nfn bar() { foo(); }").unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
        graph.index_references(&path_str, &source).unwrap();
    }

    // Run magellan status --output json twice
    let output1 = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute magellan status");

    let output2 = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute magellan status");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    assert!(output1.status.success());
    assert!(output2.status.success());

    // Parse both as JSON
    let json1: serde_json::Value = serde_json::from_str(&stdout1).unwrap();
    let json2: serde_json::Value = serde_json::from_str(&stdout2).unwrap();

    // execution_id should differ
    assert_ne!(
        json1["execution_id"],
        json2["execution_id"],
        "execution_id should be unique per run"
    );

    // data field should be identical (deterministic)
    assert_eq!(
        json1["data"],
        json2["data"],
        "data should be identical across runs (deterministic)"
    );

    // schema_version should be identical
    assert_eq!(json1["schema_version"], json2["schema_version"]);
}

#[test]
fn test_status_schema_version_present() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file
    fs::write(&file_path, b"fn test() {}").unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
    }

    // Run magellan status --output json
    let output = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute magellan status");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify schema_version field exists and has correct value
    assert!(stdout.contains("\"schema_version\""), "Missing schema_version field");
    assert!(stdout.contains("\"1.0.0\""), "Missing or incorrect schema version value");
}

#[test]
fn test_execution_id_unique() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file
    fs::write(&file_path, b"fn test() {}").unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
    }

    // Run magellan status --output json twice
    let output1 = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute magellan status");

    let output2 = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to execute magellan status");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    // Parse JSON and extract execution_ids
    let json1: serde_json::Value = serde_json::from_str(&stdout1).unwrap();
    let json2: serde_json::Value = serde_json::from_str(&stdout2).unwrap();

    let id1 = json1["execution_id"].as_str().unwrap();
    let id2 = json2["execution_id"].as_str().unwrap();

    // Execution IDs should be different
    assert_ne!(id1, id2, "execution_id should be unique per run");

    // Both should be valid non-empty strings
    assert!(!id1.is_empty(), "execution_id should not be empty");
    assert!(!id2.is_empty(), "execution_id should not be empty");
}

#[test]
fn test_status_human_mode_unchanged() {
    // Verify human mode still works as before
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with 2 functions and 1 reference
    fs::write(&file_path, b"fn foo() {}\nfn bar() { foo(); }").unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
        graph.index_references(&path_str, &source).unwrap();
    }

    // Run magellan status (default human mode, no --output flag)
    let output = Command::new(&bin_path)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("Failed to execute magellan status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());

    // Verify human-readable format (not JSON)
    assert!(stdout.contains("files: 1"), "Should show files count in human format");
    assert!(stdout.contains("symbols: 2"), "Should show symbols count in human format");
    assert!(stdout.contains("calls:"), "Should show calls count in human format");
    assert!(!stdout.contains("schema_version"), "Should not have JSON fields in human mode");
    assert!(!stdout.contains("execution_id"), "Should not have JSON fields in human mode");
}

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

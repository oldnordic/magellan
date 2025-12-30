//! Tests for initial full scan functionality
//!
//! TDD approach: Write failing test first, then implement feature.

use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_scan_initial_flag_indexes_all_files_on_startup() {
    // Setup: Create temp directory with multiple Rust files
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");

    // Create multiple Rust files BEFORE starting magellan
    // This tests that --scan-initial indexes files without requiring file events
    let file1_path = root_path.join("file1.rs");
    let file2_path = root_path.join("file2.rs");
    let subdir = root_path.join("subdir");
    fs::create_dir(&subdir).unwrap();
    let file3_path = subdir.join("file3.rs");

    fs::write(&file1_path, b"fn foo() {}").unwrap();
    fs::write(&file2_path, b"fn bar() {}").unwrap();
    fs::write(&file3_path, b"fn baz() {}").unwrap();

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan")
        .unwrap_or_else(|_| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            path.pop();
            path.push("magellan");
            path.to_str().unwrap().to_string()
        });

    // Start magellan with --scan-initial flag
    let mut child = Command::new(&bin_path)
        .arg("watch")
        .arg("--root")
        .arg(&root_path)
        .arg("--db")
        .arg(&db_path)
        .arg("--scan-initial")  // NEW FLAG
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start magellan binary");

    // Wait for scan to complete and process startup
    thread::sleep(Duration::from_millis(500));

    // Kill the process
    let _ = child.kill();
    let output = child.wait_with_output().expect("Failed to wait for process");

    // Verify stdout contains scan progress
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Scanning"), "Expected scan progress in stdout");

    // Open database and verify ALL files were indexed
    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();

    let file1_str = file1_path.to_string_lossy().to_string();
    let file2_str = file2_path.to_string_lossy().to_string();
    let file3_str = file3_path.to_string_lossy().to_string();

    let symbols1 = graph.symbols_in_file(&file1_str).unwrap();
    let symbols2 = graph.symbols_in_file(&file2_str).unwrap();
    let symbols3 = graph.symbols_in_file(&file3_str).unwrap();

    assert_eq!(symbols1.len(), 1, "file1.rs should have 1 symbol (foo)");
    assert_eq!(symbols2.len(), 1, "file2.rs should have 1 symbol (bar)");
    assert_eq!(symbols3.len(), 1, "file3.rs should have 1 symbol (baz)");

    // Verify total counts
    let file_count = graph.count_files().unwrap();
    let symbol_count = graph.count_symbols().unwrap();

    assert_eq!(file_count, 3, "Should have 3 files indexed");
    assert_eq!(symbol_count, 3, "Should have 3 symbols indexed");
}

#[test]
fn test_scan_only_processes_rs_files() {
    // Verify that --scan-initial only processes .rs files
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");

    // Create Rust file and non-Rust file
    let rs_path = root_path.join("code.rs");
    let txt_path = root_path.join("readme.txt");
    let db_txt_path = root_path.join("other.db");

    fs::write(&rs_path, b"fn test() {}").unwrap();
    fs::write(&txt_path, b"Some text").unwrap();
    fs::write(&db_txt_path, b"Database").unwrap();

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan")
        .unwrap_or_else(|_| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            path.pop();
            path.push("magellan");
            path.to_str().unwrap().to_string()
        });

    let mut child = Command::new(&bin_path)
        .arg("watch")
        .arg("--root")
        .arg(&root_path)
        .arg("--db")
        .arg(&db_path)
        .arg("--scan-initial")
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start magellan");

    thread::sleep(Duration::from_millis(300));
    let _ = child.kill();
    let _ = child.wait();

    let graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Only .rs file should be indexed
    let file_count = graph.count_files().unwrap();
    assert_eq!(file_count, 1, "Should only have 1 .rs file indexed");
}

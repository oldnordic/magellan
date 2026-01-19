//! CLI smoke tests for Magellan binary
//!
//! Tests the `magellan watch` command by spawning the binary process,
//! creating files in the watched directory, and verifying output.

use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_watch_command_indexes_file_on_create() {
    // Setup: Create temp directories for root and db
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        // Fallback: construct path to debug binary
        let mut path = std::env::current_exe().unwrap();
        path.pop(); // Remove test executable name from deps/
        path.pop(); // Remove deps/ directory
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Small delay to ensure directory is stable
    thread::sleep(Duration::from_millis(10));

    // Create and index initial placeholder file
    let placeholder_source = b"";
    fs::write(&file_path, placeholder_source).unwrap();

    // Start the magellan watch process
    let mut child = Command::new(&bin_path)
        .arg("watch")
        .arg("--root")
        .arg(&root_path)
        .arg("--db")
        .arg(&db_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start magellan binary");

    // Wait for process startup
    thread::sleep(Duration::from_millis(100));

    // Modify file with Rust code
    let rust_code = b"fn foo() {}\nfn bar() { foo(); }";
    fs::write(&file_path, rust_code).unwrap();

    // Wait for event processing and read stdout
    // Must wait longer than the default debounce_ms (500ms) for the debouncer to emit
    thread::sleep(Duration::from_millis(700));
    let _ = child.kill();
    let output = child
        .wait_with_output()
        .expect("Failed to wait for process");

    // Verify stdout contains expected log line
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MODIFY"), "Expected MODIFY event in stdout");
    assert!(stdout.contains("test.rs"), "Expected file path in stdout");
    assert!(stdout.contains("symbols=2"), "Expected 2 symbols indexed");

    // Open database and verify symbols were actually indexed
    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(
        symbols.len(),
        2,
        "Should have 2 symbols indexed: foo and bar"
    );

    // Verify references were indexed
    let foo_id = graph.symbol_id_by_name(&path_str, "foo").unwrap();
    let foo_id = foo_id.expect("foo symbol should exist");
    let references = graph.references_to_symbol(foo_id).unwrap();
    assert_eq!(references.len(), 1, "Should have 1 reference to foo");
}

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

    // Create initial placeholder file BEFORE starting watcher
    fs::write(&file_path, b"").unwrap();

    // Start watcher with short debounce for test reliability
    let mut child = Command::new(&bin_path)
        .arg("watch")
        .arg("--root")
        .arg(&root_path)
        .arg("--db")
        .arg(&db_path)
        .arg("--debounce-ms")
        .arg("100")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start magellan binary");

    // Wait for watcher to initialize (file watcher setup + first scan)
    thread::sleep(Duration::from_millis(500));

    // Modify file with Rust code
    let rust_code = b"fn foo() {}\nfn bar() { foo(); }";
    fs::write(&file_path, rust_code).unwrap();

    // Wait for debounce (100ms) + processing. Use longer wait on CI.
    let wait_ms = if std::env::var("CI").is_ok() { 3000 } else { 1500 };
    thread::sleep(Duration::from_millis(wait_ms));
    let _ = child.kill();
    let output = child
        .wait_with_output()
        .expect("Failed to wait for process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MODIFY"), "Expected MODIFY event in stdout");
    assert!(stdout.contains("test.rs"), "Expected file path in stdout");
    assert!(stdout.contains("symbols=2"), "Expected 2 symbols indexed");

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(
        symbols.len(),
        2,
        "Should have 2 symbols indexed: foo and bar"
    );

    let foo_id = graph.symbol_id_by_name(&path_str, "foo").unwrap();
    let foo_id = foo_id.expect("foo symbol should exist");
    let references = graph.references_to_symbol(foo_id).unwrap();
    assert_eq!(references.len(), 1, "Should have 1 reference to foo");
}

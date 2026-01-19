//! Error handling tests for Magellan binary
//!
//! Tests that magellan handles errors gracefully without crashing.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_unreadable_file_prints_error_and_continues() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let good_file = root_path.join("good.rs");
    let bad_file = root_path.join("bad.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Small delay to ensure directory is stable
    thread::sleep(Duration::from_millis(10));

    // Create and index initial files
    fs::write(&good_file, b"fn good() {}").unwrap();
    fs::write(&bad_file, b"fn bad() {}").unwrap();

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

    // Make bad.rs unreadable (permission denied)
    let mut perms = fs::metadata(&bad_file).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&bad_file, perms.clone()).unwrap();

    // Modify good.rs to trigger event
    fs::write(&good_file, b"fn good() { fn updated() {} }").unwrap();

    // Wait for event processing
    // Must wait longer than the default debounce_ms (500ms) for the debouncer to emit
    // Wait longer to ensure both the error event and the modify event are processed
    thread::sleep(Duration::from_millis(1000));

    // Restore permissions so we can clean up
    perms.set_mode(0o644);
    fs::set_permissions(&bad_file, perms).unwrap();

    // Kill process and read output
    let _ = child.kill();
    let output = child
        .wait_with_output()
        .expect("Failed to wait for process");

    // Verify stdout contains ERROR line for bad.rs
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ERROR"),
        "Expected ERROR in stdout for unreadable file"
    );
    assert!(
        stdout.contains("bad.rs"),
        "Expected filename in error message"
    );

    // Verify process continued processing and indexed good.rs
    assert!(
        stdout.contains("MODIFY"),
        "Expected MODIFY event for good.rs"
    );
    assert!(stdout.contains("good.rs"), "Expected good.rs in stdout");

    // Open database and verify good.rs was indexed
    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = good_file.to_string_lossy().to_string();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(
        symbols.len(),
        2,
        "Should have indexed 2 symbols from good.rs despite bad.rs error"
    );
}

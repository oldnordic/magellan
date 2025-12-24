//! Signal handling tests for Magellan binary
//!
//! Tests that magellan handles SIGINT and SIGTERM gracefully.

use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_sigterm_prints_shutdown_and_exits() {
    // Setup: Create temp directories
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan")
        .unwrap_or_else(|_| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            path.pop();
            path.push("magellan");
            path.to_str().unwrap().to_string()
        });

    // Create initial file
    fs::write(&file_path, b"fn test() {}").unwrap();

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

    // Send SIGTERM using kill command
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .arg(child.id().to_string())
            .status();
    }

    // Wait for process to exit (with timeout)
    let timeout = Duration::from_secs(2);
    let start = std::time::Instant::now();
    let output = loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                // Process has exited, get output
                break child.wait_with_output().expect("Failed to wait for process");
            }
            Ok(None) => {
                // Process still running
                if start.elapsed() > timeout {
                    panic!("Process did not exit within timeout");
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("Failed to wait for process: {}", e),
        }
    };

    // Verify stdout contains "SHUTDOWN"
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SHUTDOWN"), "Expected SHUTDOWN in stdout, got: {}", stdout);

    // Verify process exited cleanly
    assert!(output.status.success(), "Process should exit successfully");
}

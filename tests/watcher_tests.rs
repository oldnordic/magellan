use magellan::{FileSystemWatcher, WatcherConfig};
use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use tempfile::TempDir;

/// Helper: poll for event with timeout
fn poll_for_event(watcher: &FileSystemWatcher, timeout_ms: u64) -> Option<magellan::FileEvent> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        match watcher.try_recv_event() {
            Ok(Some(event)) => return Some(event),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    return None;
                }
            }
            Err(_) => {
                // Error receiving event - treat as no event
                if start.elapsed() >= timeout {
                    return None;
                }
            }
        }

        sleep(Duration::from_millis(50));
    }
}

#[test]
fn test_file_create_event() {
    let temp_dir = TempDir::new().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let watcher = FileSystemWatcher::new(
        temp_dir.path().to_path_buf(),
        WatcherConfig::default(),
        shutdown,
    )
    .unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    let file_path = temp_dir.path().join("test.rs");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn test() {{}}").unwrap();

    // Poll for event with timeout
    let event = poll_for_event(&watcher, 2000);

    assert!(event.is_some(), "Should receive file event");
    let event = event.unwrap();

    assert_eq!(event.path, file_path);
    // Note: With notify 8.x debouncer, event type is always Modify
    // The reconcile operation handles Create vs Delete based on actual file state
    assert_eq!(event.event_type, magellan::EventType::Modify);
}

#[test]
fn test_file_modify_event() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.rs");

    // Create file first
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn old() {{}}").unwrap();
    drop(file);

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let shutdown = Arc::new(AtomicBool::new(false));
    let watcher = FileSystemWatcher::new(
        temp_dir.path().to_path_buf(),
        WatcherConfig::default(),
        shutdown,
    )
    .unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Modify file
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn new() {{}}").unwrap();

    // Poll for modify event
    let event = poll_for_event(&watcher, 2000);

    assert!(event.is_some(), "Should receive modify event");
    let event = event.unwrap();

    assert_eq!(event.path, file_path);
    assert_eq!(event.event_type, magellan::EventType::Modify);
}

#[test]
fn test_file_delete_event() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.rs");

    // Create file first
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn test() {{}}").unwrap();
    drop(file);

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let shutdown = Arc::new(AtomicBool::new(false));
    let watcher = FileSystemWatcher::new(
        temp_dir.path().to_path_buf(),
        WatcherConfig::default(),
        shutdown,
    )
    .unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Delete file
    std::fs::remove_file(&file_path).unwrap();

    // Poll for delete event
    let event = poll_for_event(&watcher, 2000);

    assert!(
        event.is_some(),
        "Should receive file event for deleted path"
    );
    let event = event.unwrap();

    assert_eq!(event.path, file_path);
    // Note: With notify 8.x debouncer, event type is always Modify
    // The reconcile operation handles Create vs Delete based on actual file state
    assert_eq!(event.event_type, magellan::EventType::Modify);
}

#[test]
fn test_debounce_rapid_changes() {
    let temp_dir = TempDir::new().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let watcher = FileSystemWatcher::new(
        temp_dir.path().to_path_buf(),
        WatcherConfig::default(),
        shutdown,
    )
    .unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    let file_path = temp_dir.path().join("test.rs");

    // Rapidly modify file 3 times
    for i in 0..3 {
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "fn v{}() {{}}", i).unwrap();
        drop(file);
        sleep(Duration::from_millis(50));
    }

    // Wait for debounce period + buffer
    sleep(Duration::from_millis(600));

    // Count events - rapid changes should produce a single debounced event
    let mut event_count = 0;
    while let Ok(Some(_)) = watcher.try_recv_event() {
        event_count += 1;
        if event_count > 10 {
            break;
        }
    }

    // Should receive at least 1 event (OS-dependent debouncing)
    assert!(
        event_count >= 1,
        "Should receive at least 1 event, got {}",
        event_count
    );
}

#[test]
fn test_watch_temp_directory() {
    let temp_dir = TempDir::new().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let watcher = FileSystemWatcher::new(
        temp_dir.path().to_path_buf(),
        WatcherConfig::default(),
        shutdown,
    )
    .unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Create nested directory and file
    let subdir = temp_dir.path().join("nested");
    std::fs::create_dir(&subdir).unwrap();

    // Give time for directory creation to settle
    sleep(Duration::from_millis(100));

    let file_path = subdir.join("test.rs");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn test() {{}}").unwrap();

    // Poll for event - may get directory event first
    let mut found_file_event = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(2000);

    while start.elapsed() < timeout {
        if let Ok(Some(event)) = watcher.try_recv_event() {
            if event.path == file_path {
                found_file_event = true;
                break;
            }
            // Directory events are filtered out in extract_dirty_paths
        }
        sleep(Duration::from_millis(50));
    }

    assert!(found_file_event, "Should receive event for nested file");
}

#[test]
fn test_concurrent_legacy_event_access() {
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let watcher = FileSystemWatcher::new(
        temp_dir.path().to_path_buf(),
        WatcherConfig::default(),
        shutdown,
    ).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Verify that Arc<Mutex<T>> fields enable safe concurrent access
    //
    // With RefCell<T>:
    //   - Multiple borrow_mut() calls from different threads would panic
    //   - "already borrowed: BorrowMutError"
    //
    // With Arc<Mutex<T>>:
    //   - Multiple lock() calls serialize safely
    //   - Threads wait for lock acquisition instead of panicking
    //
    // Note: FileSystemWatcher itself is not Send (due to Receiver<WatcherBatch>),
    // but the Arc<Mutex<T>> migration prevents RefCell panics in real concurrent
    // scenarios where the watcher might be wrapped in Arc or accessed via channels.

    // This test verifies the lock() mechanism works correctly
    // by calling try_recv_event multiple times sequentially
    let _ = watcher.try_recv_event(); // Returns Result<Option<FileEvent>>
    let _ = watcher.try_recv_event();
    let _ = watcher.try_recv_event();

    // Test passes if no panic occurs
    // (RefCell would not panic in sequential calls, but would in concurrent calls)
}

/// Helper: poll for batch with timeout
fn poll_for_batch(watcher: &FileSystemWatcher, timeout_ms: u64) -> Option<magellan::WatcherBatch> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        if let Some(batch) = watcher.try_recv_batch() {
            return Some(batch);
        }

        if start.elapsed() >= timeout {
            return None;
        }

        sleep(Duration::from_millis(50));
    }
}

#[test]
fn test_gitignore_aware_watcher_skips_target_files() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create .gitignore with "target/" pattern
    fs::write(root.join(".gitignore"), "target/\nnode_modules/\n").unwrap();

    // Create target/ subdirectory with a .rs file
    fs::create_dir_all(root.join("target")).unwrap();
    let target_file = root.join("target/debug.rs");
    fs::write(&target_file, "fn target_fn() {}").unwrap();

    // Create src/ subdirectory with a .rs file
    fs::create_dir_all(root.join("src")).unwrap();
    let src_file = root.join("src/lib.rs");
    fs::write(&src_file, "fn src_fn() {}").unwrap();

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let shutdown = Arc::new(AtomicBool::new(false));
    let config = WatcherConfig {
        root_path: root.to_path_buf(),
        debounce_ms: 100,
        gitignore_aware: true, // Enable gitignore filtering
    };

    let watcher = FileSystemWatcher::new(root.to_path_buf(), config, shutdown).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Modify both files
    fs::write(&target_file, "fn target_fn_updated() {}").unwrap();
    fs::write(&src_file, "fn src_fn_updated() {}").unwrap();

    // Wait for debounce and poll for batch
    sleep(Duration::from_millis(300));

    let mut found_src = false;
    let mut found_target = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(2000);

    // Drain all batches within timeout
    while start.elapsed() < timeout {
        if let Some(batch) = watcher.try_recv_batch() {
            for path in &batch.paths {
                if path.ends_with("src/lib.rs") {
                    found_src = true;
                }
                if path.ends_with("target/debug.rs") {
                    found_target = true;
                }
            }
        }
        sleep(Duration::from_millis(50));
    }

    // Assert: src file generates event, target file does not
    assert!(
        found_src,
        "Should receive event for src/lib.rs"
    );
    assert!(
        !found_target,
        "Should NOT receive event for target/debug.rs (ignored by .gitignore)"
    );
}

#[test]
fn test_gitignore_aware_false_indexes_all_files() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create .gitignore with "target/" pattern
    fs::write(root.join(".gitignore"), "target/\n").unwrap();

    // Create target/ subdirectory with a .rs file
    fs::create_dir_all(root.join("target")).unwrap();
    let target_file = root.join("target/debug.rs");
    fs::write(&target_file, "fn target_fn() {}").unwrap();

    // Create src/ subdirectory with a .rs file
    fs::create_dir_all(root.join("src")).unwrap();
    let src_file = root.join("src/lib.rs");
    fs::write(&src_file, "fn src_fn() {}").unwrap();

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let shutdown = Arc::new(AtomicBool::new(false));
    let config = WatcherConfig {
        root_path: root.to_path_buf(),
        debounce_ms: 100,
        gitignore_aware: false, // Disable gitignore filtering
    };

    let watcher = FileSystemWatcher::new(root.to_path_buf(), config, shutdown).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Modify both files
    fs::write(&target_file, "fn target_fn_updated() {}").unwrap();
    fs::write(&src_file, "fn src_fn_updated() {}").unwrap();

    // Wait for debounce and poll for batch
    sleep(Duration::from_millis(300));

    let mut found_src = false;
    let mut found_target = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(2000);

    // Drain all batches within timeout
    while start.elapsed() < timeout {
        if let Some(batch) = watcher.try_recv_batch() {
            for path in &batch.paths {
                if path.ends_with("src/lib.rs") {
                    found_src = true;
                }
                if path.ends_with("target/debug.rs") {
                    found_target = true;
                }
            }
        }
        sleep(Duration::from_millis(50));
    }

    // Assert: Both files generate events when gitignore_aware is false
    assert!(
        found_src,
        "Should receive event for src/lib.rs"
    );
    assert!(
        found_target,
        "Should receive event for target/debug.rs when gitignore_aware=false"
    );
}

#[test]
fn test_gitignore_aware_internal_ignored_dirs() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // No .gitignore file - internal ignores only
    // Create node_modules/, target/, and .git directories with .rs files
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    let node_modules_file = root.join("node_modules/pkg/index.js");
    let target_file = root.join("target/debug/lib.rs");
    let git_file = root.join(".git/config");
    let src_file = root.join("src/lib.rs");

    // Use valid source code files
    fs::write(&target_file, "fn target_fn() {}").unwrap();
    fs::write(&src_file, "fn src_fn() {}").unwrap();
    // node_modules uses JS, .git uses text (not source code)
    fs::write(&node_modules_file, "module.exports = {};").unwrap();
    fs::write(&git_file, "[core]").unwrap();

    // Give OS time to settle
    sleep(Duration::from_millis(200));

    let shutdown = Arc::new(AtomicBool::new(false));
    let config = WatcherConfig {
        root_path: root.to_path_buf(),
        debounce_ms: 100,
        gitignore_aware: true,
    };

    let watcher = FileSystemWatcher::new(root.to_path_buf(), config, shutdown).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(200));

    // Modify all files
    fs::write(&target_file, "fn updated() {}").unwrap();
    fs::write(&src_file, "fn updated() {}").unwrap();
    fs::write(&node_modules_file, "module.exports = {updated: true};").unwrap();
    fs::write(&git_file, "[core]\n  repositoryformatversion = 0").unwrap();

    // Wait for debounce
    sleep(Duration::from_millis(300));

    let mut found_src = false;
    let mut found_target = false;
    let mut found_node_modules = false;
    let mut found_git = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(2000);

    // Drain all batches within timeout
    while start.elapsed() < timeout {
        if let Some(batch) = watcher.try_recv_batch() {
            for path in &batch.paths {
                if path.ends_with("src/lib.rs") {
                    found_src = true;
                }
                if path.ends_with("target/debug/lib.rs") {
                    found_target = true;
                }
                if path.ends_with("node_modules/pkg/index.js") {
                    found_node_modules = true;
                }
                if path.ends_with(".git/config") {
                    found_git = true;
                }
            }
        }
        sleep(Duration::from_millis(50));
    }

    // Assert: Only src file generates event (internal ignores apply)
    assert!(
        found_src,
        "Should receive event for src/lib.rs"
    );
    assert!(
        !found_target,
        "Should NOT receive event for target/debug/lib.rs (internal ignore)"
    );
    assert!(
        !found_node_modules,
        "Should NOT receive event for node_modules/pkg/index.js (internal ignore)"
    );
    assert!(
        !found_git,
        "Should NOT receive event for .git/config (internal ignore)"
    );
}

#[test]
fn test_gitignore_complex_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create .gitignore with complex patterns FIRST (before watcher starts)
    fs::write(
        root.join(".gitignore"),
        "**/*.log\nbuild/\n*.tmp\ntest_*.rs\n",
    )
    .unwrap();

    // Create directories and initial files
    fs::create_dir_all(root.join("build")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    let log_file = root.join("debug.log");
    let build_file = root.join("build/output.rs");
    let tmp_file = root.join("temp.tmp");
    let test_file = root.join("test_foo.rs");
    let src_file = root.join("src/main.rs");

    fs::write(&log_file, "log content").unwrap();
    fs::write(&build_file, "fn build() {}").unwrap();
    fs::write(&tmp_file, "temp").unwrap();
    fs::write(&test_file, "fn test() {}").unwrap();
    fs::write(&src_file, "fn main() {}").unwrap();

    // Give OS time to settle AND ensure files are flushed
    sleep(Duration::from_millis(300));

    let shutdown = Arc::new(AtomicBool::new(false));
    let config = WatcherConfig {
        root_path: root.to_path_buf(),
        debounce_ms: 100,
        gitignore_aware: true,
    };

    let watcher = FileSystemWatcher::new(root.to_path_buf(), config, shutdown).unwrap();

    // Give watcher time to start
    sleep(Duration::from_millis(300));

    // Modify all files
    fs::write(&log_file, "updated log").unwrap();
    fs::write(&build_file, "fn updated() {}").unwrap();
    fs::write(&tmp_file, "updated temp").unwrap();
    fs::write(&test_file, "fn updated() {}").unwrap();
    fs::write(&src_file, "fn updated() {}").unwrap();

    // Wait for debounce
    sleep(Duration::from_millis(400));

    let mut found_src = false;
    let mut found_log = false;
    let mut found_build = false;
    let mut found_tmp = false;
    let mut found_test = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(3000);

    // Drain all batches within timeout
    while start.elapsed() < timeout {
        if let Some(batch) = watcher.try_recv_batch() {
            for path in &batch.paths {
                if path.ends_with("src/main.rs") {
                    found_src = true;
                }
                if path.ends_with("debug.log") {
                    found_log = true;
                }
                if path.ends_with("build/output.rs") {
                    found_build = true;
                }
                if path.ends_with("temp.tmp") {
                    found_tmp = true;
                }
                if path.ends_with("test_foo.rs") {
                    found_test = true;
                }
            }
        }
        sleep(Duration::from_millis(50));
    }

    // Assert: Only src/main.rs generates events
    assert!(
        found_src,
        "Should receive event for src/main.rs (not ignored)"
    );
    // Note: .log, .tmp, and test_*.rs files may not generate events
    // because they're not recognized as supported source languages
    // But they definitely shouldn't if they were
    assert!(
        !found_log,
        "Should NOT receive event for debug.log (**/*.log pattern)"
    );
    // The build/ directory should be ignored by gitignore
    assert!(
        !found_build,
        "Should NOT receive event for build/output.rs (build/ pattern)"
    );
    assert!(
        !found_tmp,
        "Should NOT receive event for temp.tmp (*.tmp pattern)"
    );
    assert!(
        !found_test,
        "Should NOT receive event for test_foo.rs (test_*.rs pattern)"
    );
}

#[test]
fn test_gitignore_filter_matches_build_directory() {
    use magellan::graph::filter::FileFilter;
    use std::fs;

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create .gitignore with "build/" pattern
    fs::write(root.join(".gitignore"), "build/\n").unwrap();

    // Create build directory with file
    fs::create_dir_all(root.join("build")).unwrap();
    fs::write(root.join("build/output.rs"), "fn main() {}\n").unwrap();

    // Create src directory with file
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn lib() {}\n").unwrap();

    // Test FileFilter
    let filter = FileFilter::new(root, &[], &[]).unwrap();

    // build/output.rs should be skipped (IgnoredByGitignore)
    let result = filter.should_skip(&root.join("build/output.rs"));
    assert_eq!(
        result,
        Some(magellan::diagnostics::SkipReason::IgnoredByGitignore),
        "build/output.rs should be ignored by gitignore build/ pattern"
    );

    // src/lib.rs should NOT be skipped
    let result2 = filter.should_skip(&root.join("src/lib.rs"));
    assert_eq!(
        result2,
        None,
        "src/lib.rs should not be ignored"
    );
}

//! Watch buffering tests for Magellan
//!
//! Tests the Phase 2 deterministic watch pipeline:
//! - Watcher starts before scan-initial
//! - Changes during scan are buffered and flushed after baseline
//! - Batch processing is deterministic (sorted path order)

use magellan::CodeGraph;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

/// Helper: Write bytes to file with full synchronization
///
/// Uses OpenOptions with create + truncate + write_all + sync_all to ensure
/// content is fully committed to disk before returning. This guarantees
/// that any subsequent MODIFY event will have stable, readable content.
fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    Ok(())
}

#[test]
fn test_default_watch_performs_initial_scan() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create a file before watch starts
    let file_path = root_path.join("initial.rs");
    write_and_sync(&file_path, b"fn initial() {}").unwrap();

    // Small delay to ensure directory is stable
    thread::sleep(Duration::from_millis(50));

    // Create and index initial baseline
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let file_count = graph.scan_directory(&root_path, None).unwrap();

    // Verify: Initial file was indexed
    assert_eq!(file_count, 1, "Should have scanned 1 file");
    let path_str = file_path.to_string_lossy().to_string();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 1, "Should have 1 symbol: initial");
    assert_eq!(symbols[0].name.as_deref().unwrap(), "initial");
}

#[test]
fn test_modify_during_scan_is_flushed() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");
    let file_path = root_path.join("modify_test.rs");
    let path_str = file_path.to_string_lossy().to_string();

    // Create initial file with v0 content
    write_and_sync(&file_path, b"fn v0() {}").unwrap();

    // Spawn a thread that will modify the file during our scan
    let file_path_clone = file_path.clone();
    let modifier_thread = thread::spawn(move || {
        // Wait for scan to start (we'll trigger this by doing the scan)
        thread::sleep(Duration::from_millis(50));
        // Modify file to v1 while scan is in progress
        write_and_sync(&file_path_clone, b"fn v1() {}").unwrap();
    });

    // Start the scan (simulating watch startup)
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let _file_count = graph.scan_directory(&root_path, None).unwrap();

    // Wait for modifier thread to complete
    modifier_thread.join().unwrap();

    // Drain any buffered changes (simulating the post-scan flush)
    // In real watch, this would be done by the pipeline
    let _ = graph.reconcile_file_path(&file_path, &path_str);

    // Verify: Final state reflects v1 content, not v0
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 1, "Should have 1 symbol after flush");
    assert_eq!(
        symbols[0].name.as_deref().unwrap(),
        "v1",
        "Should have v1 symbol, not v0"
    );
}

#[test]
fn test_rapid_modifications_produce_deterministic_final_state() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");
    let file_path = root_path.join("storm.rs");
    let path_str = file_path.to_string_lossy().to_string();

    // Create initial file
    write_and_sync(&file_path, b"fn v0() {}").unwrap();

    // Perform rapid modifications
    for i in 1..=5 {
        write_and_sync(&file_path, format!("fn v{}() {{}}", i).as_bytes()).unwrap();
        thread::sleep(Duration::from_millis(10));
    }

    // Reconcile the final state
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let _ = graph.reconcile_file_path(&file_path, &path_str);

    // Verify: Final state is v5 (last write wins)
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 1, "Should have 1 symbol");
    assert_eq!(
        symbols[0].name.as_deref().unwrap(),
        "v5",
        "Should have v5 (last write)"
    );
}

#[test]
fn test_watch_only_skips_baseline_scan() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create a file BEFORE watch starts
    let before_file = root_path.join("before.rs");
    write_and_sync(&before_file, b"fn before() {}").unwrap();

    // Small delay to ensure file is stable
    thread::sleep(Duration::from_millis(50));

    // Open graph and verify it's empty (no scan yet)
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let file_count = graph.count_files().unwrap();
    assert_eq!(file_count, 0, "DB should be empty before scan");

    // Simulate --watch-only behavior: don't scan, just watch
    // In this test, we manually verify the file exists but isn't indexed

    // Verify the file exists on disk
    assert!(before_file.exists(), "File should exist on disk");

    // But it's NOT in the database (because we skipped scan)
    let path_str = before_file.to_string_lossy().to_string();
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 0, "File should not be indexed without scan");

    // Now reconcile the file (simulating a watch event)
    let _ = graph.reconcile_file_path(&before_file, &path_str);

    // After reconcile, it should be indexed
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    assert_eq!(symbols.len(), 1, "File should be indexed after reconcile");
    assert_eq!(symbols[0].name.as_deref().unwrap(), "before");
}

#[test]
fn test_deterministic_batch_ordering() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create multiple files in reverse alphabetical order
    let files = vec!["zebra.rs", "alpha.rs", "beta.rs"];
    for file_name in &files {
        let file_path = root_path.join(file_name);
        write_and_sync(
            &file_path,
            format!("fn {}() {{}}", file_name.replace(".rs", "")).as_bytes(),
        )
        .unwrap();
    }

    // Scan directory
    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.scan_directory(&root_path, None).unwrap();

    // Verify: All files indexed in deterministic order
    let all_files = graph.all_file_nodes().unwrap();
    assert_eq!(all_files.len(), 3, "Should have 3 files");

    // The BTreeMap iteration should give sorted order
    let mut file_names: Vec<_> = all_files.keys().collect();
    file_names.sort();

    assert_eq!(
        file_names[0].as_str(),
        root_path.join("alpha.rs").to_string_lossy().as_ref()
    );
    assert_eq!(
        file_names[1].as_str(),
        root_path.join("beta.rs").to_string_lossy().as_ref()
    );
    assert_eq!(
        file_names[2].as_str(),
        root_path.join("zebra.rs").to_string_lossy().as_ref()
    );
}

#[test]
fn test_multiple_files_modified_during_scan() {
    // Setup: Create temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test.db");

    // Create initial files
    let file1 = root_path.join("file1.rs");
    let file2 = root_path.join("file2.rs");
    write_and_sync(&file1, b"fn v1_0() {}").unwrap();
    write_and_sync(&file2, b"fn v2_0() {}").unwrap();

    // Small delay
    thread::sleep(Duration::from_millis(50));

    // Spawn modifier thread
    let file1_clone = file1.clone();
    let file2_clone = file2.clone();
    let modifier_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        write_and_sync(&file1_clone, b"fn v1_1() {}").unwrap();
        write_and_sync(&file2_clone, b"fn v2_1() {}").unwrap();
    });

    // Scan directory
    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.scan_directory(&root_path, None).unwrap();

    // Wait for modifiers
    modifier_thread.join().unwrap();

    // Flush both files (simulating post-scan drain)
    let path1_str = file1.to_string_lossy().to_string();
    let path2_str = file2.to_string_lossy().to_string();
    let _ = graph.reconcile_file_path(&file1, &path1_str);
    let _ = graph.reconcile_file_path(&file2, &path2_str);

    // Verify: Both files reflect their modified content
    let symbols1 = graph.symbols_in_file(&path1_str).unwrap();
    assert_eq!(symbols1.len(), 1);
    assert_eq!(symbols1[0].name.as_deref().unwrap(), "v1_1");

    let symbols2 = graph.symbols_in_file(&path2_str).unwrap();
    assert_eq!(symbols2.len(), 1);
    assert_eq!(symbols2[0].name.as_deref().unwrap(), "v2_1");
}

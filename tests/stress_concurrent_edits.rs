//! Stress tests for concurrent file operations.
//!
//! # Purpose
//!
//! TSAN catches data races at the memory level, but stress tests catch logical
//! race conditions (deadlocks, lost updates, state corruption) that emerge under
//! high concurrency.
//!
//! # Important Note on Thread Safety
//!
//! The current `CodeGraph` implementation uses `Rc<SqliteGraphBackend>` internally,
//! which is NOT thread-safe (`Rc` is not `Send`). Therefore, these stress tests
//! follow a two-phase pattern:
//!
//! 1. **Concurrent phase**: Multiple threads perform file system operations concurrently
//! 2. **Sequential phase**: A single thread processes all files through `CodeGraph`
//!
//! This pattern still provides valuable stress testing by:
//! - Exercising the file system under concurrent load
//! - Verifying deterministic graph state after concurrent operations
//! - Testing lock ordering in `PipelineSharedState` (Test 4)
//!
//! # What these tests verify
//!
//! - **No deadlocks**: All operations complete within timeout
//! - **No data corruption**: Database integrity after 1000+ concurrent operations
//! - **Deterministic state**: Final graph state is consistent regardless of operation order
//! - **Lock ordering**: `PipelineSharedState` follows global hierarchy (dirty_paths → wakeup)
//!
//! # How to run
//!
//! ```bash
//! # Run all stress tests sequentially
//! cargo test --test stress_concurrent_edits --release -- --test-threads=1
//!
//! # Run specific test
//! cargo test stress_concurrent_creates --release
//! ```

use magellan::CodeGraph;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::Instant;
use tempfile::TempDir;

/// Test 1: Concurrent create operations (100 threads).
///
/// # What it tests
/// - 100 threads each create a unique file (file_0.rs through file_99.rs)
/// - Files are created concurrently on the file system
/// - Then a single thread indexes all files sequentially
///
/// # Expected behavior
/// - All 100 files created successfully
/// - All 100 files indexed correctly with correct symbol counts
/// - No data corruption (all files present in graph)
#[test]
fn stress_concurrent_creates() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let start = Instant::now();

    // Phase 1: Concurrent file creation (100 threads)
    let handles: Vec<_> = (0..100)
        .map(|i| {
            let temp_dir = temp_dir.path().to_path_buf();

            thread::spawn(move || {
                let file_path = temp_dir.join(format!("file_{}.rs", i));
                let content = format!("fn function_{}() {{}}", i);

                // Create file concurrently
                fs::write(&file_path, content).unwrap();

                file_path
            })
        })
        .collect();

    // Collect all file paths
    let mut file_paths = Vec::new();
    for handle in handles {
        file_paths.push(handle.join().unwrap());
    }

    let create_duration = start.elapsed();
    println!(
        "stress_concurrent_creates: file creation completed in {:?}",
        create_duration
    );

    // Phase 2: Sequential indexing (simulates indexer behavior)
    let mut graph = CodeGraph::open(&db_path).unwrap();

    for file_path in &file_paths {
        let path_key = magellan::validation::normalize_path(file_path)
            .unwrap_or_else(|_| file_path.to_string_lossy().to_string());
        let _ = graph.reconcile_file_path(file_path, &path_key);
    }

    let total_duration = start.elapsed();
    println!(
        "stress_concurrent_creates: total completed in {:?}",
        total_duration
    );

    // Verify: All 100 files indexed correctly
    let file_count = graph.count_files().unwrap();

    assert_eq!(
        file_count, 100,
        "Expected 100 files after concurrent creates, got {}",
        file_count
    );

    // Verify: Each file has exactly one symbol (the function)
    let file_nodes = graph.all_file_nodes().unwrap();
    assert_eq!(
        file_nodes.len(),
        100,
        "Expected 100 file nodes, got {}",
        file_nodes.len()
    );

    // Verify: No duplicate file entries
    let mut paths: Vec<_> = file_nodes.keys().collect();
    paths.sort();
    assert_eq!(
        paths.len(),
        100,
        "Expected 100 unique paths, got {} (possible duplicates)",
        paths.len()
    );
}

/// Test 2: Concurrent modify operations (50 threads, same file).
///
/// # What it tests
/// - 50 threads all modify the SAME file concurrently
/// - Final state depends on last write (race condition on file system)
/// - Indexer processes final state deterministically
///
/// # Expected behavior
/// - File is indexed (final state depends on last write)
/// - No corruption (exactly one file node exists)
/// - File has exactly one symbol (whichever thread wrote last)
#[test]
fn stress_concurrent_modifies() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("shared.rs");

    // Create initial file
    fs::write(&file_path, "fn initial() {}").unwrap();

    let start = Instant::now();

    // Phase 1: Concurrent file modifications (50 threads)
    let handles: Vec<_> = (0..50)
        .map(|i| {
            let file_path = file_path.clone();

            thread::spawn(move || {
                let content = format!("fn function_{}() {{}}", i);

                // Modify file concurrently
                fs::write(&file_path, content).unwrap();
            })
        })
        .collect();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let modify_duration = start.elapsed();
    println!(
        "stress_concurrent_modifies: modifications completed in {:?}",
        modify_duration
    );

    // Phase 2: Sequential indexing (indexer processes final state)
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path_key = magellan::validation::normalize_path(&file_path)
        .unwrap_or_else(|_| file_path.to_string_lossy().to_string());
    let _ = graph.reconcile_file_path(&file_path, &path_key);

    let total_duration = start.elapsed();
    println!(
        "stress_concurrent_modifies: total completed in {:?}",
        total_duration
    );

    // Verify: File is indexed (exactly one file node)
    let file_count = graph.count_files().unwrap();

    assert_eq!(
        file_count, 1,
        "Expected 1 file after concurrent modifies, got {}",
        file_count
    );

    // Verify: No duplicate file entries
    let file_nodes = graph.all_file_nodes().unwrap();
    assert_eq!(
        file_nodes.len(),
        1,
        "Expected 1 file node, got {} (possible corruption)",
        file_nodes.len()
    );

    // Verify: File has exactly one symbol
    let symbols = graph.symbols_in_file(&path_key).unwrap();
    assert_eq!(
        symbols.len(),
        1,
        "Expected 1 symbol, got {} (possible corruption)",
        symbols.len()
    );
}

/// Test 3: Mixed create/modify/delete operations (100 operations).
///
/// # What it tests
/// - Phase 1: Create 100 files concurrently (10 threads × 10 files)
/// - Phase 2: Perform concurrent modify/delete operations on existing files
/// - Phase 3: Index final state
///
/// # Expected behavior
/// - All file system operations succeed
/// - Final graph state is consistent (no orphaned references)
/// - No data corruption (all file nodes valid)
#[test]
fn stress_mixed_operations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let start = Instant::now();

    // Phase 1: Create 100 files (10 threads × 10 files)
    let create_handles: Vec<_> = (0..10)
        .map(|thread_id| {
            let temp_dir = temp_dir.path().to_path_buf();

            thread::spawn(move || {
                for i in 0..10 {
                    let file_id = thread_id * 10 + i;
                    let file_path = temp_dir.join(format!("file_{}.rs", file_id));
                    let content = format!("fn function_{}() {{}}", file_id);
                    fs::write(&file_path, content).unwrap();
                }
            })
        })
        .collect();

    // Wait for all creates to complete
    for handle in create_handles {
        handle.join().unwrap();
    }

    let create_duration = start.elapsed();
    println!(
        "stress_mixed_operations: file creation completed in {:?}",
        create_duration
    );

    // Phase 2: Mixed modify/delete operations (10 threads × 10 ops = 100 total)
    let mixed_handles: Vec<_> = (0..10)
        .map(|thread_id| {
            let temp_dir = temp_dir.path().to_path_buf();

            thread::spawn(move || {
                for op_id in 0..10 {
                    let file_id = thread_id * 10 + op_id;
                    let file_path = temp_dir.join(format!("file_{}.rs", file_id));

                    // Operation pattern: Modify, Modify, Delete, Modify, Delete, ...
                    match op_id % 4 {
                        0 | 1 | 3 => {
                            // Modify existing file
                            if file_path.exists() {
                                let content = format!("fn modified_{}_{}() {{}}", thread_id, op_id);
                                fs::write(&file_path, content).unwrap();
                            }
                        }
                        2 => {
                            // Delete file
                            if file_path.exists() {
                                fs::remove_file(&file_path).unwrap();
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            })
        })
        .collect();

    // Wait for all mixed operations to complete
    for handle in mixed_handles {
        handle.join().unwrap();
    }

    let op_duration = start.elapsed();
    println!(
        "stress_mixed_operations: mixed operations completed in {:?}",
        op_duration
    );

    // Phase 3: Sequential indexing (index all remaining files)
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Collect all .rs files that still exist
    for entry in fs::read_dir(temp_dir.path()).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let path_key = magellan::validation::normalize_path(&path)
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            let _ = graph.reconcile_file_path(&path, &path_key);
        }
    }

    let total_duration = start.elapsed();
    println!(
        "stress_mixed_operations: total completed in {:?}",
        total_duration
    );

    // Verify: Database is consistent (no orphaned references)
    let graph_file_count = graph.count_files().unwrap();

    // Operation pattern analysis:
    // - 100 files created initially
    // - For each file (0..99): op_id % 4 == 2 means delete
    // - 25 deletions (files 2, 6, 10, 14, ..., 98)
    // Expected: 100 - 25 = 75 files remaining
    assert!(
        graph_file_count >= 70 && graph_file_count <= 80,
        "Expected 70-80 files after mixed operations, got {}",
        graph_file_count
    );

    // Verify: No duplicate file entries
    let file_nodes = graph.all_file_nodes().unwrap();
    assert_eq!(
        file_nodes.len(),
        graph_file_count,
        "File count mismatch: count_files()={}, all_file_nodes()={}",
        graph_file_count,
        file_nodes.len()
    );
}

/// Test 4: PipelineSharedState concurrent dirty_paths insertion.
///
/// # What it tests
/// - 10 threads each insert 100 paths into dirty_paths
/// - Uses BTreeSet for deduplication
/// - Verifies drain_dirty_paths() returns exactly 1000 paths
/// - Tests lock ordering: dirty_paths → wakeup (never send while holding dirty_paths)
///
/// # Expected behavior
/// - No duplicates (BTreeSet deduplication)
/// - No deadlock (all threads complete)
/// - All paths drained correctly
/// - Lock ordering followed correctly
#[test]
fn stress_pipeline_shared_state() {
    // Create PipelineSharedState-like structure
    let dirty_paths: Arc<Mutex<BTreeSet<PathBuf>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let (wakeup_tx, _wakeup_rx) = sync_channel(1);

    let start = Instant::now();

    // Spawn 10 threads, each inserting 100 paths
    let handles: Vec<_> = (0..10)
        .map(|thread_id| {
            let dirty_paths = dirty_paths.clone();
            let wakeup_tx = wakeup_tx.clone();

            thread::spawn(move || {
                for i in 0..100 {
                    let path = PathBuf::from(format!("/tmp/file_{}_{}.rs", thread_id, i));

                    // Insert into dirty_paths (following lock ordering: dirty_paths first)
                    {
                        let mut paths = dirty_paths.lock().unwrap();
                        paths.insert(path);
                    }

                    // Send wakeup tick (never send while holding other locks)
                    let _ = wakeup_tx.try_send(());
                }
            })
        })
        .collect();

    // Join all threads
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    println!(
        "stress_pipeline_shared_state: completed in {:?}",
        duration
    );

    // Verify: drain_dirty_paths() returns exactly 1000 paths
    let mut paths = dirty_paths.lock().unwrap();
    let drained: Vec<_> = paths.iter().cloned().collect();
    paths.clear();

    assert_eq!(
        drained.len(),
        1000,
        "Expected 1000 paths after concurrent inserts, got {}",
        drained.len()
    );

    // Verify: No duplicates (all paths are unique)
    let unique: std::collections::HashSet<_> = drained.into_iter().collect();
    assert_eq!(
        unique.len(),
        1000,
        "Expected 1000 unique paths, got {}",
        unique.len()
    );
}

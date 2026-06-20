//! Indexer coordinator for Magellan
//!
//! Wires filesystem watcher events to graph updates.
//! Maintains synchronous consistency between filesystem and sqlitegraph.
//!
//! # Lock Ordering
//!
//! This module defines the global lock ordering hierarchy:
//!
//! 1. **dirty_paths lock** (PipelineSharedState)—acquired first
//! 2. **wakeup channel send**—acquired last
//!
//! **Rule:** Never send to wakeup channel while holding other locks.
//! See `PipelineSharedState` for detailed documentation.
//!
//! # L3 Cache-Aware Batching
//!
//! Indexing performance is optimized by keeping data in L3 cache during processing:
//!
//! 1. **Batch size calculation**: Groups files to fit ~50% of L3 cache
//! 2. **Read phase**: All source files in batch read into memory
//! 3. **Parse phase**: All files parsed (ASTs stay in cache)
//! 4. **Insert phase**: Batch insert in single transaction
//!
//! This keeps symbol facts and references hot during batch processing.

pub mod async_io;

pub mod watch;
pub use watch::{run_watch_pipeline, WatchPipelineConfig};

// L3 Cache-Aware Batching Configuration
//
// Target ~50% of L3 cache to leave room for AST, parsed data, and SQLite buffers.
// Typical L3 sizes: 8MB (mobile/small core) to 32MB (desktop/server)
pub(crate) const DEFAULT_L3_CACHE_SIZE: usize = 16 * 1024 * 1024; // 16MB default
pub(crate) const TARGET_CACHE_USAGE: f64 = 0.50; // Use 50% of L3 for working set
const _AVG_SOURCE_FILE_SIZE: usize = 50 * 1024; // 50KB average

/// Result of a single file batch operation
#[derive(Debug)]
pub struct FileBatchResult {
    /// Number of symbols indexed
    pub symbols: usize,
    /// Number of references indexed
    pub references: usize,
    /// Number of calls indexed
    pub calls: usize,
    /// Bytes processed
    pub bytes_processed: usize,
}

/// Compute optimal batch size based on file sizes.
///
/// Groups file *indices* into batches that fit within L3 cache target.
///
/// Returns `Vec<Vec<usize>>` where each inner Vec contains indices into the input
/// sizes slice. Zero-allocation batching: only `usize` sizes are read, no paths cloned.
pub fn compute_l3_cache_batch_indices(
    sizes: &[usize],
    target_cache_bytes: usize,
) -> Vec<Vec<usize>> {
    if sizes.is_empty() {
        return Vec::new();
    }

    let mut batches: Vec<Vec<usize>> = Vec::new();
    let mut current_batch: Vec<usize> = Vec::new();
    let mut current_batch_size: usize = 0;

    for (idx, size) in sizes.iter().enumerate() {
        if current_batch.is_empty() || current_batch_size + size <= target_cache_bytes {
            current_batch.push(idx);
            current_batch_size += size;
        } else {
            // Start new batch with this file
            batches.push(std::mem::take(&mut current_batch));
            current_batch.push(idx);
            current_batch_size = *size;
        }
    }

    // Don't forget the last batch
    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    batches
}

/// Groups files into batches that fit within L3 cache target.
/// Each batch contains paths whose total size <= target_cache_bytes.
/// Read source files for a batch of paths.
///
/// Returns a map from path to (source bytes, file size) for files that exist.
/// Missing files are not included in the result.
pub fn read_batch_sources<P: AsRef<std::path::Path>>(
    paths: &[P],
) -> Vec<(PathBuf, Vec<u8>, usize)> {
    paths
        .iter()
        .filter_map(|path| {
            let path = path.as_ref();
            std::fs::read(path).ok().map(|source| {
                let len = source.len();
                (path.to_path_buf(), source, len)
            })
        })
        .collect()
}

// Debug macro - only enabled when debug-prints feature is active
#[cfg(feature = "debug-prints")]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        { eprintln!($($arg)*); }
    };
}

#[cfg(not(feature = "debug-prints"))]
#[allow(
    unused_macros,
    reason = "noop stub: only used when feature debug-prints is enabled"
)]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        // Optimized out when debug-prints feature is disabled
        // Always return () to work in expression context
        {
            #[allow(
                clippy::unused_unit,
                reason = "macro must expand to unit in expression position when disabled"
            )]
            ()
        }
    };
}

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::{CodeGraph, FileEvent, FileSystemWatcher, WatcherConfig};

/// Reconcile files that exist in DB but not on filesystem.
///
/// This handles the case where files were deleted while the indexer wasn't running.
/// It scans all File nodes in the database and deletes any whose path doesn't exist
/// on the filesystem.
fn reconcile_deleted_files(graph: &mut CodeGraph, root_path: &std::path::Path) -> Result<()> {
    let file_nodes = graph.all_file_nodes()?;

    for (path, _file_node) in file_nodes {
        let file_path = std::path::Path::new(&path);
        // Only check files within our watched root
        if file_path.starts_with(root_path) && !file_path.exists() {
            graph.delete_file(&path)?;
        }
    }

    Ok(())
}

/// Handle a single file event, updating the graph appropriately.
///
/// # Arguments
/// * `graph` - Mutable reference to CodeGraph
/// * `event` - FileEvent to process
///
/// # Behavior
/// - Uses reconcile_file_path for deterministic update-or-delete semantics:
///   - File exists and changed: delete old data, re-index
///   - File exists and unchanged: no-op (skip re-indexing)
///   - File doesn't exist: delete all data
/// - This works for both Create/Modify/Delete events since we check filesystem state
fn handle_event(graph: &mut CodeGraph, event: FileEvent) -> Result<()> {
    // Use reconcile for deterministic handling regardless of event type
    // The debouncer doesn't preserve event types, so we check actual file state
    let path_key = crate::validation::normalize_path(&event.path)
        .unwrap_or_else(|_| event.path.to_string_lossy().to_string());
    let _outcome = graph.reconcile_file_path(&event.path, &path_key)?;
    Ok(())
}

/// Run the indexer loop, watching for filesystem changes and updating the graph.
///
/// # Arguments
/// * `root_path` - Directory to watch recursively
/// * `db_path` - Path to sqlitegraph database
///
/// # Behavior
/// - Creates FileSystemWatcher for root_path
/// - Opens CodeGraph at db_path
/// - Enters blocking loop processing events:
///   - Create/Modify: read file, delete old data, index symbols and references
///   - Delete: delete file and all derived data
/// - Errors propagate (no swallowing)
///
/// # Guarantees
/// - Synchronous: single-threaded, deterministic event processing
/// - Order-preserving: events processed in order received
/// - Idempotent: re-indexing same file produces same graph state
///
/// # Note
/// This is a BLOCKING function. It runs forever until the watcher
/// thread terminates (which typically never happens in normal operation).
pub fn run_indexer(root_path: PathBuf, db_path: PathBuf) -> Result<()> {
    // Delegate to bounded version with usize::MAX events
    run_indexer_n(root_path, db_path, usize::MAX)?;
    Ok(())
}

/// Run the indexer with a bounded number of events.
///
/// # Arguments
/// * `root_path` - Directory to watch recursively
/// * `db_path` - Path to sqlitegraph database
/// * `max_events` - Maximum number of events to process before returning
///
/// # Returns
/// Number of events processed
///
/// # Behavior
/// - Creates FileSystemWatcher for root_path
/// - Opens CodeGraph at db_path
/// - Processes up to max_events events, then returns
/// - Create/Modify: read file, delete old data, index symbols and references
/// - Delete: delete file and all derived data
/// - Errors propagate (no swallowing)
///
/// # Guarantees
/// - Synchronous: single-threaded, deterministic event processing
/// - Order-preserving: events processed in order received
/// - Idempotent: re-indexing same file produces same graph state
/// - Bounded: will NOT hang forever (unlike run_indexer)
///
/// # Use Cases
/// - Testing: process specific number of events and verify results
/// - Batch mode: process available events and exit
pub fn run_indexer_n(root_path: PathBuf, db_path: PathBuf, max_events: usize) -> Result<usize> {
    // Create shutdown signal for bounded execution
    // The watcher will exit when this function returns and the shutdown is dropped
    let shutdown = Arc::new(AtomicBool::new(false));

    // Create watcher
    let watcher = FileSystemWatcher::new(
        root_path.clone(),
        WatcherConfig::default(),
        shutdown.clone(),
    )?;

    // Open graph
    let mut graph = CodeGraph::open(&db_path)?;

    // Reconcile: Check for files that exist in DB but not on filesystem
    // This handles the case where files were deleted while indexer wasn't running
    reconcile_deleted_files(&mut graph, &root_path)?;

    // Process up to max_events events.
    //
    // IMPORTANT: In tests, notify may coalesce events or fail to emit the expected
    // number of events on some platforms/filesystems. We therefore include an idle
    // timeout to prevent hangs while preserving the "process up to N events" contract.
    let mut processed = 0;
    let mut idle_for = std::time::Duration::from_secs(0);
    let idle_step = std::time::Duration::from_millis(10);
    // Configurable timeout: default 5s (increased from 2s for slow filesystems/large files)
    // Can be overridden via MAGELLAN_WATCH_TIMEOUT_MS environment variable
    let idle_timeout_ms = std::env::var("MAGELLAN_WATCH_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5000);
    let idle_timeout = std::time::Duration::from_millis(idle_timeout_ms);

    while processed < max_events {
        match watcher.try_recv_event() {
            Ok(Some(event)) => {
                handle_event(&mut graph, event)?;
                processed += 1;
                idle_for = std::time::Duration::from_secs(0);
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                // Watcher event channel closed or unavailable — fail fast
                return Err(e.context("watcher event recv failed"));
            }
        }

        if idle_for >= idle_timeout {
            break;
        }

        std::thread::sleep(idle_step);
        idle_for += idle_step;
    }

    // Signal shutdown before watcher is dropped
    shutdown.store(true, Ordering::SeqCst);

    Ok(processed)
}

// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compute_l3_cache_batches_empty() {
        let sizes: Vec<usize> = Vec::new();
        let batches = compute_l3_cache_batch_indices(&sizes, 1024);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_compute_l3_cache_batches_single_file() {
        let sizes = vec![100usize];
        let batches = compute_l3_cache_batch_indices(&sizes, 1024);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0], 0);
    }

    #[test]
    fn test_compute_l3_cache_batches_fits_in_one_batch() {
        let sizes = vec![100usize, 200, 300];
        let batches = compute_l3_cache_batch_indices(&sizes, 1000);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
    }

    #[test]
    fn test_compute_l3_cache_batches_splits_on_limit() {
        let sizes = vec![500usize, 600, 300];
        let batches = compute_l3_cache_batch_indices(&sizes, 1000);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 1); // 500 only
        assert_eq!(batches[1].len(), 2); // 600,300
    }

    #[test]
    fn test_compute_l3_cache_batches_large_file() {
        // Large file that exceeds target gets its own batch,
        // and subsequent files that don't fit with it also get new batches
        let sizes = vec![100usize, 2000, 50];
        let batches = compute_l3_cache_batch_indices(&sizes, 1000);
        // Each file ends up in its own batch because:
        // - index 0 (100 bytes): batch 0
        // - index 1 (2000 bytes): doesn't fit with 0 (100+2000 > 1000), batch 1
        // - index 2 (50 bytes): doesn't fit with 1 (2000+50 > 1000), batch 2
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 1); // [0]
        assert_eq!(batches[1].len(), 1); // [1]
        assert_eq!(batches[2].len(), 1); // [2]
    }

    #[test]
    fn test_read_batch_sources_missing_files() {
        let paths = vec![PathBuf::from("/nonexistent/path/file.rs")];
        let sources = read_batch_sources(&paths);
        assert!(sources.is_empty());
    }

    #[test]
    fn test_read_batch_sources_existing_files() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("test1.rs");
        let file2 = temp_dir.path().join("test2.rs");

        fs::write(&file1, "fn main() {}").unwrap();
        fs::write(&file2, "fn other() {}").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        let sources = read_batch_sources(&paths);

        assert_eq!(sources.len(), 2);
        // Check that sources contain the right data
        let found: std::collections::HashSet<_> =
            sources.iter().map(|(p, _, _)| p.clone()).collect();
        assert!(found.contains(&file1));
        assert!(found.contains(&file2));
    }

    #[test]
    fn test_l3_cache_size_constants() {
        // Verify constants are reasonable
        assert_eq!(DEFAULT_L3_CACHE_SIZE, 16 * 1024 * 1024); // 16MB
        assert!((TARGET_CACHE_USAGE - 0.50).abs() < 0.001); // 50%
        assert_eq!(_AVG_SOURCE_FILE_SIZE, 50 * 1024); // 50KB
    }

    #[test]
    fn test_reconcile_with_source_uses_provided_bytes() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        let source = b"fn main() { let x = 1; }";
        let path = dir.path().join("test.rs");
        std::fs::write(&path, source).unwrap();

        let outcome = graph
            .reconcile_file_path_with_source(&path, "test.rs", source)
            .unwrap();

        match outcome {
            crate::ReconcileOutcome::Reindexed { symbols, .. } => {
                assert!(symbols > 0, "Should index symbols from provided source");
            }
            other => panic!("Expected Reindexed, got {:?}", other),
        }
    }

    /// Repro harness for the watch+scan-initial DB corruption bug.
    ///
    /// Runs the full watch pipeline (scan_initial=true) against the magellan
    /// source tree itself, then shuts down cleanly and verifies:
    /// 1. SQLite integrity_check passes
    /// 2. File entities exist in the DB
    /// 3. IMPLEMENTS edges are present (from impl indexing)
    /// 4. No duplicate file entries
    ///
    /// This tests the race between baseline scan and watcher startup,
    /// and the buffered-path drain that follows.
    #[test]
    #[ignore = "slow integration test: run with --ignored or --include-ignored"]
    fn test_watch_scan_initial_db_integrity() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("watch-repro.db");

        // Use the magellan src/ as the source tree (real workload)
        let magellan_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        if !magellan_src.exists() {
            eprintln!("Skipping: magellan src/ not found");
            return;
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_signal = shutdown.clone();

        let config = WatchPipelineConfig::new(
            magellan_src.clone(),
            db_path.clone(),
            crate::watcher::WatcherConfig {
                root_path: magellan_src.clone(),
                debounce_ms: 50, // Short debounce for faster test
                gitignore_aware: true,
            },
            true, // scan_initial = true
        );

        // Run pipeline — this does baseline scan + drain + enters watch loop
        let handle = std::thread::spawn(move || run_watch_pipeline(config, shutdown));

        // Give baseline scan + drain time to complete, then signal shutdown
        std::thread::sleep(std::time::Duration::from_secs(4));
        shutdown_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        // Wait for clean exit (up to 30s — pipeline has its own cleanup timeout)
        let result = handle.join().expect("watch pipeline thread panicked");
        assert!(result.is_ok(), "watch pipeline failed: {:?}", result.err());

        // Verify DB integrity
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok", "DB integrity check failed: {}", integrity);

        // Verify file entities exist
        let file_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_entities WHERE kind = 'File'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            file_count > 50,
            "Expected >50 file entities after scanning magellan src/, got {}",
            file_count
        );

        // Verify IMPLEMENTS edges exist (at least 1 from impl indexing)
        let impl_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'IMPLEMENTS'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            impl_count >= 1,
            "Expected >=1 IMPLEMENTS edges, got {}",
            impl_count
        );

        // Verify no duplicate file paths in graph_entities
        let distinct_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT file_path) FROM graph_entities WHERE kind = 'File'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            file_count, distinct_count,
            "Duplicate file paths detected: {} total vs {} distinct",
            file_count, distinct_count
        );
    }

    /// Stress test: run watch+scan-initial 5 times in sequence on fresh DBs.
    /// Catches intermittent corruption that a single run might miss.
    #[test]
    #[ignore = "slow integration test: run with --ignored or --include-ignored"]
    fn test_watch_scan_initial_stress() {
        use tempfile::tempdir;

        let magellan_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        if !magellan_src.exists() {
            eprintln!("Skipping: magellan src/ not found");
            return;
        }

        for i in 0..5 {
            let dir = tempdir().unwrap();
            let db_path = dir.path().join(format!("watch-stress-{}.db", i));

            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown_signal = shutdown.clone();
            let src_clone = magellan_src.clone();

            let config = WatchPipelineConfig::new(
                src_clone,
                db_path.clone(),
                crate::watcher::WatcherConfig {
                    root_path: magellan_src.clone(),
                    debounce_ms: 50,
                    gitignore_aware: true,
                },
                true,
            );

            let handle = std::thread::spawn(move || run_watch_pipeline(config, shutdown));

            std::thread::sleep(std::time::Duration::from_secs(4));
            shutdown_signal.store(true, std::sync::atomic::Ordering::SeqCst);

            let result = handle.join().expect("watch pipeline thread panicked");
            assert!(
                result.is_ok(),
                "Stress run {} failed: {:?}",
                i,
                result.err()
            );

            // Verify integrity
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let integrity: String = conn
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .unwrap();
            assert_eq!(
                integrity, "ok",
                "Stress run {} DB integrity failed: {}",
                i, integrity
            );
        }
    }

    #[test]
    fn test_watch_pipeline_config_basic() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("test.db");

        let config = WatchPipelineConfig::new(
            temp.path().to_path_buf(),
            db_path.clone(),
            WatcherConfig::default(),
            false,
        );

        assert_eq!(config.db_path, db_path);
        assert_eq!(config.root_path, temp.path().to_path_buf());
    }
}

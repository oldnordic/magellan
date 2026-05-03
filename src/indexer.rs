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

// L3 Cache-Aware Batching Configuration
//
// Target ~50% of L3 cache to leave room for AST, parsed data, and SQLite buffers.
// Typical L3 sizes: 8MB (mobile/small core) to 32MB (desktop/server)
const DEFAULT_L3_CACHE_SIZE: usize = 16 * 1024 * 1024; // 16MB default
const TARGET_CACHE_USAGE: f64 = 0.50; // Use 50% of L3 for working set
const AVG_SOURCE_FILE_SIZE: usize = 50 * 1024; // 50KB average

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
/// Groups files into batches that fit within L3 cache target.
/// Each batch contains paths whose total size <= target_cache_bytes.
pub fn compute_l3_cache_batches(
    paths_with_sizes: &[(PathBuf, usize)],
    target_cache_bytes: usize,
) -> Vec<Vec<(PathBuf, usize)>> {
    if paths_with_sizes.is_empty() {
        return Vec::new();
    }

    let mut batches: Vec<Vec<(PathBuf, usize)>> = Vec::new();
    let mut current_batch: Vec<(PathBuf, usize)> = Vec::new();
    let mut current_batch_size: usize = 0;

    for (path, size) in paths_with_sizes {
        // If current batch is empty or adding this file fits within target, add to current batch
        if current_batch.is_empty() || current_batch_size + size <= target_cache_bytes {
            current_batch.push((path.clone(), *size));
            current_batch_size += size;
        } else {
            // Start new batch with this file
            batches.push(std::mem::take(&mut current_batch));
            current_batch.push((path.clone(), *size));
            current_batch_size = *size;
        }
    }

    // Don't forget the last batch
    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    batches
}

/// Read source files for a batch of paths.
///
/// Returns a map from path to (source bytes, file size) for files that exist.
/// Missing files are not included in the result.
pub fn read_batch_sources(paths: &[PathBuf]) -> Vec<(PathBuf, Vec<u8>, usize)> {
    paths
        .iter()
        .filter_map(|path| {
            std::fs::read(path).ok().map(|source| {
                let len = source.len();
                (path.clone(), source, len)
            })
        })
        .collect()
}

// Debug macro - enabled for debug-prints or when geometric-backend needs it
#[cfg(any(feature = "debug-prints", feature = "geometric-backend"))]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        { eprintln!($($arg)*); }
    };
}

#[cfg(not(any(feature = "debug-prints", feature = "geometric-backend")))]
#[allow(unused_macros)] // Macro is used when geometric-backend feature is enabled
macro_rules! debug_print {
    ($($arg:tt)*) => {
        // Optimized out when debug-prints feature is disabled
        // Always return () to work in expression context
        {
            ()
        }
    };
}

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

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
            let _ = graph.delete_file(&path);
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
                // Mutex poisoned - fail fast
                return Err(e.context("watcher mutex poisoned during event recv"));
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
// Phase 2: Deterministic watch pipeline with buffering
// ============================================================================

/// Configuration for the deterministic watch pipeline.
#[derive(Debug, Clone)]
pub struct WatchPipelineConfig {
    /// Root directory to watch
    pub root_path: PathBuf,
    /// Path to the database
    pub db_path: PathBuf,
    /// Watcher configuration
    pub watcher_config: WatcherConfig,
    /// Whether to run initial baseline scan
    pub scan_initial: bool,
}

impl WatchPipelineConfig {
    /// Create a new pipeline configuration.
    pub fn new(
        root_path: PathBuf,
        db_path: PathBuf,
        watcher_config: WatcherConfig,
        scan_initial: bool,
    ) -> Self {
        Self {
            root_path,
            db_path,
            watcher_config,
            scan_initial,
        }
    }
}

/// Shared state for the watch pipeline.
///
/// # Lock Ordering
///
/// To prevent deadlocks, follow this global lock acquisition order:
///
/// 1. **dirty_paths lock** (lowest priority)—acquired first
/// 2. **wakeup channel send** (highest priority)—acquired last
///
/// **CRITICAL:** Never send to wakeup channel while holding other locks.
/// Always acquire dirty_paths lock, modify data, send wakeup, THEN release.
///
/// This ordering prevents:
/// - Lost wakeups (lock held during send ensures data isn't drained before signal)
/// - Deadlocks (no circular wait: main thread waits for dirty_paths, never reverse)
///
/// # Invariants
///
/// - `dirty_paths` contains sorted, deduplicated paths (BTreeSet guarantees ordering)
/// - `wakeup_tx` is a bounded channel (capacity 1) to prevent unbounded buffering
#[derive(Clone)]
struct PipelineSharedState {
    /// Dirty paths collected during scan/watch (sorted deterministically)
    dirty_paths: Arc<std::sync::Mutex<BTreeSet<PathBuf>>>,
    /// Wakeup channel (bounded, capacity 1)
    wakeup_tx: std::sync::mpsc::SyncSender<()>,
}

impl PipelineSharedState {
    /// Create a new shared state.
    fn new() -> (Self, std::sync::mpsc::Receiver<()>) {
        let (wakeup_tx, wakeup_rx) = std::sync::mpsc::sync_channel(1);
        (
            Self {
                dirty_paths: Arc::new(std::sync::Mutex::new(BTreeSet::new())),
                wakeup_tx,
            },
            wakeup_rx,
        )
    }

    /// Insert multiple dirty paths from a batch.
    ///
    /// # Lock Ordering
    ///
    /// Follows global ordering: dirty_paths -> wakeup send.
    /// Lock is held during send to prevent lost wakeup race condition.
    ///
    /// # Why Lock Held During Send
    ///
    /// If lock is released before send:
    /// 1. Thread A inserts paths, releases lock
    /// 2. Thread B drains paths (finds data, processes it)
    /// 3. Thread A sends wakeup signal
    /// 4. Main thread wakes up, drains paths (finds empty—LOST DATA)
    ///
    /// By holding lock during send, we ensure data isn't drained before wakeup.
    fn insert_dirty_paths(&self, paths: &[PathBuf]) -> Result<()> {
        let mut dirty_paths = self
            .dirty_paths
            .lock()
            .map_err(|e| anyhow::anyhow!("dirty_paths mutex poisoned: {}", e))?;
        for path in paths {
            dirty_paths.insert(path.clone());
        }
        // Try to send wakeup tick, but don't block if channel is full
        let _ = self.wakeup_tx.try_send(());
        Ok(())
    }

    /// Snapshot and clear the dirty path set.
    ///
    /// # Lock Ordering
    ///
    /// Only acquires dirty_paths lock (no wakeup send).
    /// Safe to call from any context following global ordering.
    ///
    /// Returns all dirty paths in lexicographic order and clears the set.
    fn drain_dirty_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = self
            .dirty_paths
            .lock()
            .map_err(|e| anyhow::anyhow!("dirty_paths mutex poisoned: {}", e))?;
        let snapshot: Vec<PathBuf> = paths.iter().cloned().collect();
        paths.clear();
        Ok(snapshot)
    }
}

/// Run the deterministic watch pipeline with buffering.
///
/// # Phase 2 Pipeline Behavior
///
/// 1. **Start watcher immediately** - Filesystem events start buffering right away
/// 2. **Baseline scan (if enabled)** - scan_directory provides complete baseline
/// 3. **Drain buffered edits** - Any edits during scan are flushed after baseline
/// 4. **Main watch loop** - Process dirty paths in sorted order as batches arrive
///
/// # Concurrency Model
/// - One watcher thread (notify/debouncer callback) produces batches
/// - One main/indexer thread performs scan and processes dirty paths
/// - BTreeSet ensures deterministic ordering regardless of event arrival
///
/// # Buffering Model
/// - BTreeSet<PathBuf> for dirty path collection (sorted, deduplicated)
/// - Bounded sync_channel(1) for wakeup ticks (non-blocking insertion)
/// - Snapshot+clear drain semantics for deterministic processing
///
/// # Arguments
/// * `config` - Pipeline configuration
/// * `shutdown` - AtomicBool for graceful shutdown
///
/// # Returns
/// Number of paths processed during watch phase
pub fn run_watch_pipeline(config: WatchPipelineConfig, shutdown: Arc<AtomicBool>) -> Result<usize> {
    // Open graph first so we can get the backend for pub/sub subscription
    let mut graph = CodeGraph::open(&config.db_path)?;

    // Create shared state for buffering dirty paths
    let (shared_state, wakeup_rx) = PipelineSharedState::new();

    // Keep a reference for the main thread to drain dirty paths
    let main_state = shared_state.clone();

    // Start watcher thread
    let watcher_thread = {
        let root_path = config.root_path.clone();
        let watcher_config = config.watcher_config.clone();
        let shared_state = Arc::new(shared_state);
        let shutdown_watch = shutdown.clone();

        thread::spawn(move || {
            let result = watcher_loop(root_path, watcher_config, shared_state, shutdown_watch);

            // Clean up parsers before thread exit to prevent tcache_thread_shutdown crash
            crate::ingest::pool::cleanup_parsers();

            if let Err(e) = result {
                eprintln!("Watcher thread error: {:?}", e);
            }
        })
    };

    // Baseline scan if requested
    if config.scan_initial {
        use indicatif::HumanCount;

        graph.scan_directory(
            &config.root_path,
            Some(&|current, total, file_path| {
                // Progress bar is created on first call
                static PB: std::sync::OnceLock<ProgressBar> = std::sync::OnceLock::new();
                let pb = PB.get_or_init(|| {
                    let pb = ProgressBar::new(total as u64);
                    pb.set_style(
                        ProgressStyle::default_bar()
                            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) ETA: {eta}\n{msg}")
                            .unwrap()
                            .progress_chars("=>-"),
                    );
                    pb
                });
                pb.set_position(current as u64);
                pb.set_message(format!("Scanning: {}", file_path));
                if current >= total {
                    pb.finish_with_message(format!("Scanned {} files", HumanCount(total as u64)));
                }
            }),
        )?;
    }

    // Drain any dirty paths that accumulated during scan
    let mut total_processed = 0;
    let paths_during_scan = main_state.drain_dirty_paths()?;
    if !paths_during_scan.is_empty() {
        println!(
            "Flushing {} buffered path(s) from scan...",
            paths_during_scan.len()
        );
        total_processed += process_dirty_paths(&mut graph, &paths_during_scan)?;
    }

    // Main watch loop
    println!("Magellan watching: {}", config.root_path.display());
    println!("Database: {}", config.db_path.display());

    // Main watch loop
    while !shutdown.load(Ordering::SeqCst) {
        // Wait for wakeup tick with timeout
        match wakeup_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(()) => {
                let dirty_paths = main_state.drain_dirty_paths()?;
                if !dirty_paths.is_empty() {
                    total_processed += process_dirty_paths(&mut graph, &dirty_paths)?;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    // Wait for watcher thread to finish with extended timeout
    // Signal handler gives us 30 seconds, so we should have time to clean up
    let timeout = Duration::from_secs(25);
    let start = Instant::now();
    let mut finished = false;

    while !watcher_thread.is_finished() {
        if start.elapsed() >= timeout {
            eprintln!(
                "Warning: Watcher thread did not finish within {:?}, forcing shutdown",
                timeout
            );
            eprintln!(
                "Note: Data may not be flushed. Use Ctrl+C (not timeout) for clean shutdown."
            );
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Only call join() if the thread finished (avoid hang on timeout)
    if watcher_thread.is_finished() {
        finished = true;
    }

    if finished {
        match watcher_thread.join() {
            Ok(_) => {
                // Clean exit
            }
            Err(panic_payload) => {
                // Extract and log panic information
                if let Some(msg) = panic_payload.downcast_ref::<&str>() {
                    eprintln!("Watcher thread panicked: {}", msg);
                } else if let Some(msg) = panic_payload.downcast_ref::<String>() {
                    eprintln!("Watcher thread panicked: {}", msg);
                } else {
                    eprintln!("Watcher thread panicked with unknown payload");
                }
            }
        }
    }

    // Clean up main thread parsers before returning to prevent tcache_thread_shutdown crash
    crate::ingest::pool::cleanup_parsers();

    Ok(total_processed)
}

/// Watcher loop that receives batches and inserts paths into shared state.
fn watcher_loop(
    root_path: PathBuf,
    config: WatcherConfig,
    shared_state: Arc<PipelineSharedState>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    // Use filesystem-only watcher
    let watcher = FileSystemWatcher::new(root_path, config, shutdown.clone())?;

    // Receive batches and insert dirty paths
    // Use timeout-based checking to respond to shutdown signal
    while !shutdown.load(Ordering::SeqCst) {
        match watcher.recv_batch_timeout(Duration::from_millis(100)) {
            Ok(Some(batch)) => {
                shared_state.insert_dirty_paths(&batch.paths)?;
            }
            Ok(None) => {
                // Channel closed, exit
                break;
            }
            Err(_) => {
                // Timeout, check shutdown flag and continue
                continue;
            }
        }
    }

    Ok(())
}

/// Process a list of dirty paths, reconciling each in sorted order.
///
/// Paths are already sorted because they came from a BTreeSet.
fn process_dirty_paths(graph: &mut CodeGraph, dirty_paths: &[PathBuf]) -> Result<usize> {
    // Use L3 cache-aware batching for better performance
    process_dirty_paths_batched(graph, dirty_paths)
}

/// Process dirty paths using L3 cache-aware batching.
///
/// This version is optimized to keep data in L3 cache during processing:
/// 1. Get file sizes for batch calculation
/// 2. Group files into L3 cache-sized batches
/// 3. For each batch: pre-read all sources (warms OS cache), then process
fn process_dirty_paths_batched(graph: &mut CodeGraph, dirty_paths: &[PathBuf]) -> Result<usize> {
    if dirty_paths.is_empty() {
        return Ok(0);
    }

    let batch_start = Instant::now();
    let _total_files = dirty_paths.len(); // Only for debugging/logging if needed

    // Step 1: Get file sizes for batch calculation (only for existing files)
    let size_start = Instant::now();
    let paths_with_sizes: Vec<(PathBuf, usize)> = dirty_paths
        .iter()
        .filter_map(|path| {
            std::fs::metadata(path)
                .ok()
                .map(|meta| (path.clone(), meta.len() as usize))
        })
        .collect();
    let size_time = size_start.elapsed();

    // Step 2: Calculate target cache size (50% of L3)
    let target_cache_bytes = (DEFAULT_L3_CACHE_SIZE as f64 * TARGET_CACHE_USAGE) as usize;

    // Step 3: Group into L3 cache-sized batches
    let batch_start_compute = Instant::now();
    let batches = compute_l3_cache_batches(&paths_with_sizes, target_cache_bytes);
    let batch_count = batches.len();
    let batch_compute_time = batch_start_compute.elapsed();

    let mut total_processed = 0;
    let mut total_read_time = std::time::Duration::ZERO;
    let mut total_reconcile_time = std::time::Duration::ZERO;

    // Step 4: Process each batch
    for batch in batches {
        let batch_paths: Vec<PathBuf> = batch.iter().map(|(p, _)| p.clone()).collect();

        // Pre-read all sources in batch to warm OS cache (data stays in L3)
        let read_start = Instant::now();
        let sources = read_batch_sources(&batch_paths);
        total_read_time += read_start.elapsed();

        // Build a lookup from path to pre-read source bytes
        let source_map: std::collections::HashMap<PathBuf, Vec<u8>> = sources
            .into_iter()
            .map(|(path, source, _)| (path, source))
            .collect();

        // Now process each file - use pre-read source when available
        for path in &batch_paths {
            let path_key = crate::validation::normalize_path(path)
                .unwrap_or_else(|_| path.to_string_lossy().to_string());

            let reconcile_start = Instant::now();
            let outcome = if let Some(source) = source_map.get(path) {
                graph.reconcile_file_path_with_source(path, &path_key, source)
            } else {
                graph.reconcile_file_path(path, &path_key)
            };

            match outcome {
                Ok(outcome) => {
                    total_reconcile_time += reconcile_start.elapsed();
                    let path_str = path.to_string_lossy();
                    let was_modified = match outcome {
                        crate::ReconcileOutcome::Deleted => {
                            println!("DELETE {}", path_str);
                            true
                        }
                        crate::ReconcileOutcome::Unchanged => {
                            // Skip logging for unchanged files
                            false
                        }
                        crate::ReconcileOutcome::Reindexed {
                            symbols,
                            references,
                            calls,
                        } => {
                            println!(
                                "MODIFY {} symbols={} refs={} calls={}",
                                path_str, symbols, references, calls
                            );
                            true
                        }
                    };

                    // Only count as processed if actually changed
                    if was_modified {
                        total_processed += 1;
                    }
                }
                Err(e) => {
                    total_reconcile_time += reconcile_start.elapsed();
                    let path_str = path.to_string_lossy();
                    println!("ERROR {} {}", path_str, e);
                }
            }
        }
    }

    // Also handle deleted files (paths in dirty_paths but not on filesystem)
    for path in dirty_paths {
        if !path.exists() {
            let path_key = crate::validation::normalize_path(path)
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            let _ = graph.delete_file_facts(&path_key);
            println!("DELETE {}", path.to_string_lossy());
            total_processed += 1;
        }
    }

    let elapsed = batch_start.elapsed();
    // Only print batch stats when actual work was done (not just periodic checks)
    if total_processed > 0 {
        eprintln!(
            "L3 Batch: {} files processed, {} batches, {}ms total (size:{}ms batch:{}ms read:{}ms reconcile:{}ms)",
            total_processed,
            batch_count,
            elapsed.as_millis(),
            size_time.as_millis(),
            batch_compute_time.as_millis(),
            total_read_time.as_millis(),
            total_reconcile_time.as_millis()
        );
        
        // Rebuild FTS5 index after batch processing to keep search index synchronized
        // This is efficient when called per-batch (~500ms per 1,000 files)
        if let Err(e) = crate::graph::CodeGraph::rebuild_fts5_index(graph.db_path()) {
            eprintln!("Warning: FTS5 rebuild failed: {}", e);
        }
    }

    Ok(total_processed)
}

/// Run the watch pipeline for geometric backend databases
#[cfg(feature = "geometric-backend")]
pub fn run_watch_pipeline_geometric(
    config: crate::WatchPipelineConfig,
    shutdown: Arc<AtomicBool>,
) -> Result<usize> {
    use crate::graph::geo_index;
    use crate::graph::geometric_backend::GeometricBackend;
    use indicatif::{HumanCount, ProgressBar, ProgressStyle};

    // Open or create geometric backend (mutable for initial scan)
    // For new databases, this creates the file with empty sections
    let mut backend = if config.db_path.exists() {
        GeometricBackend::open(&config.db_path)?
    } else {
        GeometricBackend::create(&config.db_path)?
    };

    // Create shared state for buffering dirty paths
    let (shared_state, wakeup_rx) = PipelineSharedState::new();
    let main_state = shared_state.clone();

    // Start watcher thread
    let _watcher_thread = {
        let root_path = config.root_path.clone();
        let watcher_config = config.watcher_config.clone();
        let shared_state = Arc::new(shared_state);
        let shutdown_watch = shutdown.clone();

        thread::spawn(move || {
            let _ = watcher_loop(root_path, watcher_config, shared_state, shutdown_watch);
            crate::ingest::pool::cleanup_parsers();
        })
    };

    // Baseline scan if requested
    let mut total_processed = 0;
    if config.scan_initial {
        println!("Starting initial scan of: {}", config.root_path.display());
        use indicatif::HumanCount;

        match geo_index::scan_directory_with_progress(
            &mut backend,
            &config.root_path,
            Some(&|current, total| {
                // Progress bar is created on first call
                static PB: std::sync::OnceLock<ProgressBar> = std::sync::OnceLock::new();
                let pb = PB.get_or_init(|| {
                    let pb = ProgressBar::new(total as u64);
                    pb.set_style(
                        ProgressStyle::default_bar()
                            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) ETA: {eta}\n{msg}")
                            .unwrap()
                            .progress_chars("=>-"),
                    );
                    pb
                });
                pb.set_position(current as u64);
                pb.set_message(format!(
                    "Indexing: {}/{} files",
                    HumanCount(current as u64),
                    HumanCount(total as u64)
                ));
                if current >= total {
                    pb.finish_with_message(format!("Scanned {} files", HumanCount(total as u64)));
                }
            }),
            geo_index::IndexingMode::CfgFirst,
        ) {
            Ok(count) => {
                println!("Initial scan complete: indexed {} files", count);
                total_processed += count;

                // CRITICAL FIX: Save data immediately after initial scan
                // Don't wait for watch loop to exit (which might not happen on SIGINT)
                debug_print!("[WATCH_DEBUG] Saving data after initial scan...");
                match backend.save_to_disk() {
                    Ok(_) => debug_print!("[WATCH_DEBUG] Data saved successfully"),
                    Err(e) => debug_print!("[WATCH_DEBUG] ERROR saving data: {}", e),
                }
                debug_print!("[WATCH_DEBUG] Save operation completed.");
            }
            Err(e) => {
                eprintln!("Initial scan error: {}", e);
            }
        }
    }

    // Main watch loop
    println!(
        "Magellan watching (Geometric): {}",
        config.root_path.display()
    );
    println!("Database: {}", config.db_path.display());

    // Counter for periodic saves (every 100 files in watch mode)
    let mut save_counter = 0;

    while !shutdown.load(Ordering::SeqCst) {
        match wakeup_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(()) => {
                let dirty_paths = main_state.drain_dirty_paths()?;
                if !dirty_paths.is_empty() {
                    let batch_count = process_dirty_paths_geometric(&backend, &dirty_paths)?;
                    total_processed += batch_count;
                    save_counter += batch_count;

                    // Periodic save every 100 files to prevent OOM
                    if save_counter >= 100 {
                        backend.save_to_disk()?;
                        save_counter = 0;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Timeout - check shutdown flag and continue
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    // Explicitly save before exiting
    println!("Flushing data to disk...");
    backend.save_to_disk()?;
    println!("Data flushed.");

    Ok(total_processed)
}

/// Process changed files for geometric backend
#[cfg(feature = "geometric-backend")]
fn process_dirty_paths_geometric(
    backend: &crate::graph::geometric_backend::GeometricBackend,
    dirty_paths: &[PathBuf],
) -> Result<usize> {
    use crate::graph::geometric_backend::extract_symbols_cfg_and_calls_from_file;
    use crate::ingest::detect_language;
    use std::fs;

    let mut count = 0;
    for path in dirty_paths {
        if !path.exists() {
            continue;
        }

        let language = match detect_language(path) {
            Some(l) => l,
            None => continue,
        };

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        match extract_symbols_cfg_and_calls_from_file(path, &content, language) {
            Ok((symbols, cfg_blocks, cfg_edges, _call_edges)) => {
                let sym_count: usize = symbols.len();
                let symbol_ids = backend.insert_symbols(symbols)?;

                let cfg_blocks_nonempty: bool = !cfg_blocks.is_empty();
                if cfg_blocks_nonempty {
                    // Track block indices to their logical IDs for edge insertion
                    // Note: CfgBlock.id is the logical ID stored in NodeRec, not the storage ID
                    let mut block_id_map: std::collections::HashMap<usize, u64> =
                        std::collections::HashMap::new();

                    for (idx, mut block) in cfg_blocks.into_iter().enumerate() {
                        let local_sym_idx = block.function_id as usize;
                        if local_sym_idx < symbol_ids.len() {
                            block.function_id = symbol_ids[local_sym_idx] as i64;
                        }
                        // Track the logical block ID (block.id) which is what edges need to reference
                        let logical_id = block.id;
                        block_id_map.insert(idx, logical_id);
                        let _ = backend.insert_cfg_block(block);
                    }

                    // Insert edges - CfgEdge from geographdb_core has src_id/dst_id as block IDs
                    for edge in cfg_edges {
                        // edge.src_id and edge.dst_id are already block IDs
                        // edge_type is u32: 0=normal, 1=branch_true, 2=branch_false
                        let edge_type_str = match edge.edge_type {
                            1 => "branch_true",
                            2 => "branch_false",
                            _ => "normal",
                        };
                        let _ = backend.insert_edge(edge.src_id, edge.dst_id, edge_type_str);
                    }
                }

                println!("MODIFY {} symbols={}", path.display(), sym_count);
                count += 1;
            }
            Err(e) => println!("ERROR {} {}", path.display(), e),
        }
    }
    Ok(count)
}

#[cfg(not(feature = "geometric-backend"))]
pub fn run_watch_pipeline_geometric(
    _config: crate::WatchPipelineConfig,
    _shutdown: Arc<AtomicBool>,
) -> Result<usize> {
    Err(anyhow::anyhow!("Geometric backend not enabled"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compute_l3_cache_batches_empty() {
        let paths: Vec<(PathBuf, usize)> = Vec::new();
        let batches = compute_l3_cache_batches(&paths, 1024);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_compute_l3_cache_batches_single_file() {
        let paths = vec![(PathBuf::from("test.rs"), 100)];
        let batches = compute_l3_cache_batches(&paths, 1024);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].0, PathBuf::from("test.rs"));
        assert_eq!(batches[0][0].1, 100);
    }

    #[test]
    fn test_compute_l3_cache_batches_fits_in_one_batch() {
        let paths = vec![
            (PathBuf::from("a.rs"), 100),
            (PathBuf::from("b.rs"), 200),
            (PathBuf::from("c.rs"), 300),
        ];
        let batches = compute_l3_cache_batches(&paths, 1000);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
    }

    #[test]
    fn test_compute_l3_cache_batches_splits_on_limit() {
        let paths = vec![
            (PathBuf::from("a.rs"), 500),
            (PathBuf::from("b.rs"), 600), // Total: 1100 > 1000, new batch
            (PathBuf::from("c.rs"), 300),
        ];
        let batches = compute_l3_cache_batches(&paths, 1000);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 1); // a.rs only
        assert_eq!(batches[1].len(), 2); // b.rs, c.rs
    }

    #[test]
    fn test_compute_l3_cache_batches_large_file() {
        // Large file that exceeds target gets its own batch,
        // and subsequent files that don't fit with it also get new batches
        let paths = vec![
            (PathBuf::from("small.rs"), 100),
            (PathBuf::from("huge.rs"), 2000), // Exceeds target, starts batch 1
            (PathBuf::from("tiny.rs"), 50),   // Doesn't fit with huge (2050 > 1000), starts batch 2
        ];
        let batches = compute_l3_cache_batches(&paths, 1000);
        // Each file ends up in its own batch because:
        // - small.rs: batch 0 (100 bytes)
        // - huge.rs: doesn't fit with small (100+2000 > 1000), batch 1 (2000 bytes)
        // - tiny.rs: doesn't fit with huge (2000+50 > 1000), batch 2 (50 bytes)
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 1); // small.rs
        assert_eq!(batches[1].len(), 1); // huge.rs
        assert_eq!(batches[2].len(), 1); // tiny.rs
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
        assert_eq!(AVG_SOURCE_FILE_SIZE, 50 * 1024); // 50KB
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
}

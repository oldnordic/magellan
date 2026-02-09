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

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::{CodeGraph, FileEvent, FileSystemWatcher, WatcherConfig};

#[cfg(feature = "native-v2")]
use std::sync::mpsc::channel as mpsc_channel;

#[cfg(feature = "native-v2")]
use sqlitegraph::GraphBackend;

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

    // Process up to max_events events.
    //
    // IMPORTANT: In tests, notify may coalesce events or fail to emit the expected
    // number of events on some platforms/filesystems. We therefore include an idle
    // timeout to prevent hangs while preserving the "process up to N events" contract.
    let mut processed = 0;
    let mut idle_for = std::time::Duration::from_secs(0);
    let idle_step = std::time::Duration::from_millis(10);
    let idle_timeout = std::time::Duration::from_secs(2);

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
        let mut dirty_paths = self.dirty_paths.lock()
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
        let mut paths = self.dirty_paths.lock()
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
/// # Pub/Sub Integration (native-v2 feature)
///
/// When the native-v2 feature is enabled, the watcher subscribes to graph mutation
/// events for reactive cache invalidation. The backend is passed to the watcher
/// thread which creates a PubSubEventReceiver.
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

    // Create channel for pub/sub cache invalidation (only used with native-v2)
    // The sender will be cloned and passed to the watcher thread for pub/sub
    // The receiver is used in the main loop to receive file paths from pub/sub events
    #[cfg(feature = "native-v2")]
    let (pubsub_cache_tx, pubsub_cache_rx) = mpsc_channel();

    // Start watcher thread
    let watcher_thread = {
        let root_path = config.root_path.clone();
        let watcher_config = config.watcher_config.clone();
        let shared_state = Arc::new(shared_state);
        let shutdown_watch = shutdown.clone();
        let _db_path = config.db_path.clone();

        #[cfg(feature = "native-v2")]
        let pubsub_sender = pubsub_cache_tx.clone();

        thread::spawn(move || {
            #[cfg(feature = "native-v2")]
            let result = watcher_loop_with_native_backend(
                root_path,
                watcher_config,
                shared_state,
                shutdown_watch,
                db_path,
                Some(pubsub_sender),
            );

            #[cfg(not(feature = "native-v2"))]
            let result = watcher_loop(
                root_path,
                watcher_config,
                shared_state,
                shutdown_watch,
            );

            // Clean up parsers before thread exit to prevent tcache_thread_shutdown crash
            crate::ingest::pool::cleanup_parsers();

            if let Err(e) = result {
                eprintln!("Watcher thread error: {:?}", e);
            }
        })
    };

    // Baseline scan if requested
    if config.scan_initial {
        println!("Scanning {}...", config.root_path.display());
        let file_count = graph.scan_directory(
            &config.root_path,
            Some(&|current, total| {
                println!("Scanning... {}/{}", current, total);
            }),
        )?;
        println!("Scanned {} files", file_count);
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

    // Native-V2: Poll both pubsub_cache_rx (for backend mutations) and wakeup_rx (for filesystem events)
    #[cfg(feature = "native-v2")]
    while !shutdown.load(Ordering::SeqCst) {
        // Priority 1: Check for pub/sub events (non-blocking)
        match pubsub_cache_rx.try_recv() {
            Ok(path) => {
                // Pub/sub event received - insert as dirty path
                main_state.insert_dirty_paths(&[PathBuf::from(path)])?;
                // Continue to next iteration to check for more pub/sub events
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No pub/sub events - proceed to wait for wakeup tick
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Pub/sub receiver dropped - break out of loop
                break;
            }
        }

        // Priority 2: Wait for wakeup tick with timeout
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

    // Non-native-v2: Original loop that only waits for filesystem events
    #[cfg(not(feature = "native-v2"))]
    while !shutdown.load(Ordering::SeqCst) {
        // Wait for wakeup tick with timeout
        match wakeup_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(()) => {
                // Drain and process all dirty paths
                let dirty_paths = main_state.drain_dirty_paths()?;
                if !dirty_paths.is_empty() {
                    total_processed += process_dirty_paths(&mut graph, &dirty_paths)?;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Timeout is normal - check shutdown flag and continue
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Watcher thread terminated
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
            eprintln!("Note: Data may not be flushed. Use Ctrl+C (not timeout) for clean shutdown.");
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
///
/// When native-v2 is enabled and a backend is provided, this uses the pub/sub-enabled
/// watcher for reactive cache invalidation. Otherwise, uses the filesystem-only watcher.
fn watcher_loop(
    root_path: PathBuf,
    config: WatcherConfig,
    shared_state: Arc<PipelineSharedState>,
    shutdown: Arc<AtomicBool>,
    #[cfg(feature = "native-v2")] pubsub_args: Option<(
        Arc<dyn GraphBackend + Send + Sync>,
        mpsc::Sender<String>,
    )>,
) -> Result<()> {
    #[cfg(not(feature = "native-v2"))]
    let watcher = FileSystemWatcher::new(root_path, config, shutdown.clone())?;

    #[cfg(feature = "native-v2")]
    let watcher = match pubsub_args {
        Some((backend, cache_sender)) => {
            // Use pub/sub-enabled watcher for reactive cache invalidation
            FileSystemWatcher::with_pubsub(root_path, config, shutdown.clone(), backend, cache_sender)?
        }
        None => {
            // Use filesystem-only watcher
            FileSystemWatcher::new(root_path, config, shutdown.clone())?
        }
    };

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

/// Watcher loop wrapper for native-v2 feature.
///
/// This function opens its own backend connection for pub/sub subscription,
/// then uses the pub/sub-enabled watcher for reactive cache invalidation.
#[cfg(feature = "native-v2")]
fn watcher_loop_with_native_backend(
    root_path: PathBuf,
    config: WatcherConfig,
    shared_state: Arc<PipelineSharedState>,
    shutdown: Arc<AtomicBool>,
    db_path: PathBuf,
    cache_sender: Option<mpsc::Sender<String>>,
) -> Result<()> {
    use sqlitegraph::NativeGraphBackend;

    // Try to open a Native backend for pub/sub subscription
    let pubsub_args = match cache_sender {
        Some(sender) => {
            // Open a new backend connection for pub/sub
            // Note: NativeGraphBackend::open will use the same database file
            match NativeGraphBackend::open(&db_path) {
                Ok(backend) => {
                    // Wrap in Arc for thread-safe sharing
                    let backend_arc: Arc<dyn GraphBackend + Send + Sync> = Arc::new(backend);
                    Some((backend_arc, sender))
                }
                Err(e) => {
                    eprintln!("Warning: Failed to open native backend for pub/sub: {:?}. Using filesystem-only watching.", e);
                    None
                }
            }
        }
        None => None,
    };

    watcher_loop(root_path, config, shared_state, shutdown, pubsub_args)
}

/// Process a list of dirty paths, reconciling each in sorted order.
///
/// Paths are already sorted because they came from a BTreeSet.
fn process_dirty_paths(graph: &mut CodeGraph, dirty_paths: &[PathBuf]) -> Result<usize> {
    for path in dirty_paths {
        let path_key = crate::validation::normalize_path(path)
            .unwrap_or_else(|_| path.to_string_lossy().to_string());
        match graph.reconcile_file_path(path, &path_key) {
            Ok(outcome) => {
                // Log outcome
                let path_str = path.to_string_lossy();
                match outcome {
                    crate::ReconcileOutcome::Deleted => {
                        println!("DELETE {}", path_str);
                    }
                    crate::ReconcileOutcome::Unchanged => {
                        // Skip logging for unchanged files
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
                    }
                }
            }
            Err(e) => {
                // Log error but continue processing other paths
                let path_str = path.to_string_lossy();
                println!("ERROR {} {}", path_str, e);
            }
        }
    }
    Ok(dirty_paths.len())
}

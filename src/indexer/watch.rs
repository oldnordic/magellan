//! Watch pipeline for SQLite backend.

use crate::indexer::{
    compute_l3_cache_batch_indices, read_batch_sources, DEFAULT_L3_CACHE_SIZE, TARGET_CACHE_USAGE,
};
use crate::manifest::detect_include_paths_from_root;
use crate::project_config::ProjectConfig;
use crate::{CodeGraph, FileEvent, FileSystemWatcher, WatcherConfig};
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

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
    /// Include glob patterns (from .magellan.toml or CLI). Empty = include all.
    pub include_patterns: Vec<String>,
    /// Exclude glob patterns
    pub exclude_patterns: Vec<String>,
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
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
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
    dirty_paths: Arc<parking_lot::Mutex<BTreeSet<PathBuf>>>,
    /// Wakeup channel (bounded, capacity 1)
    wakeup_tx: std::sync::mpsc::SyncSender<()>,
}

impl PipelineSharedState {
    /// Create a new shared state.
    fn new() -> (Self, std::sync::mpsc::Receiver<()>) {
        let (wakeup_tx, wakeup_rx) = std::sync::mpsc::sync_channel(1);
        (
            Self {
                dirty_paths: Arc::new(parking_lot::Mutex::new(BTreeSet::new())),
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
        let mut dirty_paths = self.dirty_paths.lock();
        dirty_paths.extend(paths.iter().cloned());
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
        let mut paths = self.dirty_paths.lock();
        let snapshot: Vec<PathBuf> = std::mem::take(&mut *paths).into_iter().collect();
        Ok(snapshot)
    }
}

/// Merge include/exclude patterns from .magellan.toml, Cargo.toml targets, and CLI overrides.
///
/// # Priority Order
/// 1. CLI `--include` / `--exclude` (highest priority, override everything)
/// 2. `.magellan.toml` index section (if CLI is empty)
/// 3. `Cargo.toml` target dirs (auto-inferred when no config set)
///
/// Auto-include is skipped when `--root` is already a subdirectory (e.g. `./src`)
/// because relative glob patterns like `"src/"` won't match.
fn merge_scan_config(
    scan_root: &std::path::Path,
    config: &WatchPipelineConfig,
) -> Result<crate::project_config::ProjectConfig> {
    let project_config = ProjectConfig::load(scan_root).context("Failed to load .magellan.toml")?;

    let auto_detected = if project_config.index.include.is_empty() {
        detect_include_paths_from_root(scan_root)
    } else {
        Vec::new()
    };

    let root_is_subdir = scan_root
        .file_name()
        .is_some_and(|n| matches!(n.to_str(), Some("src") | Some("tests") | Some("benches")));

    let merged_include = if config.include_patterns.is_empty() {
        if root_is_subdir {
            vec![]
        } else if project_config.index.include.is_empty() {
            auto_detected
        } else {
            project_config.index.include.clone()
        }
    } else {
        config.include_patterns.clone()
    };

    Ok(crate::project_config::ProjectConfig {
        index: crate::project_config::IndexSection {
            include: merged_include,
            exclude: if config.exclude_patterns.is_empty() {
                project_config.index.exclude.clone()
            } else {
                config.exclude_patterns.clone()
            },
        },
        ..project_config
    })
}

/// Wait for the watcher thread to finish with a timeout.
///
/// Returns early if the thread times out without joining (avoids hang).
/// On clean exit, joins the thread and logs any panic payload.
fn wait_for_watcher_thread(thread: std::thread::JoinHandle<()>, timeout: Duration) {
    let start = Instant::now();
    while !thread.is_finished() {
        if start.elapsed() >= timeout {
            eprintln!(
                "Warning: Watcher thread did not finish within {:?}, forcing shutdown",
                timeout
            );
            eprintln!(
                "Note: Data may not be flushed. Use Ctrl+C (not timeout) for clean shutdown."
            );
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    // Thread finished naturally — join and log any panic payload
    match thread.join() {
        Ok(()) => {}
        Err(payload) => {
            if let Some(msg) = payload.downcast_ref::<&str>() {
                eprintln!("Watcher thread panicked: {}", msg);
            } else if let Some(msg) = payload.downcast_ref::<String>() {
                eprintln!("Watcher thread panicked: {}", msg);
            } else {
                eprintln!("Watcher thread panicked with unknown payload");
            }
        }
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
/// - One watcher thread (notify callback with custom debounce) produces batches
/// - One main/indexer thread performs scan and processes dirty paths
/// - BTreeSet ensures deterministic ordering regardless of event arrival
///
/// # Buffering Model
/// - `BTreeSet<PathBuf>` for dirty path collection (sorted, deduplicated)
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
    // Canonicalize root so walkdir and FileFilter both use absolute paths.
    // Without this, walkdir yields relative paths but FileFilter canonicalizes
    // its root, causing strip_prefix to fail and include globs to mismatch.
    let scan_root =
        std::fs::canonicalize(&config.root_path).unwrap_or_else(|_| config.root_path.clone());

    // Merge include/exclude patterns from .magellan.toml, Cargo.toml targets, and CLI overrides.
    let merged_config = merge_scan_config(&scan_root, &config)?;

    // Open graph
    let mut graph = CodeGraph::open(&config.db_path)?;

    // Parse Cargo.toml and store manifest metadata in magellan_meta
    if let Ok(manifest) = crate::manifest::CargoManifest::parse(&scan_root) {
        if manifest.package_name.is_some() {
            if let Ok(conn) = rusqlite::Connection::open(&config.db_path) {
                if let Err(e) = manifest.store_in_db(&conn) {
                    eprintln!("Failed to store Cargo manifest metadata: {}", e);
                }
            }
        }
    }

    // Disable batch mode for watch to avoid BEGIN IMMEDIATE deadlock
    // on the single pooled connection during rapid flush cycles.
    // This must happen BEFORE any file processing (scan + dirty path flush).
    graph.batch_mode = false;

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
            crate::ingest::pool::cleanup_parsers();
            if let Err(e) = result {
                eprintln!("Watcher thread error: {:?}", e);
            }
        })
    };

    // Baseline scan if requested
    if config.scan_initial {
        use indicatif::HumanCount;

        let file_filter = merged_config.to_file_filter(&scan_root)?;

        graph.scan_directory_with_filter(
            &scan_root,
            &file_filter,
            Some(&|current, total, file_path| {
                // Progress bar is created on first call
                static PB: std::sync::OnceLock<ProgressBar> = std::sync::OnceLock::new();
                let pb = PB.get_or_init(|| {
                    let pb = ProgressBar::new(total as u64);
                    pb.set_style(
                        ProgressStyle::default_bar()
                            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) ETA: {eta}\n{msg}")
                            .expect("invariant: hardcoded ProgressStyle template string is valid")
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

        // Rebuild FTS5 index after bulk scan — direct inserts into graph_entities
        // don't fire FTS triggers, leaving the index empty.
        if let Err(e) = graph.rebuild_fts5() {
            eprintln!("Warning: FTS5 rebuild after scan failed: {}", e);
        }
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
        if let Err(e) = graph.checkpoint_wal() {
            eprintln!("Warning: WAL checkpoint failed after scan flush: {}", e);
        }
        // Verify database integrity after initial scan + flush
        if let Err(e) = verify_db_integrity(&config.db_path) {
            eprintln!(
                "Warning: Database integrity check failed after scan flush: {}",
                e
            );
        }
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
                    if let Err(e) = graph.checkpoint_wal() {
                        eprintln!("Warning: WAL checkpoint failed after watch batch: {}", e);
                    }
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
    wait_for_watcher_thread(watcher_thread, Duration::from_secs(25));

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

    // Step 1: Get file sizes for batch calculation (only for existing files)
    let size_start = Instant::now();
    let sizes: Vec<usize> = dirty_paths
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok().map(|meta| meta.len() as usize))
        .collect();
    let size_time = size_start.elapsed();

    // Step 2: Calculate target cache size (50% of L3)
    let target_cache_bytes = (DEFAULT_L3_CACHE_SIZE as f64 * TARGET_CACHE_USAGE) as usize;

    // Step 3: Group into L3 cache-sized batches (index-based, zero path clones)
    let batch_start_compute = Instant::now();
    let batches = compute_l3_cache_batch_indices(&sizes, target_cache_bytes);
    let batch_count = batches.len();
    let batch_compute_time = batch_start_compute.elapsed();

    let mut total_processed = 0;
    let mut total_read_time = std::time::Duration::ZERO;
    let mut total_reconcile_time = std::time::Duration::ZERO;

    // Step 4: Process each batch
    for batch in &batches {
        // Index into dirty_paths directly — no intermediate path clones
        let batch_paths: Vec<&PathBuf> = batch.iter().map(|&idx| &dirty_paths[idx]).collect();

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
        for &path in &batch_paths {
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
            if let Err(e) = graph.delete_file_facts(&path_key) {
                eprintln!("Failed to delete file facts for {}: {}", path.display(), e);
            }
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
        // Uses the graph's side connection to avoid uncoordinated WAL access
        if let Err(e) = graph.rebuild_fts5() {
            eprintln!("Warning: FTS5 rebuild failed: {}", e);
        }
    }

    Ok(total_processed)
}

/// Verify SQLite database integrity.
///
/// Runs PRAGMA integrity_check and returns an error if any issues are found.
fn verify_db_integrity(db_path: &std::path::Path) -> Result<()> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open DB for integrity check: {}", e))?;
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| anyhow::anyhow!("Integrity check query failed: {}", e))?;
    if result != "ok" {
        return Err(anyhow::anyhow!(
            "Database integrity check failed: {}",
            result
        ));
    }
    Ok(())
}

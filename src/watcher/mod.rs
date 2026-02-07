//! Filesystem watcher with debounced batch events.
//!
//! Provides deterministic event coalescing: all events within a debounce window
//! are collected, de-duplicated, sorted lexicographically, and emitted as a single
//! batch. This ensures the same final DB state regardless of event arrival order.
//!
//! # Threading Design
//!
//! This watcher uses thread-safe synchronization for concurrent access.
//! The legacy pending state fields use `Arc<Mutex<T>>` to allow safe access
//! from multiple threads during concurrent operations and shutdown.
//!
//! **Thread safety:** `Arc<Mutex<T>>` provides runtime mutual exclusion
//! and is safe to share across threads. The mutex will panic if poisoned
//! (consistent with RefCell behavior).
//!
//! # Global Lock Ordering
//!
//! This module participates in the global lock ordering hierarchy:
//!
//! 1. **watcher state locks** (legacy_pending_batch, legacy_pending_index)—acquired first
//! 2. **indexer shared state locks** (dirty_paths)—acquired second
//! 3. **wakeup channel send** (highest priority)—acquired last
//!
//! **Rule:** Never send to wakeup channel while holding other locks.
//!
//! See `src/indexer.rs::PipelineSharedState` for full lock ordering documentation.
//!
//! See MANUAL.md for architecture details.

// Pub/Sub event receiver for Native V2 backend (feature-gated)
#[cfg(feature = "native-v2")]
pub mod pubsub_receiver;

#[cfg(feature = "native-v2")]
pub use pubsub_receiver::PubSubEventReceiver;

use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::graph::filter::FileFilter;

/// Deterministic batch of dirty file paths.
///
/// Contains ONLY paths (no timestamps, no event types) to ensure deterministic
/// behavior. Paths are sorted lexicographically before emission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatcherBatch {
    /// Dirty file paths to reconcile, in lexicographic order
    pub paths: Vec<PathBuf>,
}

impl WatcherBatch {
    /// Create a new batch from a set of paths, sorting them deterministically.
    fn from_set(paths: BTreeSet<PathBuf>) -> Self {
        Self {
            paths: paths.into_iter().collect(),
        }
    }

    /// Empty batch for when no dirty paths exist after filtering.
    pub fn empty() -> Self {
        Self { paths: Vec::new() }
    }

    /// Whether this batch contains any paths.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// Filesystem watcher configuration
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Root directory for path validation
    pub root_path: PathBuf,
    /// Debounce delay in milliseconds
    pub debounce_ms: u64,
    /// Enable .gitignore filtering (default: true)
    pub gitignore_aware: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("."),
            debounce_ms: 500,
            gitignore_aware: true,
        }
    }
}

/// Filesystem watcher that emits debounced batches of dirty paths.
///
/// Uses notify-debouncer-mini for event coalescing. All paths within the
/// debounce window are collected, de-duplicated, sorted, and emitted as a
/// single WatcherBatch.
///
/// With native-v2 feature, can also receive graph mutation events via pub/sub
/// for reactive cache invalidation.
pub struct FileSystemWatcher {
    /// Watcher thread handle (wrapped in ManuallyDrop for custom Drop/shutdown logic)
    _watcher_thread: ManuallyDrop<thread::JoinHandle<()>>,
    batch_receiver: Receiver<WatcherBatch>,
    /// Legacy compatibility: pending batch to emit one path at a time
    /// Thread-safe: wrapped in Arc<Mutex<T>> for concurrent access
    legacy_pending_batch: Arc<Mutex<Option<WatcherBatch>>>,
    /// Legacy compatibility: current index into pending batch
    /// Thread-safe: wrapped in Arc<Mutex<T>> for concurrent access
    legacy_pending_index: Arc<Mutex<usize>>,
    /// Pub/sub event receiver (feature-gated to native-v2)
    /// Uses Box for size erasure since PubSubEventReceiver contains JoinHandle
    #[cfg(feature = "native-v2")]
    _pubsub_receiver: Option<Box<PubSubEventReceiver>>,
    /// Channel for receiving file paths from pub/sub events
    /// The pub/sub receiver thread sends file paths here for cache invalidation
    #[cfg(feature = "native-v2")]
    pubsub_file_rx: Receiver<String>,
}

impl FileSystemWatcher {
    /// Create a new watcher for the given directory.
    ///
    /// # Arguments
    /// * `path` - Directory to watch recursively (also used as root_path for validation)
    /// * `config` - Watcher configuration
    /// * `shutdown` - AtomicBool for graceful shutdown
    ///
    /// # Returns
    /// A watcher that can be polled for batch events
    pub fn new(path: PathBuf, config: WatcherConfig, shutdown: Arc<AtomicBool>) -> Result<Self> {
        let (batch_tx, batch_rx) = mpsc::channel();

        // Ensure root_path is set to the watched directory for validation
        let config = WatcherConfig {
            root_path: path.clone(),
            ..config
        };

        let thread = thread::spawn(move || {
            if let Err(e) = run_watcher(path, batch_tx, config, shutdown) {
                eprintln!("Watcher error: {:?}", e);
            }
        });

        // Initialize fields differently based on feature flag
        // This creates a dummy channel that's never used without native-v2
        #[cfg(feature = "native-v2")]
        let (_pubsub_receiver, pubsub_file_rx) = {
            let (_, rx) = mpsc::channel();
            (None, rx)
        };

        Ok(Self {
            _watcher_thread: ManuallyDrop::new(thread),
            batch_receiver: batch_rx,
            legacy_pending_batch: Arc::new(Mutex::new(None)),
            legacy_pending_index: Arc::new(Mutex::new(0)),
            #[cfg(feature = "native-v2")]
            _pubsub_receiver,
            #[cfg(feature = "native-v2")]
            pubsub_file_rx,
        })
    }

    /// Create a new watcher with pub/sub support for Native V2 backend.
    ///
    /// # Arguments
    /// * `path` - Directory to watch recursively
    /// * `config` - Watcher configuration
    /// * `shutdown` - AtomicBool for graceful shutdown
    /// * `backend` - Thread-safe graph backend for pub/sub subscription (must be Native V2)
    /// * `cache_sender` - Channel to send file paths for cache invalidation
    ///
    /// # Returns
    /// A watcher that receives both filesystem and pub/sub events
    ///
    /// # Errors
    /// Returns Ok even if pub/sub subscription fails (graceful degradation).
    /// The watcher will continue with filesystem-only watching in that case.
    #[cfg(feature = "native-v2")]
    pub fn with_pubsub(
        path: PathBuf,
        config: WatcherConfig,
        shutdown: Arc<AtomicBool>,
        backend: Arc<dyn sqlitegraph::GraphBackend + Send + Sync>,
        cache_sender: mpsc::Sender<String>,
    ) -> Result<Self> {
        let (batch_tx, batch_rx) = mpsc::channel();

        // Ensure root_path is set to the watched directory for validation
        let config = WatcherConfig {
            root_path: path.clone(),
            ..config
        };

        // Create channel for pub/sub file paths
        // Note: sender is dropped immediately - pub/sub receiver sends directly to cache_sender
        let (_pubsub_file_tx, pubsub_file_rx) = mpsc::channel();

        // Create pub/sub event receiver with graceful degradation
        let _pubsub_receiver = match PubSubEventReceiver::new(backend, cache_sender) {
            Ok(receiver) => Some(Box::new(receiver)),
            Err(e) => {
                eprintln!("Warning: Failed to create pub/sub receiver: {:?}. Continuing with filesystem-only watching.", e);
                None
            }
        };

        let thread = thread::spawn(move || {
            if let Err(e) = run_watcher(path, batch_tx, config, shutdown) {
                eprintln!("Watcher error: {:?}", e);
            }
        });

        Ok(Self {
            _watcher_thread: ManuallyDrop::new(thread),
            batch_receiver: batch_rx,
            legacy_pending_batch: Arc::new(Mutex::new(None)),
            legacy_pending_index: Arc::new(Mutex::new(0)),
            _pubsub_receiver,
            pubsub_file_rx,
        })
    }

    /// Receive the next batch, blocking until available.
    ///
    /// # Returns
    /// `None` if the watcher thread has terminated
    pub fn recv_batch(&self) -> Option<WatcherBatch> {
        self.batch_receiver.recv().ok()
    }

    /// Try to receive a batch without blocking.
    ///
    /// # Returns
    /// - `Some(batch)` if a batch is available
    /// - `None` if no batch is available or watcher terminated
    pub fn try_recv_batch(&self) -> Option<WatcherBatch> {
        self.batch_receiver.try_recv().ok()
    }

    /// Receive the next batch with a timeout.
    ///
    /// # Returns
    /// - `Ok(Some(batch))` if a batch is available
    /// - `Ok(None)` if the watcher thread has terminated
    /// - `Err` if timeout elapsed
    pub fn recv_batch_timeout(&self, timeout: Duration) -> Result<Option<WatcherBatch>, ()> {
        match self.batch_receiver.recv_timeout(timeout) {
            Ok(batch) => Ok(Some(batch)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(()),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    /// Receive merged filesystem and pub/sub events with timeout.
    ///
    /// This method is only available with the native-v2 feature. It prioritizes
    /// filesystem events over pub/sub events, falling back to pub/sub if no
    /// filesystem batch is available within the timeout.
    ///
    /// # Returns
    /// - `Ok(batch)` if a filesystem or pub/sub event is available
    /// - `Err(())` if no events are available within the timeout
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait for events
    ///
    /// # Behavior
    /// 1. First tries to receive a filesystem batch via `batch_receiver.recv_timeout(timeout)`
    /// 2. If filesystem batch times out, tries pub/sub file path via `pubsub_file_rx.try_recv()`
    /// 3. Returns empty batch on disconnect (caller can handle shutdown)
    /// 4. Returns error only if neither source has events
    #[cfg(feature = "native-v2")]
    pub fn recv_batch_merging(&self, timeout: Duration) -> Result<WatcherBatch, ()> {
        // Priority 1: Try to receive filesystem batch
        match self.batch_receiver.recv_timeout(timeout) {
            Ok(batch) => return Ok(batch),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(WatcherBatch::empty()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Timeout is expected - fall through to pub/sub check
            }
        }

        // Priority 2: Try to receive pub/sub file path (non-blocking)
        match self.pubsub_file_rx.try_recv() {
            Ok(path) => {
                // Pub/sub events are single-path batches
                // Caller will merge with existing batch if needed
                Ok(WatcherBatch {
                    paths: vec![PathBuf::from(path)],
                })
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Ok(WatcherBatch::empty()),
            Err(std::sync::mpsc::TryRecvError::Empty) => Err(()),
        }
    }

    // ========================================================================
    // LEGACY: Old single-event API for backward compatibility during migration
    // ========================================================================

    /// Legacy: Try to receive a single event without blocking (DEPRECATED).
    ///
    /// This method converts batch events to single events for backward
    /// compatibility. Paths from each batch are returned one at a time
    /// in sorted order.
    ///
    /// # Deprecated
    /// Use `try_recv_batch()` instead for deterministic batch processing.
    pub fn try_recv_event(&self) -> Option<FileEvent> {
        // First, check if we have a pending batch to continue from
        {
            let mut pending_batch = self.legacy_pending_batch.lock().unwrap();
            let mut pending_index = self.legacy_pending_index.lock().unwrap();

            if let Some(ref batch) = *pending_batch {
                if *pending_index < batch.paths.len() {
                    let path = batch.paths[*pending_index].clone();
                    *pending_index += 1;

                    // Check if we've exhausted this batch
                    if *pending_index >= batch.paths.len() {
                        *pending_batch = None;
                        *pending_index = 0;
                    }

                    return Some(FileEvent {
                        path,
                        event_type: EventType::Modify,
                    });
                }
            }
        }

        // No pending batch or batch exhausted, try to get a new batch
        if let Ok(batch) = self.batch_receiver.try_recv() {
            if batch.paths.is_empty() {
                return None;
            }

            // If there are multiple paths, store the batch for next call
            if batch.paths.len() > 1 {
                let path = batch.paths[0].clone();
                let mut pending_batch = self.legacy_pending_batch.lock().unwrap();
                let mut pending_index = self.legacy_pending_index.lock().unwrap();
                *pending_batch = Some(batch);
                *pending_index = 1; // Next call will return index 1
                drop(pending_batch);
                drop(pending_index);
                return Some(FileEvent {
                    path,
                    event_type: EventType::Modify,
                });
            }

            // Single path, return it directly
            Some(FileEvent {
                path: batch.paths[0].clone(),
                event_type: EventType::Modify,
            })
        } else {
            None
        }
    }

    /// Legacy: Receive the next event, blocking until available (DEPRECATED).
    ///
    /// This method converts batch events to single events for backward
    /// compatibility. Paths from each batch are returned one at a time
    /// in sorted order.
    ///
    /// # Deprecated
    /// Use `recv_batch()` instead for deterministic batch processing.
    pub fn recv_event(&self) -> Option<FileEvent> {
        // First, check if we have a pending batch to continue from
        {
            let mut pending_batch = self.legacy_pending_batch.lock().unwrap();
            let mut pending_index = self.legacy_pending_index.lock().unwrap();

            if let Some(ref batch) = *pending_batch {
                if *pending_index < batch.paths.len() {
                    let path = batch.paths[*pending_index].clone();
                    *pending_index += 1;

                    // Check if we've exhausted this batch
                    if *pending_index >= batch.paths.len() {
                        *pending_batch = None;
                        *pending_index = 0;
                    }

                    return Some(FileEvent {
                        path,
                        event_type: EventType::Modify,
                    });
                }
            }
        }

        // No pending batch or batch exhausted, block for a new batch
        if let Ok(batch) = self.batch_receiver.recv() {
            if batch.paths.is_empty() {
                return None;
            }

            // If there are multiple paths, store the batch for next call
            if batch.paths.len() > 1 {
                let path = batch.paths[0].clone();
                let mut pending_batch = self.legacy_pending_batch.lock().unwrap();
                let mut pending_index = self.legacy_pending_index.lock().unwrap();
                *pending_batch = Some(batch);
                *pending_index = 1; // Next call will return index 1
                drop(pending_batch);
                drop(pending_index);
                return Some(FileEvent {
                    path,
                    event_type: EventType::Modify,
                });
            }

            // Single path, return it directly
            Some(FileEvent {
                path: batch.paths[0].clone(),
                event_type: EventType::Modify,
            })
        } else {
            None
        }
    }

    /// Explicitly shut down the watcher and join all background threads.
    ///
    /// This method consumes the watcher, ensuring that:
    /// 1. The pub/sub receiver is shut down cleanly (if present)
    /// 2. The watcher thread is joined (waits for clean termination)
    ///
    /// # Note
    ///
    /// This method should be called during graceful shutdown to ensure
    /// all threads have terminated before the program exits.
    pub fn shutdown(mut self) {
        // Take ownership of self (consume it)
        // SAFETY: We're consuming self, so we can safely extract the JoinHandle
        let thread = unsafe { ManuallyDrop::take(&mut self._watcher_thread) };
        // Join the thread - this waits for the watcher to exit cleanly
        let _ = thread.join();
        // Note: pubsub_receiver is dropped here, triggering its Drop impl
    }
}

impl Drop for FileSystemWatcher {
    fn drop(&mut self) {
        #[cfg(feature = "native-v2")]
        if let Some(pubsub) = self._pubsub_receiver.take() {
            // Call shutdown on the pub/sub receiver to join its thread
            pubsub.shutdown();
        }
        // SAFETY: Drop is running, we can safely extract the JoinHandle
        // and drop it without running its destructor (thread should be shutting down)
        let _thread = unsafe { ManuallyDrop::take(&mut self._watcher_thread) };
        drop(_thread);
        // Note: The watcher thread will exit when shutdown flag is set
    }
}

/// Run the debounced watcher in a dedicated thread.
///
/// Uses notify-debouncer-mini for event coalescing. Batches are emitted
/// after the debounce delay expires with all paths that changed during
/// the window.
fn run_watcher(
    path: PathBuf,
    tx: Sender<WatcherBatch>,
    config: WatcherConfig,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    // Convert debounce_ms to Duration
    let debounce_duration = Duration::from_millis(config.debounce_ms);

    // Get the root path for validation
    let root_path = config.root_path.clone();

    // Create gitignore filter if enabled (created ONCE before debouncer)
    // This avoids re-parsing .gitignore on every event
    let filter = if config.gitignore_aware {
        match FileFilter::new(&root_path, &[], &[]) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("Warning: Failed to create gitignore filter: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Create debouncer with notify 8.x API
    // The debouncer calls our closure on each batch of events
    let mut debouncer = new_debouncer(
        debounce_duration,
        move |result: notify_debouncer_mini::DebounceEventResult| {
            match result {
                Ok(events) => {
                    // Collect all dirty paths from this batch
                    // Pass filter reference (moved into closure)
                    let dirty_paths = extract_dirty_paths(&events, &root_path, filter.as_ref());

                    if !dirty_paths.is_empty() {
                        let batch = WatcherBatch::from_set(dirty_paths);
                        let _ = tx.send(batch);
                    }
                }
                Err(error) => {
                    eprintln!("Watcher error: {:?}", error);
                }
            }
        },
    )?;

    // Watch the directory recursively via the inner watcher
    debouncer.watcher().watch(&path, RecursiveMode::Recursive)?;

    // Keep the thread alive until shutdown is signaled
    // The debouncer runs in the background and sends batches via callback
    while !shutdown.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}

/// Extract dirty paths from a batch of debouncer events.
///
/// Filtering rules:
/// - Exclude directories (only process files)
/// - Exclude database-related files (.db, .sqlite, etc.)
/// - Apply gitignore filter if provided (skip ignored files)
/// - Validate paths are within project root (security: prevent path traversal)
/// - De-duplicate via BTreeSet
///
/// Returns: BTreeSet of dirty paths (sorted deterministically)
fn extract_dirty_paths(
    events: &[notify_debouncer_mini::DebouncedEvent],
    root: &Path,
    filter: Option<&FileFilter>,
) -> BTreeSet<PathBuf> {
    let mut dirty_paths = BTreeSet::new();

    for event in events {
        let path = &event.path;

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Skip database-related files to avoid feedback loop
        let path_str = path.to_string_lossy();
        if is_database_file(&path_str) {
            continue;
        }

        // Apply gitignore filter if enabled
        // This checks .gitignore patterns and internal ignores (target/, node_modules/, etc.)
        if let Some(f) = filter {
            if f.should_skip(path).is_some() {
                // Path is ignored by gitignore, skip without logging
                // (would be too noisy to log every ignored file)
                continue;
            }
        }

        // Validate path is within project root (security: prevent path traversal)
        match crate::validation::validate_path_within_root(path, root) {
            Ok(_) => {
                // Path is safe, normalize before inserting
                let normalized = crate::validation::normalize_path(path)
                    .unwrap_or_else(|_| path.to_string_lossy().to_string());
                dirty_paths.insert(PathBuf::from(normalized));
            }
            Err(crate::validation::PathValidationError::OutsideRoot(p, _)) => {
                // Log the rejection but don't crash
                eprintln!("WARNING: Watcher rejected path outside project root: {}", p);
            }
            Err(crate::validation::PathValidationError::SuspiciousTraversal(p)) => {
                // Log suspicious path patterns
                eprintln!(
                    "WARNING: Watcher rejected suspicious traversal pattern: {}",
                    p
                );
            }
            Err(crate::validation::PathValidationError::SymlinkEscape(from, to)) => {
                eprintln!(
                    "WARNING: Watcher rejected symlink escaping root: {} -> {}",
                    from, to
                );
            }
            Err(crate::validation::PathValidationError::CannotCanonicalize(_)) => {
                // Path doesn't exist or can't be accessed - skip
                // This is normal for files that are deleted
            }
        }
    }

    dirty_paths
}

/// Check if a path is a database file that should be excluded from watching.
///
/// Database files are excluded because the indexer writes to them, which
/// would create a feedback loop (write event -> indexer writes again -> ...).
fn is_database_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    path_lower.ends_with(".db")
        || path_lower.ends_with(".db-journal")
        || path_lower.ends_with(".db-wal")
        || path_lower.ends_with(".db-shm")
        || path_lower.ends_with(".sqlite")
        || path_lower.ends_with(".sqlite3")
}

// ============================================================================
// LEGACY: Old single-event types for backward compatibility during migration
// ============================================================================

/// Legacy: File event emitted by the watcher (DEPRECATED).
///
/// This type is kept for backward compatibility during the migration to
/// batch-based processing. New code should use `WatcherBatch` instead.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileEvent {
    /// Path of the affected file
    pub path: PathBuf,
    /// Type of event (DEPRECATED - not used in batch processing)
    pub event_type: EventType,
}

/// Type of file event (DEPRECATED - not used in batch processing).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventType {
    /// File was created
    Create,
    /// File was modified
    Modify,
    /// File was deleted
    Delete,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Create => write!(f, "CREATE"),
            EventType::Modify => write!(f, "MODIFY"),
            EventType::Delete => write!(f, "DELETE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_is_empty() {
        let batch = WatcherBatch::empty();
        assert!(batch.is_empty());
    }

    #[test]
    fn test_batch_from_set_sorts_deterministically() {
        let mut set = BTreeSet::new();
        set.insert(PathBuf::from("/zebra.rs"));
        set.insert(PathBuf::from("/alpha.rs"));
        set.insert(PathBuf::from("/beta.rs"));

        let batch = WatcherBatch::from_set(set);

        // BTreeSet iterates in sorted order
        assert_eq!(batch.paths[0], PathBuf::from("/alpha.rs"));
        assert_eq!(batch.paths[1], PathBuf::from("/beta.rs"));
        assert_eq!(batch.paths[2], PathBuf::from("/zebra.rs"));
    }

    #[test]
    fn test_database_file_detection() {
        assert!(is_database_file("test.db"));
        assert!(is_database_file("test.sqlite"));
        assert!(is_database_file("test.db-journal"));
        assert!(is_database_file("test.DB")); // Case insensitive
        assert!(is_database_file("test.SQLITE"));

        assert!(!is_database_file("test.rs"));
        assert!(!is_database_file("test.py"));
        assert!(!is_database_file("database.rs")); // Extension matters
    }

    #[test]
    fn test_batch_serialization() {
        let batch = WatcherBatch {
            paths: vec![PathBuf::from("/alpha.rs"), PathBuf::from("/beta.rs")],
        };

        let json = serde_json::to_string(&batch).unwrap();
        let deserialized: WatcherBatch = serde_json::from_str(&json).unwrap();

        assert_eq!(batch.paths, deserialized.paths);
    }

    #[test]
    fn test_watcher_config_has_root() {
        let config = WatcherConfig {
            root_path: PathBuf::from("/test/root"),
            debounce_ms: 100,
            gitignore_aware: true,
        };

        assert_eq!(config.root_path, PathBuf::from("/test/root"));
        assert_eq!(config.debounce_ms, 100);
        assert!(config.gitignore_aware);
    }

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();

        assert_eq!(config.root_path, PathBuf::from("."));
        assert_eq!(config.debounce_ms, 500);
        assert!(config.gitignore_aware);
    }

    #[test]
    fn test_extract_dirty_paths_filters_traversal() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create a valid file
        let valid_file = root.join("valid.rs");
        fs::write(&valid_file, b"fn valid() {}").unwrap();

        // Test the validation logic directly
        // since DebouncedEvent cannot be easily constructed in tests
        let result = crate::validation::validate_path_within_root(&valid_file, root);
        assert!(result.is_ok());

        // Test that traversal is rejected
        let outside = root.join("../../../etc/passwd");
        let result_outside = crate::validation::validate_path_within_root(&outside, root);
        assert!(result_outside.is_err());
    }
}

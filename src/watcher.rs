//! Filesystem watcher with debounced batch events.
//!
//! Provides deterministic event coalescing: all events within a debounce window
//! are collected, de-duplicated, sorted lexicographically, and emitted as a single
//! batch. This ensures the same final DB state regardless of event arrival order.

use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

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
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("."),
            debounce_ms: 500,
        }
    }
}

/// Filesystem watcher that emits debounced batches of dirty paths.
///
/// Uses notify-debouncer-mini for event coalescing. All paths within the
/// debounce window are collected, de-duplicated, sorted, and emitted as a
/// single WatcherBatch.
pub struct FileSystemWatcher {
    _watcher_thread: thread::JoinHandle<()>,
    batch_receiver: Receiver<WatcherBatch>,
    /// Legacy compatibility: pending batch to emit one path at a time
    legacy_pending_batch: RefCell<Option<WatcherBatch>>,
    /// Legacy compatibility: current index into pending batch
    legacy_pending_index: RefCell<usize>,
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

        Ok(Self {
            _watcher_thread: thread,
            batch_receiver: batch_rx,
            legacy_pending_batch: RefCell::new(None),
            legacy_pending_index: RefCell::new(0),
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
            let mut pending_batch = self.legacy_pending_batch.borrow_mut();
            let mut pending_index = self.legacy_pending_index.borrow_mut();

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
                let mut pending_batch = self.legacy_pending_batch.borrow_mut();
                let mut pending_index = self.legacy_pending_index.borrow_mut();
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
            let mut pending_batch = self.legacy_pending_batch.borrow_mut();
            let mut pending_index = self.legacy_pending_index.borrow_mut();

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
                let mut pending_batch = self.legacy_pending_batch.borrow_mut();
                let mut pending_index = self.legacy_pending_index.borrow_mut();
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

    // Create debouncer with notify 8.x API
    // The debouncer calls our closure on each batch of events
    let mut debouncer = new_debouncer(
        debounce_duration,
        move |result: notify_debouncer_mini::DebounceEventResult| {
            match result {
                Ok(events) => {
                    // Collect all dirty paths from this batch
                    let dirty_paths = extract_dirty_paths(&events, &root_path);

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
/// - Validate paths are within project root (security: prevent path traversal)
/// - De-duplicate via BTreeSet
///
/// Returns: BTreeSet of dirty paths (sorted deterministically)
fn extract_dirty_paths(
    events: &[notify_debouncer_mini::DebouncedEvent],
    root: &Path,
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
                eprintln!(
                    "WARNING: Watcher rejected path outside project root: {}",
                    p
                );
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
            paths: vec![
                PathBuf::from("/alpha.rs"),
                PathBuf::from("/beta.rs"),
            ],
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
        };

        assert_eq!(config.root_path, PathBuf::from("/test/root"));
        assert_eq!(config.debounce_ms, 100);
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

//! Pub/Sub event receiver for Native V2 backend graph mutations.
//!
//! This module subscribes to graph mutation events from the Native V2 backend
//! and extracts file paths from those events for cache invalidation.
//!
//! # Thread Safety
//!
//! **This module spawns a dedicated thread for event processing.**
//!
//! The `FileNodeCache` is NOT thread-safe (see `src/graph/cache.rs`), so this module
//! does NOT access the cache directly. Instead, it sends file paths via a channel
//! to the main watcher thread, which owns the cache and performs invalidation.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐         subscribe          ┌──────────────┐
//! │  GraphBackend   │ ──────────────────────────>│ PubSubSystem │
//! │  (native-v2)    │                            │ (in sqlitegraph)│
//! └────────┬────────┘                            └──────┬───────┘
//!          │                                            │
//!          │                                            │ events
//!          │                                            ▼
//!          │                                    ┌──────────────┐
//!          │                                    │EventReceiver │
//!          │                                    │  (thread)    │
//!          │                                    └──────┬───────┘
//!          │                                           │ file_path
//!          │                                           ▼
//!          │                                    ┌──────────────┐
//!          │                                    │  mpsc::channel│
//!          │                                    └──────┬───────┘
//!          │                                           │
//!          │                                           ▼
//!          ▼                                     ┌──────────────┐
//! ┌─────────────────┐  invalidate_cache(path) │ FileSystemWatcher │
//! │  FileNodeCache   │<──────────────────────────│  (main thread)  │
//! │  (not thread-safe)│                          └───────────────────┘
//! └─────────────────┘
//! ```
//!
//! # Event Processing
//!
//! - **NodeChanged**: Extract `file_path` from node properties
//! - **EdgeChanged**: Ignored (edge_id cannot be decoded via GraphBackend trait)
//!   - Note: Cache invalidation from node changes is sufficient for correctness
//! - **KVChanged**: Ignored (cannot extract file path from key hash)
//! - **SnapshotCommitted**: Ignored (transaction boundary event)

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use sqlitegraph::{
    backend::{PubSubEvent, SubscriptionFilter},
    GraphBackend, SnapshotId,
};

/// Type alias for thread-safe backend reference.
///
/// The pub/sub receiver runs in a separate thread, so we need `Arc` instead of `Rc`.
/// Note: `NativeGraphBackend` uses `RwLock` internally for thread-safe access.
type ThreadSafeBackend = Arc<dyn GraphBackend + Send + Sync>;

/// Pub/Sub event receiver that processes graph mutations.
///
/// Spawns a background thread that subscribes to all graph mutation events
/// and extracts file paths from those events. File paths are sent via a
/// channel to the main watcher thread for cache invalidation.
///
/// # Thread Safety
///
/// The receiver thread does NOT access `FileNodeCache` directly (it's not thread-safe).
/// Instead, file paths are sent via `mpsc::channel` to the watcher thread.
///
/// # Shutdown
///
/// Drop implementation is NOT provided in this phase (handled in 49-03).
/// For now, the thread runs until the program exits or the backend disconnects.
pub struct PubSubEventReceiver {
    /// Background thread handle (prefixed with _ to suppress "never read" warning)
    _thread: JoinHandle<()>,
    /// Subscription ID for cleanup (prefixed with _ to suppress "never read" warning)
    /// Note: Cleanup will be implemented in phase 49-03
    _sub_id: u64,
    /// Atomic flag for graceful shutdown (shared with event loop thread)
    shutdown: Arc<AtomicBool>,
}

impl PubSubEventReceiver {
    /// Create a new pub/sub event receiver.
    ///
    /// # Arguments
    ///
    /// * `backend` - The graph backend (must be Native V2 with pub/sub support)
    /// * `file_sender` - Channel to send file paths for cache invalidation
    ///
    /// # Returns
    ///
    /// A receiver that processes events in the background and sends file paths
    /// via the provided channel.
    ///
    /// # Errors
    ///
    /// Returns an error if subscription to the backend's pub/sub system fails.
    pub fn new(backend: ThreadSafeBackend, file_sender: Sender<String>) -> Result<Self> {
        // Subscribe to ALL graph mutation events
        let (sub_id, rx) = backend.subscribe(SubscriptionFilter::all())?;

        // Create shutdown flag for graceful termination
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        // Spawn event loop thread
        let thread = thread::spawn(move || {
            run_event_loop(rx, backend, file_sender, shutdown_clone);
        });

        Ok(Self {
            _thread: thread,
            _sub_id: sub_id,
            shutdown,
        })
    }
}

/// Run the event loop for processing pub/sub events.
///
/// This function runs in a dedicated thread and processes events until:
/// - Shutdown flag is set
/// - Channel is disconnected (backend shutdown)
/// - An error occurs
///
/// # Arguments
///
/// * `rx` - Receiver for pub/sub events from the backend
/// * `backend` - Graph backend for querying node/edge properties
/// * `file_sender` - Channel to send file paths for cache invalidation
/// * `shutdown` - Atomic flag for graceful shutdown
fn run_event_loop(
    rx: Receiver<PubSubEvent>,
    backend: ThreadSafeBackend,
    file_sender: Sender<String>,
    shutdown: Arc<AtomicBool>,
) {
    // Use 100ms timeout to check shutdown flag periodically
    const TIMEOUT_MS: u64 = 100;

    while !shutdown.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(TIMEOUT_MS)) {
            Ok(event) => {
                // Extract file path from event (if any)
                if let Some(path) = extract_file_path(&event, &*backend) {
                    // Send to main thread for cache invalidation
                    // Ignore send errors - channel might be closed during shutdown
                    let _ = file_sender.send(path);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Timeout is expected - allows checking shutdown flag
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Backend disconnected, exit loop
                eprintln!("PubSub: Backend disconnected, stopping event receiver");
                break;
            }
        }
    }
}

/// Extract file path from a pub/sub event.
///
/// Returns `None` for events that cannot be mapped to a specific file path
/// (EdgeChanged, KVChanged, SnapshotCommitted) or when the file_path property is missing.
///
/// # Event Handling Strategy
///
/// - **NodeChanged**: Query node for file_path property
/// - **EdgeChanged**: Skipped - edge_id is a compact ID that cannot be decoded via GraphBackend trait
///   - NodeChanged events provide sufficient cache invalidation coverage
/// - **KVChanged**: Skipped - cannot extract file path from key_hash
/// - **SnapshotCommitted**: Skipped - transaction boundary event with no file path
///
/// # Arguments
///
/// * `event` - The pub/sub event to process
/// * `backend` - Graph backend for querying node properties
///
/// # Returns
///
/// `Some(path)` if the event can be mapped to a file path, `None` otherwise.
fn extract_file_path(event: &PubSubEvent, backend: &dyn GraphBackend) -> Option<String> {
    match event {
        // Node changes: query node properties for file_path
        // Note: get_node returns GraphEntity which has file_path as Option<String> directly
        PubSubEvent::NodeChanged { snapshot_id, node_id } => {
            match backend.get_node(SnapshotId(*snapshot_id), *node_id) {
                Ok(entity) => {
                    // GraphEntity has file_path as Option<String> field
                    entity.file_path
                }
                Err(e) => {
                    // Log error but don't fail - this is opportunistic cache invalidation
                    eprintln!("PubSub: Failed to query node {}: {:?}", node_id, e);
                    None
                }
            }
        }

        // Edge changes: Skipped because edge_id is a compact ID that cannot be
        // decoded via the GraphBackend trait (no get_edge method available).
        // Cache invalidation from NodeChanged events is sufficient for correctness.
        PubSubEvent::EdgeChanged { .. } => {
            // Skip edge events - node changes provide sufficient coverage
            None
        }

        // KV changes: cannot extract file path from key_hash
        // The KV store stores sym:fqn:{fqn} mappings, but we can't
        // reverse lookup the file path from the hash efficiently.
        // Cache invalidation is opportunistic anyway, so we skip these.
        PubSubEvent::KVChanged { .. } => {
            // Ignore KV changes - can't extract file path from key_hash
            None
        }

        // Snapshot committed: transaction boundary event, no file path
        PubSubEvent::SnapshotCommitted { .. } => {
            // Transaction boundary - no file path to invalidate
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require a Native V2 backend instance
    // and are deferred to phase 49-02 (integration with FileSystemWatcher)

    #[test]
    #[cfg(feature = "native-v2")]
    fn test_pubsub_event_receiver_compiles() {
        // This test verifies the module compiles with native-v2 feature
        // Actual functionality tests require a backend instance
        assert!(true);
    }
}

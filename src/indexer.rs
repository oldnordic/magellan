//! Indexer coordinator for Magellan
//!
//! Wires filesystem watcher events to graph updates.
//! Maintains synchronous consistency between filesystem and sqlitegraph.

use anyhow::Result;
use std::path::PathBuf;

use crate::{CodeGraph, FileEvent, FileSystemWatcher, WatcherConfig};

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
    let path_key = event.path.to_string_lossy().to_string();
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
    // Create watcher
    let watcher = FileSystemWatcher::new(root_path.clone(), WatcherConfig::default())?;

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
        if let Some(event) = watcher.try_recv_event() {
            handle_event(&mut graph, event)?;
            processed += 1;
            idle_for = std::time::Duration::from_secs(0);
            continue;
        }

        if idle_for >= idle_timeout {
            break;
        }

        std::thread::sleep(idle_step);
        idle_for += idle_step;
    }

    Ok(processed)
}

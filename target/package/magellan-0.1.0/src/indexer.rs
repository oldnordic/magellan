//! Indexer coordinator for Magellan
//!
//! Wires filesystem watcher events to graph updates.
//! Maintains synchronous consistency between filesystem and sqlitegraph.

use anyhow::Result;
use std::path::PathBuf;

use crate::{CodeGraph, FileSystemWatcher, WatcherConfig, FileEvent};

/// Handle a single file event, updating the graph appropriately.
///
/// # Arguments
/// * `graph` - Mutable reference to CodeGraph
/// * `event` - FileEvent to process
///
/// # Behavior
/// - Create/Modify: read file, delete old data, index symbols and references
/// - Delete: delete file and all derived data
fn handle_event(graph: &mut CodeGraph, event: FileEvent) -> Result<()> {
    // Convert path to string for graph API
    let path = event.path.to_string_lossy().to_string();

    // Handle event by type
    match event.event_type {
        crate::EventType::Create | crate::EventType::Modify => {
            // Read file contents - handle case where file doesn't exist yet
            let source = match std::fs::read(&event.path) {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // File was deleted or doesn't exist yet, skip this event
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            };

            // Delete old data (idempotent)
            graph.delete_file(&path)?;

            // Index symbols
            graph.index_file(&path, &source)?;

            // Index references
            graph.index_references(&path, &source)?;
        }
        crate::EventType::Delete => {
            // Delete file and all derived data
            graph.delete_file(&path)?;
        }
    }

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

    // Process up to max_events events
    let mut processed = 0;
    while processed < max_events {
        match watcher.recv_event() {
            Some(event) => {
                handle_event(&mut graph, event)?;
                processed += 1;
            }
            None => {
                // Watcher terminated, stop processing
                break;
            }
        }
    }

    Ok(processed)
}

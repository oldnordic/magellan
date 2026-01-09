use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::SystemTime;

/// File event emitted by the watcher
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEvent {
    /// Path of the affected file
    pub path: PathBuf,
    /// Type of event
    pub event_type: EventType,
    /// Timestamp when event was detected
    pub timestamp: SystemTime,
}

/// Type of file event
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

/// Filesystem watcher configuration
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce delay in milliseconds
    pub debounce_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self { debounce_ms: 500 }
    }
}

/// Filesystem watcher that emits events on a channel
pub struct FileSystemWatcher {
    _watcher_thread: thread::JoinHandle<()>,
    event_receiver: Receiver<FileEvent>,
}

impl FileSystemWatcher {
    /// Create a new watcher for the given directory
    ///
    /// # Arguments
    /// * `path` - Directory to watch recursively
    /// * `config` - Watcher configuration
    ///
    /// # Returns
    /// A watcher that can be polled for events
    pub fn new(path: PathBuf, config: WatcherConfig) -> Result<Self> {
        let (tx, rx) = mpsc::channel();

        let thread = thread::spawn(move || {
            if let Err(e) = run_watcher(path, tx, config) {
                eprintln!("Watcher error: {:?}", e);
            }
        });

        Ok(Self {
            _watcher_thread: thread,
            event_receiver: rx,
        })
    }

    /// Receive the next event, blocking until available
    ///
    /// # Returns
    /// `None` if the watcher thread has terminated
    pub fn recv_event(&self) -> Option<FileEvent> {
        self.event_receiver.recv().ok()
    }

    /// Try to receive an event without blocking
    ///
    /// # Returns
    /// - `Some(event)` if an event is available
    /// - `None` if no event is available or watcher terminated
    pub fn try_recv_event(&self) -> Option<FileEvent> {
        self.event_receiver.try_recv().ok()
    }
}

/// Run the watcher in a dedicated thread
fn run_watcher(path: PathBuf, tx: Sender<FileEvent>, _config: WatcherConfig) -> Result<()> {
    let _tx = tx.clone(); // Clone for closure

    let mut notify_watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            if let Some(file_event) = convert_notify_event(event) {
                let _ = _tx.send(file_event);
            }
        }
    })?;

    notify_watcher.watch(&path, RecursiveMode::Recursive)?;

    // Keep the thread alive - block forever
    // The watcher runs in the background and sends events via callback
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// Convert notify event to FileEvent
fn convert_notify_event(event: notify::Event) -> Option<FileEvent> {
    // Only process files, not directories
    if event.paths.iter().any(|p| p.is_dir()) {
        return None;
    }

    let event_type = match event.kind {
        EventKind::Create(_) => Some(EventType::Create),
        EventKind::Modify(_) => Some(EventType::Modify),
        EventKind::Remove(_) => Some(EventType::Delete),
        _ => None,
    };

    let event_type = event_type?;

    // Use first path (notify can emit multiple paths in one event)
    let path = event.paths.first()?.clone();

    // Skip database-related files to avoid feedback loop
    // ( indexer writes to DB → generates event → indexer writes again )
    let path_str = path.to_string_lossy();
    if path_str.ends_with(".db")
        || path_str.ends_with(".db-journal")
        || path_str.ends_with(".db-wal")
        || path_str.ends_with(".db-shm")
        || path_str.ends_with(".sqlite")
        || path_str.ends_with(".sqlite3")
    {
        return None;
    }

    Some(FileEvent {
        path,
        event_type,
        timestamp: SystemTime::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_serialization() {
        let event = FileEvent {
            path: PathBuf::from("/test/file.rs"),
            event_type: EventType::Create,
            timestamp: SystemTime::now(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: FileEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.path, deserialized.path);
        assert_eq!(event.event_type, deserialized.event_type);
    }
}

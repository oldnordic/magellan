//! Async filesystem watcher with tokio
//!
//! Provides non-blocking file watching using tokio runtime.
//! Supports backpressure via bounded channels.

use anyhow::{Context, Result};
use async_channel::{bounded, Receiver};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::watcher::{WatcherBatch, WatcherConfig};

/// Async file watcher using tokio
pub struct AsyncWatcher {
    /// Channel receiver for file events
    event_rx: Receiver<WatcherBatch>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Watcher handle (for cleanup)
    _watcher: Option<RecommendedWatcher>,
}

impl AsyncWatcher {
    /// Create a new async watcher
    pub fn new(
        root_path: PathBuf,
        _config: WatcherConfig,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = bounded::<WatcherBatch>(100); // Backpressure: max 100 pending batches

        let shutdown_clone = shutdown.clone();

        // Create notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    // Convert paths to batch
                    let paths: Vec<PathBuf> = event.paths.iter().cloned().collect();
                    if !paths.is_empty() {
                        let batch = WatcherBatch::from_set(paths.into_iter().collect());
                        let _ = event_tx.try_send(batch);
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .context("Failed to create notify watcher")?;

        // Watch root directory recursively
        watcher
            .watch(&root_path, RecursiveMode::Recursive)
            .context("Failed to watch directory")?;

        Ok(Self {
            event_rx,
            shutdown: shutdown_clone,
            _watcher: Some(watcher),
        })
    }

    /// Receive next batch of events (async)
    pub async fn recv_batch(&self) -> Option<WatcherBatch> {
        if self.shutdown.load(Ordering::SeqCst) {
            return None;
        }
        self.event_rx.recv().await.ok()
    }

    /// Try to receive batch without blocking
    pub fn try_recv_batch(&self) -> Option<WatcherBatch> {
        self.event_rx.try_recv().ok()
    }

    /// Check if watcher should shutdown
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }
}

impl Drop for AsyncWatcher {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_async_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let config = WatcherConfig::default();

        let watcher = AsyncWatcher::new(temp_dir.path().to_path_buf(), config, shutdown);
        assert!(watcher.is_ok());
    }
}

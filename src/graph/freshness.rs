//! Database freshness checking
//!
//! Provides staleness detection for graph databases.

use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::CodeGraph;

/// Staleness threshold in seconds (5 minutes)
pub const STALE_THRESHOLD_SECS: i64 = 300;

/// Freshness status of the database
#[derive(Debug, Clone)]
pub struct FreshnessStatus {
    /// Whether the database is stale
    is_stale: bool,
    /// Seconds since the most recent file was indexed
    seconds_since_index: i64,
    /// Number of files in the database
    file_count: usize,
}

impl FreshnessStatus {
    /// Create a new FreshnessStatus
    pub fn new(is_stale: bool, seconds_since_index: i64, file_count: usize) -> Self {
        Self {
            is_stale,
            seconds_since_index,
            file_count,
        }
    }

    /// Check if the database is stale
    pub fn is_stale(&self) -> bool {
        self.is_stale
    }

    /// Get minutes since the most recent file was indexed
    pub fn minutes_since_index(&self) -> i64 {
        self.seconds_since_index / 60
    }

    /// Get seconds since the most recent file was indexed
    pub fn seconds_since_index(&self) -> i64 {
        self.seconds_since_index
    }

    /// Get the number of files in the database
    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// Generate a warning message for stale database
    pub fn warning_message(
        &self,
        db_path: std::path::PathBuf,
        root_path: std::path::PathBuf,
    ) -> String {
        let mins = self.minutes_since_index();
        let db_str = db_path.to_string_lossy();
        let root_str = root_path.to_string_lossy();

        format!(
            "WARNING: Database may be stale (last indexed {} minutes ago)\n  Run 'magellan verify --db {} --root {}' to check\n  Consider running 'magellan watch' for automatic updates",
            mins, db_str, root_str
        )
    }
}

/// Get current Unix timestamp in seconds
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Check database freshness
///
/// Scans all File nodes and determines if the database is stale.
/// A database is considered stale if the most recent file was indexed
/// more than STALE_THRESHOLD_SECS seconds ago.
///
/// # Arguments
/// * `graph` - CodeGraph instance
///
/// # Returns
/// FreshnessStatus with staleness information
pub fn check_freshness(graph: &CodeGraph) -> Result<FreshnessStatus> {
    // Use read-only API to get all file nodes without mutation
    let file_nodes = graph.files.all_file_nodes_readonly()?;
    let file_count = file_nodes.len();

    if file_count == 0 {
        // Empty database is not stale
        return Ok(FreshnessStatus::new(false, 0, 0));
    }

    // Find the most recent index time
    let mut max_indexed_at: i64 = 0;
    for file_node in file_nodes.values() {
        if file_node.last_indexed_at > max_indexed_at {
            max_indexed_at = file_node.last_indexed_at;
        }
    }

    let now = now_secs();
    let seconds_since = if max_indexed_at > 0 {
        now.saturating_sub(max_indexed_at)
    } else {
        // Files with timestamp 0 are from old databases without timestamps
        // Treat as very stale
        i64::MAX
    };

    let is_stale = seconds_since >= STALE_THRESHOLD_SECS;

    Ok(FreshnessStatus::new(is_stale, seconds_since, file_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_freshness_status_constants() {
        assert_eq!(STALE_THRESHOLD_SECS, 300);
    }

    #[test]
    fn test_freshness_status_methods() {
        let status = FreshnessStatus::new(true, 600, 10);

        assert!(status.is_stale());
        assert_eq!(status.minutes_since_index(), 10);
        assert_eq!(status.seconds_since_index(), 600);
        assert_eq!(status.file_count(), 10);
    }

    #[test]
    fn test_warning_message_format() {
        let status = FreshnessStatus::new(true, 600, 10);

        let msg = status.warning_message(
            std::path::PathBuf::from("/path/to/db"),
            std::path::PathBuf::from("/path/to/root"),
        );

        assert!(msg.contains("WARNING"));
        assert!(msg.contains("10 minutes"));
        assert!(msg.contains("magellan verify"));
        assert!(msg.contains("magellan watch"));
    }
}

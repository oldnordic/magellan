//! Database verification module
//!
//! Compares database state vs filesystem to detect inconsistencies.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::graph::{CodeGraph, FileNode};

/// Report of database verification results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    /// Files in database but missing on filesystem
    pub missing: Vec<String>,
    /// Files on filesystem but not in database
    pub new: Vec<String>,
    /// Files on filesystem with different hash than database
    pub modified: Vec<String>,
    /// Files indexed more than 5 minutes ago (stale)
    pub stale: Vec<String>,
}

impl VerifyReport {
    /// Total number of issues found
    pub fn total_issues(&self) -> usize {
        self.missing.len() + self.new.len() + self.modified.len() + self.stale.len()
    }

    /// Check if verification is clean (no issues)
    pub fn is_clean(&self) -> bool {
        self.total_issues() == 0
    }
}

/// Staleness threshold in seconds (default: 5 minutes)
const STALE_THRESHOLD_SECS: i64 = 300;

/// Verify database state against filesystem
///
/// # Arguments
/// * `graph` - CodeGraph instance to query
/// * `root` - Root directory to scan
///
/// # Returns
/// VerifyReport with any discrepancies found
pub fn verify_graph(graph: &mut CodeGraph, root: &Path) -> Result<VerifyReport> {
    let mut missing = Vec::new();
    let mut new = Vec::new();
    let mut modified = Vec::new();
    let mut stale = Vec::new();

    // Get all file paths from the database
    let db_files = get_all_db_files(graph)?;

    // Get all .rs file paths from the filesystem
    let fs_files = get_all_fs_files(root)?;

    // Convert fs paths to strings for comparison
    let fs_paths: HashSet<String> = fs_files
        .keys()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Find missing files (in DB but not on filesystem)
    for path in db_files.keys() {
        let path_buf = PathBuf::from(path);
        if !fs_paths.contains(path) && !path_exists_on_disk(&path_buf) {
            // File is in DB but doesn't exist on filesystem
            missing.push(path.clone());
        }
    }

    // Find new and modified files
    for (fs_path, fs_hash) in &fs_files {
        let path_str = fs_path.to_string_lossy().to_string();

        if let Some(db_file) = db_files.get(&path_str) {
            // File exists in both - check if modified
            if db_file.hash != *fs_hash {
                modified.push(path_str);
            }
        } else {
            // File on filesystem but not in DB
            new.push(path_str);
        }
    }

    // Find stale files (indexed more than threshold ago)
    let now = now_secs();
    for (path, file_node) in &db_files {
        if now - file_node.last_indexed_at > STALE_THRESHOLD_SECS {
            // Also check if file still exists on disk
            let path_buf = PathBuf::from(path);
            if path_exists_on_disk(&path_buf) {
                stale.push(path.clone());
            } else {
                // File doesn't exist, add to missing if not already there
                if !missing.contains(path) {
                    missing.push(path.clone());
                }
            }
        }
    }

    // Sort all vectors for deterministic output
    missing.sort();
    new.sort();
    modified.sort();
    stale.sort();

    Ok(VerifyReport {
        missing,
        new,
        modified,
        stale,
    })
}

/// Get all files from the database as a map of path -> FileNode
fn get_all_db_files(graph: &mut CodeGraph) -> Result<HashMap<String, FileNode>> {
    graph.all_file_nodes()
}

/// Get all .rs files from filesystem as a map of path -> hash
fn get_all_fs_files(root: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut result = HashMap::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            // Skip database files
            if let Some(file_name) = path.file_name() {
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".db") || file_name_str.ends_with(".db-journal") {
                    continue;
                }
            }

            // Read file and compute hash
            if let Ok(content) = std::fs::read(path) {
                let hash = compute_hash(&content);
                result.insert(path.to_path_buf(), hash);
            }
        }
    }

    Ok(result)
}

/// Compute SHA-256 hash of file contents
fn compute_hash(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// Get current Unix timestamp in seconds
///
/// Returns 0 if system clock is set before UNIX epoch (e.g., VM snapshots,
/// NTP adjustments, manual changes). This prevents panic on backward clock changes.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0))
        .as_secs() as i64
}

/// Check if a path exists on disk (non-trivial check for temp files)
fn path_exists_on_disk(path: &Path) -> bool {
    path.exists()
}

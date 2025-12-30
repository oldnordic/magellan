//! Directory scanning operations for CodeGraph
//!
//! Handles initial full scan of directory trees for supported source files.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{CodeGraph, ScanProgress};
use crate::ingest::detect_language;

/// Scan a directory and index all supported source files found
///
/// # Behavior
/// 1. Walk directory recursively
/// 2. Find all supported source files (.rs, .py, .java, etc.)
/// 3. Index each file (symbols + references)
/// 4. Report progress via callback
///
/// # Arguments
/// * `graph` - CodeGraph instance (mutable for indexing)
/// * `dir_path` - Directory to scan
/// * `progress` - Optional callback for progress reporting (current, total)
///
/// # Returns
/// Number of files indexed
///
/// # Guarantees
/// - Only supported language files are processed
/// - Files are indexed in sorted order for determinism
/// - Unsupported files are silently skipped
pub fn scan_directory(
    graph: &mut CodeGraph,
    dir_path: &Path,
    progress: Option<&ScanProgress>,
) -> Result<usize> {
    // Collect all supported source files first (for sorted order)
    let mut source_files: Vec<PathBuf> = Vec::new();

    // Use walkdir to collect all supported source files
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        // Use language detection to filter supported files
        if detect_language(path).is_some() {
            // Skip database files
            if let Some(file_name) = path.file_name() {
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".db") || file_name_str.ends_with(".db-journal") {
                    continue;
                }
            }
            source_files.push(path.to_path_buf());
        }
    }

    // Sort for deterministic ordering
    source_files.sort();

    let total = source_files.len();

    // Index each file
    for (idx, path) in source_files.iter().enumerate() {
        // Report progress
        if let Some(cb) = progress {
            cb(idx + 1, total);
        }

        // Read file contents
        let source = match std::fs::read(path) {
            Ok(s) => s,
            Err(_) => continue, // Skip unreadable files
        };

        // Get path as string
        let path_str = path.to_string_lossy().to_string();

        // Delete old data (idempotent)
        let _ = graph.delete_file(&path_str);

        // Index symbols
        let _ = graph.index_file(&path_str, &source);

        // Index references
        let _ = graph.index_references(&path_str, &source);
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_filters_database_files() {
        // Verify that .db and .db-journal files are filtered out
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create some test files
        let code_rs = temp_dir.path().join("code.rs");
        let data_db = temp_dir.path().join("data.db");
        let journal = temp_dir.path().join("test.db-journal");

        std::fs::write(&code_rs, b"fn test() {}").unwrap();
        std::fs::write(&data_db, b"database data").unwrap();
        std::fs::write(&journal, b"journal data").unwrap();

        // Scan should only index .rs files (not .db files)
        let count = scan_directory(&mut graph, temp_dir.path(), None).unwrap();
        assert_eq!(count, 1, "Should only scan 1 .rs file");

        // Verify the code file was indexed
        let symbols = graph.symbols_in_file(code_rs.to_str().unwrap()).unwrap();
        assert_eq!(symbols.len(), 1);
    }
}

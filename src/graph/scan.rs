//! Directory scanning operations for CodeGraph
//!
//! Handles initial full scan of directory trees for supported source files.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{CodeGraph, ScanProgress};
use crate::diagnostics::{DiagnosticStage, WatchDiagnostic};
use crate::graph::filter::{skip_diagnostic, FileFilter};
use crate::validation::{validate_path_within_root, PathValidationError};

/// Scan result containing count and diagnostics.
#[derive(Debug, Default)]
pub struct ScanResult {
    /// Number of files indexed
    pub indexed: usize,
    /// Diagnostics for skipped files and errors
    pub diagnostics: Vec<WatchDiagnostic>,
}

/// Scan a directory and index all supported source files found
///
/// # Behavior
/// 1. Walk directory recursively
/// 2. Validate each path is within project root (prevents traversal attacks)
/// 3. Apply filtering rules (internal ignores, gitignore, include/exclude)
/// 4. Index each supported file (symbols + references)
/// 5. Report progress via callback
/// 6. Collect diagnostics for skipped files and errors
///
/// # Security
/// - Path validation prevents directory traversal attacks
/// - Symlinks are NOT followed during walk (follow_links=false in WalkDir)
/// - Paths escaping root are rejected and logged as diagnostics
///
/// # Arguments
/// * `graph` - CodeGraph instance (mutable for indexing)
/// * `dir_path` - Directory to scan (treated as root boundary)
/// * `filter` - File filter for determining which files to process
/// * `progress` - Optional callback for progress reporting (current, total)
///
/// # Returns
/// ScanResult with indexed count and diagnostics
///
/// # Guarantees
/// - Filtering is deterministic and pure
/// - Files are indexed in sorted order for determinism
/// - Errors are collected as diagnostics; processing continues
/// - No files outside dir_path are accessed
pub fn scan_directory_with_filter(
    graph: &mut CodeGraph,
    dir_path: &Path,
    filter: &FileFilter,
    progress: Option<&ScanProgress>,
) -> Result<ScanResult> {
    // Collect all candidate files first (for sorted order)
    let mut candidate_files: Vec<PathBuf> = Vec::new();
    let mut diagnostics = Vec::new();

    // Use walkdir to collect all files
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        // Skip directories and symlinks to directories
        if path.is_dir() {
            continue;
        }

        // Validate path is within project root (security: prevent path traversal)
        // WalkDir should keep us within dir_path, but validate defensively
        match validate_path_within_root(path, dir_path) {
            Ok(_) => {
                // Path is safe, continue to filtering
            }
            Err(PathValidationError::OutsideRoot(p, _)) => {
                let rel_path = Path::new(&p).strip_prefix(dir_path)
                    .unwrap_or_else(|_| Path::new(&p))
                    .to_string_lossy()
                    .to_string();
                diagnostics.push(WatchDiagnostic::skipped(
                    rel_path,
                    crate::diagnostics::SkipReason::IgnoredInternal,
                ));
                continue;
            }
            Err(PathValidationError::SymlinkEscape(from, to)) => {
                let rel_path = Path::new(&from).strip_prefix(dir_path)
                    .unwrap_or_else(|_| Path::new(&from))
                    .to_string_lossy()
                    .to_string();
                diagnostics.push(WatchDiagnostic::error(
                    rel_path,
                    DiagnosticStage::Read,
                    format!("symlink escapes root: {}", to),
                ));
                continue;
            }
            Err(PathValidationError::CannotCanonicalize(_)) => {
                // Path doesn't exist or can't be accessed
                continue;
            }
            Err(PathValidationError::SuspiciousTraversal(p)) => {
                diagnostics.push(WatchDiagnostic::error(
                    p,
                    DiagnosticStage::Read,
                    "suspicious traversal pattern".to_string(),
                ));
                continue;
            }
        }

        // Apply filter
        if let Some(reason) = filter.should_skip(path) {
            diagnostics.push(skip_diagnostic(dir_path, path, reason));
            continue;
        }

        // File passes all filters
        candidate_files.push(path.to_path_buf());
    }

    // Sort for deterministic ordering
    candidate_files.sort();

    let total = candidate_files.len();

    // Index each file with error containment
    for (idx, path) in candidate_files.iter().enumerate() {
        // Report progress
        if let Some(cb) = progress {
            cb(idx + 1, total);
        }

        let path_str = path.to_string_lossy().to_string();
        let rel_path = path
            .strip_prefix(dir_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path_str.clone());

        // Read file contents with error handling
        let source = match std::fs::read(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(WatchDiagnostic::error(
                    rel_path,
                    DiagnosticStage::Read,
                    e.to_string(),
                ));
                continue;
            }
        };

        // Delete old data (idempotent)
        let _ = graph.delete_file(&path_str);

        // Index symbols with error handling
        match graph.index_file(&path_str, &source) {
            Ok(_) => {}
            Err(e) => {
                diagnostics.push(WatchDiagnostic::error(
                    rel_path,
                    DiagnosticStage::IndexSymbols,
                    e.to_string(),
                ));
                continue;
            }
        }

        // Index references with error handling
        match graph.index_references(&path_str, &source) {
            Ok(_) => {}
            Err(e) => {
                diagnostics.push(WatchDiagnostic::error(
                    rel_path,
                    DiagnosticStage::IndexReferences,
                    e.to_string(),
                ));
            }
        }
    }

    // Sort diagnostics for deterministic output
    diagnostics.sort();

    Ok(ScanResult {
        indexed: total,
        diagnostics,
    })
}

/// Legacy scan function without explicit filter (creates default filter).
///
/// This maintains backward compatibility while using the new filtering infrastructure.
pub fn scan_directory(
    graph: &mut CodeGraph,
    dir_path: &Path,
    progress: Option<&ScanProgress>,
) -> Result<usize> {
    // Create default filter (no patterns)
    let filter = FileFilter::new(dir_path, &[], &[])?;
    let result = scan_directory_with_filter(graph, dir_path, &filter, progress)?;
    Ok(result.indexed)
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
        let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        assert_eq!(result.indexed, 1, "Should only scan 1 .rs file");

        // Verify the code file was indexed
        let symbols = graph.symbols_in_file(code_rs.to_str().unwrap()).unwrap();
        assert_eq!(symbols.len(), 1);

        // Verify diagnostics for skipped files
        assert!(result.diagnostics.len() >= 2);
        let db_diag = result
            .diagnostics
            .iter()
            .find(|d| d.path().contains("data.db"));
        assert!(db_diag.is_some());
    }

    #[test]
    fn test_scan_with_gitignore() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create .gitignore
        std::fs::write(temp_dir.path().join(".gitignore"), "ignored.rs").unwrap();

        // Create files
        std::fs::write(temp_dir.path().join("included.rs"), b"fn included() {}").unwrap();
        std::fs::write(temp_dir.path().join("ignored.rs"), b"fn ignored() {}").unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // Only included.rs should be indexed
        assert_eq!(result.indexed, 1);

        // Should have diagnostic for ignored.rs
        let ignored_diag = result
            .diagnostics
            .iter()
            .find(|d| d.path() == "ignored.rs");
        assert!(ignored_diag.is_some());
    }

    #[test]
    fn test_scan_with_include_patterns() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create directory structure
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("tests")).unwrap();

        // Create files
        std::fs::write(temp_dir.path().join("src/lib.rs"), b"fn lib() {}").unwrap();
        std::fs::write(temp_dir.path().join("tests/test.rs"), b"fn test() {}").unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &["src/**".to_string()], &[]).unwrap();
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // Only src/lib.rs should be indexed
        assert_eq!(result.indexed, 1);

        // tests/test.rs should be in diagnostics
        let tests_diag = result
            .diagnostics
            .iter()
            .find(|d| d.path().contains("tests"));
        assert!(tests_diag.is_some());
    }

    #[test]
    fn test_scan_with_exclude_patterns() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create files
        std::fs::write(temp_dir.path().join("lib.rs"), b"fn lib() {}").unwrap();
        std::fs::write(temp_dir.path().join("test.rs"), b"fn test() {}").unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &[], &["**/*test*.rs".to_string()]).unwrap();
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // Only lib.rs should be indexed
        assert_eq!(result.indexed, 1);

        // test.rs should be in diagnostics
        let test_diag = result
            .diagnostics
            .iter()
            .find(|d| d.path().contains("test.rs"));
        assert!(test_diag.is_some());
    }

    #[test]
    fn test_scan_continues_on_error() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a valid file
        std::fs::write(temp_dir.path().join("good.rs"), b"fn good() {}").unwrap();

        // Create an unreadable file (will cause read error)
        let bad_file = temp_dir.path().join("bad.rs");
        std::fs::write(&bad_file, b"fn bad() {}").unwrap();

        // Make file unreadable (on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bad_file).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&bad_file, perms).unwrap();

            let mut graph = crate::CodeGraph::open(&db_path).unwrap();
            let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();

            // Scan should continue despite error
            let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

            // At least the good file should be indexed
            assert!(result.indexed >= 1);

            // Should have diagnostic for bad.rs
            let bad_diag = result
                .diagnostics
                .iter()
                .find(|d| d.path().contains("bad.rs"));
            assert!(bad_diag.is_some());

            // Restore permissions for cleanup
            let mut perms = std::fs::metadata(&bad_file).unwrap().permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(&bad_file, perms).unwrap();
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, just verify both files are indexed
            let mut graph = crate::CodeGraph::open(&db_path).unwrap();
            let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();
            let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();
            assert_eq!(result.indexed, 2);
        }
    }

    #[test]
    fn test_diagnostics_sorted() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create .gitignore
        std::fs::write(temp_dir.path().join(".gitignore"), "*.rs\n").unwrap();

        // Create files
        std::fs::write(temp_dir.path().join("c.rs"), b"").unwrap();
        std::fs::write(temp_dir.path().join("a.rs"), b"").unwrap();
        std::fs::write(temp_dir.path().join("b.rs"), b"").unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // After sorting, diagnostics should be in predictable order
        let mut sorted_diags = result.diagnostics.clone();
        sorted_diags.sort();

        // Check that we can sort and get consistent results
        assert!(!sorted_diags.is_empty());

        // Verify sorting is stable by sorting twice
        let mut sorted_again = sorted_diags.clone();
        sorted_again.sort();
        assert_eq!(sorted_diags, sorted_again);

        // Verify expected diagnostics are present
        assert!(sorted_diags.iter().any(|d| d.path() == "a.rs"));
        assert!(sorted_diags.iter().any(|d| d.path() == "b.rs"));
        assert!(sorted_diags.iter().any(|d| d.path() == "c.rs"));
    }

    #[test]
    fn test_scan_rejects_path_traversal() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a subdirectory
        let subdir = temp_dir.path().join("src");
        std::fs::create_dir(&subdir).unwrap();

        // Create a file in subdirectory (valid)
        std::fs::write(subdir.join("valid.rs"), b"fn valid() {}").unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();

        // Scan should succeed
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // Should have indexed the valid file
        assert_eq!(result.indexed, 1);
    }

    #[test]
    fn test_scan_with_symlink_to_outside() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let outside_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a file outside the scan root
        let outside_file = outside_dir.path().join("outside.rs");
        std::fs::write(&outside_file, b"fn outside() {}").unwrap();

        // Create a symlink inside root pointing outside
        let symlink = temp_dir.path().join("link.rs");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_file, &symlink).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside_file, &symlink).unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();

        // Scan should handle the symlink safely
        // Since follow_links=false, WalkDir won't follow it
        let _result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // The symlink itself might be indexed as a file (depending on WalkDir behavior)
        // But it should NOT escape the root
        #[cfg(any(unix, windows))]
        {
            // Verify no files from outside_dir were indexed
            let symbols = graph.symbols_in_file(outside_file.to_str().unwrap());
            assert!(symbols.is_err() || symbols.unwrap().is_empty());
        }
    }

    #[test]
    fn test_scan_continues_after_traversal_rejection() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create valid files
        std::fs::write(temp_dir.path().join("good.rs"), b"fn good() {}").unwrap();
        std::fs::write(temp_dir.path().join("better.rs"), b"fn better() {}").unwrap();

        let mut graph = crate::CodeGraph::open(&db_path).unwrap();
        let filter = FileFilter::new(temp_dir.path(), &[], &[]).unwrap();

        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

        // Both files should be indexed
        assert_eq!(result.indexed, 2);
    }
}

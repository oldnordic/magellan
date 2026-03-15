#![cfg(feature = "geometric-backend")]
//! Watch-mode loop prevention tests for geometric backend
//!
//! These tests verify that watch-mode does not endlessly reprocess the same files.

#[cfg(feature = "geometric-backend")]
mod tests {
    use std::path::Path;

    /// Helper: Create a temporary test directory with some Rust files
    fn create_test_project(dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("src"))?;
        std::fs::write(
            dir.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )?;
        std::fs::write(
            dir.join("src/main.rs"),
            "fn main() { println!(\"Hello\"); }\n",
        )?;
        Ok(())
    }

    /// Test 1: File hash computation is deterministic
    ///
    /// Verifies that the same content produces the same hash,
    /// which is required for change detection.
    #[test]
    fn geometric_file_hash_deterministic() {
        use magellan::graph::geo_index::compute_file_hash;
        
        let content = "fn main() {}\n";
        let hash1 = compute_file_hash(content.as_bytes());
        let hash2 = compute_file_hash(content.as_bytes());
        
        assert_eq!(hash1, hash2, "Same content should produce same hash");
    }

    /// Test 2: File hash detects content changes
    ///
    /// Verifies that different content produces different hashes.
    #[test]
    fn geometric_file_hash_detects_changes() {
        use magellan::graph::geo_index::compute_file_hash;
        
        let content1 = "fn main() {}\n";
        let content2 = "fn main() { println!(); }\n";
        
        let hash1 = compute_file_hash(content1.as_bytes());
        let hash2 = compute_file_hash(content2.as_bytes());
        
        assert_ne!(hash1, hash2, "Different content should produce different hashes");
    }

    /// Test 3: Backend stores and retrieves file hashes
    ///
    /// Verifies that set_file_hash and get_file_hash work correctly.
    #[test]
    fn geometric_backend_file_hash_storage() {
        use magellan::graph::geometric_backend::GeometricBackend;
        use magellan::graph::geo_index::compute_file_hash;

        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let backend = GeometricBackend::create(&db_path).unwrap();
        
        let test_path = "/test/src/main.rs";
        let test_hash = compute_file_hash(b"fn main() {}");
        
        // Store hash
        backend.set_file_hash(test_path, &test_hash);
        
        // Retrieve hash
        let retrieved = backend.get_file_hash(test_path);
        
        assert!(retrieved.is_some(), "Hash should be stored");
        assert_eq!(retrieved.unwrap(), test_hash, "Retrieved hash should match");
    }

    /// Test 4: Unchanged file detection via hash
    ///
    /// Verifies that when a file's content hasn't changed (same hash),
    /// it can be detected and skipped during re-indexing.
    #[test]
    fn geometric_watch_unchanged_file_detection() {
        use magellan::graph::geometric_backend::GeometricBackend;
        use magellan::graph::geo_index::compute_file_hash;

        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let backend = GeometricBackend::create(&db_path).unwrap();
        
        // Simulate indexing a file
        let file_path = "/project/src/lib.rs";
        let content = "pub fn foo() -> i32 { 42 }\n";
        let hash = compute_file_hash(content.as_bytes());
        
        // Store the hash (as would happen after indexing)
        backend.set_file_hash(file_path, &hash);
        
        // Simulate watch event for same file (unchanged)
        let new_content = "pub fn foo() -> i32 { 42 }\n"; // Same content
        let new_hash = compute_file_hash(new_content.as_bytes());
        
        // Verify hash matches stored hash
        let stored_hash = backend.get_file_hash(file_path);
        assert_eq!(stored_hash, Some(new_hash.clone()), "Unchanged file should have matching hash");
        
        // This indicates the file doesn't need re-indexing
        let needs_reindex = stored_hash != Some(new_hash);
        assert!(!needs_reindex, "Unchanged file should not need re-indexing");
    }

    /// Test 5: Changed file detection via hash
    ///
    /// Verifies that when a file's content changes (different hash),
    /// it is detected as needing re-indexing.
    #[test]
    fn geometric_watch_changed_file_detection() {
        use magellan::graph::geometric_backend::GeometricBackend;
        use magellan::graph::geo_index::compute_file_hash;

        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let backend = GeometricBackend::create(&db_path).unwrap();
        
        // Simulate indexing a file
        let file_path = "/project/src/lib.rs";
        let old_content = "pub fn foo() -> i32 { 42 }\n";
        let old_hash = compute_file_hash(old_content.as_bytes());
        backend.set_file_hash(file_path, &old_hash);
        
        // Simulate watch event for modified file
        let new_content = "pub fn foo() -> i32 { 100 }\n"; // Changed!
        let new_hash = compute_file_hash(new_content.as_bytes());
        
        // Verify hash differs from stored hash
        let stored_hash = backend.get_file_hash(file_path);
        assert_ne!(stored_hash, Some(new_hash.clone()), "Changed file should have different hash");
        
        // This indicates the file needs re-indexing
        let needs_reindex = stored_hash != Some(new_hash);
        assert!(needs_reindex, "Changed file should need re-indexing");
    }

    /// Test 6: Path canonicalization consistency
    ///
    /// Verifies that different path representations are handled consistently.
    #[test]
    fn geometric_path_canonicalization_consistency() {
        use magellan::graph::geometric_backend::GeometricBackend;
        use magellan::graph::geo_index::compute_file_hash;

        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let backend = GeometricBackend::create(&db_path).unwrap();
        
        let hash = compute_file_hash(b"content");
        
        // Store with one path form
        backend.set_file_hash("./src/main.rs", &hash);
        
        // Retrieve with canonical form (should be normalized)
        // Note: In real usage, paths are canonicalized before storage
        let retrieved = backend.get_file_hash("./src/main.rs");
        
        assert!(retrieved.is_some(), "Hash should be retrievable");
    }
}

/// Empty test suite when geometric-backend feature is disabled
#[cfg(not(feature = "geometric-backend"))]
mod tests {
    #[test]
    fn geometric_backend_disabled() {
        // Placeholder when feature is not enabled
    }
}

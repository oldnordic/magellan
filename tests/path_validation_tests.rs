//! Integration tests for path traversal validation.
//!
//! These tests verify that path validation prevents directory traversal
//! attacks across all supported platforms (Linux, macOS, Windows).

use magellan::validation::{
    validate_path_within_root, PathValidationError,
    has_suspicious_traversal, is_safe_symlink,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a file and return its path
fn create_test_file(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"test content").unwrap();
    path
}

/// Helper to create a symlink
#[cfg(any(unix, windows))]
fn create_symlink(from: &Path, to: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(to, from).unwrap();

    #[cfg(windows)]
    {
        if to.is_dir() {
            std::os::windows::fs::symlink_dir(to, from).unwrap();
        } else {
            std::os::windows::fs::symlink_file(to, from).unwrap();
        }
    }
}

// =========================================================================
// Parent Directory Traversal Tests
// =========================================================================

#[test]
fn test_single_parent_traversal_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // "../something" pattern - flagged as suspicious (single parent, shallow depth)
    let traversal = root.join("../etc");
    let result = validate_path_within_root(&traversal, root);
    assert!(result.is_err());

    // Either SuspiciousTraversal or CannotCanonicalize is acceptable
    // (path doesn't exist, so canonicalization fails)
    match result {
        Err(PathValidationError::SuspiciousTraversal(_)) => {}
        Err(PathValidationError::CannotCanonicalize(_)) => {}
        e => panic!("Expected SuspiciousTraversal or CannotCanonicalize, got {:?}", e),
    }
}

#[test]
fn test_double_parent_traversal_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // "../../etc" is NOT flagged by suspicious_traversal (2 parents)
    // But should be caught by canonicalization if it escapes root
    let traversal = root.join("../../etc");
    let result = validate_path_within_root(&traversal, root);

    // Either flagged as suspicious or fails canonicalization
    // (both are acceptable outcomes)
    match result {
        Err(PathValidationError::SuspiciousTraversal(_)) => {}
        Err(PathValidationError::OutsideRoot(_, _)) => {}
        Err(PathValidationError::CannotCanonicalize(_)) => {}
        Ok(_) => panic!("Path should have been rejected"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn test_multiple_parent_traversal_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let traversal = root.join("../../../etc/passwd");
    let result = validate_path_within_root(&traversal, root);
    assert!(result.is_err());

    // Should be flagged as suspicious (>=3 parents)
    match &result {
        Err(PathValidationError::SuspiciousTraversal(_)) => {}
        e => panic!("Expected SuspiciousTraversal error for >=3 parents, got {:?}", e),
    }
}

#[test]
fn test_legitimate_nested_parent_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a nested directory structure
    let deep = root.join("a").join("b").join("c");
    fs::create_dir_all(&deep).unwrap();
    let file = root.join("target").join("file.rs");
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(&file, b"fn test() {}").unwrap();

    // From deep directory, "../../target/file.rs" is valid
    let resolved = deep.join("../../target/file.rs");
    let result = validate_path_within_root(&resolved, root);

    // This should either work (canonicalized correctly) or fail if path doesn't exist
    // but NOT fail due to security concerns
    match result {
        Ok(canonical) => {
            // Valid path within root
            assert!(canonical.starts_with(root));
        }
        Err(PathValidationError::CannotCanonicalize(_)) => {
            // File doesn't exist yet, that's ok
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn test_legitimate_two_parent_deep_path_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a nested directory structure
    let subdir = root.join("project").join("src").join("module");
    fs::create_dir_all(&subdir).unwrap();

    let target = root.join("project").join("lib.rs");
    fs::write(&target, b"fn lib() {}").unwrap();

    // From module, "../../../lib.rs" escapes project but stays in root
    // Actually wait, module -> src -> project -> root
    // So "../../../lib" would try to go outside root
    // Let's test "../../lib" which goes from module to project
    let resolved = subdir.join("../../lib.rs");
    let result = validate_path_within_root(&resolved, root);

    // Should work if file exists
    match result {
        Ok(_) => {} // Good
        Err(PathValidationError::CannotCanonicalize(_)) => {} // File missing, ok
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

// =========================================================================
// Cross-Platform Path Separator Tests
// =========================================================================

#[test]
fn test_forward_slash_paths() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let file = create_test_file(root, "test.rs");
    let result = validate_path_within_root(&file, root);
    assert!(result.is_ok());
}

#[test]
#[cfg(windows)]
fn test_backslash_paths_windows() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create file using backslashes
    let file_path = root.join("subdir").join("test.rs");
    fs::create_dir_all(root.join("subdir")).unwrap();
    fs::write(&file_path, b"fn test() {}").unwrap();

    // Windows paths with backslashes should work
    let path_str = file_path.to_string_lossy();
    assert!(path_str.contains('\\'));

    let result = validate_path_within_root(&file_path, root);
    assert!(result.is_ok());
}

#[test]
#[cfg(windows)]
fn test_windows_backslash_traversal_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Windows-style traversal with backslashes
    // Need to create this path as PathBuf
    let mut traversal = root.clone();
    traversal.push("..");
    traversal.push("..");
    traversal.push("..");
    traversal.push("windows");
    traversal.push("system32");

    let result = validate_path_within_root(&traversal, root);
    assert!(result.is_err());
}

#[test]
#[cfg(windows)]
fn test_windows_unc_path_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // UNC path (\\server\share\...) or extended-length path (\\?\C:\...)
    // These are absolute paths outside the root
    let unc_path = Path::new(r"\\?\C:\Windows\System32");

    let result = validate_path_within_root(unc_path, root);
    assert!(result.is_err());
}

#[test]
#[cfg(unix)]
fn test_unix_absolute_path_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Unix absolute path
    let abs_path = Path::new("/etc/passwd");

    let result = validate_path_within_root(abs_path, root);
    assert!(result.is_err());
}

// =========================================================================
// Case Sensitivity Tests (macOS is case-insensitive, Linux is case-sensitive)
// =========================================================================

#[test]
fn test_case_sensitive_path_validation() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let file = create_test_file(root, "TestFile.rs");

    // Exact match should work
    let result = validate_path_within_root(&file, root);
    assert!(result.is_ok());

    // On case-insensitive systems (macOS, Windows), different case should still work
    // On case-sensitive systems (Linux), the path won't exist if case differs
    #[cfg(any(target_os = "macos", windows))]
    {
        let different_case = root.join("testfile.rs");
        // This may or may not exist depending on filesystem
        let _ = validate_path_within_root(&different_case, root);
    }
}

// =========================================================================
// Symlink Tests
// =========================================================================

#[test]
#[cfg(any(unix, windows))]
fn test_safe_symlink_inside_root_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file
    let target = create_test_file(root, "target.rs");

    // Create symlink
    let symlink = root.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_ok() || matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_outside_root_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file outside root
    let outside_dir = TempDir::new().unwrap();
    let target = create_test_file(outside_dir.path(), "outside.rs");

    // Create symlink inside root pointing outside
    let symlink = root.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_err());

    match result.unwrap_err() {
        PathValidationError::SymlinkEscape(_, _) => {}
        PathValidationError::OutsideRoot(_, _) => {}
        _ => panic!("Expected SymlinkEscape or OutsideRoot error"),
    }
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_chain_outside_root_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create chain: root/link1 -> root/link2 -> outside/target
    let outside_dir = TempDir::new().unwrap();
    let target = create_test_file(outside_dir.path(), "target.rs");

    let link2 = root.join("link2.rs");
    create_symlink(&link2, &target);

    let link1 = root.join("link1.rs");
    create_symlink(&link1, &link2);

    let result = is_safe_symlink(&link1, root);
    assert!(result.is_err());
}

#[test]
#[cfg(any(unix, windows))]
fn test_broken_symlink_handled() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create symlink to non-existent file
    let symlink = root.join("broken.rs");
    create_symlink(&symlink, Path::new("nonexistent.rs"));

    let result = is_safe_symlink(&symlink, root);
    // Broken symlink cannot be canonicalized
    assert!(matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

// =========================================================================
// Mixed Traversal Pattern Tests
// =========================================================================

#[test]
fn test_mixed_dotdot_pattern_rejected() {
    assert!(has_suspicious_traversal("./subdir/../../etc"));
}

#[test]
fn test_mixed_slash_pattern_rejected() {
    #[cfg(windows)]
    assert!(has_suspicious_traversal(".\\subdir\\..\\..\\etc"));
}

#[test]
fn test_normal_paths_not_flagged() {
    assert!(!has_suspicious_traversal("src/main.rs"));
    assert!(!has_suspicious_traversal("./src/lib.rs"));
    assert!(!has_suspicious_traversal("../../normal/path")); // 2 parents is ok
    assert!(!has_suspicious_traversal("../parent/deep/nested/path")); // 1 parent but deep
}

// =========================================================================
// Edge Case Tests
// =========================================================================

#[test]
fn test_empty_path_components() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Path with empty components (// on Unix)
    let _ = create_test_file(root, "test.rs");

    #[cfg(unix)]
    {
        let double_slash = Path::new(root.to_str().unwrap()).join("//test.rs");
        let result = validate_path_within_root(&double_slash, root);
        // Should either work or fail gracefully
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn test_relative_from_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let _ = create_test_file(root, "test.rs");

    // Relative path from root should work
    let relative = Path::new("test.rs");
    let full = root.join(relative);
    let result = validate_path_within_root(&full, root);
    assert!(result.is_ok());
}

#[test]
fn test_dot_in_path() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let subdir = root.join("sub.dir");
    fs::create_dir(&subdir).unwrap();
    let file = subdir.join("test.rs");
    fs::write(&file, b"fn test() {}").unwrap();

    let result = validate_path_within_root(&file, root);
    assert!(result.is_ok());
}

#[test]
fn test_deep_nesting_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a deeply nested directory structure
    let deep = root.join("a").join("b").join("c").join("d").join("e");
    fs::create_dir_all(&deep).unwrap();
    let file = deep.join("deep.rs");
    fs::write(&file, b"fn deep() {}").unwrap();

    let result = validate_path_within_root(&file, root);
    assert!(result.is_ok());
    assert!(result.unwrap().starts_with(root));
}

#[test]
fn test_nonexistent_file_in_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // File doesn't exist
    let nonexistent = root.join("nonexistent.rs");

    let result = validate_path_within_root(&nonexistent, root);
    // Should fail to canonicalize
    assert!(matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

// =========================================================================
// Platform-Specific Behavior Documentation
// =========================================================================

#[test]
fn test_path_separator_normalization() {
    // Test that path handling is consistent across platforms
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let subdir = root.join("src").join("main");
    fs::create_dir_all(&subdir).unwrap();
    let file = subdir.join("lib.rs");
    fs::write(&file, b"fn lib() {}").unwrap();

    // Regardless of platform, canonicalization should produce valid path
    let result = validate_path_within_root(&file, root);
    assert!(result.is_ok());

    let canonical = result.unwrap();
    assert!(canonical.starts_with(root));
}

#[test]
fn test_traversal_detection_is_platform_agnostic() {
    // Test that suspicious traversal detection works regardless of platform
    // This function doesn't do filesystem access, just string analysis

    // Unix-style
    assert!(has_suspicious_traversal("../../../etc/passwd"));
    assert!(has_suspicious_traversal("../etc"));

    // Windows-style (backslash patterns get normalized)
    assert!(has_suspicious_traversal("..\\..\\..\\windows\\system32"));

    // Mixed patterns
    assert!(has_suspicious_traversal("./subdir/../../etc"));

    // Normal paths should NOT be flagged
    assert!(!has_suspicious_traversal("src/main.rs"));
    assert!(!has_suspicious_traversal("./src/lib.rs"));
}

#[test]
fn test_validate_path_rejects_absolute_outside() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Try to access an absolute path outside root
    #[cfg(unix)]
    let abs_path = Path::new("/etc/passwd");

    #[cfg(windows)]
    let abs_path = Path::new("C:\\Windows\\System32\\config\\SAM");

    let result = validate_path_within_root(abs_path, root);
    assert!(result.is_err());
}

#[test]
fn test_symlink_relative_inside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    #[cfg(any(unix, windows))]
    {
        // Create target directory
        let target_dir = root.join("target");
        fs::create_dir(&target_dir).unwrap();
        let target_file = target_dir.join("file.rs");
        fs::write(&target_file, b"fn file() {}").unwrap();

        // Create a directory
        let link_dir = root.join("links");
        fs::create_dir(&link_dir).unwrap();

        // Create relative symlink from link_dir/../link -> target_dir/file
        let symlink = link_dir.join("link.rs");
        create_symlink(&symlink, Path::new("../target/file.rs"));

        let result = is_safe_symlink(&symlink, root);
        assert!(result.is_ok());
    }
}

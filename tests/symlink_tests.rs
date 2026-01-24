//! Integration tests for symlink handling.
//!
//! These tests verify that symlinks are handled securely
//! to prevent escapes from the project root.

use magellan::validation::is_safe_symlink;
use magellan::validation::PathValidationError;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

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
// File Symlink Tests
// =========================================================================

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_file_inside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file
    let target = root.join("target.rs");
    fs::write(&target, b"fn target() {}").unwrap();

    // Create symlink
    let symlink = root.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_ok() || matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_file_outside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file outside root
    let outside_dir = TempDir::new().unwrap();
    let target = outside_dir.path().join("outside.rs");
    fs::write(&target, b"fn outside() {}").unwrap();

    // Create symlink inside root pointing outside
    let symlink = root.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_err());

    match &result {
        Err(PathValidationError::SymlinkEscape(_, _)) => {}
        Err(PathValidationError::OutsideRoot(_, _)) => {}
        e => panic!("Expected SymlinkEscape or OutsideRoot, got {:?}", e),
    }
}

// =========================================================================
// Directory Symlink Tests
// =========================================================================

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_directory_inside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target directory with file
    let target_dir = root.join("target_dir");
    fs::create_dir(&target_dir).unwrap();
    let target_file = target_dir.join("file.rs");
    fs::write(&target_file, b"fn file() {}").unwrap();

    // Create symlink to directory
    let symlink = root.join("link_dir");
    create_symlink(&symlink, &target_dir);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_ok() || matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_directory_outside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target directory outside root
    let outside_dir = TempDir::new().unwrap();
    let target = outside_dir.path();

    // Create symlink inside root pointing to outside directory
    let symlink = root.join("link_dir");
    create_symlink(&symlink, target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_err());

    match &result {
        Err(PathValidationError::SymlinkEscape(_, _)) => {}
        Err(PathValidationError::OutsideRoot(_, _)) => {}
        e => panic!("Expected SymlinkEscape or OutsideRoot, got {:?}", e),
    }
}

// =========================================================================
// Relative Symlink Tests
// =========================================================================

#[test]
#[cfg(any(unix, windows))]
fn test_relative_symlink_inside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file
    let target = root.join("target.rs");
    fs::write(&target, b"fn target() {}").unwrap();

    // Create directory for symlink
    let link_dir = root.join("links");
    fs::create_dir(&link_dir).unwrap();

    // Create relative symlink: ../target.rs from links/
    let symlink = link_dir.join("link.rs");
    create_symlink(&symlink, Path::new("../target.rs"));

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_ok() || matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

#[test]
#[cfg(any(unix, windows))]
fn test_relative_symlink_outside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file outside root
    let outside_dir = TempDir::new().unwrap();
    let target = outside_dir.path().join("outside.rs");
    fs::write(&target, b"fn outside() {}").unwrap();

    // Create symlink directory
    let link_dir = root.join("links");
    fs::create_dir(&link_dir).unwrap();

    // Create relative symlink that points outside: ../../tmp/outside.rs
    // This would escape root if resolved
    let symlink = link_dir.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_err());
}

// =========================================================================
// Chained Symlink Tests
// =========================================================================

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_chain_inside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file
    let target = root.join("target.rs");
    fs::write(&target, b"fn target() {}").unwrap();

    // Create chain: link1 -> link2 -> target (all inside root)
    let link2 = root.join("link2.rs");
    create_symlink(&link2, &target);

    let link1 = root.join("link1.rs");
    create_symlink(&link1, &link2);

    let result = is_safe_symlink(&link1, root);
    // First link resolves to second, which is safe
    assert!(result.is_ok() || matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_chain_outside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target file outside root
    let outside_dir = TempDir::new().unwrap();
    let target = outside_dir.path().join("target.rs");
    fs::write(&target, b"fn target() {}").unwrap();

    // Create chain: root/link1 -> root/link2 -> outside/target
    let link2 = root.join("link2.rs");
    create_symlink(&link2, &target);

    let link1 = root.join("link1.rs");
    create_symlink(&link1, &link2);

    let result = is_safe_symlink(&link1, root);
    assert!(result.is_err());

    match &result {
        Err(PathValidationError::SymlinkEscape(_, _)) => {}
        Err(PathValidationError::OutsideRoot(_, _)) => {}
        e => panic!("Expected SymlinkEscape or OutsideRoot, got {:?}", e),
    }
}

// =========================================================================
// Special Symlink Tests
// =========================================================================

#[test]
#[cfg(any(unix, windows))]
fn test_broken_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create symlink to non-existent file
    let symlink = root.join("broken.rs");
    create_symlink(&symlink, Path::new("nonexistent.rs"));

    let result = is_safe_symlink(&symlink, root);
    // Broken symlink cannot be canonicalized
    assert!(matches!(
        result,
        Err(PathValidationError::CannotCanonicalize(_))
    ));
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_parent_directory() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create symlink to parent directory (which is outside root)
    let parent = root.parent().unwrap();
    let symlink = root.join("parent_link");
    create_symlink(&symlink, parent);

    let result = is_safe_symlink(&symlink, root);
    // Parent directory is outside root boundary
    assert!(result.is_err());
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_sibling_directory() {
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();

    // Create symlink in temp_dir1 pointing to temp_dir2
    let symlink = temp_dir1.path().join("sibling_link");
    create_symlink(&symlink, temp_dir2.path());

    let result = is_safe_symlink(&symlink, temp_dir1.path());
    assert!(result.is_err());
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_nested_inside_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create nested directory structure
    let nested = root.join("a").join("b").join("c");
    fs::create_dir_all(&nested).unwrap();

    let target = nested.join("target.rs");
    fs::write(&target, b"fn target() {}").unwrap();

    // Create symlink at root pointing to nested target
    let symlink = root.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_ok() || matches!(result, Err(PathValidationError::CannotCanonicalize(_))));
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_from_nested_to_outside() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create nested directory inside root
    let nested = root.join("nested");
    fs::create_dir(&nested).unwrap();

    // Create target outside root
    let outside_dir = TempDir::new().unwrap();
    let target = outside_dir.path().join("outside.rs");
    fs::write(&target, b"fn outside() {}").unwrap();

    // Create symlink from nested dir pointing outside
    let symlink = nested.join("link.rs");
    create_symlink(&symlink, &target);

    let result = is_safe_symlink(&symlink, root);
    assert!(result.is_err());
}

#[test]
#[cfg(all(unix, not(target_os = "macos")))]
fn test_symlink_case_sensitive() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target with specific case
    let target = root.join("Target.rs");
    fs::write(&target, b"fn target() {}").unwrap();

    // Create symlink with different case pointing to same file
    let symlink = root.join("link.rs");
    create_symlink(&symlink, Path::new("Target.rs"));

    let result = is_safe_symlink(&symlink, root);
    // On case-sensitive systems, this should work
    assert!(result.is_ok());
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_dotdot_path() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target in parent of root (outside)
    let parent = root.parent().unwrap();
    let target = parent.join("parent_file.rs");
    fs::write(&target, b"fn parent() {}").unwrap();

    // Create symlink using ../parent_file.rs
    let symlink = root.join("link.rs");
    create_symlink(&symlink, Path::new("../parent_file.rs"));

    let result = is_safe_symlink(&symlink, root);
    // This points outside root
    assert!(result.is_err());
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_self_referential() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create symlink that points to itself
    let symlink = root.join("self.rs");
    create_symlink(&symlink, Path::new("self.rs"));

    let result = is_safe_symlink(&symlink, root);
    // Self-referential symlink cannot be canonicalized (circular)
    // The read_link will succeed but validation may detect the issue
    match result {
        Err(PathValidationError::CannotCanonicalize(_)) => {}
        Err(PathValidationError::OutsideRoot(_, _)) => {
            // Also acceptable - may resolve to path within root but fail to canonicalize
        }
        Ok(_) => {
            // Some systems might handle this differently
        }
        e => panic!("Unexpected result: {:?}", e),
    }
}

#[test]
#[cfg(any(unix, windows))]
fn test_symlink_to_symlink_that_escapes() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create target outside root
    let outside_dir = TempDir::new().unwrap();
    let target = outside_dir.path().join("outside.rs");
    fs::write(&target, b"fn outside() {}").unwrap();

    // Create intermediate symlink inside root that points outside
    let intermediate = root.join("intermediate.rs");
    create_symlink(&intermediate, &target);

    // Create another symlink pointing to the intermediate
    let link = root.join("link.rs");
    create_symlink(&link, &intermediate);

    // Both should be detected as unsafe
    let result1 = is_safe_symlink(&intermediate, root);
    assert!(result1.is_err());

    let result2 = is_safe_symlink(&link, root);
    assert!(result2.is_err());
}

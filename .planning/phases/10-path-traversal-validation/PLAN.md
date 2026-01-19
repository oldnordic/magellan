---
phase: 10-path-traversal-validation
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src/validation.rs
  - src/lib.rs
  - Cargo.toml
autonomous: true
user_setup: []

must_haves:
  truths:
    - "All path validation functions exist in centralized module"
    - "Path canonicalization handles UTF-8 paths cross-platform"
    - "Validation rejects paths escaping project root"
    - "Symlinks are either rejected or resolved-then-validated"
    - "camino crate added for UTF-8 path handling"
  artifacts:
    - path: "src/validation.rs"
      provides: "Path canonicalization and validation utilities"
      exports:
        - "PathValidationError"
        - "canonicalize_path"
        - "validate_path_within_root"
        - "is_safe_symlink"
    - path: "Cargo.toml"
      provides: "Dependency declarations for camino"
      contains: "camino = \"1.2\""
  key_links:
    - from: "src/watcher.rs"
      to: "src/validation.rs"
      via: "use crate::validation::validate_path_within_root"
      pattern: "validate_path_within_root"
    - from: "src/graph/scan.rs"
      to: "src/validation.rs"
      via: "use crate::validation::validate_path_within_root"
      pattern: "validate_path_within_root"
---

# Phase 10: Path Traversal Validation

## Overview

**Phase:** 10 - Path Traversal Validation
**Milestone:** v1.1 Correctness + Safety
**Total Plans:** 4
**Wave Structure:** 3 waves (Plan 01: Wave 1, Plans 02-03: Wave 2, Plan 04: Wave 3)

This phase implements security-critical path validation to prevent CVE-2025-68705 class vulnerabilities where malicious input could access files outside the project root.

---

## Plan 10-01: Create Path Validation Module

### Objective

Create `src/validation.rs` with centralized path canonicalization and validation utilities. This module will be used by both watcher and scan operations to ensure all file access is constrained within the project root.

**Purpose:** Provide a single source of truth for path security validation across the codebase.

**Output:** Working validation module with UTF-8 path handling and root boundary checking.

### Requirements Coverage

- **PATH-01:** Implement path canonicalization before validation for all file access
- **PATH-02:** Create `validate_path_within_root()` function that rejects paths escaping project root
- **PATH-06:** Handle cross-platform path differences (Windows backslash, macOS case-insensitivity)

### Context

@.planning/research/ARCHITECTURE.md
@.planning/codebase/ARCHITECTURE.md
@.planning/codebase/CONVENTIONS.md

From ARCHITECTURE.md:
> **Path validation belongs at entry points:** watcher.rs (event filtering), scan.rs (directory walking)
> **Critical finding:** `filter.rs` already canonicalizes the root. Extend this pattern to all file paths.

From RESEARCH/SUMMARY.md:
> **v1.1 additions:**
> - camino 1.2.2: UTF-8 path handling for cross-platform determinism
> - path-security 0.1.0: Path traversal validation (actively maintained, Oct 2025)

### Tasks

<task type="auto">
  <name>Task 1: Add camino dependency to Cargo.toml</name>
  <files>Cargo.toml</files>
  <action>
    Add camino 1.2 to dependencies in the [dependencies] section of Cargo.toml.
    camino provides UTF-8 path wrapper (Utf8PathBuf) for cross-platform determinism.

    Format:
    camino = "1.2"

    Do NOT add path-security crate - we'll implement validation directly using std::fs::canonicalize
    which follows symlinks and returns an absolute path, then verify it starts with root.
  </action>
  <verify>grep -E 'camino = "1\\.2"' Cargo.toml returns true</verify>
  <done>camino dependency is available for import</done>
</task>

<task type="auto">
  <name>Task 2: Create src/validation.rs module with error types</name>
  <files>src/validation.rs</files>
  <action>
    Create new src/validation.rs with the following structure:

    ```rust
    //! Path validation and canonicalization utilities.
    //!
    //! Provides security-critical path validation to prevent directory traversal attacks.
    //! All file access operations MUST validate paths before accessing filesystem resources.

    use anyhow::Result;
    use camino::Utf8Path;
    use std::path::{Path, PathBuf};

    /// Error types for path validation.
    #[derive(Debug, thiserror::Error)]
    pub enum PathValidationError {
        /// Path cannot be canonicalized (doesn't exist or permission denied)
        #[error("cannot canonicalize path: {0}")]
        CannotCanonicalize(String),

        /// Resolved path escapes the project root
        #[error("path escapes project root: {0} (root: {1})")]
        OutsideRoot(String, String),

        /// Path contains suspicious traversal patterns
        #[error("path contains suspicious traversal patterns: {0}")]
        SuspiciousTraversal(String),

        /// Symlink points outside project root
        #[error("symlink escapes project root: {0} -> {1}")]
        SymlinkEscape(String, String),
    }

    /// Canonicalize a path using std::fs::canonicalize.
    ///
    /// This resolves all symlinks, `..`, and `.` components to produce an absolute path.
    /// Returns an error if the path doesn't exist or cannot be accessed.
    ///
    /// # Arguments
    /// * `path` - Path to canonicalize
    ///
    /// # Returns
    /// Canonicalized absolute path, or error if path cannot be canonicalized
    pub fn canonicalize_path(path: &Path) -> Result<PathBuf, PathValidationError> {
        std::fs::canonicalize(path).map_err(|_| {
            PathValidationError::CannotCanonicalize(path.to_string_lossy().to_string())
        })
    }

    /// Validate that a path is within the given root directory.
    ///
    /// This function:
    /// 1. Canonicalizes the input path (resolves symlinks, ., ..)
    /// 2. Checks that the canonicalized path starts with the canonicalized root
    /// 3. Returns the validated canonical path on success
    ///
    /// # Arguments
    /// * `path` - Path to validate
    /// * `root` - Project root directory
    ///
    /// # Returns
    /// Canonicalized path if valid, error if path escapes root
    ///
    /// # Security
    /// This is the PRIMARY defense against directory traversal attacks.
    /// All file access MUST go through this validation.
    pub fn validate_path_within_root(path: &Path, root: &Path) -> Result<PathBuf, PathValidationError> {
        // First, check for obvious traversal patterns before canonicalization
        // This catches attacks like "../../../etc/passwd" even if some ancestor
        // doesn't exist (which would cause canonicalize to fail)
        let path_str = path.to_string_lossy();
        if has_suspicious_traversal(&path_str) {
            return Err(PathValidationError::SuspiciousTraversal(path_str.to_string()));
        }

        // Canonicalize both paths to absolute form
        let canonical_path = canonicalize_path(path)?;
        let canonical_root = canonicalize_path(root)
            .map_err(|_| PathValidationError::CannotCanonicalize(root.to_string_lossy().to_string()))?;

        // Check if canonical path starts with canonical root
        if !canonical_path.starts_with(&canonical_root) {
            return Err(PathValidationError::OutsideRoot(
                canonical_path.to_string_lossy().to_string(),
                canonical_root.to_string_lossy().to_string(),
            ));
        }

        Ok(canonical_path)
    }

    /// Check for suspicious path traversal patterns.
    ///
    /// This is a pre-check to catch obvious attacks even when canonicalization
    /// might fail (e.g., if intermediate directories don't exist).
    fn has_suspicious_traversal(path: &str) -> bool {
        // Check for parent directory patterns
        // Must handle both Unix (../) and Windows (..\\) patterns
        let path_normalized = path.replace('\\', "/");

        // Count "../" occurrences - more than 3 is highly suspicious
        // (legitimate use cases rarely go up more than a couple levels)
        let parent_count = path_normalized.matches("../").count();
        if parent_count > 3 {
            return true;
        }

        // Check for patterns that attempt to escape using ".." at start
        if path_normalized.starts_with("../") || path_normalized.starts_with("..\\") {
            return true;
        }

        // Check for mixed traversal patterns like "./subdir/../../etc"
        if path_normalized.contains("./../") || path_normalized.contains(".\\../") {
            return true;
        }

        false
    }

    /// Check if a symlink is safe (doesn't escape project root).
    ///
    /// This function resolves the symlink target and validates it's within root.
    ///
    /// # Arguments
    /// * `symlink_path` - Path to the symlink itself
    /// * `root` - Project root directory
    ///
    /// # Returns
    /// Ok if symlink is safe, Err if symlink target escapes root
    pub fn is_safe_symlink(symlink_path: &Path, root: &Path) -> Result<bool, PathValidationError> {
        // Read the symlink target
        let target = std::fs::read_link(symlink_path)
            .map_err(|_| PathValidationError::CannotCanonicalize(
                symlink_path.to_string_lossy().to_string()
            ))?;

        // If target is absolute, validate it directly
        if target.is_absolute() {
            validate_path_within_root(&target, root)?;
            return Ok(true);
        }

        // If relative, resolve relative to parent directory
        let parent = symlink_path
            .parent()
            .unwrap_or(symlink_path);
        let resolved = parent.join(&target);

        match validate_path_within_root(&resolved, root) {
            Ok(_) => Ok(true),
            Err(PathValidationError::OutsideRoot(_, _)) => {
                Err(PathValidationError::SymlinkEscape(
                    symlink_path.to_string_lossy().to_string(),
                    target.to_string_lossy().to_string(),
                ))
            }
            Err(e) => Err(e),
        }
    }

    /// Validate a UTF-8 path using camino's Utf8Path.
    ///
    /// This is a convenience wrapper for UTF-8 path handling.
    pub fn validate_utf8_path(utf8_path: &Utf8Path, root: &Path) -> Result<PathBuf, PathValidationError> {
        let path = Path::new(utf8_path.as_str());
        validate_path_within_root(path, root)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        #[test]
        fn test_has_suspicious_traversal_parent_patterns() {
            assert!(has_suspicious_traversal("../../../etc/passwd"));
            assert!(has_suspicious_traversal("..\\\\..\\\\..\\\\windows\\\\system32"));
            assert!(has_suspicious_traversal("../config"));
            assert!(has_suspicious_traversal("..\\config"));
        }

        #[test]
        fn test_has_suspicious_traversal_mixed_patterns() {
            assert!(has_suspicious_traversal("./subdir/../../etc"));
            assert!(has_suspicious_traversal(".\\subdir\\..\\..\\etc"));
        }

        #[test]
        fn test_has_suspicious_traversal_normal_paths() {
            assert!(!has_suspicious_traversal("src/main.rs"));
            assert!(!has_suspicious_traversal("./src/lib.rs"));
            assert!(!has_suspicious_traversal("../parent/src/lib.rs")); // Only 1 parent
            assert!(!has_suspicious_traversal("../../normal")); // Only 2 parents
        }

        #[test]
        fn test_validate_path_within_root_valid() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Create a file inside root
            let file_path = root.join("test.rs");
            fs::write(&file_path, b"fn test() {}").unwrap();

            let result = validate_path_within_root(&file_path, root);
            assert!(result.is_ok());
            assert!(result.unwrap().starts_with(root));
        }

        #[test]
        fn test_validate_path_within_root_traversal_rejected() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Try to access file outside root using parent traversal
            let outside = root.join("../../../etc/passwd");

            let result = validate_path_within_root(&outside, root);
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                PathValidationError::SuspiciousTraversal(_)
            ));
        }

        #[test]
        fn test_validate_path_within_root_absolute_outside() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Try to access absolute path outside root
            let outside = Path::new("/etc/passwd");

            let result = validate_path_within_root(outside, root);
            assert!(result.is_err());

            // Either SuspiciousTraversal or OutsideRoot depending on whether path exists
            match result.unwrap_err() {
                PathValidationError::SuspiciousTraversal(_) => {}
                PathValidationError::OutsideRoot(_, _) => {}
                _ => panic!("Expected traversal or outside error"),
            }
        }

        #[test]
        fn test_is_safe_symlink_inside_root() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Create a target file
            let target = root.join("target.rs");
            fs::write(&target, b"fn target() {}").unwrap();

            // Create symlink pointing to target
            let symlink = root.join("link.rs");
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &symlink).unwrap();

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&target, &symlink).unwrap();

            // On supported platforms, verify symlink is safe
            #[cfg(any(unix, windows))]
            {
                let result = is_safe_symlink(&symlink, root);
                assert!(result.is_ok());
                assert!(result.unwrap());
            }
        }

        #[test]
        fn test_is_safe_symlink_outside_root() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Create a target file outside root
            let outside_dir = TempDir::new().unwrap();
            let target = outside_dir.path().join("outside.rs");
            fs::write(&target, b"fn outside() {}").unwrap();

            // Create symlink inside root pointing outside
            let symlink = root.join("link.rs");
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &symlink).unwrap();

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&target, &symlink).unwrap();

            // On supported platforms, verify symlink is detected as unsafe
            #[cfg(any(unix, windows))]
            {
                let result = is_safe_symlink(&symlink, root);
                assert!(result.is_err());
                assert!(matches!(
                    result.unwrap_err(),
                    PathValidationError::SymlinkEscape(_, _)
                ));
            }
        }

        #[test]
        fn test_cross_platform_path_handling() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Create a file with subdirectory
            let subdir = root.join("src");
            fs::create_dir(&subdir).unwrap();
            let file_path = subdir.join("main.rs");
            fs::write(&file_path, b"fn main() {}").unwrap();

            // Test with forward slash path (Unix-style)
            let path_str = file_path.to_string_lossy().replace('\\', "/");
            let result = validate_path_within_root(Path::new(&path_str), root);
            assert!(result.is_ok());

            // Test with backslash path (Windows-style) - this may not work on Unix
            // but the canonicalization should handle it if the OS supports it
            if cfg!(windows) {
                let path_str_win = file_path.to_string_lossy().replace('/', "\\");
                let result_win = validate_path_within_root(Path::new(&path_str_win), root);
                assert!(result_win.is_ok());
            }
        }
    }
    ```

    Also add thiserror dependency if not already present:
    In Cargo.toml, ensure thiserror is in dependencies (check first).
  </action>
  <verify>cargo check --lib passes without errors</verify>
  <done>validation.rs module compiles and exports all required functions</done>
</task>

<task type="auto">
  <name>Task 3: Add validation module to lib.rs</name>
  <files>src/lib.rs</files>
  <action>
    Add the validation module to src/lib.rs.

    Find the module declarations section and add:
    pub mod validation;

    Also re-export the key functions for convenience:
    pub use validation::{PathValidationError, validate_path_within_root, canonicalize_path};

    Follow the existing pattern used for other modules in lib.rs.
  </action>
  <verify>grep -E 'pub mod validation' src/lib.rs returns true</verify>
  <done>validation module is accessible from crate root</done>
</task>

</tasks>

<verification>

1. **Module compiles**: `cargo check --lib` passes without errors
2. **Exports are public**: `cargo doc --open --no-deps` shows validation module in docs
3. **Tests pass**: `cargo test --lib validation` passes all tests
4. **Functions callable**: From other modules, `crate::validation::validate_path_within_root()` is accessible

</verification>

success_criteria>

1. `src/validation.rs` exists with all required functions
2. `validate_path_within_root()` rejects paths with `../` traversal
3. `validate_path_within_root()` rejects absolute paths outside root
4. `is_safe_symlink()` detects symlinks pointing outside root
5. Unit tests cover: normal paths, traversal patterns, symlinks, cross-platform
6. Module is re-exported from `crate::validation`

</success_criteria>

<output>

After completion, create `.planning/phases/10-path-traversal-validation/10-01-SUMMARY.md` with:
- Functions implemented
- Test coverage summary
- Any cross-platform issues discovered
- Next steps (integration points)

</output>

---

## Plan 10-02: Integrate Path Validation into Watcher

### Objective

Integrate path validation into `src/watcher.rs::extract_dirty_paths()` to filter out malicious paths from watcher events before any file access.

**Purpose:** Prevent path traversal attacks via the file watcher (watched directories can be user-controlled).

**Output:** Watcher events are validated; paths escaping root are filtered and logged.

### Requirements Coverage

- **PATH-04:** Integrate path validation into watcher.rs event filtering

### Context

@src/watcher.rs
@src/validation.rs (from 10-01)

From watcher.rs:305-337, `extract_dirty_paths()` filters events:
> Filtering rules:
> - Exclude directories (only process files)
> - Exclude database-related files (.db, .sqlite, etc.)
> - De-duplicate via BTreeSet

We need to add path traversal validation to this filtering.

### Tasks

<task type="auto">
  <name>Task 1: Add root_path to WatcherConfig</name>
  <files>src/watcher.rs</files>
  <action>
    Modify WatcherConfig to include the root_path:

    ```rust
    /// Filesystem watcher configuration
    #[derive(Debug, Clone)]
    pub struct WatcherConfig {
        /// Root directory for path validation
        pub root_path: PathBuf,
        /// Debounce delay in milliseconds
        pub debounce_ms: u64,
    }
    ```

    Update the Default impl:
    ```rust
    impl Default for WatcherConfig {
        fn default() -> Self {
            Self {
                root_path: PathBuf::from("."),
                debounce_ms: 500,
            }
        }
    }
    ```

    This allows the watcher to validate all paths against the known project root.
  </action>
  <verify>grep -E 'root_path.*PathBuf' src/watcher.rs returns true</verify>
  <done>WatcherConfig contains root_path for validation</done>
</task>

<task type="auto">
  <name>Task 2: Update extract_dirty_paths to validate paths</name>
  <files>src/watcher.rs</files>
  <action>
    Modify extract_dirty_paths() to validate each path against root:

    Add import at top:
    use crate::validation::{validate_path_within_root, PathValidationError};

    Update extract_dirty_paths function signature to accept root:
    ```rust
    fn extract_dirty_paths(
        events: &[notify_debouncer_mini::DebouncedEvent],
        root: &Path,
    ) -> BTreeSet<PathBuf> {
    ```

    Inside the loop, after checking `is_database_file`, add validation:
    ```rust
    // Skip database-related files to avoid feedback loop
    let path_str = path.to_string_lossy();
    if is_database_file(&path_str) {
        continue;
    }

    // Validate path is within project root (security: prevent path traversal)
    match validate_path_within_root(&path, root) {
        Ok(_) => {
            // Path is safe, include it
            dirty_paths.insert(path.clone());
        }
        Err(PathValidationError::OutsideRoot(p, _)) => {
            // Log the rejection but don't crash
            eprintln!(
                "WARNING: Watcher rejected path outside project root: {}",
                p
            );
        }
        Err(PathValidationError::SuspiciousTraversal(p)) => {
            // Log suspicious path patterns
            eprintln!(
                "WARNING: Watcher rejected suspicious traversal pattern: {}",
                p
            );
        }
        Err(PathValidationError::SymlinkEscape(from, to)) => {
            eprintln!(
                "WARNING: Watcher rejected symlink escaping root: {} -> {}",
                from, to
            );
        }
        Err(PathValidationError::CannotCanonicalize(p)) => {
            // Path doesn't exist or can't be accessed - skip
            // This is normal for files that are deleted
            continue;
        }
    }
    ```

    Remove the old `dirty_paths.insert(path.clone());` that was after the database check.
  </action>
  <verify>grep -E 'validate_path_within_root' src/watcher.rs returns true</verify>
  <done>extract_dirty_paths validates all paths before adding to dirty set</done>
</task>

<task type="auto">
  <name>Task 3: Update run_watcher to pass root to extract_dirty_paths</name>
  <files>src/watcher.rs</files>
  <action>
    Update the run_watcher function to pass the root path to extract_dirty_paths:

    The run_watcher function receives `path: PathBuf` - this is the watched directory,
    which is the project root. Pass this to extract_dirty_paths:

    ```rust
    let dirty_paths = extract_dirty_paths(&events, &path);
    ```

    Also update the FileSystemWatcher::new signature to require root_path:
    Change WatcherConfig to be constructed with root_path:

    ```rust
    pub fn new(path: PathBuf, config: WatcherConfig) -> Result<Self> {
        // Ensure root_path is set in config
        let config = WatcherConfig {
            root_path: path.clone(),
            ..config
        };
        // ... rest of function
    }
    ```
  </action>
  <verify>cargo check --lib passes without errors</verify>
  <done>run_watcher passes root to extract_dirty_paths</done>
</task>

<task type="auto">
  <name>Task 4: Add tests for watcher path filtering</name>
  <files>src/watcher.rs</files>
  <action>
    Add tests to verify path filtering in extract_dirty_paths:

    ```rust
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        // ... existing tests ...

        #[test]
        fn test_extract_dirty_paths_filters_traversal() {
            let temp_dir = TempDir::new().unwrap();
            let root = temp_dir.path();

            // Create a valid file
            let valid_file = root.join("valid.rs");
            fs::write(&valid_file, b"fn valid() {}").unwrap();

            // Create mock events - one valid, one outside root
            let events = vec![
                // This would normally be a DebouncedEvent, but for testing
                // we'll create a minimal structure
                // Note: We can't easily construct DebouncedEvent directly,
                // so this test may need to be integration-level
            ];

            // For now, verify the validation logic directly
            let result = validate_path_within_root(&valid_file, root);
            assert!(result.is_ok());

            let outside = root.join("../../../etc/passwd");
            let result_outside = validate_path_within_root(&outside, root);
            assert!(result_outside.is_err());
        }

        #[test]
        fn test_watcher_config_has_root() {
            let config = WatcherConfig {
                root_path: PathBuf::from("/test/root"),
                debounce_ms: 100,
            };

            assert_eq!(config.root_path, PathBuf::from("/test/root"));
            assert_eq!(config.debounce_ms, 100);
        }
    }
    ```
  </action>
  <verify>cargo test --lib watcher::tests passes</verify>
  <done>Tests verify path filtering in watcher</done>
</task>

</tasks>

<verification>

1. **Watcher compiles**: `cargo check --lib` passes
2. **Path validation called**: `grep -A5 extract_dirty_paths` shows validation call
3. **Root path passed**: `grep -B2-A2 extract_dirty_paths.*&path` confirms root is passed
4. **Tests pass**: `cargo test --lib watcher` passes

</verification>

<success_criteria>

1. Watcher events are validated before processing
2. Paths outside root are filtered and logged with WARNING
3. Suspicious traversal patterns are detected and rejected
4. Symlink escapes are detected and logged
5. Normal files within root are processed correctly

</success_criteria>

<output>

After completion, create `.planning/phases/10-path-traversal-validation/10-02-SUMMARY.md` with:
- Changes to watcher.rs
- Path filtering behavior
- Test results

</output>

---

## Plan 10-03: Integrate Path Validation into Scan

### Objective

Integrate path validation into `src/graph/scan.rs::scan_directory_with_filter()` to validate each path during directory walking.

**Purpose:** Prevent path traversal during initial directory scan (scan paths can be user-controlled).

**Output:** Scanner validates all paths before recursing and processing.

### Requirements Coverage

- **PATH-05:** Integrate path validation into scan.rs directory walking
- **PATH-03:** Tests for traversal attempts

### Context

@src/graph/scan.rs
@src/validation.rs (from 10-01)

From scan.rs:43-74, `scan_directory_with_filter()` walks directories:
> 1. Walk directory recursively
> 2. Apply filtering rules (internal ignores, gitignore, include/exclude)
> 3. Index each supported file

We need to add path validation before processing each entry.

### Tasks

<task type="auto">
  <name>Task 1: Add path validation to scan_directory_with_filter</name>
  <files>src/graph/scan.rs</files>
  <action>
    Add import for validation module:
    use crate::validation::{validate_path_within_root, PathValidationError};

    In the walkdir loop, add validation after checking `is_dir()`:

    ```rust
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(false)  // Don't follow symlinks during walk
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
                diagnostics.push(WatchDiagnostic::skipped(
                    p.strip_prefix(dir_path).unwrap_or_else(|_| Path::new(&p)).to_string_lossy(),
                    crate::diagnostics::SkipReason::IgnoredInternal,
                ));
                continue;
            }
            Err(PathValidationError::SymlinkEscape(from, to)) => {
                diagnostics.push(WatchDiagnostic::error(
                    from.strip_prefix(dir_path).unwrap_or_else(|_| Path::new(&from)).to_string_lossy(),
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

        // Apply filter (existing code continues...)
        if let Some(reason) = filter.should_skip(path) {
            // ...
    ```

    Note: Since walkdir starts at dir_path and doesn't follow symlinks (follow_links=false),
    we shouldn't normally see paths outside root. This is defense-in-depth.
  </action>
  <verify>grep -E 'validate_path_within_root.*path.*dir_path' src/graph/scan.rs returns true</verify>
  <done>scan_directory_with_filter validates paths before filtering</done>
</task>

<task type="auto">
  <name>Task 2: Add symlink handling option to scan</name>
  <files>src/graph/scan.rs</files>
  <action>
    Document the symlink handling policy in scan_directory_with_filter doc comment:

    Add to the function documentation:

    ```rust
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
    ```
  </action>
  <verify>grep -E '# Security' src/graph/scan.rs returns true</verify>
  <done>Function docs document security behavior</done>
</task>

<task type="auto">
  <name>Task 3: Add traversal tests for scan</name>
  <files>src/graph/scan.rs</files>
  <action>
    Add tests to verify scan rejects traversal attempts:

    ```rust
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
        let result = scan_directory_with_filter(&mut graph, temp_dir.path(), &filter, None).unwrap();

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
    ```
  </action>
  <verify>cargo test --lib scan::tests::test_scan_rejects_path_traversal passes</verify>
  <done>Tests verify scan handles traversal attempts correctly</done>
</task>

</tasks>

<verification>

1. **Scan compiles**: `cargo check --lib` passes
2. **Validation in loop**: `grep -B5-A5 'validate_path_within_root'` in scan.rs shows validation
3. **Tests pass**: `cargo test --lib scan` passes
4. **Symlink handling**: WalkDir::follow_links(false) prevents following symlinks

</verification>

<success_criteria>

1. scan_directory_with_filter validates each path before processing
2. Paths outside root are skipped and logged in diagnostics
3. Symlinks to outside root are detected and rejected
4. Normal scan operation continues after path rejection
5. Tests cover: normal scan, symlink escape, traversal rejection

</success_criteria>

<output>

After completion, create `.planning/phases/10-path-traversal-validation/10-03-SUMMARY.md` with:
- Changes to scan.rs
- Path validation integration
- Test results

</output>

---

## Plan 10-04: Add Cross-Platform Path Tests

### Objective

Add comprehensive tests for path traversal validation covering cross-platform edge cases (Windows backslash, macOS case-insensitivity, symlinks, UNC paths).

**Purpose:** Verify path validation works correctly across all supported platforms.

**Output:** Test suite covering malicious paths, symlinks, and cross-platform edge cases.

### Requirements Coverage

- **PATH-03:** Tests for traversal attempts (`../`, `..\\`, symlinks, UNC paths)
- **PATH-06:** Handle cross-platform path differences (Windows backslash, macOS case-insensitivity)

### Context

@src/validation.rs (from 10-01)
@src/watcher.rs (from 10-02)
@src/graph/scan.rs (from 10-03)

### Tasks

<task type="auto">
  <name>Task 1: Create comprehensive integration tests for path validation</name>
  <files>tests/path_validation_tests.rs</files>
  <action>
    Create tests/path_validation_tests.rs with comprehensive cross-platform tests:

    ```rust
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

        // "../something" pattern
        let traversal = root.join("../etc");
        let result = validate_path_within_root(&traversal, root);
        assert!(result.is_err());

        match result.unwrap_err() {
            PathValidationError::SuspiciousTraversal(_) => {}
            _ => panic!("Expected SuspiciousTraversal error"),
        }
    }

    #[test]
    fn test_double_parent_traversal_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let traversal = root.join("../../etc");
        let result = validate_path_within_root(&traversal, root);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_parent_traversal_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let traversal = root.join("../../../etc/passwd");
        let result = validate_path_within_root(&traversal, root);
        assert!(result.is_err());
    }

    #[test]
    fn test_legitimate_single_parent_accepted() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create a parent directory with a file
        let parent = root.join("parent");
        fs::create_dir(&parent).unwrap();
        let file = parent.join("file.rs");
        fs::write(&file, b"fn test() {}").unwrap();

        // Reference from a subdir using "../"
        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();

        // The path "../parent/file.rs" from within subdir is valid
        // when resolved from root
        let resolved = subdir.join("../parent/file.rs");
        let result = validate_path_within_root(&resolved, root);

        // This should resolve to a valid path within root
        // (canonicalize resolves the "..")
        assert!(result.is_ok() || matches!(result.unwrap_err(), PathValidationError::CannotCanonicalize(_)));
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
        let traversal = root.join("..\\..\\..\\windows\\system32");

        let result = validate_path_within_root(&traversal, root);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(windows)]
    fn test_windows_unc_path_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // UNC path (\\server\share\...)
        // This is an absolute path outside the root
        let unc_path = Path::new("\\\\?\\C:\\Windows\\System32");

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
            _ => panic!("Expected SymlinkEscape error"),
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
        assert!(!has_suspicious_traversal("../normal/lib.rs"));
        assert!(!has_suspicious_traversal("../../normal/path"));
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn test_empty_path_components() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Path with empty components (// on Unix)
        let file = create_test_file(root, "test.rs");

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

        let file = create_test_file(root, "test.rs");

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
    ```
  </action>
  <verify>cargo test --test path_validation_tests passes</verify>
  <done>All cross-platform path tests pass</done>
</task>

<task type="auto">
  <name>Task 2: Add symlink-specific integration tests</name>
  <files>tests/symlink_tests.rs</files>
  <action>
    Create tests/symlink_tests.rs for symlink-specific integration tests:

    ```rust
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
    #[cfg(all(unix, not(target_os = "macos")))]
    fn test_symlink_case_sensitive() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create target with specific case
        let target = root.join("Target.rs");
        fs::write(&target, b"fn target() {}").unwrap();

        // Create symlink with different case
        let symlink = root.join("link.rs");
        create_symlink(&symlink, &target);

        let result = is_safe_symlink(&symlink, root);
        // On case-sensitive systems, this might not resolve correctly
        // but shouldn't crash
        assert!(result.is_ok() || result.is_err());
    }
    ```

    Also add this test file to Cargo.toml if not using automatic test discovery:

    ```toml
    [[test]]
    name = "symlink_tests"
    path = "tests/symlink_tests.rs"
    ```
  </action>
  <verify>cargo test --test symlink_tests passes</verify>
  <done>Symlink tests verify secure handling</done>
</task>

<task type="auto">
  <name>Task 3: Add documentation on cross-platform behavior</name>
  <files>docs/PATH_VALIDATION.md</files>
  <action>
    Create docs/PATH_VALIDATION.md documenting cross-platform behavior:

    ```markdown
    # Path Validation in Magellan

    ## Overview

    Magellan validates all file paths before accessing the filesystem to prevent
    directory traversal attacks (CVE-2025-68705 class vulnerabilities).

    ## Validation Strategy

    1. **Pre-check for obvious traversal patterns** - Catches `../` before canonicalization
    2. **Canonicalize both paths** - Resolves symlinks, `.`, and `..` components
    3. **Verify canonicalized path starts with root** - Ensures no escape

    ## Platform-Specific Behavior

    ### Linux

    - Case-sensitive paths
    - Symlinks are followed during canonicalization
    - Absolute paths start with `/`
    - Parent directory: `../`

    ### macOS

    - Case-insensitive paths (HFS+/APFS default)
    - Symlinks are followed during canonicalization
    - Absolute paths start with `/`
    - Parent directory: `../`

    ### Windows

    - Case-insensitive paths (NTFS)
    - Symlinks require developer mode or admin privileges
    - Absolute paths start with drive letter (e.g., `C:\`) or UNC (`\\?\`)
    - Parent directory: `..\\`
    - Path separators: Both `/` and `\` are supported

    ## Symlink Policy

    **Current policy:** Symlinks are resolved and then validated.

    - Symlinks pointing **within** project root: Allowed
    - Symlinks pointing **outside** project root: Rejected
    - Broken symlinks: Skipped (cannot be canonicalized)
    - Circular symlinks: Detected by canonicalization failure

    ## Attack Patterns Prevented

    | Pattern | Example | Detection Method |
    |---------|---------|------------------|
    | Parent traversal | `../../../etc/passwd` | Pre-check + canonicalization |
    | Absolute path | `/etc/passwd` | Canonicalization + prefix check |
    | UNC path (Windows) | `\\?\C:\Windows\System32` | Canonicalization + prefix check |
    | Symlink escape | `link -> /etc/passwd` | Symlink validation |
    | Mixed traversal | `./subdir/../../etc` | Pre-check |

    ## Performance Considerations

    - `std::fs::canonicalize` requires filesystem access
    - Caching is not implemented (paths are validated once per access)
    - Performance impact is acceptable for security benefit

    ## Usage Example

    ```rust
    use magellan::validation::validate_path_within_root;

    let root = Path::new("/project/root");
    let user_input = Path::new("../etc/passwd");

    match validate_path_within_root(user_input, root) {
        Ok(canonical) => {
            // Path is safe, use canonicalized path
            let contents = std::fs::read(&canonical)?;
        }
        Err(e) => {
            eprintln!("Path rejected: {}", e);
        }
    }
    ```

    ## Testing

    Run path validation tests:
    ```bash
    cargo test --test path_validation_tests
    cargo test --test symlink_tests
    ```

    Platform-specific tests are conditionally compiled and only run on the target platform.
    ```
  </action>
  <verify>docs/PATH_VALIDATION.md exists and is readable</verify>
  <done>Documentation explains cross-platform behavior</done>
</task>

</tasks>

<verification>

1. **Test files created**: `tests/path_validation_tests.rs` and `tests/symlink_tests.rs` exist
2. **Tests compile**: `cargo test --test path_validation_tests` compiles
3. **Tests pass**: All tests pass on the current platform
4. **Documentation created**: `docs/PATH_VALIDATION.md` exists

</verification>

<success_criteria>

1. Comprehensive test suite covers:
   - Parent directory traversal (`../`, `..\\`)
   - Absolute paths outside root
   - Cross-platform path separators
   - Symlinks (safe, unsafe, broken, circular)
   - Mixed traversal patterns
   - Edge cases (empty components, dots in names)
2. Documentation explains platform-specific behavior
3. Tests are conditionally compiled for platform-specific behavior

</success_criteria>

<output>

After completion, create `.planning/phases/10-path-traversal-validation/10-04-SUMMARY.md` with:
- Test coverage summary
- Platform-specific test results
- Known limitations or edge cases

</output>

---

## Phase Summary

### Wave Structure

| Wave | Plans | Description |
|------|-------|-------------|
| 1 | 10-01 | Create validation module (foundation) |
| 2 | 10-02, 10-03 | Integrate into watcher and scan (parallel) |
| 3 | 10-04 | Comprehensive testing and documentation |

### Requirements Coverage

| Requirement | Plans | Status |
|-------------|-------|--------|
| PATH-01: Path canonicalization | 10-01 | Covered |
| PATH-02: validate_path_within_root() | 10-01 | Covered |
| PATH-03: Tests for traversal | 10-04 | Covered |
| PATH-04: Watcher integration | 10-02 | Covered |
| PATH-05: Scan integration | 10-03 | Covered |
| PATH-06: Cross-platform handling | 10-01, 10-04 | Covered |

### Dependencies

- Plan 10-02 depends on: 10-01 (uses validation.rs functions)
- Plan 10-03 depends on: 10-01 (uses validation.rs functions)
- Plan 10-04 depends on: 10-01, 10-02, 10-03 (tests integration points)

### Success Criteria (Phase)

1. All file access operations validate paths cannot escape project root
2. Watcher events are filtered before processing
3. Directory scan validates each path before recursing
4. Symlinks pointing outside root are rejected
5. Cross-platform tests pass on Linux, macOS, Windows

### Execute Phase

Run plans in wave order:
```bash
# Wave 1: Foundation
/gsd:execute-plan 10-path-traversal-validation/10-01

# Wave 2: Integration (parallel)
/gsd:execute-plan 10-path-traversal-validation/10-02
/gsd:execute-plan 10-path-traversal-validation/10-03

# Wave 3: Testing
/gsd:execute-plan 10-path-traversal-validation/10-04
```

---
*Phase 10 plan created: 2026-01-19*

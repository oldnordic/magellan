# Path Validation in Magellan

## Overview

Magellan validates all file paths before accessing the filesystem to prevent
directory traversal attacks (CVE-2025-68705 class vulnerabilities).

## Validation Strategy

Path validation in Magellan uses a defense-in-depth approach:

1. **Pre-check for obvious traversal patterns** - Catches `../` before canonicalization
2. **Canonicalize both paths** - Resolves symlinks, `.`, and `..` components
3. **Verify canonicalized path starts with root** - Ensures no escape

### Validation Function

The primary entry point is `validate_path_within_root()` in `src/validation.rs`:

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

## Platform-Specific Behavior

### Linux

- **Case-sensitive paths**: `/src/Main.rs` and `/src/main.rs` are different files
- **Symlinks**: Followed during canonicalization
- **Absolute paths**: Start with `/`
- **Parent directory**: `../`
- **Path separator**: `/`

### macOS

- **Case-insensitive paths**: HFS+/APFS treat `/src/Main.rs` and `/src/main.rs` as the same
- **Symlinks**: Followed during canonicalization
- **Absolute paths**: Start with `/`
- **Parent directory**: `../`
- **Path separator**: `/`

### Windows

- **Case-insensitive paths**: NTFS treats paths as case-insensitive
- **Symlinks**: Require developer mode or admin privileges to create
- **Absolute paths**: Start with drive letter (e.g., `C:\`) or UNC (`\\?\`)
- **Parent directory**: `..\\`
- **Path separators**: Both `/` and `\` are supported

## Symlink Policy

**Current policy:** Symlinks are resolved and then validated.

- Symlinks pointing **within** project root: Allowed
- Symlinks pointing **outside** project root: Rejected with `SymlinkEscape` error
- Broken symlinks: Skipped with `CannotCanonicalize` error
- Circular symlinks: Detected by canonicalization failure

### Symlink Validation

Use `is_safe_symlink()` to validate symlinks specifically:

```rust
use magellan::validation::is_safe_symlink;

let result = is_safe_symlink(&symlink_path, &root)?;
if result {
    // Symlink is safe to follow
}
```

## Attack Patterns Prevented

| Pattern | Example | Detection Method |
|---------|---------|------------------|
| Single parent traversal | `../etc/passwd` | Pre-check (if shallow) or canonicalization |
| Multiple parent traversal | `../../../etc/passwd` | Pre-check (>=3 parents) |
| Double parent traversal | `../../etc/passwd` | Canonicalization |
| Absolute path (Unix) | `/etc/passwd` | Canonicalization + prefix check |
| Absolute path (Windows) | `C:\Windows\System32` | Canonicalization + prefix check |
| UNC path (Windows) | `\\?\C:\Windows\System32` | Canonicalization + prefix check |
| Symlink escape | `link -> /etc/passwd` | Symlink validation |
| Mixed traversal | `./subdir/../../etc` | Pre-check (mixed ./.. patterns) |

### Suspicious Traversal Detection

The `has_suspicious_traversal()` function flags:

- Paths with 3 or more `../` patterns (e.g., `../../../etc`)
- Single parent with shallow depth (e.g., `../config`, `../etc/file`)
- Mixed `./` followed by `../` patterns (e.g., `./src/../../etc`)
- Windows-style equivalents (e.g., `..\\..\\..\\windows`)

**Allowed patterns:**

- `../../normal/path` (2 parents is acceptable)
- `../parent/deep/nested` (single parent but deeply nested)

## Error Types

```rust
pub enum PathValidationError {
    /// Path cannot be canonicalized (doesn't exist or permission denied)
    CannotCanonicalize(String),

    /// Resolved path escapes the project root
    OutsideRoot(String, String),

    /// Path contains suspicious traversal patterns
    SuspiciousTraversal(String),

    /// Symlink points outside project root
    SymlinkEscape(String, String),
}
```

## Integration Points

### Watcher Integration

The watcher (`src/watcher.rs`) validates all event paths before processing:

```rust
// In extract_dirty_paths()
match validate_path_within_root(&path, root) {
    Ok(_) => {
        // Path is safe, include it
        dirty_paths.insert(path.clone());
    }
    Err(PathValidationError::OutsideRoot(p, _)) => {
        eprintln!("WARNING: Watcher rejected path outside project root: {}", p);
    }
    // ... other error handling
}
```

### Scan Integration

The scanner (`src/graph/scan.rs`) validates each path during directory walking:

```rust
// In scan_directory_with_filter()
match validate_path_within_root(path, dir_path) {
    Ok(_) => {
        // Path is safe, continue to filtering
    }
    Err(PathValidationError::OutsideRoot(p, _)) => {
        // Log and skip
        continue;
    }
    // ... other error handling
}
```

## Performance Considerations

- `std::fs::canonicalize` requires filesystem access
- Caching is not implemented (paths are validated once per access)
- Performance impact is acceptable for security benefit
- Watch mode: validation occurs on debounced event batches
- Scan mode: validation occurs once per file during initial walk

## Testing

Run path validation tests:

```bash
# All path validation tests
cargo test --test path_validation_tests

# Symlink-specific tests
cargo test --test symlink_tests

# Unit tests in validation.rs
cargo test --lib validation
```

Platform-specific tests are conditionally compiled and only run on the target platform:

- `#[cfg(unix)]` - Linux/macOS specific tests
- `#[cfg(windows)]` - Windows specific tests
- `#[cfg(any(unix, windows))]` - Tests that run on both platforms but not elsewhere

## Usage Example

### Command Line

When using magellan CLI, path validation is automatic:

```bash
# Scan a directory - all paths validated against project root
magellan scan --db magellan.db /path/to/project

# Watch mode - event paths validated before processing
magellan watch --db magellan.db /path/to/project
```

### Library Usage

```rust
use magellan::CodeGraph;
use magellan::validation::validate_path_within_root;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new("/project/root");
    let user_path = Path::new("src/main.rs");

    // Validate before any file access
    let canonical = validate_path_within_root(user_path, root)?;

    // Safe to use canonical path
    let contents = std::fs::read_to_string(&canonical)?;

    Ok(())
}
```

## Security Considerations

1. **Always validate before access**: Never use user-provided paths without validation
2. **Use canonicalized paths**: After validation, use the returned canonical path
3. **Handle errors gracefully**: Log rejected paths but continue processing
4. **Symlinks are followed**: The canonicalization follows symlinks, then validates
5. **Race conditions**: Validation is point-in-time; file may change between validate and access

## Future Enhancements

Potential improvements for future versions:

- Path validation caching for frequently-accessed paths
- Allow-list mode for specific external paths (opt-in)
- Configurable traversal depth thresholds
- Symlink policy options (follow/reject/stat-only)

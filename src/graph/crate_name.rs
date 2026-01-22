//! Crate name detection for Rust projects
//!
//! This module provides functionality to detect the crate name for a given
//! file path. This is essential for cross-crate symbol disambiguation in
//! SymbolId generation.
//!
//! # Detection Strategy
//!
//! The crate name is detected using the following priority order:
//!
//! 1. **Cargo.toml parsing**: Read `[package].name` from the Cargo.toml file
//!    located at the project root
//! 2. **Directory name fallback**: Use the directory name of the project root
//! 3. **Ultimate fallback**: Return "unknown"
//!
//! # Examples
//!
//! ```ignore
//! use std::path::Path;
//! use crate::graph::crate_name::detect_crate_name;
//!
//! let project_root = Path::new("/path/to/project");
//! let file_path = Path::new("/path/to/project/src/lib.rs");
//!
//! let crate_name = detect_crate_name(project_root, file_path);
//! assert_eq!(crate_name, "project");
//! ```

use std::fs;
use std::path::Path;

/// Detect the crate name for a given file path.
///
/// This function attempts to determine the Rust crate name by:
///
/// 1. Looking for a `Cargo.toml` file at `project_root/Cargo.toml` and
///    extracting the `[package].name` value
/// 2. Falling back to the directory name of `project_root` if no Cargo.toml
///    is found or it cannot be parsed
/// 3. Returning "unknown" as the ultimate fallback
///
/// # Arguments
///
/// * `project_root` - The root directory of the Rust project/crate
/// * `file_path` - The path to the source file (currently unused but kept for
///   future workspace member detection)
///
/// # Returns
///
/// The detected crate name as a `String`.
///
/// # Notes
///
/// - This is a heuristic implementation. For Cargo workspaces, each member
///   crate has its own `Cargo.toml`, and this function should be called with
///   the appropriate project root for each member
/// - The function uses simple string-based parsing to avoid adding a `toml`
///   dependency. It looks for the `name = "..."` pattern within the
///   `[package]` section
/// - Future work may include proper workspace member detection and more
///   robust TOML parsing
///
/// # Examples
///
/// ```ignore
/// use std::path::Path;
/// use crate::graph::crate_name::detect_crate_name;
///
/// // Project with valid Cargo.toml
/// let root = Path::new("/home/user/magellan");
/// let file = Path::new("/home/user/magellan/src/lib.rs");
/// let name = detect_crate_name(root, file);
/// // Returns: "magellan" (from Cargo.toml)
///
/// // Project without Cargo.toml
/// let root = Path::new("/tmp/single-file-rs");
/// let file = Path::new("/tmp/single-file-rs/main.rs");
/// let name = detect_crate_name(root, file);
/// // Returns: "single-file-rs" (from directory name)
/// ```
pub fn detect_crate_name(project_root: &Path, _file_path: &Path) -> String {
    // Priority 1: Try to read from Cargo.toml
    let cargo_toml = project_root.join("Cargo.toml");

    if let Ok(content) = fs::read_to_string(&cargo_toml) {
        if let Some(name) = parse_cargo_toml_name(&content) {
            return name;
        }
    } else {
        // Cargo.toml not found or unreadable - fall back to directory name
    }

    // Priority 2: Use directory name
    if let Some(dir_name) = project_root.file_name() {
        if let Some(name) = dir_name.to_str() {
            return name.to_string();
        }
    }

    // Priority 3: Ultimate fallback
    "unknown".to_string()
}

/// Parse the crate name from Cargo.toml content.
///
/// This function performs simple string-based parsing to extract the
/// `[package].name` value without requiring a full TOML parser.
///
/// # Arguments
///
/// * `content` - The contents of a Cargo.toml file as a string
///
/// # Returns
///
/// `Some(name)` if the name was found, `None` otherwise.
///
/// # Parsing Strategy
///
/// 1. Look for the `[package]` section header
/// 2. After finding `[package]`, look for a line starting with `name =`
/// 3. Extract the value between the quotes (supports both `"` and `'`)
///
/// # Notes
///
/// - This is a simplified parser that handles common Cargo.toml formats
/// - It does not handle inline tables, comments, or complex TOML features
/// - The function stops looking once it finds another section header
///   (e.g., `[dependencies]`) to avoid finding `name` in wrong sections
fn parse_cargo_toml_name(content: &str) -> Option<String> {
    let mut in_package_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for [package] section header
        if trimmed == "[package]" {
            in_package_section = true;
            continue;
        }

        // Exit package section if we hit another section
        if trimmed.starts_with('[') {
            if in_package_section && trimmed != "[package]" {
                // We've moved to a different section
                break;
            }
            continue;
        }

        // Look for name = "..." or name = '...' within [package] section
        // Use starts_with("name") then check for '=' to handle variable spacing
        if in_package_section && trimmed.starts_with("name") && trimmed.contains('=') {
            return extract_quoted_value(trimmed);
        }
    }

    None
}

/// Extract a quoted value from a TOML key-value line.
///
/// # Arguments
///
/// * `line` - A line containing a key-value pair like `name = "crate-name"`
///
/// # Returns
///
/// `Some(value)` if a quoted value was found, `None` otherwise.
///
/// # Examples
///
/// ```
/// use magellan::graph::crate_name::extract_quoted_value;
///
/// assert_eq!(extract_quoted_value(r#"name = "my-crate""#), Some("my-crate".to_string()));
/// assert_eq!(extract_quoted_value(r#"version='1.0'"#), Some("1.0".to_string()));
/// assert_eq!(extract_quoted_value(r#"name = my-crate"#), None);
/// ```
fn extract_quoted_value(line: &str) -> Option<String> {
    // Find the equals sign
    let eq_pos = line.find('=')?;
    let after_eq = &line[eq_pos + 1..];

    // Trim whitespace and look for the first quote
    let trimmed = after_eq.trim_start();
    let quote_char = trimmed.chars().next()?;
    if quote_char != '"' && quote_char != '\'' {
        return None;
    }

    // Find the opening quote in the trimmed string
    let quote_offset = trimmed.find(quote_char)?;
    let start = quote_offset + 1;
    let remaining = &trimmed[start..];

    // Find the closing quote
    let end = remaining.find(quote_char)?;

    Some(remaining[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests use tempfile to create test directories
    // The actual test implementations are below

    #[test]
    fn test_extract_quoted_value_double_quotes() {
        assert_eq!(
            extract_quoted_value(r#"name = "my-crate""#),
            Some("my-crate".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_value_single_quotes() {
        assert_eq!(
            extract_quoted_value(r#"name = 'my-crate'"#),
            Some("my-crate".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_value_no_quotes() {
        assert_eq!(extract_quoted_value(r#"name = my-crate"#), None);
    }

    #[test]
    fn test_extract_quoted_value_empty() {
        assert_eq!(extract_quoted_value(r#"name = """#), Some("".to_string()));
    }

    #[test]
    fn test_parse_cargo_toml_name_valid() {
        let content = r#"
[package]
name = "test-crate"
version = "0.1.0"

[dependencies]
"#;
        assert_eq!(
            parse_cargo_toml_name(content),
            Some("test-crate".to_string())
        );
    }

    #[test]
    fn test_parse_cargo_toml_name_with_spaces() {
        let content = r#"
[package]
name    =    "test-crate-with-spaces"
"#;
        assert_eq!(
            parse_cargo_toml_name(content),
            Some("test-crate-with-spaces".to_string())
        );
    }

    #[test]
    fn test_parse_cargo_toml_name_empty() {
        assert_eq!(parse_cargo_toml_name(""), None);
    }

    #[test]
    fn test_parse_cargo_toml_name_no_package_section() {
        let content = r#"
[dependencies]
serde = "1.0"
"#;
        assert_eq!(parse_cargo_toml_name(content), None);
    }

    #[test]
    fn test_parse_cargo_toml_name_no_name_field() {
        let content = r#"
[package]
version = "0.1.0"
"#;
        assert_eq!(parse_cargo_toml_name(content), None);
    }

    #[test]
    fn test_parse_cargo_toml_name_stops_at_next_section() {
        // Should not pick up name from [package.metadata] section
        let content = r#"
[package]
version = "0.1.0"

[package.metadata]
name = "should-not-pick-this"

[dependencies]
"#;
        assert_eq!(parse_cargo_toml_name(content), None);
    }

    #[test]
    fn test_detect_from_cargo_toml() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cargo_toml = temp_dir.path().join("Cargo.toml");

        fs::write(
            &cargo_toml,
            r#"
[package]
name = "test-crate"
version = "0.1.0"
"#,
        )
        .unwrap();

        let file_path = temp_dir.path().join("src/lib.rs");
        let result = detect_crate_name(temp_dir.path(), &file_path);

        assert_eq!(result, "test-crate");
    }

    #[test]
    fn test_fallback_to_directory_name() {
        use tempfile::TempDir;

        let temp_dir = TempDir::with_prefix("my-test-crate-").unwrap();
        // No Cargo.toml created

        let file_path = temp_dir.path().join("src/lib.rs");
        let result = detect_crate_name(temp_dir.path(), &file_path);

        // Directory name starts with "my-test-crate-" (tempfile adds random suffix)
        assert!(result.starts_with("my-test-crate-"));
    }

    #[test]
    fn test_unknown_fallback_empty_string_name() {
        // Use a path that has no meaningful directory name
        // This is a bit tricky to test in practice, but we can use "/"

        let file_path = std::path::PathBuf::from("/tmp/file.rs");
        let result = detect_crate_name(std::path::Path::new("/"), &file_path);

        // "/" has no file_name, so we should get "unknown"
        assert_eq!(result, "unknown");
    }

    #[test]
    fn test_malformed_cargo_toml() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cargo_toml = temp_dir.path().join("Cargo.toml");

        // Write completely invalid content
        fs::write(&cargo_toml, "not valid toml at all!!!").unwrap();

        let file_path = temp_dir.path().join("src/lib.rs");
        let result = detect_crate_name(temp_dir.path(), &file_path);

        // Should fall back to directory name
        assert!(!result.is_empty());
        assert_ne!(result, "unknown"); // temp dir has a name
    }

    #[test]
    fn test_missing_name_field() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cargo_toml = temp_dir.path().join("Cargo.toml");

        // Cargo.toml without name field
        fs::write(
            &cargo_toml,
            r#"
[package]
version = "0.1.0"
"#,
        )
        .unwrap();

        let file_path = temp_dir.path().join("src/lib.rs");
        let result = detect_crate_name(temp_dir.path(), &file_path);

        // Should fall back to directory name
        assert!(!result.is_empty());
        assert_ne!(result, "unknown"); // temp dir has a name
    }

    #[test]
    fn test_cargo_toml_with_special_characters() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cargo_toml = temp_dir.path().join("Cargo.toml");

        // Crate names can contain hyphens but not underscores
        fs::write(
            &cargo_toml,
            r#"
[package]
name = "my-awesome-crate-v2"
version = "0.1.0"
"#,
        )
        .unwrap();

        let file_path = temp_dir.path().join("src/lib.rs");
        let result = detect_crate_name(temp_dir.path(), &file_path);

        assert_eq!(result, "my-awesome-crate-v2");
    }
}

//! Common utility functions shared across command modules
//!
//! This module provides helper functions that were previously duplicated
//! across multiple command modules (find_cmd, query_cmd, refs_cmd, get_cmd).

use std::path::{Path, PathBuf};

use crate::validation::normalize_path;
use crate::SymbolKind;

/// Detect programming language from file path extension
///
/// Maps file extensions to language names for semantic annotations.
///
/// # Arguments
/// * `path` - File path to examine
///
/// # Returns
/// Language name as string, or "unknown" if extension not recognized
///
/// # Supported Extensions
/// - `.rs` → "rust"
/// - `.py` → "python"
/// - `.js` → "javascript"
/// - `.ts`, `.tsx` → "typescript"
/// - `.java` → "java"
/// - `.c` → "c"
/// - `.cpp`, `.cc`, `.cxx`, `.hpp` → "cpp"
/// - `.go` → "go"
/// - `.rb` → "ruby"
/// - `.php` → "php"
pub fn detect_language_from_path(path: &str) -> String {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust".to_string(),
        "py" => "python".to_string(),
        "js" => "javascript".to_string(),
        "ts" | "tsx" => "typescript".to_string(),
        "java" => "java".to_string(),
        "c" => "c".to_string(),
        "cpp" | "cc" | "cxx" | "hpp" => "cpp".to_string(),
        "go" => "go".to_string(),
        "rb" => "ruby".to_string(),
        "php" => "php".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Resolve a file path against an optional root directory
///
/// # Arguments
/// * `file_path` - The file path (may be relative or absolute)
/// * `root` - Optional root directory for resolving relative paths
///
/// # Returns
/// Normalized path string
///
/// # Behavior
/// - If `file_path` is absolute, return it normalized
/// - If `root` is provided, resolve `file_path` relative to `root` and normalize
/// - If `root` is None and `file_path` is relative, canonicalize from current directory
pub fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    let resolved = if file_path.is_absolute() {
        file_path.clone()
    } else if let Some(ref root) = root {
        root.join(file_path)
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .unwrap_or_else(|| file_path.clone())
    };

    // Normalize the resolved path
    normalize_path(&resolved).unwrap_or_else(|_| resolved.to_string_lossy().to_string())
}

/// Format a SymbolKind for display
///
/// # Arguments
/// * `kind` - The SymbolKind to format
///
/// # Returns
/// Human-readable string representation of the symbol kind
pub fn format_symbol_kind(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "Function",
        SymbolKind::Method => "Method",
        SymbolKind::Class => "Class",
        SymbolKind::Interface => "Interface",
        SymbolKind::Enum => "Enum",
        SymbolKind::Module => "Module",
        SymbolKind::Union => "Union",
        SymbolKind::Namespace => "Namespace",
        SymbolKind::TypeAlias => "TypeAlias",
        SymbolKind::Unknown => "Unknown",
    }
}

/// Parse a string into a SymbolKind (case-insensitive)
///
/// # Arguments
/// * `s` - String to parse
///
/// # Returns
/// Some(SymbolKind) if recognized, None otherwise
///
/// # Supported values (case-insensitive)
/// - "function", "fn" → Function
/// - "method" → Method
/// - "class", "struct" → Class
/// - "interface", "trait" → Interface
/// - "enum" → Enum
/// - "module", "mod" → Module
/// - "union" → Union
/// - "namespace", "ns" → Namespace
/// - "type", "typealias", "type alias" → TypeAlias
pub fn parse_symbol_kind(s: &str) -> Option<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "class" | "struct" => Some(SymbolKind::Class),
        "interface" | "trait" => Some(SymbolKind::Interface),
        "enum" => Some(SymbolKind::Enum),
        "module" | "mod" => Some(SymbolKind::Module),
        "union" => Some(SymbolKind::Union),
        "namespace" | "ns" => Some(SymbolKind::Namespace),
        "type" | "typealias" | "type alias" => Some(SymbolKind::TypeAlias),
        _ => None,
    }
}

/// Safely extract a byte slice from source with bounds checking
///
/// Returns None if the slice range is invalid or exceeds source length.
/// Use this instead of direct slicing to prevent panics on malformed input.
///
/// # Arguments
/// * `source` - Source byte slice
/// * `start` - Start byte offset (inclusive)
/// * `end` - End byte offset (exclusive)
///
/// # Returns
/// Some(&[u8]) if bounds are valid, None otherwise
///
/// # Example
/// ```rust
/// use magellan::common::safe_slice;
/// let source = b"hello world";
/// let slice = safe_slice(source, 0, 5); // Some(b"hello")
/// let invalid = safe_slice(source, 10, 20); // None (out of bounds)
/// ```
pub fn safe_slice<'a>(source: &'a [u8], start: usize, end: usize) -> Option<&'a [u8]> {
    if start <= end && end <= source.len() {
        Some(&source[start..end])
    } else {
        None
    }
}

/// Safely extract a UTF-8 string slice with bounds checking
///
/// Returns None if the slice range is invalid, exceeds source length,
/// or contains invalid UTF-8 boundaries.
///
/// # Arguments
/// * `source` - Source string
/// * `start` - Start byte offset (inclusive)
/// * `end` - End byte offset (exclusive)
///
/// # Returns
/// Some(&str) if bounds are valid and UTF-8 safe, None otherwise
pub fn safe_str_slice<'a>(source: &'a str, start: usize, end: usize) -> Option<&'a str> {
    if start <= end && end <= source.len() {
        source.get(start..end)
    } else {
        None
    }
}

/// Safely extract symbol content from source bytes, handling multi-byte UTF-8 boundaries
///
/// tree-sitter provides byte offsets that can split multi-byte UTF-8 characters
/// (emojis, CJK, accented letters). Direct slicing with these offsets will panic
/// if the offsets fall within a multi-byte character.
///
/// This function:
/// 1. Validates the byte offsets are within bounds
/// 2. Converts bytes to UTF-8 string
/// 3. Adjusts the end offset to the nearest valid UTF-8 character boundary if needed
/// 4. Returns the extracted content as a String, or None if extraction fails
///
/// # Arguments
/// * `source` - Source file contents as bytes
/// * `byte_start` - Start byte offset (inclusive) from tree-sitter
/// * `byte_end` - End byte offset (exclusive) from tree-sitter
///
/// # Returns
/// Some(String) with the extracted content, or None if:
/// - Offsets are out of bounds
/// - Source is not valid UTF-8
/// - Start offset is not at a valid UTF-8 character boundary
///
/// # Example
/// ```rust
/// use magellan::common::extract_symbol_content_safe;
/// let source = "fn hello() { // Hi \u{1f44b} }"; // contains emoji
/// let content = extract_symbol_content_safe(source.as_bytes(), 0, source.len());
/// assert!(content.is_some());
/// ```
pub fn extract_symbol_content_safe(source: &[u8], byte_start: usize, byte_end: usize) -> Option<String> {
    // Validate bounds
    if byte_start > byte_end || byte_end > source.len() {
        return None;
    }

    // Convert to UTF-8 string
    let source_str = std::str::from_utf8(source).ok()?;

    // Validate start is at a character boundary
    if !source_str.is_char_boundary(byte_start) {
        // Start offset splits a multi-byte character - return None
        // rather than returning corrupted data
        return None;
    }

    // Find the nearest valid UTF-8 boundary at or before byte_end
    // This handles cases where tree-sitter's end offset splits a multi-byte character
    let adjusted_end = find_char_boundary_before(source_str, byte_end);

    // Extract content using the adjusted end offset
    source_str.get(byte_start..adjusted_end).map(|s| s.to_string())
}

/// Safely extract context lines from source bytes around a byte span
///
/// Similar to `extract_symbol_content_safe`, but extracts context around
/// a span rather than the span itself. This is useful for extracting
/// surrounding lines for better context in code intelligence tools.
///
/// # Arguments
/// * `source` - Source file contents as bytes
/// * `byte_start` - Start byte offset of the span (inclusive)
/// * `byte_end` - End byte offset of the span (exclusive)
/// * `context_bytes` - Number of additional bytes to extract before and after
///
/// # Returns
/// Some(String) with the extracted context, or None if extraction fails
///
/// # Example
/// ```rust
/// use magellan::common::extract_context_safe;
/// let source = "line1\nline2\nline3\nline4";
/// let context = extract_context_safe(source.as_bytes(), 7, 13, 5);
/// // Extracts around "line2" with 5 bytes of context
/// ```
pub fn extract_context_safe(
    source: &[u8],
    byte_start: usize,
    byte_end: usize,
    context_bytes: usize,
) -> Option<String> {
    // Validate bounds
    if byte_start > byte_end || byte_end > source.len() {
        return None;
    }

    // Convert to UTF-8 string
    let source_str = std::str::from_utf8(source).ok()?;

    // Calculate context bounds
    let context_start = byte_start.saturating_sub(context_bytes);
    let context_end = (byte_end + context_bytes).min(source.len());

    // Adjust to valid UTF-8 boundaries
    let adjusted_start = find_char_boundary_after(source_str, context_start);
    let adjusted_end = find_char_boundary_before(source_str, context_end);

    // Extract context
    source_str
        .get(adjusted_start..adjusted_end)
        .map(|s| s.to_string())
}

/// Find the nearest valid UTF-8 character boundary at or before the given byte offset
///
/// If the offset is already at a valid boundary, returns it unchanged.
/// Otherwise, searches backward to find the previous valid boundary.
fn find_char_boundary_before(s: &str, offset: usize) -> usize {
    let mut pos = offset.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Find the nearest valid UTF-8 character boundary at or after the given byte offset
///
/// If the offset is already at a valid boundary, returns it unchanged.
/// Otherwise, searches forward to find the next valid boundary.
fn find_char_boundary_after(s: &str, offset: usize) -> usize {
    let mut pos = offset;
    let len = s.len();
    while pos < len && !s.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_from_path() {
        assert_eq!(detect_language_from_path("src/main.rs"), "rust");
        assert_eq!(detect_language_from_path("script.py"), "python");
        assert_eq!(detect_language_from_path("app.js"), "javascript");
        assert_eq!(detect_language_from_path("component.ts"), "typescript");
        assert_eq!(detect_language_from_path("component.tsx"), "typescript");
        assert_eq!(detect_language_from_path("Main.java"), "java");
        assert_eq!(detect_language_from_path("header.c"), "c");
        assert_eq!(detect_language_from_path("source.cpp"), "cpp");
        assert_eq!(detect_language_from_path("source.cc"), "cpp");
        assert_eq!(detect_language_from_path("source.cxx"), "cpp");
        assert_eq!(detect_language_from_path("header.hpp"), "cpp");
        assert_eq!(detect_language_from_path("main.go"), "go");
        assert_eq!(detect_language_from_path("file.rb"), "ruby");
        assert_eq!(detect_language_from_path("index.php"), "php");
    }

    #[test]
    fn test_detect_language_unknown() {
        assert_eq!(detect_language_from_path("file.xyz"), "unknown");
        assert_eq!(detect_language_from_path("README"), "unknown");
        assert_eq!(detect_language_from_path(".gitignore"), "unknown");
        assert_eq!(detect_language_from_path(""), "unknown");
    }

    #[test]
    fn test_resolve_path_absolute() {
        let path = PathBuf::from("/absolute/path/to/file.rs");
        let root = None;
        assert_eq!(resolve_path(&path, &root), "/absolute/path/to/file.rs");

        let root = Some(PathBuf::from("/some/root"));
        assert_eq!(resolve_path(&path, &root), "/absolute/path/to/file.rs");
    }

    #[test]
    fn test_resolve_path_relative_with_root() {
        let path = PathBuf::from("src/main.rs");
        let root = Some(PathBuf::from("/project"));
        let result = resolve_path(&path, &root);
        // The path will be normalized - if the file doesn't exist, it will preserve the structure
        assert!(result.contains("src/main.rs") || result.contains("project"));

        let path = PathBuf::from("../lib.rs");
        let root = Some(PathBuf::from("/project/src"));
        let result = resolve_path(&path, &root);
        // Will be normalized (./ may be stripped) or canonicalized if file exists
        assert!(result.contains("lib.rs") || result.contains("project"));
    }

    #[test]
    fn test_resolve_path_relative_no_root() {
        // Without root, it tries to canonicalize or returns as-is
        let path = PathBuf::from("src/main.rs");
        let root = None;
        let result = resolve_path(&path, &root);
        // Result depends on current directory, but should not panic
        assert!(result.contains("src/main.rs") || result.ends_with("src/main.rs"));
    }

    #[test]
    fn test_format_symbol_kind() {
        assert_eq!(format_symbol_kind(&SymbolKind::Function), "Function");
        assert_eq!(format_symbol_kind(&SymbolKind::Method), "Method");
        assert_eq!(format_symbol_kind(&SymbolKind::Class), "Class");
        assert_eq!(format_symbol_kind(&SymbolKind::Interface), "Interface");
        assert_eq!(format_symbol_kind(&SymbolKind::Enum), "Enum");
        assert_eq!(format_symbol_kind(&SymbolKind::Module), "Module");
        assert_eq!(format_symbol_kind(&SymbolKind::Union), "Union");
        assert_eq!(format_symbol_kind(&SymbolKind::Namespace), "Namespace");
        assert_eq!(format_symbol_kind(&SymbolKind::TypeAlias), "TypeAlias");
        assert_eq!(format_symbol_kind(&SymbolKind::Unknown), "Unknown");
    }

    #[test]
    fn test_parse_symbol_kind() {
        // Test primary names
        assert_eq!(parse_symbol_kind("function"), Some(SymbolKind::Function));
        assert_eq!(parse_symbol_kind("method"), Some(SymbolKind::Method));
        assert_eq!(parse_symbol_kind("class"), Some(SymbolKind::Class));
        assert_eq!(parse_symbol_kind("interface"), Some(SymbolKind::Interface));
        assert_eq!(parse_symbol_kind("enum"), Some(SymbolKind::Enum));
        assert_eq!(parse_symbol_kind("module"), Some(SymbolKind::Module));
        assert_eq!(parse_symbol_kind("union"), Some(SymbolKind::Union));
        assert_eq!(parse_symbol_kind("namespace"), Some(SymbolKind::Namespace));
        assert_eq!(parse_symbol_kind("typealias"), Some(SymbolKind::TypeAlias));

        // Test aliases
        assert_eq!(parse_symbol_kind("fn"), Some(SymbolKind::Function));
        assert_eq!(parse_symbol_kind("struct"), Some(SymbolKind::Class));
        assert_eq!(parse_symbol_kind("trait"), Some(SymbolKind::Interface));
        assert_eq!(parse_symbol_kind("mod"), Some(SymbolKind::Module));
        assert_eq!(parse_symbol_kind("ns"), Some(SymbolKind::Namespace));
        assert_eq!(parse_symbol_kind("type"), Some(SymbolKind::TypeAlias));
        assert_eq!(parse_symbol_kind("type alias"), Some(SymbolKind::TypeAlias));
    }

    #[test]
    fn test_parse_symbol_kind_case_insensitive() {
        assert_eq!(parse_symbol_kind("FUNCTION"), Some(SymbolKind::Function));
        assert_eq!(parse_symbol_kind("Function"), Some(SymbolKind::Function));
        assert_eq!(parse_symbol_kind("Fn"), Some(SymbolKind::Function));
        assert_eq!(parse_symbol_kind("CLASS"), Some(SymbolKind::Class));
        assert_eq!(parse_symbol_kind("Struct"), Some(SymbolKind::Class));
    }

    #[test]
    fn test_parse_symbol_kind_unknown() {
        assert_eq!(parse_symbol_kind("unknown_kind"), None);
        assert_eq!(parse_symbol_kind(""), None);
        assert_eq!(parse_symbol_kind("xyz"), None);
    }

    #[test]
    fn test_extract_symbol_content_safe_ascii() {
        let source = b"fn hello() { return 42; }";
        let result = extract_symbol_content_safe(source, 0, source.len());
        assert_eq!(result, Some("fn hello() { return 42; }".to_string()));
    }

    #[test]
    fn test_extract_symbol_content_safe_partial_range() {
        let source = b"fn hello() { return 42; }";
        let result = extract_symbol_content_safe(source, 3, 8);
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_extract_symbol_content_safe_emoji() {
        // Emoji (4 bytes in UTF-8): \u{1f44b} = " waving hand"
        let source = "fn test() { //  \u{1f44b}  }";
        let bytes = source.as_bytes();
        let result = extract_symbol_content_safe(bytes, 0, bytes.len());
        assert_eq!(result, Some(source.to_string()));
    }

    #[test]
    fn test_extract_symbol_content_safe_emoji_splits_end() {
        // Test when byte_end splits the emoji (4 bytes)
        // If end is in middle of emoji, should adjust to complete character
        // Emoji \u{1f44b} is 4 bytes in UTF-8: [0xF0, 0x9F, 0x91, 0x8B]
        let source: Vec<u8> = vec![
            b'h', b'i',  // "hi"
            0xF0, 0x9F, 0x91, 0x8B,  // emoji
        ];
        // Emoji bytes at positions 2,3,4,5
        // If we end at position 4 (in middle of emoji), should adjust
        let result = extract_symbol_content_safe(&source, 0, 4);
        // Should return "hi" (stopping before the incomplete emoji)
        assert_eq!(result, Some("hi".to_string()));
    }

    #[test]
    fn test_extract_symbol_content_safe_start_at_boundary() {
        let source = "abc\u{1f44b}xyz"; // "abc" + emoji + "xyz"
        let bytes = source.as_bytes();
        // Start at emoji boundary (position 3)
        let result = extract_symbol_content_safe(bytes, 3, bytes.len());
        assert_eq!(result, Some("\u{1F44B}xyz".to_string()));
    }

    #[test]
    fn test_extract_symbol_content_safe_start_splits_char_returns_none() {
        let source = "abc\u{1f44b}xyz"; // "abc" + emoji + "xyz"
        let bytes = source.as_bytes();
        // Emoji is at positions 3-6 (4 bytes)
        // Start at position 4 (in middle of emoji) should return None
        let result = extract_symbol_content_safe(bytes, 4, bytes.len());
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_symbol_content_safe_out_of_bounds() {
        let source = b"hello";
        assert_eq!(extract_symbol_content_safe(source, 0, 100), None);
        assert_eq!(extract_symbol_content_safe(source, 10, 20), None);
        assert_eq!(extract_symbol_content_safe(source, 5, 3), None);
    }

    #[test]
    fn test_extract_symbol_content_safe_cjk() {
        // CJK characters (Chinese) are 3 bytes each in UTF-8
        let source = "fn 你好() { return 世界; }";
        let bytes = source.as_bytes();
        let result = extract_symbol_content_safe(bytes, 0, bytes.len());
        assert_eq!(result, Some(source.to_string()));
    }

    #[test]
    fn test_extract_symbol_content_safe_accented() {
        // Accented letters (2 bytes in UTF-8)
        let source = "fn héllo() { return café; }";
        let bytes = source.as_bytes();
        let result = extract_symbol_content_safe(bytes, 0, bytes.len());
        assert_eq!(result, Some(source.to_string()));
    }

    #[test]
    fn test_extract_context_safe() {
        let source = "line1\nline2\nline3\nline4";
        let bytes = source.as_bytes();
        // Extract around "line2" (positions 6-11)
        let result = extract_context_safe(bytes, 6, 11, 3);
        // Should include context around line2
        assert!(result.is_some());
        let context = result.unwrap();
        assert!(context.contains("line2"));
    }

    #[test]
    fn test_extract_context_safe_with_emoji() {
        let source = "before\u{1f44b}after";
        let bytes = source.as_bytes();
        // Emoji is 4 bytes starting at position 6
        // Extract around the emoji (positions 6-10)
        let result = extract_context_safe(bytes, 6, 10, 2);
        assert!(result.is_some());
        let context = result.unwrap();
        // Should contain valid UTF-8 (String is always valid UTF-8)
        assert!(!context.is_empty());
        // Verify the emoji is present or at least some valid content
        assert!(context.len() > 0);
    }

    #[test]
    fn test_find_char_boundary_before() {
        let s = "a\u{1f44b}b"; // "a" + emoji + "b"
        // Emoji starts at position 1, ends at position 5 (4 bytes)
        assert_eq!(find_char_boundary_before(s, 5), 5); // Already at boundary
        assert_eq!(find_char_boundary_before(s, 4), 1); // In middle of emoji, go to start
        assert_eq!(find_char_boundary_before(s, 3), 1); // In middle of emoji, go to start
        assert_eq!(find_char_boundary_before(s, 2), 1); // In middle of emoji, go to start
        assert_eq!(find_char_boundary_before(s, 1), 1); // At boundary
        assert_eq!(find_char_boundary_before(s, 0), 0); // At start
    }

    #[test]
    fn test_find_char_boundary_after() {
        let s = "a\u{1f44b}b"; // "a" + emoji + "b"
        // Emoji starts at position 1, ends at position 5 (4 bytes)
        assert_eq!(find_char_boundary_after(s, 0), 0); // Already at boundary
        assert_eq!(find_char_boundary_after(s, 1), 1); // At boundary
        assert_eq!(find_char_boundary_after(s, 2), 5); // In middle of emoji, go to end
        assert_eq!(find_char_boundary_after(s, 3), 5); // In middle of emoji, go to end
        assert_eq!(find_char_boundary_after(s, 4), 5); // In middle of emoji, go to end
        assert_eq!(find_char_boundary_after(s, 5), 5); // At boundary
    }

    #[test]
    fn test_extract_symbol_content_safe_invalid_utf8() {
        let source: &[u8] = &[0xFF, 0xFE, 0xFD]; // Invalid UTF-8
        let result = extract_symbol_content_safe(source, 0, 3);
        assert_eq!(result, None);
    }
}

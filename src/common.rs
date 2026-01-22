//! Common utility functions shared across command modules
//!
//! This module provides helper functions that were previously duplicated
//! across multiple command modules (find_cmd, query_cmd, refs_cmd, get_cmd).

use std::path::{Path, PathBuf};

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
/// Absolute path string
///
/// # Behavior
/// - If `file_path` is absolute, return it as-is
/// - If `root` is provided, resolve `file_path` relative to `root`
/// - If `root` is None and `file_path` is relative, canonicalize from current directory
pub fn resolve_path(file_path: &PathBuf, root: &Option<PathBuf>) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }

    if let Some(ref root) = root {
        root.join(file_path)
            .to_string_lossy()
            .to_string()
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.join(file_path).canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string())
    }
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
        assert_eq!(resolve_path(&path, &root), "/project/src/main.rs");

        let path = PathBuf::from("../lib.rs");
        let root = Some(PathBuf::from("/project/src"));
        assert_eq!(resolve_path(&path, &root), "/project/src/../lib.rs");
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
}

//! Import statement extraction from source code
//!
//! Provides language-specific import extraction using tree-sitter.

use crate::common::safe_slice;
use std::path::PathBuf;

/// Kind of import statement
///
/// Language-agnostic import kinds that map across multiple programming languages.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum ImportKind {
    /// `crate::` prefix in Rust (absolute path from crate root)
    UseCrate,
    /// `super::` prefix in Rust (relative to parent module)
    UseSuper,
    /// `self::` prefix in Rust (relative to current module)
    UseSelf,
    /// `extern crate` declaration in Rust
    ExternCrate,
    /// Plain `use` statement in Rust without special prefix
    PlainUse,
    /// `from ... import` in Python
    FromImport,
    /// `import` statement in Python/TypeScript
    ImportStatement,
}

impl ImportKind {
    /// Return the normalized string key for this import kind (used for storage)
    pub fn normalized_key(&self) -> &'static str {
        match self {
            ImportKind::UseCrate => "use_crate",
            ImportKind::UseSuper => "use_super",
            ImportKind::UseSelf => "use_self",
            ImportKind::ExternCrate => "extern_crate",
            ImportKind::PlainUse => "plain_use",
            ImportKind::FromImport => "from_import",
            ImportKind::ImportStatement => "import_statement",
        }
    }

    /// Parse a string key back to ImportKind
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "use_crate" => Some(ImportKind::UseCrate),
            "use_super" => Some(ImportKind::UseSuper),
            "use_self" => Some(ImportKind::UseSelf),
            "extern_crate" => Some(ImportKind::ExternCrate),
            "plain_use" => Some(ImportKind::PlainUse),
            "from_import" => Some(ImportKind::FromImport),
            "import_statement" => Some(ImportKind::ImportStatement),
            _ => None,
        }
    }
}

/// A fact about an import statement extracted from source code
///
/// Pure data structure. No behavior. No semantic analysis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ImportFact {
    /// File containing this import
    pub file_path: PathBuf,
    /// Kind of import statement
    pub import_kind: ImportKind,
    /// Full import path as components (e.g., ["crate", "foo", "bar"] for crate::foo::bar)
    pub import_path: Vec<String>,
    /// Specific names imported
    pub imported_names: Vec<String>,
    /// Whether this is a glob import (e.g., use foo::*)
    pub is_glob: bool,
    /// Byte offset where import starts
    pub byte_start: usize,
    /// Byte offset where import ends
    pub byte_end: usize,
    /// Line where import starts (1-indexed)
    pub start_line: usize,
    /// Column where import starts (0-indexed)
    pub start_col: usize,
    /// Line where import ends (1-indexed)
    pub end_line: usize,
    /// Column where import ends (0-indexed)
    pub end_col: usize,
}

/// Import extractor for Rust source code
pub struct ImportExtractor {
    /// tree-sitter parser for Rust grammar
    parser: tree_sitter::Parser,
}

impl ImportExtractor {
    /// Create a new import extractor for Rust source code
    pub fn new() -> anyhow::Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_rust::language();
        parser.set_language(&language)?;

        Ok(Self { parser })
    }

    /// Extract import facts from Rust source code
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    ///
    /// # Returns
    /// Vector of import facts found in the source
    pub fn extract_imports_rust(&mut self, file_path: PathBuf, source: &[u8]) -> Vec<ImportFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root_node = tree.root_node();
        let mut facts = Vec::new();

        // Walk tree looking for use_statement, use_declaration, and mod_item nodes
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            match child.kind() {
                "use_statement" | "use_declaration" => {
                    if let Some(fact) = self.extract_use_statement(&child, source, &file_path) {
                        facts.push(fact);
                    }
                }
                "mod_item" => {
                    // mod declarations are also imports (they reference other files)
                    if let Some(fact) = self.extract_mod_item(&child, source, &file_path) {
                        facts.push(fact);
                    }
                }
                _ => {}
            }
        }

        facts
    }

    /// Extract import from a use_statement or use_declaration node
    fn extract_use_statement(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
    ) -> Option<ImportFact> {
        let start = node.start_byte();
        let end = node.end_byte();
        let start_line = node.start_position().row + 1;
        let start_col = node.start_position().column;
        let end_line = node.end_position().row + 1;
        let end_col = node.end_position().column;

        // Find the argument (the path being imported)
        let argument = node.child_by_field_name("argument")?;
        let argument_text = safe_slice(source, argument.start_byte(), argument.end_byte())?;
        let import_str = std::str::from_utf8(argument_text).ok()?;

        // Parse the import path and determine the kind
        let (import_kind, import_path, imported_names, is_glob) =
            self.parse_rust_import_path(import_str);

        Some(ImportFact {
            file_path: file_path.clone(),
            import_kind,
            import_path,
            imported_names,
            is_glob,
            byte_start: start,
            byte_end: end,
            start_line,
            start_col,
            end_line,
            end_col,
        })
    }

    /// Extract import from a mod_item node
    fn extract_mod_item(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
    ) -> Option<ImportFact> {
        // Only extract mod items that refer to external files (not inline mod { ... } blocks)
        // Check if there's a semicolon (external file reference) vs a block (inline module)
        let mut cursor = node.walk();
        let has_semicolon = node.children(&mut cursor).any(|n| n.kind() == ";");
        if !has_semicolon {
            return None; // Inline module definition, not an import
        }

        let start = node.start_byte();
        let end = node.end_byte();
        let start_line = node.start_position().row + 1;
        let start_col = node.start_position().column;
        let end_line = node.end_position().row + 1;
        let end_col = node.end_position().column;

        // Get the module name
        let name_node = node.child_by_field_name("name")?;
        let name_bytes = safe_slice(source, name_node.start_byte(), name_node.end_byte())?;
        let name = std::str::from_utf8(name_bytes).ok()?;

        Some(ImportFact {
            file_path: file_path.clone(),
            import_kind: ImportKind::PlainUse,
            import_path: vec![name.to_string()],
            imported_names: vec![name.to_string()],
            is_glob: false,
            byte_start: start,
            byte_end: end,
            start_line,
            start_col,
            end_line,
            end_col,
        })
    }

    /// Parse a Rust import path string into components
    ///
    /// Examples:
    /// - "crate::foo::bar" -> (UseCrate, ["crate", "foo", "bar"], [], false)
    /// - "super::foo" -> (UseSuper, ["super", "foo"], [], false)
    /// - "self::foo" -> (UseSelf, ["self", "foo"], [], false)
    /// - "std::collections::HashMap" -> (PlainUse, ["std", "collections", "HashMap"], ["HashMap"], false)
    /// - "std::collections::*" -> (PlainUse, ["std", "collections"], [], true)
    /// - "foo::{bar, baz}" -> (PlainUse, ["foo"], ["bar", "baz"], false)
    fn parse_rust_import_path(
        &self,
        path: &str,
    ) -> (ImportKind, Vec<String>, Vec<String>, bool) {
        // Check for glob import
        if path.contains('*') {
            let components: Vec<String> = path
                .split("::")
                .map(|s| s.trim().trim_end_matches('*').trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let kind = if components.first().map(|s| s.as_str()) == Some("crate") {
                ImportKind::UseCrate
            } else if components.first().map(|s| s.as_str()) == Some("super") {
                ImportKind::UseSuper
            } else if components.first().map(|s| s.as_str()) == Some("self") {
                ImportKind::UseSelf
            } else {
                ImportKind::PlainUse
            };

            return (kind, components, Vec::new(), true);
        }

        // Check for braced list: foo::{bar, baz as qux}
        if path.contains('{') {
            let base_end = path.find('{').unwrap();
            let base_path = path[..base_end].trim();
            let list_str = &path[base_end + 1..];
            let list_end = list_str.rfind('}').unwrap_or(list_str.len());
            let list = &list_str[..list_end];

            // Parse base path
            let base_components: Vec<String> = base_path
                .split("::")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Parse imported names (handle "as" aliases)
            let imported_names: Vec<String> = list
                .split(',')
                .map(|s| {
                    s.trim()
                        .split(" as ")
                        .next()
                        .unwrap()
                        .trim()
                        .to_string()
                })
                .filter(|s| !s.is_empty())
                .collect();

            let kind = if base_components.first().map(|s| s.as_str()) == Some("crate") {
                ImportKind::UseCrate
            } else if base_components.first().map(|s| s.as_str()) == Some("super") {
                ImportKind::UseSuper
            } else if base_components.first().map(|s| s.as_str()) == Some("self") {
                ImportKind::UseSelf
            } else {
                ImportKind::PlainUse
            };

            return (kind, base_components, imported_names, false);
        }

        // Simple path import
        let components: Vec<String> = path.split("::").map(|s| s.trim().to_string()).collect();

        let kind = if components.first().map(|s| s.as_str()) == Some("crate") {
            ImportKind::UseCrate
        } else if components.first().map(|s| s.as_str()) == Some("super") {
            ImportKind::UseSuper
        } else if components.first().map(|s| s.as_str()) == Some("self") {
            ImportKind::UseSelf
        } else {
            ImportKind::PlainUse
        };

        // The last component is the imported name
        let imported_names = if let Some(last) = components.last() {
            vec![last.clone()]
        } else {
            Vec::new()
        };

        (kind, components, imported_names, false)
    }
}

impl Default for ImportExtractor {
    fn default() -> Self {
        Self::new().expect("Failed to create ImportExtractor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_crate_import() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("crate::foo::bar");
        assert_eq!(kind, ImportKind::UseCrate);
        assert_eq!(path, vec!["crate", "foo", "bar"]);
        assert_eq!(names, vec!["bar"]);
        assert!(!is_glob);
    }

    #[test]
    fn test_parse_super_import() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("super::parent::foo");
        assert_eq!(kind, ImportKind::UseSuper);
        assert_eq!(path, vec!["super", "parent", "foo"]);
        assert_eq!(names, vec!["foo"]);
        assert!(!is_glob);
    }

    #[test]
    fn test_parse_self_import() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("self::local::baz");
        assert_eq!(kind, ImportKind::UseSelf);
        assert_eq!(path, vec!["self", "local", "baz"]);
        assert_eq!(names, vec!["baz"]);
        assert!(!is_glob);
    }

    #[test]
    fn test_parse_plain_import() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("std::collections::HashMap");
        assert_eq!(kind, ImportKind::PlainUse);
        assert_eq!(path, vec!["std", "collections", "HashMap"]);
        assert_eq!(names, vec!["HashMap"]);
        assert!(!is_glob);
    }

    #[test]
    fn test_parse_glob_import() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("std::collections::*");
        assert_eq!(kind, ImportKind::PlainUse);
        assert_eq!(path, vec!["std", "collections"]);
        assert!(names.is_empty());
        assert!(is_glob);
    }

    #[test]
    fn test_parse_braced_import() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("std::collections::{HashMap, HashSet}");
        assert_eq!(kind, ImportKind::PlainUse);
        assert_eq!(path, vec!["std", "collections"]);
        assert_eq!(names, vec!["HashMap", "HashSet"]);
        assert!(!is_glob);
    }

    #[test]
    fn test_parse_braced_import_with_as() {
        let extractor = ImportExtractor::new().unwrap();
        let (kind, path, names, is_glob) = extractor.parse_rust_import_path("std::collections::{HashMap as Map, HashSet}");
        assert_eq!(kind, ImportKind::PlainUse);
        assert_eq!(path, vec!["std", "collections"]);
        assert_eq!(names, vec!["HashMap", "HashSet"]);
        assert!(!is_glob);
    }

    #[test]
    fn test_extract_imports_from_rust_code() {
        let source = br#"
use std::collections::HashMap;
use crate::my_module::foo;
use super::parent::bar;
use self::local::baz;
use std::collections::*;
use std::collections::{HashMap, HashSet};
"#;
        let mut extractor = ImportExtractor::new().unwrap();
        let facts = extractor.extract_imports_rust(PathBuf::from("test.rs"), source);

        assert_eq!(facts.len(), 6);

        // Check first import (std::collections::HashMap)
        assert_eq!(facts[0].import_kind, ImportKind::PlainUse);
        assert_eq!(facts[0].import_path, vec!["std", "collections", "HashMap"]);
        assert_eq!(facts[0].imported_names, vec!["HashMap"]);
        assert!(!facts[0].is_glob);

        // Check crate import
        assert_eq!(facts[1].import_kind, ImportKind::UseCrate);
        assert_eq!(facts[1].import_path, vec!["crate", "my_module", "foo"]);

        // Check super import
        assert_eq!(facts[2].import_kind, ImportKind::UseSuper);

        // Check self import
        assert_eq!(facts[3].import_kind, ImportKind::UseSelf);

        // Check glob import
        assert!(facts[4].is_glob);

        // Check braced import
        assert_eq!(facts[5].imported_names, vec!["HashMap", "HashSet"]);
    }

    #[test]
    fn test_import_kind_serialization() {
        let kind = ImportKind::UseCrate;
        let json = serde_json::to_string(&kind).unwrap();
        let deserialized: ImportKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, deserialized);
    }

    #[test]
    fn test_import_kind_normalized_key() {
        assert_eq!(ImportKind::UseCrate.normalized_key(), "use_crate");
        assert_eq!(ImportKind::UseSuper.normalized_key(), "use_super");
        assert_eq!(ImportKind::UseSelf.normalized_key(), "use_self");
        assert_eq!(ImportKind::ExternCrate.normalized_key(), "extern_crate");
        assert_eq!(ImportKind::PlainUse.normalized_key(), "plain_use");
        assert_eq!(ImportKind::FromImport.normalized_key(), "from_import");
        assert_eq!(ImportKind::ImportStatement.normalized_key(), "import_statement");
    }

    #[test]
    fn test_import_kind_from_str() {
        assert_eq!(ImportKind::from_str("use_crate"), Some(ImportKind::UseCrate));
        assert_eq!(ImportKind::from_str("use_super"), Some(ImportKind::UseSuper));
        assert_eq!(ImportKind::from_str("use_self"), Some(ImportKind::UseSelf));
        assert_eq!(ImportKind::from_str("extern_crate"), Some(ImportKind::ExternCrate));
        assert_eq!(ImportKind::from_str("plain_use"), Some(ImportKind::PlainUse));
        assert_eq!(ImportKind::from_str("from_import"), Some(ImportKind::FromImport));
        assert_eq!(ImportKind::from_str("import_statement"), Some(ImportKind::ImportStatement));
        assert_eq!(ImportKind::from_str("invalid"), None);
    }
}

pub mod c;
pub mod cpp;
pub mod detect;
pub mod java;
pub mod javascript;
pub mod python;
pub mod typescript;

// Re-exports from detect module
pub use detect::{Language, detect_language};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Kind of symbol extracted from source code
///
/// Language-agnostic symbol kinds that map across multiple programming languages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolKind {
    /// Function definition
    Function,
    /// Method inside a class/impl block
    Method,
    /// Class or struct-like type definition
    /// Covers: Rust struct, Python class, Java class, C++ class, JS/TS class
    Class,
    /// Interface or trait definition
    /// Covers: Rust trait, Java interface, TypeScript interface
    Interface,
    /// Enum definition
    Enum,
    /// Module or package declaration
    /// Covers: Rust mod, Python module, Java package, JS/TS module
    Module,
    /// Union definition (C/C++)
    Union,
    /// Namespace definition
    /// Covers: C++ namespace, TypeScript namespace
    Namespace,
    /// Type alias
    /// Covers: TypeScript type, Rust type alias
    TypeAlias,
    /// Unknown symbol type
    Unknown,
}

/// A fact about a symbol extracted from source code
///
/// Pure data structure. No behavior. No semantic analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolFact {
    /// File containing this symbol
    pub file_path: PathBuf,
    /// Kind of symbol
    pub kind: SymbolKind,
    /// Symbol name (if any - some symbols like impl blocks may not have names)
    pub name: Option<String>,
    /// Byte offset where symbol starts in file
    pub byte_start: usize,
    /// Byte offset where symbol ends in file
    pub byte_end: usize,
    /// Line where symbol starts (1-indexed)
    pub start_line: usize,
    /// Column where symbol starts (0-indexed, bytes)
    pub start_col: usize,
    /// Line where symbol ends (1-indexed)
    pub end_line: usize,
    /// Column where symbol ends (0-indexed, bytes)
    pub end_col: usize,
}

/// Parser that extracts symbol facts from Rust source code
///
/// Pure function: Input (path, contents) → Output Vec<SymbolFact>
/// No filesystem access. No global state. No caching.
pub struct Parser {
    /// tree-sitter parser for Rust grammar
    parser: tree_sitter::Parser,
}

impl Parser {
    /// Create a new parser for Rust source code
    pub fn new() -> anyhow::Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_rust::language();
        parser.set_language(&language)?;

        Ok(Self { parser })
    }

    /// Extract symbol facts from Rust source code
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    ///
    /// # Returns
    /// Vector of symbol facts found in the source
    ///
    /// # Guarantees
    /// - Pure function: same input → same output
    /// - No side effects
    /// - No filesystem access
    pub fn extract_symbols(&mut self, file_path: PathBuf, source: &[u8]) -> Vec<SymbolFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(), // Parse error: return empty
        };

        let root_node = tree.root_node();
        let mut facts = Vec::new();

        // Walk the tree and extract symbols
        self.walk_tree(&root_node, &source, &file_path, &mut facts);

        facts
    }

    /// Walk tree-sitter tree recursively and extract symbols
    fn walk_tree(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
    ) {
        // Check if this node is a symbol we care about
        if let Some(fact) = self.extract_symbol(node, source, file_path) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(&child, source, file_path, facts);
        }
    }

    /// Extract a symbol fact from a tree-sitter node, if applicable
    fn extract_symbol(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Class,     // Rust struct → Class (language-agnostic)
            "enum_item" => SymbolKind::Enum,
            "trait_item" => SymbolKind::Interface,  // Rust trait → Interface (language-agnostic)
            "impl_item" => SymbolKind::Unknown,     // impl blocks have no name in v0
            "mod_item" => SymbolKind::Module,
            _ => return None, // Not a symbol we track
        };

        // Try to extract name
        let name = self.extract_name(node, source);

        Some(SymbolFact {
            file_path: file_path.clone(),
            kind: symbol_kind,
            name,
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
            start_line: node.start_position().row + 1, // tree-sitter is 0-indexed
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Extract name from a symbol node
    fn extract_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // For most items, the name is in a child named "identifier" or "type_identifier"
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" => {
                    let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }

        // Some symbols (like impl blocks) may not have names
        None
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new().expect("Failed to create parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_serialization() {
        let fact = SymbolFact {
            file_path: PathBuf::from("/test/file.rs"),
            kind: SymbolKind::Function,
            name: Some("test_fn".to_string()),
            byte_start: 0,
            byte_end: 100,
            start_line: 1,
            start_col: 0,
            end_line: 3,
            end_col: 1,
        };

        let json = serde_json::to_string(&fact).unwrap();
        let deserialized: SymbolFact = serde_json::from_str(&json).unwrap();

        assert_eq!(fact.file_path, deserialized.file_path);
        assert_eq!(fact.kind, deserialized.kind);
        assert_eq!(fact.name, deserialized.name);
    }
}

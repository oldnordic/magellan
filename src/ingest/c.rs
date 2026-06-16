//! C symbol extraction using tree-sitter-c.
//!
//! Extracts functions, structs, enums, and unions from C source code.

use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parser that extracts symbol facts from C source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct CParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl CParser {
    /// Create a new parser for C source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from C source code.
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
        // Per decision FQN-17, use "." as project_root placeholder for C
        let package_name = ".";
        self.walk_tree(&root_node, source, &file_path, package_name, &mut facts);

        facts
    }

    /// Walk tree-sitter tree recursively and extract symbols.
    fn walk_tree(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        package_name: &str,
        facts: &mut Vec<SymbolFact>,
    ) {
        // Check if this node is a symbol we care about
        if let Some(fact) = self.extract_symbol(node, source, file_path, package_name) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(&child, source, file_path, package_name, facts);
        }
    }

    /// Extract a symbol fact from a tree-sitter node, if applicable.
    fn extract_symbol(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        package_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_definition" => SymbolKind::Function,
            "struct_specifier" => SymbolKind::Class,
            "enum_specifier" => SymbolKind::Enum,
            "union_specifier" => SymbolKind::Union,
            _ => return None, // Not a symbol we track
        };

        // Try to extract name
        let name = self.extract_name(node, source);

        // Compute canonical and display FQNs
        // C has no namespaces, so we use an empty ScopeStack
        let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::DoubleColon,
        );
        let (canonical_fqn, display_fqn) = if let Some(ref name_str) = name {
            let canonical = builder.canonical(&scope_stack, symbol_kind.clone(), name_str);
            let display = builder.display(&scope_stack, symbol_kind.clone(), name_str);
            (Some(canonical), Some(display))
        } else {
            (None, None)
        };

        let normalized_kind = symbol_kind.normalized_key().to_string();
        // C has no namespaces/packages, FQN is just the symbol name
        let fqn = name.clone();
        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name,
            fqn,
            canonical_fqn,
            display_fqn,
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            start_line: node.start_position().row + 1, // tree-sitter is 0-indexed
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Extract name from a symbol node.
    fn extract_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // For C, function names are in "identifier" children
        // Struct/enum/union names are in "type_identifier" children
        // The identifier may be nested (e.g., function_definition > function_declarator > identifier)
        self.find_name_recursive(node, source)
    }

    /// Recursively search for identifier or type_identifier nodes.
    fn find_name_recursive(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" => {
                    let name_bytes = &source[child.start_byte()..child.end_byte()];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                // Skip declarator and parameter_list nodes to find the identifier within
                "function_declarator"
                | "parameter_list"
                | "field_declaration_list"
                | "enumerator_list" => {
                    if let Some(name) = self.find_name_recursive(&child, source) {
                        return Some(name);
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Extract symbol facts using an external parser (for parser pooling).
    ///
    /// This static method allows sharing a parser instance across multiple calls,
    /// reducing allocation overhead when parsing many files.
    pub fn extract_symbols_with_parser(
        parser: &mut tree_sitter::Parser,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root_node = tree.root_node();
        let mut facts = Vec::new();

        // Walk the tree and extract symbols
        // Per decision FQN-17, use "." as project_root placeholder for C
        let package_name = ".";
        Self::walk_tree_static(&root_node, source, &file_path, package_name, &mut facts);

        facts
    }

    /// Extract symbol facts from a pre-parsed tree.
    ///
    /// Avoids re-parsing when the tree is already available.
    pub fn extract_symbols_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let package_name = ".";
        Self::walk_tree_static(&root_node, source, &file_path, package_name, &mut facts);
        facts
    }

    /// Static version of walk_tree for external parser usage.
    fn walk_tree_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        package_name: &str,
        facts: &mut Vec<SymbolFact>,
    ) {
        // Check if this node is a symbol we care about
        if let Some(fact) = Self::extract_symbol_static(node, source, file_path, package_name) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk_tree_static(&child, source, file_path, package_name, facts);
        }
    }

    /// Static version of extract_symbol for external parser usage.
    fn extract_symbol_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        package_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_definition" => SymbolKind::Function,
            "struct_specifier" => SymbolKind::Class,
            "enum_specifier" => SymbolKind::Enum,
            "union_specifier" => SymbolKind::Union,
            _ => return None,
        };

        // Try to extract name
        let name = Self::find_name_recursive_static(node, source);

        // Compute canonical and display FQNs
        // C has no namespaces, so we use an empty ScopeStack
        let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::DoubleColon,
        );
        let (canonical_fqn, display_fqn) = if let Some(ref name_str) = name {
            let canonical = builder.canonical(&scope_stack, symbol_kind.clone(), name_str);
            let display = builder.display(&scope_stack, symbol_kind.clone(), name_str);
            (Some(canonical), Some(display))
        } else {
            (None, None)
        };

        let normalized_kind = symbol_kind.normalized_key().to_string();
        // C has no namespaces/packages, FQN is just the symbol name
        let fqn = name.clone();
        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name,
            fqn,
            canonical_fqn,
            display_fqn,
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Static version of find_name_recursive for external parser usage.
    fn find_name_recursive_static(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" => {
                    let name_bytes = &source[child.start_byte()..child.end_byte()];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                // Skip declarator and parameter_list nodes to find the identifier within
                "function_declarator"
                | "parameter_list"
                | "field_declaration_list"
                | "enumerator_list" => {
                    if let Some(name) = Self::find_name_recursive_static(&child, source) {
                        return Some(name);
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Extract reference facts from C source code.
    pub fn extract_references(
        &mut self,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<ReferenceFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_references_from_tree(&tree, file_path, source, symbols)
    }

    /// Extract reference facts from a pre-parsed tree.
    pub fn extract_references_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<ReferenceFact> {
        use crate::ingest::generic_extraction;
        generic_extraction::extract_references_from_tree(
            tree,
            file_path,
            source,
            symbols,
            |node| {
                matches!(
                    node.kind(),
                    "identifier" | "type_identifier" | "qualified_identifier"
                )
            },
            |node, source| {
                let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
                Some((text.to_string(), node.kind()))
            },
        )
    }

    /// Extract function call facts from C source code.
    pub fn extract_calls(
        &mut self,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<CallFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_calls_from_tree(&tree, file_path, source, symbols)
    }

    pub fn extract_calls_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<CallFact> {
        use crate::ingest::generic_extraction;
        generic_extraction::extract_calls_from_tree(
            tree,
            file_path,
            source,
            symbols,
            |node| node.kind() == "function_definition",
            Self::find_name_recursive_static,
            "call_expression",
            |node, source| {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" | "qualified_identifier" | "field_expression" => {
                            let text =
                                std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
                                    .ok()?;
                            return Some((text.to_string(), child.kind()));
                        }
                        _ => {}
                    }
                }
                None
            },
        )
    }
}

impl Default for CParser {
    fn default() -> Self {
        Self::new().expect("Failed to create C parser") // M-UNWRAP: tree-sitter language is a build-time invariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let mut parser = CParser::new().unwrap();
        let source = b"int main() {\n    return 0;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("main".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_struct() {
        let mut parser = CParser::new().unwrap();
        let source = b"struct Point {\n    int x;\n    int y;\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Point".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_enum() {
        let mut parser = CParser::new().unwrap();
        let source = b"enum Color {\n    RED,\n    GREEN,\n    BLUE\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Color".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_union() {
        let mut parser = CParser::new().unwrap();
        let source = b"union Data {\n    int i;\n    float f;\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Data".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Union);
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = CParser::new().unwrap();
        let source = b"
int main() {
    return 0;
}

struct Point {
    int x;
};

enum Color {
    RED,
    GREEN
};
";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert!(facts.len() >= 3);

        let functions: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 1);

        let structs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(structs.len(), 1);

        let enums: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);
    }

    #[test]
    fn test_empty_file() {
        let mut parser = CParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.c"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = CParser::new().unwrap();
        let source = b"int broken(\n    // invalid C";
        let facts = parser.extract_symbols(PathBuf::from("broken.c"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = CParser::new().unwrap();
        let source = b"int main() { return 0; }";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = CParser::new().unwrap();
        let source = b"int main() {\n    return 0;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Function starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'i' in 'int' is at column 0
    }

    #[test]
    fn test_fqn_is_simple_name() {
        let mut parser = CParser::new().unwrap();
        let source = b"int my_function() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.c"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fqn, Some("my_function".to_string()));
    }

    #[test]
    fn test_canonical_fqn_format() {
        let mut parser = CParser::new().unwrap();
        let source = b"int my_function() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("src/test.c"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Canonical FQN format: package_name::file_path::Kind symbol_name
        // For C: .::src/test.c::Function my_function
        assert_eq!(
            fact.canonical_fqn,
            Some(".::src/test.c::Function my_function".to_string())
        );
    }

    #[test]
    fn test_display_fqn_format() {
        let mut parser = CParser::new().unwrap();
        let source = b"int my_function() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("src/test.c"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Display FQN format: package_name::symbol_name
        // C has no namespaces, so display FQN is simpler
        assert_eq!(fact.display_fqn, Some(".::my_function".to_string()));
    }

    #[test]
    fn test_fqn_function() {
        let mut parser = CParser::new().unwrap();
        let source = b"void process_data(int x) { return; }\n";
        let facts = parser.extract_symbols(PathBuf::from("lib/helpers.c"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert_eq!(fact.name, Some("process_data".to_string()));
        assert_eq!(fact.kind, SymbolKind::Function);
        assert_eq!(
            fact.canonical_fqn,
            Some(".::lib/helpers.c::Function process_data".to_string())
        );
        assert_eq!(fact.display_fqn, Some(".::process_data".to_string()));
    }

    #[test]
    fn test_fqn_struct() {
        let mut parser = CParser::new().unwrap();
        let source = b"struct Point { int x; int y; };\n";
        let facts = parser.extract_symbols(PathBuf::from("types/geometry.c"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert_eq!(fact.name, Some("Point".to_string()));
        assert_eq!(fact.kind, SymbolKind::Class);
        // Note: kind_string maps Class -> "Struct" for canonical FQN
        assert_eq!(
            fact.canonical_fqn,
            Some(".::types/geometry.c::Struct Point".to_string())
        );
        assert_eq!(fact.display_fqn, Some(".::Point".to_string()));
    }
}

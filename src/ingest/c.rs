//! C symbol extraction using tree-sitter-c.
//!
//! Extracts functions, structs, enums, and unions from C source code.

use crate::ingest::{SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::PathBuf;

/// Parser that extracts symbol facts from C source code.
///
/// Pure function: Input (path, contents) → Output Vec<SymbolFact>
/// No filesystem access. No global state. No caching.
pub struct CParser {
    parser: tree_sitter::Parser,
}

impl CParser {
    /// Create a new parser for C source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_c::language())?;
        Ok(Self { parser })
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
        self.walk_tree(&root_node, source, &file_path, &mut facts);

        facts
    }

    /// Walk tree-sitter tree recursively and extract symbols.
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

    /// Extract a symbol fact from a tree-sitter node, if applicable.
    fn extract_symbol(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
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

        let normalized_kind = symbol_kind.normalized_key().to_string();
        // C has no namespaces/packages, FQN is just the symbol name
        let fqn = name.clone();
        Some(SymbolFact {
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name,
            fqn,
            canonical_fqn: None,
            display_fqn: None,
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
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
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
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
        Self::walk_tree_static(&root_node, source, &file_path, &mut facts);

        facts
    }

    /// Static version of walk_tree for external parser usage.
    fn walk_tree_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
    ) {
        // Check if this node is a symbol we care about
        if let Some(fact) = Self::extract_symbol_static(node, source, file_path) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk_tree_static(&child, source, file_path, facts);
        }
    }

    /// Static version of extract_symbol for external parser usage.
    fn extract_symbol_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
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

        let normalized_kind = symbol_kind.normalized_key().to_string();
        // C has no namespaces/packages, FQN is just the symbol name
        let fqn = name.clone();
        Some(SymbolFact {
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name,
            fqn,
            canonical_fqn: None,
            display_fqn: None,
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
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
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
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

        let root_node = tree.root_node();
        let mut references = Vec::new();
        self.walk_tree_for_references(&root_node, source, &file_path, symbols, &mut references);
        references
    }

    fn walk_tree_for_references(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbols: &[SymbolFact],
        references: &mut Vec<ReferenceFact>,
    ) {
        if let Some(reference) = self.extract_reference(node, source, file_path, symbols) {
            references.push(reference);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_references(&child, source, file_path, symbols, references);
        }
    }

    fn extract_reference(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbols: &[SymbolFact],
    ) -> Option<ReferenceFact> {
        if node.kind() != "identifier" && node.kind() != "type_identifier" {
            return None;
        }

        let text_bytes = &source[node.start_byte() as usize..node.end_byte() as usize];
        let text = std::str::from_utf8(text_bytes).ok()?;

        let referenced_symbol = symbols
            .iter()
            .find(|s| s.name.as_ref().map(|n| n == text).unwrap_or(false))?;

        let ref_start = node.start_byte() as usize;
        if ref_start < referenced_symbol.byte_end {
            return None;
        }

        Some(ReferenceFact {
            file_path: file_path.clone(),
            referenced_symbol: text.to_string(),
            byte_start: ref_start,
            byte_end: node.end_byte() as usize,
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
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

        let root_node = tree.root_node();
        let mut calls = Vec::new();

        let symbol_map: std::collections::HashMap<String, &SymbolFact> = symbols
            .iter()
            .filter_map(|s| s.name.as_ref().map(|name| (name.clone(), s)))
            .collect();

        let functions: Vec<&SymbolFact> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        self.walk_tree_for_calls(
            &root_node,
            source,
            &file_path,
            &symbol_map,
            &functions,
            &mut calls,
        );
        calls
    }

    fn walk_tree_for_calls(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbol_map: &std::collections::HashMap<String, &SymbolFact>,
        _functions: &[&SymbolFact],
        calls: &mut Vec<CallFact>,
    ) {
        self.walk_tree_for_calls_with_caller(node, source, file_path, symbol_map, None, calls);
    }

    fn walk_tree_for_calls_with_caller(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbol_map: &std::collections::HashMap<String, &SymbolFact>,
        current_caller: Option<&SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        let kind = node.kind();

        let caller: Option<&SymbolFact> = if kind == "function_definition" {
            self.extract_function_name(node, source)
                .and_then(|name| symbol_map.get(&name).copied())
        } else {
            current_caller
        };

        if kind == "call_expression" {
            if let Some(caller_fact) = caller {
                self.extract_calls_in_node(node, source, file_path, caller_fact, symbol_map, calls);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_calls_with_caller(
                &child, source, file_path, symbol_map, caller, calls,
            );
        }
    }

    fn extract_function_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        self.find_name_recursive(node, source)
    }

    fn extract_calls_in_node(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        caller: &SymbolFact,
        symbol_map: &std::collections::HashMap<String, &SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        if node.kind() == "call_expression" {
            if let Some(callee_name) = self.extract_callee_from_call(node, source) {
                if symbol_map.contains_key(&callee_name) {
                    let node_start = node.start_byte() as usize;
                    let node_end = node.end_byte() as usize;
                    let call_fact = CallFact {
                        file_path: file_path.clone(),
                        caller: caller.name.clone().unwrap_or_default(),
                        callee: callee_name,
                        caller_symbol_id: None,
                        callee_symbol_id: None,
                        byte_start: node_start,
                        byte_end: node_end,
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    };
                    calls.push(call_fact);
                }
            }
        }
    }

    fn extract_callee_from_call(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }
}

impl Default for CParser {
    fn default() -> Self {
        Self::new().expect("Failed to create C parser")
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
}

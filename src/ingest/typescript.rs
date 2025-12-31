//! TypeScript symbol extraction using tree-sitter-typescript.
//!
//! Extracts functions, classes, interfaces, methods, types, enums, and namespaces from TypeScript source code.

use crate::ingest::{SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::PathBuf;

/// Parser that extracts symbol facts from TypeScript source code.
///
/// Pure function: Input (path, contents) → Output Vec<SymbolFact>
/// No filesystem access. No global state. No caching.
pub struct TypeScriptParser {
    parser: tree_sitter::Parser,
}

impl TypeScriptParser {
    /// Create a new parser for TypeScript source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_typescript::language_typescript())?;
        Ok(Self { parser })
    }

    /// Extract symbol facts from TypeScript source code.
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

        // export_statement wraps the actual declaration - skip it here
        if kind == "export_statement" {
            return None;
        }

        let symbol_kind = match kind {
            "function_declaration" => SymbolKind::Function,
            "class_declaration" => SymbolKind::Class,
            "method_definition" => SymbolKind::Method,
            "interface_declaration" => SymbolKind::Interface,
            "type_alias_declaration" => SymbolKind::TypeAlias,
            "enum_declaration" => SymbolKind::Enum,
            "internal_module" => SymbolKind::Namespace,
            _ => return None, // Not a symbol we track
        };

        // Try to extract name
        let name = self.extract_name(node, source, kind);

        let normalized_kind = symbol_kind.normalized_key().to_string();
        Some(SymbolFact {
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name,
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
            start_line: node.start_position().row + 1, // tree-sitter is 0-indexed
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Extract name from a symbol node.
    fn extract_name(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        node_kind: &str,
    ) -> Option<String> {
        // For namespace (internal_module), name is in "identifier" child
        if node_kind == "internal_module" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
            }
            return None;
        }

        // For other symbols, name is in "identifier" or "type_identifier" or "property_identifier" child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" | "property_identifier" => {
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }

        None
    }

    /// Extract reference facts from TypeScript source code.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against)
    ///
    /// # Returns
    /// Vector of reference facts found in the source
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

        // Walk tree and find references
        self.walk_tree_for_references(&root_node, source, &file_path, symbols, &mut references);

        references
    }

    /// Walk tree-sitter tree recursively and extract references
    fn walk_tree_for_references(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbols: &[SymbolFact],
        references: &mut Vec<ReferenceFact>,
    ) {
        // Check if this node is a reference we care about
        if let Some(reference) = self.extract_reference(node, source, file_path, symbols) {
            references.push(reference);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_references(&child, source, file_path, symbols, references);
        }
    }

    /// Extract a reference fact from a tree-sitter node, if applicable
    fn extract_reference(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbols: &[SymbolFact],
    ) -> Option<ReferenceFact> {
        // Only process identifier nodes
        if node.kind() != "identifier" {
            return None;
        }

        // Get the text of this node
        let text_bytes = &source[node.start_byte() as usize..node.end_byte() as usize];
        let text = std::str::from_utf8(text_bytes).ok()?;

        // Find if this matches any symbol
        let referenced_symbol = symbols
            .iter()
            .find(|s| s.name.as_ref().map(|n| n == text).unwrap_or(false))?;

        // Check if reference is OUTSIDE the symbol's defining span
        let ref_start = node.start_byte() as usize;

        // Reference must start after the symbol's definition ends
        if ref_start < referenced_symbol.byte_end {
            return None; // Reference is within defining span
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

    /// Extract function call facts from TypeScript source code.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against)
    ///
    /// # Returns
    /// Vector of CallFact representing caller -> callee relationships
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

        // Build map: symbol name -> symbol fact
        let symbol_map: std::collections::HashMap<String, &SymbolFact> = symbols
            .iter()
            .filter_map(|s| s.name.as_ref().map(|name| (name.clone(), s)))
            .collect();

        // Filter to only functions (potential callers)
        let functions: Vec<&SymbolFact> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        // Walk tree and find calls
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

    /// Walk tree-sitter tree and extract function calls
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

    /// Walk tree-sitter tree and extract function calls, tracking current function
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

        // Track which function we're inside (if any)
        let caller: Option<&SymbolFact> = if kind == "function_declaration" {
            self.extract_function_name(node, source)
                .and_then(|name| symbol_map.get(&name).copied())
        } else {
            current_caller
        };

        // If we have a caller and this is a call, extract the call
        if kind == "call_expression" {
            if let Some(caller_fact) = caller {
                self.extract_calls_in_node(node, source, file_path, caller_fact, symbol_map, calls);
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_calls_with_caller(
                &child, source, file_path, symbol_map, caller, calls,
            );
        }
    }

    /// Extract function name from a function_declaration node
    fn extract_function_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }

    /// Extract calls within a node (function body)
    fn extract_calls_in_node(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        caller: &SymbolFact,
        symbol_map: &std::collections::HashMap<String, &SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        // Look for call_expression nodes
        let kind = node.kind();

        if kind == "call_expression" {
            // Extract the function being called
            if let Some(callee_name) = self.extract_callee_from_call(node, source) {
                // Only create call if callee is a known function symbol
                if symbol_map.contains_key(&callee_name) {
                    let node_start = node.start_byte() as usize;
                    let node_end = node.end_byte() as usize;
                    let call_fact = CallFact {
                        file_path: file_path.clone(),
                        caller: caller.name.clone().unwrap_or_default(),
                        callee: callee_name,
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

    /// Extract callee name from a call_expression node
    fn extract_callee_from_call(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // The callee is typically an identifier child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
            // Handle member_expression calls like obj.method() - we want the property name
            if child.kind() == "member_expression" {
                return self.extract_property_name(&child, source);
            }
        }
        None
    }

    /// Extract property name from a member_expression node (for obj.method() calls)
    fn extract_property_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // Find the property_identifier (second child in obj.method)
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        if children.len() >= 2 {
            // For member_expression, look for property_identifier
            for child in &children {
                if child.kind() == "property_identifier" {
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
            }
        }
        None
    }
}

impl Default for TypeScriptParser {
    fn default() -> Self {
        Self::new().expect("Failed to create TypeScript parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"function foo(): void {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("foo".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"class MyClass {\n    constructor() {}\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        // Should extract class and constructor method (flat structure)
        assert!(facts.len() >= 1);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, Some("MyClass".to_string()));
    }

    #[test]
    fn test_extract_interface() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"interface MyInterface {\n    method(): void;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyInterface".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_type_alias() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"type MyType = string | number;\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyType".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_enum() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"enum Color {\n    Red = 0,\n    Green\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Color".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_namespace() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"namespace MyNamespace {\n    class Foo {}\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        // Should extract namespace and nested class (flat structure)
        assert!(facts.len() >= 1);

        let namespaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Namespace)
            .collect();
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].name, Some("MyNamespace".to_string()));
    }

    #[test]
    fn test_extract_generic_class() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"class Generic<T> {\n    value: T;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        // Should extract the generic class
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Generic".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_export_interface() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"export interface MyInterface {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        // Should extract the interface (walk recurses past export_statement)
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyInterface".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_extract_export_type() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"export type MyType = string;\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        // Should extract the type alias (walk recurses past export_statement)
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyType".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"
interface MyInterface {
    method(): void;
}

type MyType = string;

enum Color {
    Red
}

function foo(): void {}
";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        // Should extract: interface, method signature, type alias, enum, function
        assert!(facts.len() >= 4);

        let interfaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 1);

        let type_aliases: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::TypeAlias)
            .collect();
        assert_eq!(type_aliases.len(), 1);

        let enums: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);

        let functions: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 1);
    }

    #[test]
    fn test_empty_file() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.ts"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"interface Broken {\n    // invalid ts";
        let facts = parser.extract_symbols(PathBuf::from("broken.ts"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"interface Foo {}";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = TypeScriptParser::new().unwrap();
        let source = b"interface Foo {\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.ts"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Interface starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'i' in 'interface' is at column 0
    }
}

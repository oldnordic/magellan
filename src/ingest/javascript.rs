//! JavaScript symbol extraction using tree-sitter-javascript.
//!
//! Extracts functions, classes, and methods from JavaScript source code.

use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::PathBuf;

/// Parser that extracts symbol facts from JavaScript source code.
///
/// Pure function: Input (path, contents) → Output Vec<SymbolFact>
/// No filesystem access. No global state. No caching.
pub struct JavaScriptParser {
    parser: tree_sitter::Parser,
}

impl JavaScriptParser {
    /// Create a new parser for JavaScript source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::language())?;
        Ok(Self { parser })
    }

    /// Extract symbol facts from JavaScript source code.
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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);

        // Walk tree with scope tracking
        self.walk_tree_with_scope(&root_node, source, &file_path, &mut facts, &mut scope_stack);

        facts
    }

    /// Walk tree-sitter tree recursively with scope tracking
    ///
    /// Tracks class scope boundaries to build proper FQNs.
    /// - class_declaration: pushes class name to scope
    fn walk_tree_with_scope(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
    ) {
        let kind = node.kind();

        // export_statement wraps the actual declaration - skip it here
        // The walk_tree will recurse into its children and find the actual symbol
        if kind == "export_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree_with_scope(&child, source, file_path, facts, scope_stack);
            }
            return;
        }

        // Track class scope
        if kind == "class_declaration" {
            if let Some(name) = self.extract_name(node, source) {
                // Create class symbol with parent scope
                if let Some(fact) = self.extract_symbol_with_fqn(node, source, file_path, scope_stack) {
                    facts.push(fact);
                }
                // Push class scope for children (methods)
                scope_stack.push(&name);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_tree_with_scope(&child, source, file_path, facts, scope_stack);
                }
                scope_stack.pop();
                return;
            }
        }

        // Check if this node is a symbol we care about
        if let Some(fact) = self.extract_symbol_with_fqn(node, source, file_path, scope_stack) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_with_scope(&child, source, file_path, facts, scope_stack);
        }
    }

    /// Extract a symbol fact with FQN from a tree-sitter node, if applicable
    ///
    /// Uses the current scope stack to build a fully-qualified name.
    /// Creates symbols for all relevant node types including class_declaration.
    fn extract_symbol_with_fqn(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        scope_stack: &ScopeStack,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_declaration" => SymbolKind::Function,
            "method_definition" => SymbolKind::Method,
            "class_declaration" => SymbolKind::Class,
            _ => return None, // Not a symbol we track
        };

        let name = self.extract_name(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        Some(SymbolFact {
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
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
        // For functions and classes, name is in "identifier" child
        // For methods, name is in "property_identifier" child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "property_identifier" => {
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }

        None
    }

    /// Extract reference facts from JavaScript source code.
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

    /// Extract function call facts from JavaScript source code.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against)
    ///
    /// # Returns
    /// Vector of CallFact representing caller → callee relationships
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

        // Build map: symbol name → symbol fact
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
        let caller: Option<&SymbolFact> =
            if kind == "function_declaration" || kind == "function_definition" {
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

    /// Extract function name from a function_declaration or function_definition node
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

    /// Extract callee name from a call_expression node
    fn extract_callee_from_call(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // The callee is typically an identifier child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
            // Handle member_expression calls like obj.method() - we want the method name
            if child.kind() == "member_expression" {
                return self.extract_member_expression_name(&child, source);
            }
        }
        None
    }

    /// Extract member expression property name (for obj.method() calls)
    fn extract_member_expression_name(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
    ) -> Option<String> {
        // For member_expression obj.prop, we want 'prop'
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        if children.len() >= 2 {
            // Find property_identifier (second child in obj.prop)
            let prop = &children[1];
            if prop.kind() == "property_identifier" {
                let name_bytes = &source[prop.start_byte() as usize..prop.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }
}

impl Default for JavaScriptParser {
    fn default() -> Self {
        Self::new().expect("Failed to create JavaScript parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"function foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("foo".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"class MyClass {\n    constructor() {}\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

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
    fn test_extract_method() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"class MyClass {\n    myMethod() {\n        return;\n    }\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        // Should extract class and method (flat structure)
        assert!(facts.len() >= 2);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, Some("myMethod".to_string()));
    }

    #[test]
    fn test_extract_export_function() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"export function foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        // Should extract the function (walk recurses past export_statement)
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("foo".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_export_class() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"export class MyClass {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        // Should extract the class (walk recurses past export_statement)
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_export_default() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"export default class Foo {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        // Should extract the class (walk recurses past export_statement)
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Foo".to_string()));
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"
function foo() {}

class Bar {
    method1() {}
    method2() {}
}

export function baz() {}
";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        // Should extract: foo, Bar, method1, method2, baz
        assert!(facts.len() >= 5);

        let functions: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 2); // foo and baz

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1); // Bar

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 2); // method1 and method2
    }

    #[test]
    fn test_empty_file() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.js"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"function broken(\n    // invalid js";
        let facts = parser.extract_symbols(PathBuf::from("broken.js"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"function foo() {}";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"function foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Function starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'f' in 'function' is at column 0
    }

    #[test]
    fn test_fqn_class_method() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"
class MyClass {
    myMethod() {
        return;
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].fqn, Some("MyClass".to_string()));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fqn, Some("MyClass.myMethod".to_string()));
    }
}

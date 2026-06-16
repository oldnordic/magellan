//! JavaScript symbol extraction using tree-sitter-javascript.
//!
//! Extracts functions, classes, and methods from JavaScript source code.

use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parser that extracts symbol facts from JavaScript source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct JavaScriptParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl JavaScriptParser {
    /// Create a new parser for JavaScript source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
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

        // Use "." as project_root placeholder per decision FQN-17
        let package_name = ".";

        // Walk tree with scope tracking
        self.walk_tree_with_scope(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            package_name,
        );

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
        file_path: &Path,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        package_name: &str,
    ) {
        let kind = node.kind();

        // export_statement wraps the actual declaration - skip it here
        // The walk_tree will recurse into its children and find the actual symbol
        if kind == "export_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree_with_scope(
                    &child,
                    source,
                    file_path,
                    facts,
                    scope_stack,
                    package_name,
                );
            }
            return;
        }

        // Track class scope
        if kind == "class_declaration" {
            if let Some(name) = self.extract_name(node, source) {
                // Create class symbol with parent scope
                if let Some(fact) =
                    self.extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
                {
                    facts.push(fact);
                }
                // Push class scope for children (methods)
                scope_stack.push(&name);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_tree_with_scope(
                        &child,
                        source,
                        file_path,
                        facts,
                        scope_stack,
                        package_name,
                    );
                }
                scope_stack.pop();
                return;
            }
        }

        // Check if this node is a symbol we care about
        if let Some(fact) =
            self.extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
        {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
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
        file_path: &Path,
        scope_stack: &ScopeStack,
        package_name: &str,
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

        // Build canonical and display FQNs using FqnBuilder
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::Dot,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
            canonical_fqn: Some(canonical_fqn),
            display_fqn: Some(display_fqn),
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
        // For functions and classes, name is in "identifier" child
        // For methods, name is in "property_identifier" child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "property_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);

        // Use "." as project_root placeholder per decision FQN-17
        let package_name = ".";

        // Walk tree with scope tracking
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            package_name,
        );

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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);
        let package_name = ".";
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            package_name,
        );
        facts
    }

    /// Static version of walk_tree_with_scope for external parser usage.
    fn walk_tree_with_scope_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        package_name: &str,
    ) {
        let kind = node.kind();

        // export_statement wraps the actual declaration - skip it here
        if kind == "export_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                Self::walk_tree_with_scope_static(
                    &child,
                    source,
                    file_path,
                    facts,
                    scope_stack,
                    package_name,
                );
            }
            return;
        }

        // Track class scope
        if kind == "class_declaration" {
            if let Some(name) = Self::extract_name_static(node, source) {
                // Create class symbol with parent scope
                if let Some(fact) = Self::extract_symbol_with_fqn_static(
                    node,
                    source,
                    file_path,
                    scope_stack,
                    package_name,
                ) {
                    facts.push(fact);
                }
                // Push class scope for children (methods)
                scope_stack.push(&name);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    Self::walk_tree_with_scope_static(
                        &child,
                        source,
                        file_path,
                        facts,
                        scope_stack,
                        package_name,
                    );
                }
                scope_stack.pop();
                return;
            }
        }

        // Check if this node is a symbol we care about
        if let Some(fact) =
            Self::extract_symbol_with_fqn_static(node, source, file_path, scope_stack, package_name)
        {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk_tree_with_scope_static(
                &child,
                source,
                file_path,
                facts,
                scope_stack,
                package_name,
            );
        }
    }

    /// Static version of extract_symbol_with_fqn for external parser usage.
    fn extract_symbol_with_fqn_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
        package_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_declaration" => SymbolKind::Function,
            "method_definition" => SymbolKind::Method,
            "class_declaration" => SymbolKind::Class,
            _ => return None,
        };

        let name = Self::extract_name_static(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Build canonical and display FQNs using FqnBuilder
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::Dot,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
            canonical_fqn: Some(canonical_fqn),
            display_fqn: Some(display_fqn),
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Static version of extract_name for external parser usage.
    fn extract_name_static(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // For functions and classes, name is in "identifier" child
        // For methods, name is in "property_identifier" child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "property_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
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
                    "identifier" | "property_identifier" | "member_expression"
                )
            },
            |node, source| {
                let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
                Some((text.to_string(), node.kind()))
            },
        )
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
            |node| {
                matches!(
                    node.kind(),
                    "function_declaration"
                        | "function_definition"
                        | "method_definition"
                        | "arrow_function"
                )
            },
            Self::extract_function_name_static,
            "call_expression",
            |node, source| {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" | "property_identifier" | "member_expression" => {
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

    fn extract_function_name_static(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "property_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }
        None
    }
}

impl Default for JavaScriptParser {
    fn default() -> Self {
        Self::new().expect("Failed to create JavaScript parser") // M-UNWRAP: tree-sitter language is a build-time invariant
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
        assert!(!facts.is_empty());

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

    #[test]
    fn test_canonical_fqn_format() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"function foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("src/test.js"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Canonical FQN format: package_name::file_path::Kind symbol_name
        assert!(fact.canonical_fqn.is_some());
        let canonical = fact.canonical_fqn.as_ref().unwrap();
        assert!(canonical.contains(".::src/test.js::Function foo"));
    }

    #[test]
    fn test_display_fqn_format() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"function foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.js"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Display FQN format: package_name.symbol_name (for top-level functions)
        // Note: package_name "." is placeholder, so we get "..foo"
        assert!(fact.display_fqn.is_some());
        let display = fact.display_fqn.as_ref().unwrap();
        assert_eq!(display, "..foo");
    }

    #[test]
    fn test_fqn_class_method_with_fqn_builder() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"
class MyClass {
    myMethod() {
        return;
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("src/test.js"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        let class_fact = &classes[0];

        // Class canonical FQN includes file path
        assert!(class_fact.canonical_fqn.is_some());
        assert!(class_fact
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains(".::src/test.js::Struct MyClass"));

        // Class display FQN is just the class name with package
        // Note: package_name "." is placeholder, so we get "..MyClass"
        assert_eq!(class_fact.display_fqn.as_ref().unwrap(), "..MyClass");

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        let method_fact = &methods[0];

        // Method canonical FQN includes file path
        assert!(method_fact.canonical_fqn.is_some());
        assert!(method_fact
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains(".::src/test.js::Method myMethod"));

        // Method display FQN includes class scope
        // Note: package_name "." is placeholder, so we get "..MyClass.myMethod"
        assert_eq!(
            method_fact.display_fqn.as_ref().unwrap(),
            "..MyClass.myMethod"
        );
    }

    #[test]
    fn test_fqn_export_function() {
        let mut parser = JavaScriptParser::new().unwrap();
        let source = b"export function foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("src/test.js"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Exported functions should have both FQN types
        assert!(fact.canonical_fqn.is_some());
        assert!(fact.display_fqn.is_some());

        // Canonical FQN includes file path
        assert!(fact
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains(".::src/test.js::Function foo"));

        // Display FQN is human-readable
        // Note: package_name "." is placeholder, so we get "..foo"
        assert_eq!(fact.display_fqn.as_ref().unwrap(), "..foo");
    }
}

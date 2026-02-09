//! Java symbol extraction using tree-sitter-java.
//!
//! Extracts classes, interfaces, enums, methods, and packages from Java source code.

use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// Parser that extracts symbol facts from Java source code.
///
/// Pure function: Input (path, contents) → Output Vec<SymbolFact>
/// No filesystem access. No global state. No caching.
pub struct JavaParser {
    parser: tree_sitter::Parser,
}

impl JavaParser {
    /// Create a new parser for Java source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::language())?;
        Ok(Self { parser })
    }

    /// Extract symbol facts from Java source code.
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

        // Find package declaration first (it comes first in the file)
        let mut pkg_name = String::new();
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if child.kind() == "package_declaration" {
                if let Some(name) = self.extract_name(&child, source, "package_declaration") {
                    pkg_name = name.clone();
                    // Extract the package symbol itself (before pushing to scope)
                    if let Some(fact) = self.extract_symbol_with_fqn(
                        &child,
                        source,
                        &file_path,
                        &scope_stack,
                        &pkg_name,
                    ) {
                        facts.push(fact);
                    }
                    // Package becomes root scope: com.example.Class
                    for part in pkg_name.split('.') {
                        scope_stack.push(part);
                    }
                }
                break;
            }
        }

        // Walk tree with scope tracking
        self.walk_tree_with_scope(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            &pkg_name,
        );

        facts
    }

    /// Walk tree-sitter tree recursively with scope tracking
    ///
    /// Tracks class and interface scope boundaries to build proper FQNs.
    /// - class_declaration: pushes class name to scope
    /// - interface_declaration: pushes interface name to scope
    /// - enum_declaration: pushes enum name to scope
    fn walk_tree_with_scope(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        package_name: &str,
    ) {
        let kind = node.kind();

        // Skip package_declaration (already handled)
        if kind == "package_declaration" {
            return;
        }

        // Track type scope
        let is_type_scope = matches!(
            kind,
            "class_declaration" | "interface_declaration" | "enum_declaration"
        );

        if is_type_scope {
            if let Some(name) = self.extract_name(node, source, kind) {
                // Create type symbol with parent scope
                if let Some(fact) =
                    self.extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
                {
                    facts.push(fact);
                }
                // Push type scope for children (methods, nested types)
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
    /// Creates symbols for all relevant node types including type scope nodes.
    fn extract_symbol_with_fqn(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        scope_stack: &ScopeStack,
        _package_name: &str, // Not used - package is in ScopeStack
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "method_declaration" => SymbolKind::Method,
            "class_declaration" => SymbolKind::Class,
            "interface_declaration" => SymbolKind::Interface,
            "enum_declaration" => SymbolKind::Enum,
            "package_declaration" => SymbolKind::Module,
            _ => return None, // Not a symbol we track
        };

        let name = self.extract_name(node, source, kind)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Compute canonical and display FQN using FqnBuilder
        // For Java: use empty crate_name since package is already in ScopeStack
        let builder = FqnBuilder::new(
            String::new(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::Dot,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.clone(),
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
    ///
    /// Java uses different identifier patterns:
    /// - Classes/interfaces/enums: direct `identifier` child
    /// - Methods: direct `identifier` child
    /// - Packages: `scoped_identifier` child (e.g., com.example)
    fn extract_name(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        node_kind: &str,
    ) -> Option<String> {
        // For package_declaration, extract from scoped_identifier
        if node_kind == "package_declaration" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
            }
            return None;
        }

        // For other symbols, name is a direct identifier child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
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

        // Find package declaration first (it comes first in the file)
        let mut pkg_name = String::new();
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if child.kind() == "package_declaration" {
                if let Some(name) = Self::extract_name_static(&child, source, "package_declaration")
                {
                    pkg_name = name.clone();
                    // Extract the package symbol itself (before pushing to scope)
                    if let Some(fact) = Self::extract_symbol_with_fqn_static(
                        &child,
                        source,
                        &file_path,
                        &scope_stack,
                        &pkg_name,
                    ) {
                        facts.push(fact);
                    }
                    // Package becomes root scope: com.example.Class
                    for part in pkg_name.split('.') {
                        scope_stack.push(part);
                    }
                }
                break;
            }
        }

        // Walk tree with scope tracking
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            &pkg_name,
        );

        facts
    }

    /// Static version of walk_tree_with_scope for external parser usage.
    fn walk_tree_with_scope_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        package_name: &str,
    ) {
        let kind = node.kind();

        // Skip package_declaration (already handled)
        if kind == "package_declaration" {
            return;
        }

        // Track type scope
        let is_type_scope = matches!(
            kind,
            "class_declaration" | "interface_declaration" | "enum_declaration"
        );

        if is_type_scope {
            if let Some(name) = Self::extract_name_static(node, source, kind) {
                // Create type symbol with parent scope
                if let Some(fact) = Self::extract_symbol_with_fqn_static(
                    node,
                    source,
                    file_path,
                    scope_stack,
                    package_name,
                ) {
                    facts.push(fact);
                }
                // Push type scope for children (methods, nested types)
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
        file_path: &PathBuf,
        scope_stack: &ScopeStack,
        _package_name: &str, // Not used - package is in ScopeStack
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "method_declaration" => SymbolKind::Method,
            "class_declaration" => SymbolKind::Class,
            "interface_declaration" => SymbolKind::Interface,
            "enum_declaration" => SymbolKind::Enum,
            "package_declaration" => SymbolKind::Module,
            _ => return None,
        };

        let name = Self::extract_name_static(node, source, kind)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Compute canonical and display FQN using FqnBuilder
        // For Java: use empty crate_name since package is already in ScopeStack
        let builder = FqnBuilder::new(
            String::new(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::Dot,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.clone(),
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
    fn extract_name_static(
        node: &tree_sitter::Node,
        source: &[u8],
        node_kind: &str,
    ) -> Option<String> {
        // For package_declaration, extract from scoped_identifier
        if node_kind == "package_declaration" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
            }
            return None;
        }

        // For other symbols, name is a direct identifier child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }

        None
    }

    /// Extract reference facts from Java source code.
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
        if node.kind() != "identifier" {
            return None;
        }

        let text_bytes = &source[node.start_byte()..node.end_byte()];
        let text = std::str::from_utf8(text_bytes).ok()?;

        let referenced_symbol = symbols
            .iter()
            .find(|s| s.name.as_ref().map(|n| n == text).unwrap_or(false))?;

        let ref_start = node.start_byte();
        if ref_start < referenced_symbol.byte_end {
            return None;
        }

        Some(ReferenceFact {
            file_path: file_path.clone(),
            referenced_symbol: text.to_string(),
            byte_start: ref_start,
            byte_end: node.end_byte(),
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Extract function call facts from Java source code.
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

        let symbol_map: HashMap<String, &SymbolFact> = symbols
            .iter()
            .filter_map(|s| s.name.as_ref().map(|name| (name.clone(), s)))
            .collect();

        let functions: Vec<&SymbolFact> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Method)
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
        symbol_map: &HashMap<String, &SymbolFact>,
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
        symbol_map: &HashMap<String, &SymbolFact>,
        current_caller: Option<&SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        let kind = node.kind();

        let caller: Option<&SymbolFact> = if kind == "method_declaration" {
            self.extract_function_name(node, source)
                .and_then(|name| symbol_map.get(&name).copied())
        } else {
            current_caller
        };

        if kind == "method_invocation" {
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
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }

    fn extract_calls_in_node(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        caller: &SymbolFact,
        symbol_map: &HashMap<String, &SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        if node.kind() == "method_invocation" {
            if let Some(callee_name) = self.extract_callee_from_call(node, source) {
                if symbol_map.contains_key(&callee_name) {
                    let node_start = node.start_byte();
                    let node_end = node.end_byte();
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
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }
}

impl Default for JavaParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Java parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_class() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"public class MyClass {\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_interface() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"interface MyInterface {\n    void method();\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract interface and method (flat structure)
        assert!(facts.len() >= 1);

        let interfaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].name, Some("MyInterface".to_string()));
    }

    #[test]
    fn test_extract_enum() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"enum Color {\n    RED, GREEN, BLUE\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Color".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_method() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class MyClass {\n    void myMethod() {}\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

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
    fn test_extract_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"package com.example;\n\nclass Foo {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract package and class
        assert!(facts.len() >= 1);

        let modules: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, Some("com.example".to_string()));
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.test;

class MyClass {
    void method1() {}
}

interface MyInterface {
    void method2();
}

enum Color {
    RED
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract: package, class, method1, interface, method2, enum
        assert!(facts.len() >= 6);

        let modules: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);

        let interfaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 1);

        let enums: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 2); // method1 and method2
    }

    #[test]
    fn test_empty_file() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.java"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class Broken {\n    // invalid java";
        let facts = parser.extract_symbols(PathBuf::from("broken.java"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class Foo {}";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class Foo {\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Class starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'c' in 'class' is at column 0
    }

    #[test]
    fn test_nested_class() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
class Outer {
    class Inner {
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract both classes (flat structure)
        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, Some("Outer".to_string()));
        assert_eq!(classes[1].name, Some("Inner".to_string()));
    }

    #[test]
    fn test_fqn_package_class_method() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let modules: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].fqn, Some("com.example".to_string()));

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].fqn, Some("com.example.MyClass".to_string()));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(
            methods[0].fqn,
            Some("com.example.MyClass.myMethod".to_string())
        );
    }

    #[test]
    fn test_fqn_nested_class() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
class Outer {
    class Inner {
        void method() {}
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].fqn, Some("Outer".to_string()));
        assert_eq!(classes[1].fqn, Some("Outer.Inner".to_string()));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fqn, Some("Outer.Inner.method".to_string()));
    }

    #[test]
    fn test_canonical_fqn_with_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("src/test/Example.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        // Canonical FQN format: crate_name::file_path::Kind symbol_name
        assert!(classes[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("src/test/Example.java"));
        assert!(classes[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("Struct"));
        assert!(classes[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("MyClass"));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert!(methods[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("src/test/Example.java"));
        assert!(methods[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("Method"));
        assert!(methods[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("myMethod"));
    }

    #[test]
    fn test_display_fqn_with_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        // Display FQN format: package.class
        let display_fqn = classes[0].display_fqn.as_ref().unwrap();
        assert_eq!(display_fqn, "com.example.MyClass");

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        // Display FQN format: package.class.method
        let display_fqn = methods[0].display_fqn.as_ref().unwrap();
        assert_eq!(display_fqn, "com.example.MyClass.myMethod");
    }

    #[test]
    fn test_all_fqn_types_computed() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);

        // Verify all three FQN types are computed
        assert!(methods[0].fqn.is_some());
        assert!(methods[0].canonical_fqn.is_some());
        assert!(methods[0].display_fqn.is_some());

        // Verify package name is included in display FQN
        assert!(methods[0]
            .display_fqn
            .as_ref()
            .unwrap()
            .starts_with("com.example"));
    }

    #[test]
    fn test_fqn_nested_class_with_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

class Outer {
    class Inner {
        void method() {}
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);

        // Outer class display FQN
        assert_eq!(
            classes[0].display_fqn.as_ref().unwrap(),
            "com.example.Outer"
        );

        // Inner class display FQN (nested)
        assert_eq!(
            classes[1].display_fqn.as_ref().unwrap(),
            "com.example.Outer.Inner"
        );

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);

        // Method display FQN in nested class
        assert_eq!(
            methods[0].display_fqn.as_ref().unwrap(),
            "com.example.Outer.Inner.method"
        );
    }
}

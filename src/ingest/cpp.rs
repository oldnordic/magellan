//! C++ symbol extraction using tree-sitter-cpp.
//!
//! Extracts functions, classes, structs, namespaces, and templates from C++ source code.

use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parser that extracts symbol facts from C++ source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct CppParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl CppParser {
    /// Create a new parser for C++ source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from C++ source code.
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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        // Per decision FQN-17, use "." as project_root placeholder for C++
        let package_name = ".";

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

    /// Extract name from a symbol node.
    ///
    /// C++ uses different identifier types:
    /// - Functions: identifier
    /// - Classes/structs: type_identifier
    /// - Namespaces: namespace_identifier
    fn extract_name(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        node_kind: &str,
    ) -> Option<String> {
        // For namespaces, use namespace_identifier
        if node_kind == "namespace_definition" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "namespace_identifier" {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
            }
            return None;
        }

        // For other symbols, search recursively for identifier or type_identifier
        self.find_name_recursive(node, source)
    }

    /// Recursively search for identifier or type_identifier nodes.
    fn find_name_recursive(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                // For qualified names like Graph::bfs, take the unqualified tail
                "scoped_identifier" | "qualified_identifier" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if name_node.kind() == "identifier" || name_node.kind() == "type_identifier" {
                            let name_bytes = safe_slice(source, name_node.start_byte(), name_node.end_byte())?;
                            if let Ok(s) = std::str::from_utf8(name_bytes) {
                                return Some(s.to_string());
                            }
                        }
                    }
                }
                // Skip certain nodes to find the identifier within
                "function_declarator"
                | "parameter_list"
                | "field_declaration_list"
                | "template_parameter_list" => {
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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        // Per decision FQN-17, use "." as project_root placeholder for C++
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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
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

        // Skip template_declaration wrapper - recurse into children
        if kind == "template_declaration" {
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

        // Track namespace scope
        if kind == "namespace_definition" {
            if let Some(name) = Self::extract_name_static(node, source, kind) {
                // Create namespace symbol with parent scope
                if !name.is_empty() {
                    if let Some(fact) = Self::extract_symbol_with_fqn_static(
                        node,
                        source,
                        file_path,
                        scope_stack,
                        package_name,
                    ) {
                        facts.push(fact);
                    }
                    scope_stack.push(&name);
                }
                // Recurse into namespace body
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
                if !name.is_empty() {
                    scope_stack.pop();
                }
                return;
            }
        }

        // Extract symbol with FQN from scope stack
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
            "function_definition" => SymbolKind::Function,
            "class_specifier" => SymbolKind::Class,
            "struct_specifier" => SymbolKind::Class,
            "namespace_definition" => SymbolKind::Namespace,
            _ => return None,
        };

        let name = Self::extract_name_static(node, source, kind)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from scope stack + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Compute canonical_fqn and display_fqn using FqnBuilder
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::DoubleColon,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name.clone()),
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
        // For namespaces, use namespace_identifier
        if node_kind == "namespace_definition" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "namespace_identifier" {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
            }
            return None;
        }

        // For other symbols, search recursively for identifier or type_identifier
        Self::find_name_recursive_static(node, source)
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
                // For qualified names like Graph::bfs, take the unqualified tail
                "scoped_identifier" | "qualified_identifier" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if name_node.kind() == "identifier" || name_node.kind() == "type_identifier" {
                            let name_bytes = &source[name_node.start_byte()..name_node.end_byte()];
                            if let Ok(s) = std::str::from_utf8(name_bytes) {
                                return Some(s.to_string());
                            }
                        }
                    }
                }
                // Skip certain nodes to find the identifier within
                "function_declarator"
                | "parameter_list"
                | "field_declaration_list"
                | "template_parameter_list" => {
                    if let Some(name) = Self::find_name_recursive_static(&child, source) {
                        return Some(name);
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Walk tree-sitter tree recursively with scope tracking for FQN extraction.
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

        // Skip template_declaration wrapper - recurse into children
        if kind == "template_declaration" {
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

        // Track namespace scope
        if kind == "namespace_definition" {
            if let Some(name) = self.extract_name(node, source, kind) {
                // Create namespace symbol with parent scope
                if !name.is_empty() {
                    if let Some(fact) = self.extract_symbol_with_fqn(
                        node,
                        source,
                        file_path,
                        scope_stack,
                        package_name,
                    ) {
                        facts.push(fact);
                    }
                    scope_stack.push(&name);
                }
                // Recurse into namespace body
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
                if !name.is_empty() {
                    scope_stack.pop();
                }
                return;
            }
        }

        // Extract symbol with FQN from scope stack
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

    /// Extract a symbol fact with FQN from a tree-sitter node, if applicable.
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
            "function_definition" => SymbolKind::Function,
            "class_specifier" => SymbolKind::Class,
            "struct_specifier" => SymbolKind::Class,
            "namespace_definition" => SymbolKind::Namespace,
            _ => return None, // Not a symbol we track
        };

        let name = self.extract_name(node, source, kind)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from scope stack + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Compute canonical_fqn and display_fqn using FqnBuilder
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::DoubleColon,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name.clone()),
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

    /// Extract reference facts from C++ source code.
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
                    "identifier"
                        | "type_identifier"
                        | "qualified_identifier"
                        | "namespace_qualified_name"
                        | "field_expression"
                        | "method_expression"
                )
            },
            |node, source| {
                let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
                Some((text.to_string(), node.kind()))
            },
        )
    }

    /// Extract function call facts from C++ source code.
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
                        "identifier"
                        | "qualified_identifier"
                        | "namespace_qualified_name"
                        | "field_expression"
                        | "method_expression" => {
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

impl Default for CppParser {
    fn default() -> Self {
        Self::new().expect("Failed to create C++ parser") // M-UNWRAP: tree-sitter language is a build-time invariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let mut parser = CppParser::new().unwrap();
        let source = b"void foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("foo".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let mut parser = CppParser::new().unwrap();
        let source = b"class MyClass {\npublic:\n    void method();\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_struct() {
        let mut parser = CppParser::new().unwrap();
        let source = b"struct Point {\n    int x;\n    int y;\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Point".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"namespace MyNamespace {\n    class Foo {};\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        // Should extract namespace and nested class (flat structure)
        assert!(!facts.is_empty());

        let namespaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Namespace)
            .collect();
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].name, Some("MyNamespace".to_string()));
    }

    #[test]
    fn test_extract_template_class() {
        let mut parser = CppParser::new().unwrap();
        let source = b"template<typename T>\nclass TemplateClass {\n    T value;\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        // Should extract the template class (walk_tree recurses into template_declaration)
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("TemplateClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_nested_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"namespace Outer {\n    namespace Inner {\n        class Foo {};\n    }\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        // Should extract both namespaces (flat structure)
        let namespaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Namespace)
            .collect();
        assert_eq!(namespaces.len(), 2);
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
void foo() {}

class Bar {
    void method();
};

namespace Baz {
    struct Nested {};
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        // Flat extraction: foo, Bar, Baz, Nested
        assert!(facts.len() >= 4);

        let functions: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("foo".to_string()));

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2); // Bar and Nested (flat extraction)

        let namespaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Namespace)
            .collect();
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].name, Some("Baz".to_string()));
    }

    #[test]
    fn test_empty_file() {
        let mut parser = CppParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.cpp"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = CppParser::new().unwrap();
        let source = b"void broken(\n    // invalid C++";
        let facts = parser.extract_symbols(PathBuf::from("broken.cpp"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = CppParser::new().unwrap();
        let source = b"void foo() { return; }";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = CppParser::new().unwrap();
        let source = b"void foo() {\n    return;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Function starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'v' in 'void' is at column 0
    }

    #[test]
    fn test_template_function() {
        let mut parser = CppParser::new().unwrap();
        let source = b"template<typename T>\nvoid template_func(T arg) {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        // Should extract the template function
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("template_func".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_fqn_simple_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
namespace MyNamespace {
    void my_function() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].fqn, Some("MyNamespace::my_function".to_string()));
    }

    #[test]
    fn test_fqn_nested_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
namespace Outer {
    namespace Inner {
        class MyClass {};
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();

        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].fqn, Some("Outer::Inner::MyClass".to_string()));
    }

    #[test]
    fn test_fqn_class_in_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
namespace ns {
    struct Point {
        int x;
        int y;
    };
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();

        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].fqn, Some("ns::Point".to_string()));
    }

    #[test]
    fn test_canonical_fqn_format() {
        let mut parser = CppParser::new().unwrap();
        let source = b"void foo() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Canonical FQN format: package_name::file_path::Kind symbol_name
        assert!(fact.canonical_fqn.is_some());
        let canonical = fact.canonical_fqn.as_ref().unwrap();
        assert!(canonical.contains("::Function foo"));
        assert!(canonical.contains("test.cpp"));
        assert!(canonical.contains(".")); // package_name
    }

    #[test]
    fn test_display_fqn_format() {
        let mut parser = CppParser::new().unwrap();
        let source = b"void foo() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Display FQN format: package_name::symbol_name (no file path)
        assert!(fact.display_fqn.is_some());
        let display = fact.display_fqn.as_ref().unwrap();
        assert_eq!(display, ".::foo"); // package_name "." + no namespace
    }

    #[test]
    fn test_fqn_namespace_function() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
namespace MyNamespace {
    void my_function() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("src/test.cpp"), source);

        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(funcs.len(), 1);
        let func = &funcs[0];

        // Verify canonical FQN includes file path
        assert!(func.canonical_fqn.is_some());
        let canonical = func.canonical_fqn.as_ref().unwrap();
        assert!(canonical.contains("src/test.cpp"));
        assert!(canonical.contains("Function my_function"));

        // Verify display FQN includes namespace but not file path
        assert!(func.display_fqn.is_some());
        let display = func.display_fqn.as_ref().unwrap();
        assert_eq!(display, ".::MyNamespace::my_function");
    }

    #[test]
    fn test_canonical_fqn_nested_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
namespace Outer {
    namespace Inner {
        void func() {}
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.cpp"), source);

        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(funcs.len(), 1);
        let func = &funcs[0];

        // Display FQN should show nested namespace
        assert_eq!(func.display_fqn, Some(".::Outer::Inner::func".to_string()));

        // Canonical FQN should include file path
        assert!(func.canonical_fqn.is_some());
        let canonical = func.canonical_fqn.as_ref().unwrap();
        assert!(canonical.contains("test.cpp"));
        assert!(canonical.contains("Function func"));
    }

    #[test]
    fn test_canonical_fqn_class_in_namespace() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
namespace ns {
    struct Point {
        int x;
        int y;
    };
}
";
        let facts = parser.extract_symbols(PathBuf::from("src/geometry.cpp"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();

        assert_eq!(classes.len(), 1);
        let cls = &classes[0];

        // Display FQN should show namespace::class
        assert_eq!(cls.display_fqn, Some(".::ns::Point".to_string()));

        // Canonical FQN should include file path and kind
        assert!(cls.canonical_fqn.is_some());
        let canonical = cls.canonical_fqn.as_ref().unwrap();
        assert!(canonical.contains("src/geometry.cpp"));
        assert!(canonical.contains("Struct Point")); // Class maps to Struct in canonical FQN
    }

    // Bug 1 regression: class method out-of-line definitions must be indexed as symbols.
    // Previously scoped_identifier (Graph::bfs) was not handled — method name was never extracted.
    #[test]
    fn test_extract_out_of_line_method() {
        let mut parser = CppParser::new().unwrap();
        let source = b"
#include <vector>
struct Graph {
    int num_nodes;
    std::vector<int> bfs(int start) const;
    void add_edge(int u, int v);
};
std::vector<int> Graph::bfs(int start) const {
    return {};
}
void Graph::add_edge(int u, int v) {
    (void)u; (void)v;
}
";
        let facts = parser.extract_symbols(PathBuf::from("graph.cpp"), source);
        let fn_names: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .filter_map(|f| f.name.as_deref())
            .collect();
        assert!(
            fn_names.contains(&"bfs"),
            "bfs must be indexed; got: {fn_names:?}"
        );
        assert!(
            fn_names.contains(&"add_edge"),
            "add_edge must be indexed; got: {fn_names:?}"
        );
    }
}

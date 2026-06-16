//! CUDA symbol extraction using tree-sitter-cuda.
//!
//! CUDA extends C/C++ with additional syntax. This parser reuses C++ patterns
//! and handles CUDA-specific constructs like __global__ functions.

use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parser that extracts symbol facts from CUDA source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct CudaParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl CudaParser {
    /// Create a new parser for CUDA source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_cuda::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from CUDA source code.
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

        // Per decision FQN-17, use "." as project_root placeholder for CUDA
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
    /// CUDA uses the same identifier types as C++:
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

    /// Extract symbol facts from a pre-parsed tree.
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
            "enum_specifier" => SymbolKind::Enum,
            "namespace_definition" => SymbolKind::Namespace,
            _ => return None,
        };

        let name = Self::extract_name_static(node, source, kind)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        let fqn = scope_stack.fqn_for_symbol(&name);

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
            "enum_specifier" => SymbolKind::Enum,
            "namespace_definition" => SymbolKind::Namespace,
            _ => return None,
        };

        let name = self.extract_name(node, source, kind)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        let fqn = scope_stack.fqn_for_symbol(&name);

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

    /// Extract reference facts from CUDA source code.
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

    /// Extract function call facts from CUDA source code.
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
            Self::extract_function_name,
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

    fn extract_function_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        Self::find_name_recursive_static(node, source)
    }
}

impl Default for CudaParser {
    fn default() -> Self {
        Self::new().expect("Failed to create CUDA parser") // M-UNWRAP: tree-sitter language is a build-time invariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let mut parser = CudaParser::new().unwrap();
        let source = b"__global__ void kernel() {\n    int x = 0;\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cu"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("kernel".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let mut parser = CudaParser::new().unwrap();
        let source = b"class MyClass {\npublic:\n    void method();\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cu"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_struct() {
        let mut parser = CudaParser::new().unwrap();
        let source = b"struct Point {\n    int x;\n    int y;\n};\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cu"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Point".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_namespace() {
        let mut parser = CudaParser::new().unwrap();
        let source = b"namespace MyNamespace {\n    class Foo {};\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.cu"), source);

        let namespaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Namespace)
            .collect();
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].name, Some("MyNamespace".to_string()));
    }

    #[test]
    fn test_empty_file() {
        let mut parser = CudaParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.cu"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = CudaParser::new().unwrap();
        let source = b"void foo() { return; }";
        let facts = parser.extract_symbols(PathBuf::from("test.cu"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];
        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }
}

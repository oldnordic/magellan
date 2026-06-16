//! Go symbol extraction using tree-sitter-go.
//!
//! Extracts functions, methods, structs, and interfaces from Go source code.

use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parser that extracts symbol facts from Go source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct GoParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl GoParser {
    /// Create a new parser for Go source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from Go source code.
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

        // Find package declaration first
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if child.kind() == "package_clause" {
                if let Some(name) = Self::extract_package_name(&child, source) {
                    scope_stack.push(&name);
                }
                break;
            }
        }

        // Walk tree with scope tracking
        Self::walk_tree_static(&root_node, source, &file_path, &mut facts, &mut scope_stack);

        facts
    }

    /// Extract package name from a package_clause node.
    fn extract_package_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "package_identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }

    /// Static walk tree for symbol extraction.
    fn walk_tree_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
    ) {
        let kind = node.kind();

        // Skip package_clause (already handled)
        if kind == "package_clause" {
            return;
        }

        // Track type scope for method receivers
        let is_type_scope = kind == "type_declaration";

        if is_type_scope {
            // type_declaration may contain multiple type_spec children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "type_spec" {
                    if let Some(fact) =
                        Self::extract_symbol_from_type_spec(&child, source, file_path, scope_stack)
                    {
                        facts.push(fact);
                    }
                }
            }
            return;
        }

        // Extract symbol from this node if applicable
        if let Some(fact) = Self::extract_symbol(node, source, file_path, scope_stack) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk_tree_static(&child, source, file_path, facts, scope_stack);
        }
    }

    /// Extract a symbol fact from a type_spec node.
    fn extract_symbol_from_type_spec(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
    ) -> Option<SymbolFact> {
        let mut name: Option<String> = None;
        let mut symbol_kind = SymbolKind::Unknown;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    name = std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                "struct_type" => {
                    symbol_kind = SymbolKind::Class;
                }
                "interface_type" => {
                    symbol_kind = SymbolKind::Interface;
                }
                _ => {}
            }
        }

        let name = name?;
        if symbol_kind == SymbolKind::Unknown {
            // Default to Class for unknown type declarations
            symbol_kind = SymbolKind::Class;
        }

        let normalized_kind = symbol_kind.normalized_key().to_string();
        let fqn = scope_stack.fqn_for_symbol(&name);

        let builder = FqnBuilder::new(
            ".".to_string(),
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

    /// Extract a symbol fact from a tree-sitter node, if applicable.
    fn extract_symbol(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_declaration" => SymbolKind::Function,
            "method_declaration" => SymbolKind::Method,
            _ => return None,
        };

        let name = Self::extract_name(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();
        let fqn = scope_stack.fqn_for_symbol(&name);

        let builder = FqnBuilder::new(
            ".".to_string(),
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

    /// Extract name from a symbol node.
    fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "field_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
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
        Self::extract_symbols_from_tree(&tree, file_path, source)
    }

    /// Extract symbol facts from a pre-parsed tree.
    pub fn extract_symbols_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);

        // Find package declaration first
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if child.kind() == "package_clause" {
                if let Some(name) = Self::extract_package_name(&child, source) {
                    scope_stack.push(&name);
                }
                break;
            }
        }

        Self::walk_tree_static(&root_node, source, &file_path, &mut facts, &mut scope_stack);
        facts
    }

    /// Extract reference facts from Go source code.
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
                    "identifier" | "type_identifier" | "selector_expression"
                )
            },
            |node, source| {
                let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
                Some((text.to_string(), node.kind()))
            },
        )
    }

    /// Extract function call facts from Go source code.
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
            |node| matches!(node.kind(), "function_declaration" | "method_declaration"),
            Self::extract_name,
            "call_expression",
            |node, source| {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" | "field_identifier" | "selector_expression" => {
                            let text = if child.kind() == "selector_expression" {
                                Self::extract_selector_name(&child, source)?
                            } else {
                                std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
                                    .ok()?
                                    .to_string()
                            };
                            return Some((text, child.kind()));
                        }
                        _ => {}
                    }
                }
                None
            },
        )
    }

    fn extract_selector_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // selector_expression is parenthesised expression in Go grammar? Actually Go's
        // tree-sitter uses selector_expression for "pkg.Field" or "obj.Method". The field
        // name is the rightmost identifier/field_identifier child.
        let mut cursor = node.walk();
        let mut last_name: Option<String> = None;
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "identifier" | "field_identifier") {
                let bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                last_name = std::str::from_utf8(bytes).ok().map(|s| s.to_string());
            }
        }
        last_name
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Go parser") // M-UNWRAP: tree-sitter language is a build-time invariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let mut parser = GoParser::new().unwrap();
        let source = b"package main\n\nfunc main() {\n    println(\"hello\")\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("main.go"), source);

        assert!(!facts.is_empty());
        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, Some("main".to_string()));
    }

    #[test]
    fn test_extract_struct() {
        let mut parser = GoParser::new().unwrap();
        let source = b"package main\n\ntype Person struct {\n    Name string\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("main.go"), source);

        let structs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, Some("Person".to_string()));
    }

    #[test]
    fn test_extract_interface() {
        let mut parser = GoParser::new().unwrap();
        let source =
            b"package main\n\ntype Reader interface {\n    Read(p []byte) (n int, err error)\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("main.go"), source);

        let interfaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].name, Some("Reader".to_string()));
    }

    #[test]
    fn test_extract_method() {
        let mut parser = GoParser::new().unwrap();
        let source =
            b"package main\n\nfunc (p *Person) Greet() string {\n    return \"hello\"\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("main.go"), source);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, Some("Greet".to_string()));
    }

    #[test]
    fn test_empty_file() {
        let mut parser = GoParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.go"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = GoParser::new().unwrap();
        let source = b"package main\n\nfunc foo() {}";
        let facts = parser.extract_symbols(PathBuf::from("test.go"), source);

        assert!(!facts.is_empty());
        let fact = &facts[0];
        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }
}

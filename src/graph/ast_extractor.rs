//! AST extraction from tree-sitter trees
//!
//! This module provides functionality for extracting structural AST nodes
//! from tree-sitter parse trees. It handles language-specific node kind
//! mapping and parent-child relationship tracking.

use tree_sitter::{Node, Tree};

use crate::graph::ast_node::{is_structural_kind, AstNode, kinds};

/// Extract AST nodes from a tree-sitter tree
///
/// This is the main entry point for AST extraction. Given a tree-sitter
/// parse tree and source code, it returns a vector of AstNode structs
/// representing the structural nodes of the AST.
pub fn extract_ast_nodes(tree: &Tree, source: &[u8]) -> Vec<AstNode> {
    let extractor = AstExtractor::new(source);
    extractor.extract(tree)
}

/// Normalize a tree-sitter node kind to a language-agnostic kind
///
/// This maps language-specific node kinds (e.g., "if_expression")
/// to normalized kinds (e.g., "If") for cross-language queries.
pub fn normalize_node_kind<'a>(kind: &'a str, _language: &str) -> &'a str {
    // Use constants where defined, return original kind otherwise
    match kind {
        // Control flow
        "if_expression" | "if_statement" => kinds::IF,
        "match_expression" | "match_statement" => kinds::MATCH,
        "while_expression" | "while_statement" => kinds::WHILE,
        "for_expression" | "for_statement" => kinds::FOR,
        "loop_expression" => kinds::LOOP,
        "return_expression" | "return_statement" => kinds::RETURN,
        "break_expression" | "break_statement" => kinds::BREAK,
        "continue_expression" | "continue_statement" => kinds::CONTINUE,

        // Definitions
        "function_item" | "function_definition" => kinds::FUNCTION,
        "method_definition" => kinds::FUNCTION,
        "struct_item" | "struct_definition" => kinds::STRUCT,
        "enum_item" | "enum_definition" => kinds::ENUM,
        "trait_item" | "trait_definition" => kinds::TRAIT,
        "impl_item" => kinds::IMPL,
        "mod_item" => kinds::MODULE,
        "class_definition" => kinds::CLASS,
        "interface_definition" => kinds::INTERFACE,

        // Blocks
        "block" | "block_expression" | "statement_block" => kinds::BLOCK,

        // Statements
        "let_statement" => kinds::LET,
        "expression_statement" => "Expression", // No constant for this one
        "assignment_expression" => kinds::ASSIGN,

        // Calls
        "call_expression" => kinds::CALL,

        // Attributes
        "attribute_item" | "decorated_definition" => kinds::ATTRIBUTE,

        // Constants
        "const_item" => kinds::CONST,
        "static_item" => kinds::STATIC,

        // Default: return original kind
        _ => kind,
    }
}

/// Detect the programming language from a file extension
pub fn language_from_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some("cpp"),
        "java" => Some("java"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        _ => None,
    }
}

/// AST extractor with state for tracking parent relationships
struct AstExtractor<'a> {
    /// Source code for validation
    source: &'a [u8],

    /// Accumulated AST nodes
    nodes: Vec<AstNode>,

    /// Stack of parent node indices
    parent_stack: Vec<Option<usize>>,
}

impl<'a> AstExtractor<'a> {
    /// Create a new AST extractor
    fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            nodes: Vec::new(),
            parent_stack: Vec::new(),
        }
    }

    /// Extract all structural AST nodes from the tree
    fn extract(mut self, tree: &Tree) -> Vec<AstNode> {
        let root = tree.root_node();
        self.traverse(&root);
        self.nodes
    }

    /// Traverse the tree and extract structural nodes
    fn traverse(&mut self, node: &Node) {
        let kind = node.kind();

        // Check if this is a structural node we want to store
        if is_structural_kind(kind) {
            let byte_start = node.start_byte();
            let byte_end = node.end_byte();

            // Validate byte ranges against source length
            if byte_end > self.source.len() || byte_start > byte_end {
                // Skip nodes with invalid byte ranges
                return;
            }

            // Create the AST node
            let ast_node = AstNode {
                id: None,  // Will be assigned by database
                parent_id: None,  // Will be updated after insertion
                kind: kind.to_string(),
                byte_start,
                byte_end,
            };

            // Add to nodes and track parent index
            let node_index = self.nodes.len();
            self.nodes.push(ast_node);

            // Update parent reference using index
            if let Some(last_index) = self.parent_stack.last() {
                if let &Some(parent_idx) = last_index {
                    // Store as negative to indicate "needs resolution"
                    self.nodes[node_index].parent_id = Some(-(parent_idx as i64) - 1);
                }
            }

            // Push this node as parent for children
            self.parent_stack.push(Some(node_index));

            // Recurse to children
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    self.traverse(&cursor.node());
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }

            // Pop from parent stack
            self.parent_stack.pop();
        } else {
            // Not a structural node, but may have structural children
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    self.traverse(&cursor.node());
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn test_extract_simple_function() {
        let source = b"fn main() { }";

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let nodes = extract_ast_nodes(&tree, source);

        // Should find function_item
        assert!(!nodes.is_empty());
        let fn_node = nodes.iter().find(|n| n.kind == "function_item");
        assert!(fn_node.is_some());
    }

    #[test]
    fn test_extract_if_expression() {
        let source = b"fn test() { if x { y } else { z } }";

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let nodes = extract_ast_nodes(&tree, source);

        // Should find if_expression
        let if_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "if_expression").collect();
        assert!(!if_nodes.is_empty());
    }

    #[test]
    fn test_extract_ignores_identifiers() {
        let source = b"fn main() { let x = 42; }";

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let nodes = extract_ast_nodes(&tree, source);

        // Should NOT have identifier nodes (leaf nodes)
        assert!(!nodes.iter().any(|n| n.kind == "identifier"));

        // Should have function_item and let_declaration
        assert!(nodes.iter().any(|n| n.kind == "function_item"));
        assert!(nodes.iter().any(|n| n.kind == "let_declaration"));
    }

    #[test]
    fn test_parent_child_relationships() {
        let source = b"fn main() { if x { y } }";

        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        let tree = parser.parse(source, None).unwrap();

        let nodes = extract_ast_nodes(&tree, source);

        // Find if_expression and verify it has a parent
        let if_node = nodes.iter().find(|n| n.kind == "if_expression");
        assert!(if_node.is_some());

        // The if_expression should be inside a block or function
        let if_node = if_node.unwrap();
        // Parent ID will be negative placeholder at this stage
        assert!(if_node.parent_id.is_some());
    }

    #[test]
    fn test_normalize_if_expression() {
        assert_eq!(normalize_node_kind("if_expression", "rust"), "If");
        assert_eq!(normalize_node_kind("if_statement", "python"), "If");
    }

    #[test]
    fn test_normalize_function() {
        assert_eq!(normalize_node_kind("function_item", "rust"), "Function");
        assert_eq!(normalize_node_kind("function_definition", "python"), "Function");
    }

    #[test]
    fn test_normalize_unknown() {
        assert_eq!(normalize_node_kind("unknown_kind", "rust"), "unknown_kind");
    }

    #[test]
    fn test_language_from_path() {
        assert_eq!(language_from_path("src/main.rs"), Some("rust"));
        assert_eq!(language_from_path("script.py"), Some("python"));
        assert_eq!(language_from_path("header.h"), Some("c"));
        assert_eq!(language_from_path("file.cpp"), Some("cpp"));
        assert_eq!(language_from_path("Main.java"), Some("java"));
        assert_eq!(language_from_path("app.js"), Some("javascript"));
        assert_eq!(language_from_path("app.ts"), Some("typescript"));
        assert_eq!(language_from_path("unknown.xyz"), None);
    }
}

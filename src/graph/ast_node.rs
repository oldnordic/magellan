//! AST node data structures for hierarchical code representation
//!
//! This module defines the core types used to store and query AST node
//! information. AST nodes provide statement-level granularity within
//! symbol spans (e.g., if-statements, loops, expressions inside a function).

use serde::{Deserialize, Serialize};

/// Normalized AST node kinds
///
/// These constants provide language-agnostic names for common node types.
/// Use these for queries that should work across languages.
pub mod kinds {
    /// Control flow nodes
    pub const IF: &str = "If";
    /// Reserved: tree-sitter includes else as part of if_expression, not a separate node
    /// Kept for potential future use with other parsers or language-specific extensions
    #[allow(dead_code)]
    pub const ELSE: &str = "Else";
    pub const MATCH: &str = "Match";
    pub const LOOP: &str = "Loop";
    pub const WHILE: &str = "While";
    pub const FOR: &str = "For";
    pub const BREAK: &str = "Break";
    pub const CONTINUE: &str = "Continue";
    pub const RETURN: &str = "Return";

    /// Definition nodes
    pub const FUNCTION: &str = "Function";
    pub const STRUCT: &str = "Struct";
    pub const ENUM: &str = "Enum";
    pub const TRAIT: &str = "Trait";
    pub const IMPL: &str = "Impl";
    pub const MODULE: &str = "Module";
    pub const CLASS: &str = "Class";
    pub const INTERFACE: &str = "Interface";

    /// Statement/expression nodes
    pub const BLOCK: &str = "Block";
    pub const CALL: &str = "Call";
    pub const ASSIGN: &str = "Assign";
    pub const LET: &str = "Let";
    pub const CONST: &str = "Const";
    pub const STATIC: &str = "Static";

    /// Other
    pub const ATTRIBUTE: &str = "Attribute";
    /// Reserved: for future comment tracking (documentation generation, TODO extraction, etc.)
    #[allow(dead_code)]
    pub const COMMENT: &str = "Comment";
}

/// AST node extracted from source code
///
/// Represents a single node in the abstract syntax tree with positional
/// information and optional parent reference for tree structure.
///
/// # Example
/// ```rust
/// let node = AstNode {
///     id: None,
///     parent_id: Some(1),
///     kind: "IfExpression".to_string(),
///     byte_start: 100,
///     byte_end: 250,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstNode {
    /// Database ID (Some after insertion, None before)
    pub id: Option<i64>,

    /// Parent node ID (None for root-level nodes)
    pub parent_id: Option<i64>,

    /// Node kind (e.g., "IfExpression", "WhileLoop", "FunctionItem")
    ///
    /// Uses tree-sitter node kind names for language-specific nodes.
    pub kind: String,

    /// Byte offset of node start in source file
    pub byte_start: usize,

    /// Byte offset of node end in source file (half-open interval)
    pub byte_end: usize,
}

impl AstNode {
    /// Create a new AstNode without database ID
    pub fn new(
        parent_id: Option<i64>,
        kind: impl Into<String>,
        byte_start: usize,
        byte_end: usize,
    ) -> Self {
        Self {
            id: None,
            parent_id,
            kind: kind.into(),
            byte_start,
            byte_end,
        }
    }

    /// Get the byte span as a tuple
    pub fn span(&self) -> (usize, usize) {
        (self.byte_start, self.byte_end)
    }

    /// Check if this node contains a given byte position
    pub fn contains(&self, position: usize) -> bool {
        self.byte_start <= position && position < self.byte_end
    }

    /// Get the length of this node in bytes
    pub fn len(&self) -> usize {
        self.byte_end - self.byte_start
    }

    /// Check if node has zero length
    pub fn is_empty(&self) -> bool {
        self.byte_end <= self.byte_start
    }
}

/// AST node with optional source text snippet
///
/// Used for query results where the source text should be included
/// (e.g., human-readable CLI output, JSON exports).
///
/// The text field is typically populated by reading the source file
/// after querying AST nodes from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstNodeWithText {
    /// The base AST node data
    #[serde(flatten)]
    pub node: AstNode,

    /// Source text snippet (optional, may be truncated)
    pub text: Option<String>,
}

impl AstNodeWithText {
    /// Create a new AstNodeWithText from an AstNode
    pub fn from_node(node: AstNode, text: Option<String>) -> Self {
        Self { node, text }
    }

    /// Get the source text, or return a placeholder if missing
    pub fn text_or<'a>(&'a self, placeholder: &'a str) -> &'a str {
        self.text.as_deref().unwrap_or(placeholder)
    }
}

impl From<AstNode> for AstNodeWithText {
    fn from(node: AstNode) -> Self {
        Self {
            node,
            text: None,
        }
    }
}

/// Check if a node kind is a structural node (should be stored)
///
/// Structural nodes are the "interesting" parts of the AST that provide
/// value for navigation, analysis, and refactoring. Leaf nodes like
/// identifiers and literals are filtered out to reduce storage.
pub fn is_structural_kind(kind: &str) -> bool {
    matches!(
        kind,
        // Control flow
        "if_expression" | "match_expression" | "while_expression"
        | "for_expression" | "loop_expression" | "return_expression"
        | "break_expression" | "continue_expression"
        | "if_statement" | "match_statement" | "while_statement"
        | "for_statement" | "break_statement" | "continue_statement"
        | "return_statement"
        // Definitions
        | "function_item" | "method_definition" | "struct_item"
        | "enum_item" | "trait_item" | "impl_item" | "mod_item"
        | "class_definition" | "interface_definition"
        // Blocks
        | "block" | "block_expression" | "statement_block"
        // Statements
        | "let_statement" | "let_declaration" | "expression_statement"
        | "assignment_expression" | "augmented_assignment_expression"
        // Calls
        | "call_expression"
        // Attributes
        | "attribute_item" | "decorated_definition"
        // Constants
        | "const_item" | "static_item"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_node_new() {
        let node = AstNode::new(Some(1), "IfExpression", 100, 250);
        assert_eq!(node.id, None);
        assert_eq!(node.parent_id, Some(1));
        assert_eq!(node.kind, "IfExpression");
        assert_eq!(node.byte_start, 100);
        assert_eq!(node.byte_end, 250);
    }

    #[test]
    fn test_ast_node_span() {
        let node = AstNode::new(None, "Block", 50, 150);
        assert_eq!(node.span(), (50, 150));
    }

    #[test]
    fn test_ast_node_contains() {
        let node = AstNode::new(None, "Block", 50, 150);
        assert!(node.contains(50));
        assert!(node.contains(100));
        assert!(!node.contains(150)); // half-open interval
        assert!(!node.contains(200));
    }

    #[test]
    fn test_ast_node_len() {
        let node = AstNode::new(None, "Block", 50, 150);
        assert_eq!(node.len(), 100);
    }

    #[test]
    fn test_ast_node_is_empty() {
        let empty = AstNode::new(None, "Empty", 100, 100);
        assert!(empty.is_empty());

        let non_empty = AstNode::new(None, "Block", 100, 200);
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_is_structural_kind() {
        // Structural nodes should return true
        assert!(is_structural_kind("if_expression"));
        assert!(is_structural_kind("function_item"));
        assert!(is_structural_kind("block"));
        assert!(is_structural_kind("let_statement"));

        // Non-structural (leaf) nodes should return false
        assert!(!is_structural_kind("identifier"));
        assert!(!is_structural_kind("string_literal"));
    }

    #[test]
    fn test_ast_node_with_text_from_node() {
        let node = AstNode::new(None, "IfExpression", 10, 50);
        let with_text = AstNodeWithText::from(node.clone());

        assert_eq!(with_text.node.kind, "IfExpression");
        assert_eq!(with_text.text, None);
    }

    #[test]
    fn test_ast_node_with_text_or() {
        let node = AstNode::new(None, "IfExpression", 10, 50);
        let with_text = AstNodeWithText {
            node,
            text: Some("if x { y }".to_string()),
        };

        assert_eq!(with_text.text_or("<missing>"), "if x { y }");

        let no_text = AstNodeWithText {
            node: AstNode::new(None, "Block", 0, 0),
            text: None,
        };
        assert_eq!(no_text.text_or("<missing>"), "<missing>");
    }
}

//! CFG Edge Extraction from tree-sitter AST
//!
//! This module extracts control flow edges between basic blocks
//! by analyzing tree-sitter AST nodes.

use crate::graph::schema::CfgBlock;
use tree_sitter::{Node, Parser as TsParser};

mod control_flow;
mod extract;
#[cfg(test)]
mod tests;

use control_flow::*;
use extract::*;

/// Extract `#[cfg(...)]` condition string from a tree-sitter node.
///
/// In tree-sitter Rust grammar, `#[cfg(...)]` attributes are siblings of
/// the item they decorate (e.g. `function_item`), not children. This
/// function looks at the node's previous siblings for `attribute_item`
/// nodes and returns the first `cfg(...)` condition found.
pub fn extract_cfg_condition(node: &Node, source: &str) -> Option<String> {
    let parent = node.parent()?;
    let mut cursor = parent.walk();

    // Collect previous siblings until we hit the target node.
    // Only consecutive `attribute_item`s immediately before the target
    // belong to it; stop at any non-attribute sibling.
    let mut prev_attrs: Vec<Node> = Vec::new();
    for sibling in parent.children(&mut cursor) {
        if sibling.id() == node.id() {
            break;
        }
        if sibling.kind() == "attribute_item" {
            prev_attrs.push(sibling);
        } else {
            prev_attrs.clear();
        }
    }

    for attr in prev_attrs.iter().rev() {
        let cond = parse_cfg_attribute(attr, source);
        if cond.is_some() {
            return cond;
        }
    }
    None
}

fn parse_cfg_attribute(attr_item: &Node, source: &str) -> Option<String> {
    let mut attr_cursor = attr_item.walk();
    for attr_child in attr_item.children(&mut attr_cursor) {
        if attr_child.kind() != "attribute" {
            continue;
        }
        let mut inner = attr_child.walk();
        let first = attr_child.children(&mut inner).next()?;
        if first.kind() != "identifier" {
            continue;
        }
        let name = &source[first.start_byte()..first.end_byte()];
        if name != "cfg" {
            continue;
        }
        let mut inner2 = attr_child.walk();
        for token_tree in attr_child.children(&mut inner2) {
            if token_tree.kind() == "token_tree" {
                let text = &source[token_tree.start_byte()..token_tree.end_byte()];
                let trimmed = text.trim();
                if trimmed.starts_with('(') && trimmed.ends_with(')') {
                    return Some(trimmed[1..trimmed.len() - 1].trim().to_string());
                }
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// A CFG edge with source and target block indices
#[derive(Debug, Clone)]
pub struct CfgEdge {
    /// Source block index (within function)
    pub source_idx: usize,
    /// Target block index (within function)
    pub target_idx: usize,
    /// Edge kind
    pub edge_type: CfgEdgeType,
}

/// Enclosing loop scope threaded through CFG extraction for `break`/`continue`
/// resolution. Both fields are `None` when extraction is not inside a loop.
#[derive(Clone, Copy)]
pub(super) struct LoopScope {
    /// Back-edge target (loop header) for `continue`.
    pub header: Option<usize>,
    /// Forward jump target (loop exit block) for `break`.
    pub exit: Option<usize>,
}

impl LoopScope {
    /// Scope used when extraction is not nested inside any loop.
    pub const NONE: LoopScope = LoopScope {
        header: None,
        exit: None,
    };
}

/// Type of CFG edge
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgEdgeType {
    /// Unconditional fall-through to next block
    Fallthrough,
    /// Conditional branch (true/then branch)
    ConditionalTrue,
    /// Conditional branch (false/else branch)
    ConditionalFalse,
    /// Unconditional jump (goto, break, continue)
    Jump,
    /// Loop back-edge
    BackEdge,
    /// Function call
    Call,
    /// Return from function
    Return,
    /// Generator yield (async/await)
    Yield,
}

impl CfgEdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CfgEdgeType::Fallthrough => "fallthrough",
            CfgEdgeType::ConditionalTrue => "conditional_true",
            CfgEdgeType::ConditionalFalse => "conditional_false",
            CfgEdgeType::Jump => "jump",
            CfgEdgeType::BackEdge => "back_edge",
            CfgEdgeType::Call => "call",
            CfgEdgeType::Return => "return",
            CfgEdgeType::Yield => "yield",
        }
    }
}

/// Result of CFG extraction with edges
#[derive(Debug, Clone)]
pub struct CfgWithEdges {
    /// CFG blocks
    pub blocks: Vec<CfgBlock>,
    /// CFG edges between blocks
    pub edges: Vec<CfgEdge>,
    /// Function ID this CFG belongs to
    pub function_id: i64,
}

/// Extract CFG with edges from a function's source code
///
/// # Arguments
/// * `source` - Source code of the function
/// * `function_id` - Database ID of the function symbol
/// * `language` - Tree-sitter language to use
///
/// # Returns
/// CfgWithEdges containing blocks and edges
pub fn extract_cfg_with_edges(
    source: &str,
    function_id: i64,
    language: tree_sitter::Language,
) -> CfgWithEdges {
    let mut parser = TsParser::new();
    if parser.set_language(&language).is_err() {
        return CfgWithEdges {
            function_id,
            blocks: Vec::new(),
            edges: Vec::new(),
        };
    }

    let Some(tree) = parser.parse(source, None) else {
        return CfgWithEdges {
            function_id,
            blocks: Vec::new(),
            edges: Vec::new(),
        };
    };
    let root = tree.root_node();

    // Find first function node (which is the function itself when called on function source)
    let func_node = find_function_node(&root);

    if let Some(func) = func_node {
        extract_cfg_from_function_node(&func, function_id, source)
    } else {
        // Fallback: analyze the root as the function body if no function_item/definition is found
        // (e.g. for anonymous functions or snippets)
        extract_cfg_from_function_node(&root, function_id, source)
    }
}

/// Find function node in AST
pub(super) fn find_function_node<'a>(root: &'a Node<'a>) -> Option<Node<'a>> {
    let mut stack = vec![*root];

    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind == "function_item"
            || kind == "function_definition"
            || kind == "function_declaration"
            || kind == "method_declaration"
        {
            return Some(node);
        }

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                stack.push(cursor.node());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    None
}

/// Extract CFG from a function node
pub fn extract_cfg_from_function_node(
    func_node: &Node,
    function_id: i64,
    source: &str,
) -> CfgWithEdges {
    let mut blocks = Vec::new();
    let mut edges = Vec::new();

    // Extract #[cfg(...)] condition from the function node
    let cfg_condition = extract_cfg_condition(func_node, source);

    // Create entry block
    let entry_block = create_entry_block(func_node, function_id, source);
    blocks.push(entry_block);

    // Find function body
    if let Some(body) = find_function_body(func_node) {
        // Extract blocks and edges from body
        let mut previous_block_idx: Option<usize> = Some(0); // Start with entry block
        extract_blocks_from_node_with_fallthrough(
            &body,
            function_id,
            source,
            &mut blocks,
            &mut edges,
            &mut previous_block_idx,
            LoopScope::NONE,
        );

        // If there's at least one body block, create edge from entry to first body
        if blocks.len() > 1 && edges.iter().all(|e| e.source_idx != 0) {
            edges.push(CfgEdge {
                source_idx: 0,
                target_idx: 1,
                edge_type: CfgEdgeType::Fallthrough,
            });
        }
    }

    // Apply #[cfg] condition to all blocks in this function
    if let Some(ref cond) = cfg_condition {
        for block in &mut blocks {
            block.cfg_condition = Some(cond.clone());
        }
    }

    CfgWithEdges {
        blocks,
        edges,
        function_id,
    }
}

/// Create entry block for a function
pub(super) fn create_entry_block(func_node: &Node, function_id: i64, _source: &str) -> CfgBlock {
    CfgBlock {
        function_id,
        kind: "entry".to_string(),
        terminator: "fallthrough".to_string(),
        byte_start: func_node.start_byte() as u64,
        byte_end: func_node.end_byte() as u64,
        start_line: func_node.start_position().row as u64 + 1,
        start_col: func_node.start_position().column as u64,
        end_line: func_node.end_position().row as u64 + 1,
        end_col: func_node.end_position().column as u64,
        cfg_hash: None,
        statements: None,
        cfg_condition: None,
    }
}

/// Find function body node
pub(super) fn find_function_body<'a>(func_node: &'a Node) -> Option<Node<'a>> {
    let mut cursor = func_node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let kind = child.kind();
            if kind == "block" || kind == "compound_statement" {
                return Some(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Create a CfgBlock from an AST node
pub(super) fn create_block_from_node(
    node: &Node,
    function_id: i64,
    _source: &str,
    kind: &str,
    terminator: &str,
) -> CfgBlock {
    CfgBlock {
        function_id,
        kind: kind.to_string(),
        terminator: terminator.to_string(),
        byte_start: node.start_byte() as u64,
        byte_end: node.end_byte() as u64,
        start_line: node.start_position().row as u64 + 1,
        start_col: node.start_position().column as u64,
        end_line: node.end_position().row as u64 + 1,
        end_col: node.end_position().column as u64,
        cfg_hash: None,
        statements: None,
        cfg_condition: None,
    }
}

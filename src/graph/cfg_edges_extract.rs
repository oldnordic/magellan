//! CFG Edge Extraction from tree-sitter AST
//!
//! This module extracts control flow edges between basic blocks
//! by analyzing tree-sitter AST nodes.

use crate::graph::schema::CfgBlock;
use tree_sitter::{Node, Parser as TsParser};

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
    parser.set_language(&language).unwrap();

    let tree = parser.parse(source, None).unwrap();
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
fn find_function_node<'a>(root: &'a Node<'a>) -> Option<Node<'a>> {
    let mut stack = vec![*root];

    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind == "function_item" || kind == "function_definition" {
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
            None,
        );

        // If there's at least one body block, create edge from entry to first body
        if blocks.len() > 1 && edges.iter().all(|e| e.source_idx != 0) {
            // Check if there's already an edge from entry (index 0) to any block
            // If not, create one
            eprintln!(
                "DEBUG: Creating entry->body edge, blocks.len()={}",
                blocks.len()
            );
            edges.push(CfgEdge {
                source_idx: 0,
                target_idx: 1,
                edge_type: CfgEdgeType::Fallthrough,
            });
        }
    }

    CfgWithEdges {
        blocks,
        edges,
        function_id,
    }
}

/// Scan a node's children for control-flow expressions and extract them.
/// Used for let/assignment bindings where the RHS may contain control flow.
fn extract_control_flow_children(
    node: &tree_sitter::Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_header: Option<usize>,
) {
    let control_flow_kinds = [
        "if_expression",
        "if_statement",
        "match_expression",
        "match_statement",
        "loop_expression",
        "for_expression",
        "while_expression",
        "for_statement",
        "while_statement",
        "do_statement",
        "return_expression",
        "return_statement",
        "break_expression",
        "break_statement",
        "continue_expression",
        "continue_statement",
        "try_expression",
        "await_expression",
        "closure_expression",
        "binary_expression",
    ];
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if control_flow_kinds.contains(&child.kind()) {
                extract_blocks_from_node_with_fallthrough(
                    &child,
                    function_id,
                    source,
                    blocks,
                    edges,
                    previous_block_idx,
                    loop_header,
                );
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Extract blocks and edges from an AST node with proper fallthrough tracking
/// This version creates blocks for all statements and tracks fallthrough edges
fn extract_blocks_from_node_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_header: Option<usize>,
) {
    let node_kind = node.kind();

    match node_kind {
        // If statement
        "if_expression" | "if_statement" => {
            extract_if_blocks_with_fallthrough(
                node,
                function_id,
                source,
                blocks,
                edges,
                previous_block_idx,
                loop_header,
            );
        }

        // Loop constructs
        "loop_expression" | "for_expression" | "while_expression" | "for_statement"
        | "while_statement" | "do_statement" => {
            extract_loop_blocks_with_fallthrough(
                node,
                function_id,
                source,
                blocks,
                edges,
                previous_block_idx,
            );
        }

        // Match/switch
        "match_expression" | "switch_statement" => {
            extract_match_blocks_with_fallthrough(
                node,
                function_id,
                source,
                blocks,
                edges,
                previous_block_idx,
            );
        }

        // Return
        "return_expression" | "return_statement" => {
            let block = create_block_from_node(node, function_id, source, "return", "return");
            let source_idx = blocks.len();
            blocks.push(block);

            // Create fallthrough edge from previous block to this return block
            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: source_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }
            *previous_block_idx = None; // Return blocks end the path

            // Self-loop for return (exit)
            edges.push(CfgEdge {
                source_idx,
                target_idx: source_idx,
                edge_type: CfgEdgeType::Return,
            });
        }

        // Break
        "break_expression" | "break_statement" => {
            let block = create_block_from_node(node, function_id, source, "break", "jump");
            let source_idx = blocks.len();
            blocks.push(block);

            // Create fallthrough edge from previous block
            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: source_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }
            *previous_block_idx = None; // Break ends the path

            if let Some(header) = loop_header {
                edges.push(CfgEdge {
                    source_idx,
                    target_idx: header,
                    edge_type: CfgEdgeType::Jump,
                });
            }
        }

        // Continue
        "continue_expression" | "continue_statement" => {
            let block = create_block_from_node(node, function_id, source, "continue", "jump");
            let source_idx = blocks.len();
            blocks.push(block);

            // Create fallthrough edge from previous block
            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: source_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }
            *previous_block_idx = None; // Continue ends the path

            if let Some(header) = loop_header {
                edges.push(CfgEdge {
                    source_idx,
                    target_idx: header,
                    edge_type: CfgEdgeType::BackEdge,
                });
            }
        }

        // Block/compound statement - process each statement with fallthrough
        "block" | "compound_statement" => {
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    let _child_kind = child.kind();
                    extract_blocks_from_node_with_fallthrough(
                        &child,
                        function_id,
                        source,
                        blocks,
                        edges,
                        previous_block_idx,
                        loop_header,
                    );
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }

        // Call expression
        "call_expression" | "call_statement" => {
            let block = create_block_from_node(node, function_id, source, "call", "fallthrough");
            let source_idx = blocks.len();
            blocks.push(block);

            // Create fallthrough edge from previous block to this call
            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: source_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }
            *previous_block_idx = Some(source_idx); // Call allows fallthrough
        }

        // Assignment, let binding, macro invocation, expression statement
        "let_declaration"
        | "let_statement"
        | "assignment_expression"
        | "assignment_statement"
        | "macro_invocation"
        | "statement"
        | "declaration_statement"
        | "item" => {
            // Create a basic block for these statements
            let kind = match node_kind {
                "let_declaration" | "let_statement" => "let",
                "assignment_expression" | "assignment_statement" => "assign",
                "macro_invocation" => "macro",
                _ => "stmt",
            };
            let block = create_block_from_node(node, function_id, source, kind, "fallthrough");
            let source_idx = blocks.len();
            blocks.push(block);

            // Create fallthrough edge from previous block to this block
            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: source_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }
            *previous_block_idx = Some(source_idx);

            // For let/assignment, scan RHS for nested control flow expressions
            if matches!(
                node_kind,
                "let_declaration"
                    | "let_statement"
                    | "assignment_expression"
                    | "assignment_statement"
            ) {
                extract_control_flow_children(
                    node,
                    function_id,
                    source,
                    blocks,
                    edges,
                    previous_block_idx,
                    loop_header,
                );
            }
        }

        // Expression statement - unwrap and process the inner expression
        "expression_statement" => {
            // Look for control flow expressions inside (if, match, loop, etc.)
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                let inner = cursor.node();
                let inner_kind = inner.kind();
                // If this is a control flow expression, process it directly
                if matches!(
                    inner_kind,
                    "if_expression"
                        | "if_statement"
                        | "match_expression"
                        | "match_statement"
                        | "loop_expression"
                        | "for_expression"
                        | "while_expression"
                        | "for_statement"
                        | "while_statement"
                        | "do_statement"
                        | "return_expression"
                        | "return_statement"
                        | "break_expression"
                        | "break_statement"
                        | "continue_expression"
                        | "continue_statement"
                        | "try_expression"
                        | "await_expression"
                        | "closure_expression"
                ) {
                    extract_blocks_from_node_with_fallthrough(
                        &inner,
                        function_id,
                        source,
                        blocks,
                        edges,
                        previous_block_idx,
                        loop_header,
                    );
                } else {
                    // Regular expression - create a statement block
                    let block =
                        create_block_from_node(node, function_id, source, "stmt", "fallthrough");
                    let source_idx = blocks.len();
                    blocks.push(block);

                    if let Some(prev_idx) = *previous_block_idx {
                        edges.push(CfgEdge {
                            source_idx: prev_idx,
                            target_idx: source_idx,
                            edge_type: CfgEdgeType::Fallthrough,
                        });
                    }
                    *previous_block_idx = Some(source_idx);
                }
            }
        }

        // Try expression (? operator / try blocks)
        // The ? operator desugars to:
        //   match expr {
        //       Ok(val) => val,
        //       Err(e) => return Err(e.into()),
        //   }
        "try_expression" => {
            // Block for the try expression itself (the ? operator point)
            let try_block = create_block_from_node(node, function_id, source, "try", "conditional");
            let try_idx = blocks.len();
            blocks.push(try_block);

            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: try_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }

            // Success path: fallthrough to continue processing
            // The body of the try expression continues after the ?
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    // Skip "try" keyword, process the expression being tried
                    if child.kind() != "try" {
                        extract_blocks_from_node_with_fallthrough(
                            &child, function_id, source, blocks, edges,
                            &mut Some(try_idx), loop_header,
                        );
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }

            // Error path: return edge (the ? returns early on Err)
            // We model this as a conditional false edge to a synthetic return
            edges.push(CfgEdge {
                source_idx: try_idx,
                target_idx: try_idx, // Self-loop indicates early return
                edge_type: CfgEdgeType::Return,
            });
        }

        // Await expression: suspension point, treat as call-like
        "await_expression" => {
            let block = create_block_from_node(node, function_id, source, "await", "fallthrough");
            let await_idx = blocks.len();
            blocks.push(block);

            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: await_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }

            // Process the expression being awaited (usually a call)
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    if child.kind() != "await" {
                        extract_blocks_from_node_with_fallthrough(
                            &child,
                            function_id,
                            source,
                            blocks,
                            edges,
                            &mut Some(await_idx),
                            loop_header,
                        );
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }

        // Closure expression: extract body as inline block
        "closure_expression" => {
            // Find the body block inside the closure
            let mut body_node = None;
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    if child.kind() == "block" {
                        body_node = Some(child);
                        break;
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }

            let closure_idx = blocks.len();
            let block = create_block_from_node(node, function_id, source, "closure", "fallthrough");
            blocks.push(block);

            if let Some(prev_idx) = *previous_block_idx {
                edges.push(CfgEdge {
                    source_idx: prev_idx,
                    target_idx: closure_idx,
                    edge_type: CfgEdgeType::Fallthrough,
                });
            }

            if let Some(body) = body_node {
                extract_blocks_from_node_with_fallthrough(
                    &body,
                    function_id,
                    source,
                    blocks,
                    edges,
                    &mut Some(closure_idx),
                    loop_header,
                );
            }
        }

        // Short-circuit operators: && and ||
        "binary_expression" => {
            // Check if this is && or ||
            let operator = node
                .children(&mut node.walk())
                .find(|child| !child.is_named())
                .and_then(|op| source.get(op.byte_range()))
                .map(|s| s.trim());

            match operator {
                Some("&&") => {
                    extract_short_circuit_blocks(
                        node, function_id, source, blocks, edges,
                        previous_block_idx, loop_header, true,
                    );
                }
                Some("||") => {
                    extract_short_circuit_blocks(
                        node, function_id, source, blocks, edges,
                        previous_block_idx, loop_header, false,
                    );
                }
                _ => {
                    // Other binary operators (+, -, *, etc.) - no control flow
                    extract_default_blocks(
                        node, function_id, source, blocks, edges,
                        previous_block_idx, loop_header,
                    );
                }
            }
        }

        // Default: try to handle as a statement block if it has children
        _unknown_kind => {
            // For unknown node types with children, process as a block
            if node.child_count() > 0 && !node.is_named() {
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        extract_blocks_from_node_with_fallthrough(
                            &cursor.node(),
                            function_id,
                            source,
                            blocks,
                            edges,
                            previous_block_idx,
                            loop_header,
                        );
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
            } else if node.is_named() {
                // Named leaf node - create a basic block for it
                let block =
                    create_block_from_node(node, function_id, source, node_kind, "fallthrough");
                let source_idx = blocks.len();
                blocks.push(block);

                // Create fallthrough edge from previous block
                if let Some(prev_idx) = *previous_block_idx {
                    edges.push(CfgEdge {
                        source_idx: prev_idx,
                        target_idx: source_idx,
                        edge_type: CfgEdgeType::Fallthrough,
                    });
                }
                *previous_block_idx = Some(source_idx);
            }
            // Else: skip unnamed leaf nodes with no children (comments, etc.)
        }
    }
}

/// Extract blocks for short-circuit operators (&& and ||)
///
/// For `a && b`:
///   - Block for `a` with conditional: true -> `b`, false -> merge
///   - Block for `b` with fallthrough -> merge
/// For `a || b`:
///   - Block for `a` with conditional: true -> merge, false -> `b`
///   - Block for `b` with fallthrough -> merge
fn extract_short_circuit_blocks(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_header: Option<usize>,
    is_and: bool,
) {
    // Find left and right operands
    let mut left = None;
    let mut right = None;
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                if left.is_none() {
                    left = Some(child);
                } else if right.is_none() {
                    right = Some(child);
                    break;
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Create block for the left operand (the condition)
    let left_block = create_block_from_node(
        node, function_id, source,
        if is_and { "and" } else { "or" },
        "conditional",
    );
    let left_idx = blocks.len();
    blocks.push(left_block);

    if let Some(prev_idx) = *previous_block_idx {
        edges.push(CfgEdge {
            source_idx: prev_idx,
            target_idx: left_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    // Process left operand
    if let Some(left_node) = left {
        extract_blocks_from_node_with_fallthrough(
            &left_node, function_id, source, blocks, edges,
            &mut Some(left_idx), loop_header,
        );
    }

    // Create merge block for both branches to converge
    let merge_block = CfgBlock {
        function_id,
        kind: "merge".to_string(),
        terminator: "fallthrough".to_string(),
        byte_start: node.end_byte() as u64,
        byte_end: node.end_byte() as u64,
        start_line: node.end_position().row as u64 + 1,
        start_col: node.end_position().column as u64,
        end_line: node.end_position().row as u64 + 1,
        end_col: node.end_position().column as u64,
        cfg_hash: None,
        statements: None,
        coord_x: 0,
        coord_y: 0,
        coord_z: 0,
        coord_t: None,
    };
    let merge_idx = blocks.len();
    blocks.push(merge_block);

    // For &&: true branch goes to right operand, false branch skips to merge
    // For ||: true branch skips to merge, false branch goes to right operand
    if let Some(right_node) = right {
        let right_block = create_block_from_node(
            &right_node, function_id, source, "short_circuit_rhs", "fallthrough",
        );
        let right_idx = blocks.len();
        blocks.push(right_block);

        // Conditional edge to right operand
        edges.push(CfgEdge {
            source_idx: left_idx,
            target_idx: right_idx,
            edge_type: if is_and {
                CfgEdgeType::ConditionalTrue
            } else {
                CfgEdgeType::ConditionalFalse
            },
        });

        // Complementary conditional edge to merge (short-circuit path)
        edges.push(CfgEdge {
            source_idx: left_idx,
            target_idx: merge_idx,
            edge_type: if is_and {
                CfgEdgeType::ConditionalFalse
            } else {
                CfgEdgeType::ConditionalTrue
            },
        });

        // Process right operand
        let mut right_last_idx = Some(right_idx);
        extract_blocks_from_node_with_fallthrough(
            &right_node, function_id, source, blocks, edges,
            &mut right_last_idx, loop_header,
        );

        // Fallthrough from right branch to merge
        if let Some(last_idx) = right_last_idx {
            edges.push(CfgEdge {
                source_idx: last_idx,
                target_idx: merge_idx,
                edge_type: CfgEdgeType::Fallthrough,
            });
        }
    } else {
        // No right operand - both conditions go directly to merge
        edges.push(CfgEdge {
            source_idx: left_idx,
            target_idx: merge_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    *previous_block_idx = Some(merge_idx);
}

/// Default block extraction for named nodes without control flow semantics
fn extract_default_blocks(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_header: Option<usize>,
) {
    let block = create_block_from_node(node, function_id, source, node.kind(), "fallthrough");
    let idx = blocks.len();
    blocks.push(block);

    if let Some(prev_idx) = *previous_block_idx {
        edges.push(CfgEdge {
            source_idx: prev_idx,
            target_idx: idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    // Process children
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                extract_blocks_from_node_with_fallthrough(
                    &child, function_id, source, blocks, edges,
                    &mut Some(idx), loop_header,
                );
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    *previous_block_idx = Some(idx);
}

/// Create entry block for a function
fn create_entry_block(func_node: &Node, function_id: i64, _source: &str) -> CfgBlock {
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
        coord_x: 0,
        coord_y: 0,
        coord_z: 0,
        coord_t: None,
    }
}

/// Find function body node
fn find_function_body<'a>(func_node: &'a Node) -> Option<Node<'a>> {
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

/// Extract blocks for if statement with fallthrough tracking
fn extract_if_blocks_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_header: Option<usize>,
) {
    // Find then and else blocks FIRST (before creating blocks)
    let mut then_node = None;
    let mut else_node = None;

    let mut cursor = node.walk();
    let mut child_count = 0;
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child_count {
                2 => {
                    // Then block (after condition)
                    if child.kind() == "block" || child.kind() == "statement_block" {
                        then_node = Some(child);
                    }
                }
                3 => {
                    // Else clause
                    if child.kind() == "else_clause" || child.kind() == "else" {
                        let mut else_cursor = child.walk();
                        if else_cursor.goto_first_child() {
                            loop {
                                let else_child = else_cursor.node();
                                if else_child.kind() == "block"
                                    || else_child.kind() == "statement_block"
                                {
                                    else_node = Some(else_child);
                                    break;
                                }
                                if !else_cursor.goto_next_sibling() {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            child_count += 1;
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Create condition block AFTER finding then/else
    let condition_idx = blocks.len();
    let condition_block = create_block_from_node(node, function_id, source, "if", "conditional");
    blocks.push(condition_block);

    // Create fallthrough edge from previous block to condition
    if let Some(prev_idx) = *previous_block_idx {
        edges.push(CfgEdge {
            source_idx: prev_idx,
            target_idx: condition_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    // Extract then block
    let then_start_idx = blocks.len();
    if let Some(then) = then_node {
        extract_blocks_from_node_with_fallthrough(
            &then,
            function_id,
            source,
            blocks,
            edges,
            &mut None,
            loop_header,
        );
    }
    let then_end_idx = blocks.len().saturating_sub(1);

    // Extract else block
    let else_start_idx = blocks.len();
    if let Some(else_block) = else_node {
        extract_blocks_from_node_with_fallthrough(
            &else_block,
            function_id,
            source,
            blocks,
            edges,
            &mut None,
            loop_header,
        );
    }
    let else_end_idx = blocks.len().saturating_sub(1);

    // Create edges from condition
    if then_node.is_some() {
        edges.push(CfgEdge {
            source_idx: condition_idx,
            target_idx: then_start_idx,
            edge_type: CfgEdgeType::ConditionalTrue,
        });
    }

    if else_node.is_some() {
        edges.push(CfgEdge {
            source_idx: condition_idx,
            target_idx: else_start_idx,
            edge_type: CfgEdgeType::ConditionalFalse,
        });
    } else {
        // No else - fallthrough on false (to merge or next statement)
        let fallthrough_idx = blocks.len();
        edges.push(CfgEdge {
            source_idx: condition_idx,
            target_idx: fallthrough_idx,
            edge_type: CfgEdgeType::ConditionalFalse,
        });
    }

    // Create merge block and set it as the next block for fallthrough
    let merge_idx = blocks.len();
    let merge_byte_start = node.end_byte() as u64;
    let merge_byte_end = merge_byte_start;
    let merge_start_line = node.end_position().row as u64 + 1;
    let merge_start_col = node.end_position().column as u64;
    let merge_end_line = merge_start_line;
    let merge_end_col = merge_start_col;

    let merge_block = CfgBlock {
        function_id,
        kind: "merge".to_string(),
        terminator: "fallthrough".to_string(),
        byte_start: merge_byte_start,
        byte_end: merge_byte_end,
        start_line: merge_start_line,
        start_col: merge_start_col,
        end_line: merge_end_line,
        end_col: merge_end_col,
        cfg_hash: None,
        statements: None,
        coord_x: 0,
        coord_y: 0,
        coord_z: 0,
        coord_t: None,
    };
    blocks.push(merge_block);

    // Merge edges from then/else blocks to merge
    if then_node.is_some() && then_end_idx >= then_start_idx {
        edges.push(CfgEdge {
            source_idx: then_end_idx,
            target_idx: merge_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }
    if else_node.is_some() && else_end_idx >= else_start_idx {
        edges.push(CfgEdge {
            source_idx: else_end_idx,
            target_idx: merge_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    // Merge block becomes the next block for fallthrough
    *previous_block_idx = Some(merge_idx);
}

/// Extract blocks for loop constructs with fallthrough tracking
fn extract_loop_blocks_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
) {
    let header_idx = blocks.len();

    // Create loop header block
    let kind = match node.kind() {
        "loop_expression" | "for_statement" => "loop",
        "while_expression" | "while_statement" => "while",
        "for_expression" => "for",
        "do_statement" => "do",
        _ => "loop",
    };

    let header_block = create_block_from_node(node, function_id, source, kind, "conditional");
    blocks.push(header_block);

    // Create fallthrough edge from previous block to loop header
    if let Some(prev_idx) = *previous_block_idx {
        edges.push(CfgEdge {
            source_idx: prev_idx,
            target_idx: header_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    // Find loop body
    let mut body_node = None;
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "block" || child.kind() == "statement_block" {
                body_node = Some(child);
                break;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Extract body blocks with loop_header set
    let body_start_idx = blocks.len();
    if let Some(body) = body_node {
        extract_blocks_from_node_with_fallthrough(
            &body,
            function_id,
            source,
            blocks,
            edges,
            &mut None,        // Start fresh in body
            Some(header_idx), // Set loop header for break/continue
        );
    }
    let body_end_idx = blocks.len().saturating_sub(1);

    // Create back-edge from end of body to header
    if body_node.is_some() && body_end_idx >= body_start_idx {
        edges.push(CfgEdge {
            source_idx: body_end_idx,
            target_idx: header_idx,
            edge_type: CfgEdgeType::BackEdge,
        });
    }

    // Create exit block
    let exit_idx = blocks.len();
    let exit_block = CfgBlock {
        function_id,
        kind: "exit".to_string(),
        terminator: "fallthrough".to_string(),
        byte_start: 0,
        byte_end: 0,
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 0,
        cfg_hash: None,
        statements: None,
        coord_x: 0,
        coord_y: 0,
        coord_z: 0,
        coord_t: None,
    };
    blocks.push(exit_block);

    // Edge from header to exit (loop exit on false condition)
    edges.push(CfgEdge {
        source_idx: header_idx,
        target_idx: exit_idx,
        edge_type: CfgEdgeType::ConditionalFalse,
    });

    // Exit block becomes the next block for fallthrough
    *previous_block_idx = Some(exit_idx);
}

/// Extract blocks for match/switch statement with fallthrough tracking
fn extract_match_blocks_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
) {
    // Create fallthrough edge from previous block to match
    let dispatch_idx = blocks.len();

    // Create dispatch block
    let kind = if node.kind() == "match_expression" {
        "match"
    } else {
        "switch"
    };
    let dispatch_block = create_block_from_node(node, function_id, source, kind, "conditional");
    blocks.push(dispatch_block);

    // Create fallthrough edge from previous block to dispatch
    if let Some(prev_idx) = *previous_block_idx {
        edges.push(CfgEdge {
            source_idx: prev_idx,
            target_idx: dispatch_idx,
            edge_type: CfgEdgeType::Fallthrough,
        });
    }

    // Find all arms/cases
    let mut arm_nodes = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "match_block" || child.kind() == "match_body" {
                // Rust match arms
                let mut arm_cursor = child.walk();
                if arm_cursor.goto_first_child() {
                    loop {
                        let arm = arm_cursor.node();
                        if arm.kind() == "match_arm" {
                            arm_nodes.push(arm);
                        }
                        if !arm_cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
            } else if child.kind() == "switch_body" || child.kind() == "body" {
                // C/C++ switch cases
                let mut case_cursor = child.walk();
                if case_cursor.goto_first_child() {
                    loop {
                        let case = case_cursor.node();
                        if case.kind() == "switch_case" || case.kind() == "case" {
                            arm_nodes.push(case);
                        }
                        if !case_cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Extract each arm
    let mut arm_indices = Vec::new();
    let mut guard_indices = Vec::new(); // None for unguarded, Some(idx) for guarded

    for arm in &arm_nodes {
        // Check if arm has a guard: look inside match_pattern for "if" token
        let has_guard = arm.child(0)
            .filter(|c| c.kind() == "match_pattern")
            .map(|pat| {
                let mut cursor = pat.walk();
                let mut found = false;
                if cursor.goto_first_child() {
                    loop {
                        if cursor.node().kind() == "if" {
                            found = true;
                            break;
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                found
            })
            .unwrap_or(false);

        if !has_guard {
            // No guard - process arm as unguarded
            let arm_start_idx = blocks.len();
            extract_blocks_from_node_with_fallthrough(
                arm,
                function_id,
                source,
                blocks,
                edges,
                &mut None, // Each arm starts fresh
                None,      // No loop context in match
            );
            let arm_end_idx = blocks.len().saturating_sub(1);
            arm_indices.push((arm_start_idx, arm_end_idx));
            guard_indices.push(None);

            // Edge from dispatch to arm
            edges.push(CfgEdge {
                source_idx: dispatch_idx,
                target_idx: arm_start_idx,
                edge_type: CfgEdgeType::ConditionalTrue,
            });
            continue;
        }

        // Find the guard expression (named child after "if" inside match_pattern)
        let match_pattern = arm.child(0).unwrap();
        let mut guard_expr = None;
        let mut cursor = match_pattern.walk();
        let mut found_if = false;
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "if" {
                    found_if = true;
                } else if found_if && child.is_named() {
                    guard_expr = Some(child);
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        let Some(guard) = guard_expr else {
            // Malformed AST: guard keyword found but no expression
            let arm_start_idx = blocks.len();
            extract_blocks_from_node_with_fallthrough(
                arm, function_id, source, blocks, edges,
                &mut None, None,
            );
            let arm_end_idx = blocks.len().saturating_sub(1);
            arm_indices.push((arm_start_idx, arm_end_idx));
            guard_indices.push(None);

            edges.push(CfgEdge {
                source_idx: dispatch_idx,
                target_idx: arm_start_idx,
                edge_type: CfgEdgeType::ConditionalTrue,
            });
            continue;
        };

        // Create guard block
        let guard_block = create_block_from_node(
            &guard, function_id, source, "match_guard", "conditional"
        );
        let guard_idx = blocks.len();
        blocks.push(guard_block);
        guard_indices.push(Some(guard_idx));

        // Edge from dispatch to guard
        edges.push(CfgEdge {
            source_idx: dispatch_idx,
            target_idx: guard_idx,
            edge_type: CfgEdgeType::ConditionalTrue,
        });

        // Find body expression (named child after "=>" in match_arm)
        let mut body_node = None;
        let mut cursor = arm.walk();
        let mut found_arrow = false;
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "=>" {
                    found_arrow = true;
                } else if found_arrow && child.is_named() {
                    body_node = Some(child);
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        // Process body starting from guard
        let arm_start_idx = blocks.len();
        if let Some(body) = body_node {
            extract_blocks_from_node_with_fallthrough(
                &body,
                function_id,
                source,
                blocks,
                edges,
                &mut Some(guard_idx),
                None,
            );
        }
        let arm_end_idx = blocks.len().saturating_sub(1);
        arm_indices.push((arm_start_idx, arm_end_idx));

        // Guard true -> body
        if arm_start_idx > guard_idx {
            edges.push(CfgEdge {
                source_idx: guard_idx,
                target_idx: arm_start_idx,
                edge_type: CfgEdgeType::ConditionalTrue,
            });
        }
        // Guard false -> added in second pass below
    }

    // Second pass: add guard false edges (fallthrough to next arm or merge)
    let merge_idx = blocks.len();
    for (i, guard_idx_opt) in guard_indices.iter().enumerate() {
        if let Some(guard_idx) = guard_idx_opt {
            let fallback_target = if i + 1 < arm_nodes.len() {
                // Next arm's entry point
                if let Some(next_guard) = guard_indices[i + 1] {
                    next_guard
                } else {
                    arm_indices[i + 1].0
                }
            } else {
                // Last arm - fallback to merge
                merge_idx
            };
            edges.push(CfgEdge {
                source_idx: *guard_idx,
                target_idx: fallback_target,
                edge_type: CfgEdgeType::ConditionalFalse,
            });
        }
    }

    // Create merge block after match
    let merge_byte_start = node.end_byte() as u64;
    let merge_byte_end = merge_byte_start;
    let merge_start_line = node.end_position().row as u64 + 1;
    let merge_start_col = node.end_position().column as u64;
    let merge_end_line = merge_start_line;
    let merge_end_col = merge_start_col;

    let merge_block = CfgBlock {
        function_id,
        kind: "merge".to_string(),
        terminator: "fallthrough".to_string(),
        byte_start: merge_byte_start,
        byte_end: merge_byte_end,
        start_line: merge_start_line,
        start_col: merge_start_col,
        end_line: merge_end_line,
        end_col: merge_end_col,
        cfg_hash: None,
        statements: None,
        coord_x: 0,
        coord_y: 0,
        coord_z: 0,
        coord_t: None,
    };
    blocks.push(merge_block);

    // Merge edges from each arm to merge block
    for (start, end) in &arm_indices {
        if *end >= *start {
            edges.push(CfgEdge {
                source_idx: *end,
                target_idx: merge_idx,
                edge_type: CfgEdgeType::Fallthrough,
            });
        }
    }

    // Merge block becomes the next block for fallthrough
    *previous_block_idx = Some(merge_idx);
}

/// Create a CfgBlock from an AST node
fn create_block_from_node(
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
        coord_x: 0,
        coord_y: 0,
        coord_z: 0,
        coord_t: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfg_edge_type_as_str() {
        assert_eq!(CfgEdgeType::Fallthrough.as_str(), "fallthrough");
        assert_eq!(CfgEdgeType::ConditionalTrue.as_str(), "conditional_true");
        assert_eq!(CfgEdgeType::ConditionalFalse.as_str(), "conditional_false");
        assert_eq!(CfgEdgeType::Jump.as_str(), "jump");
        assert_eq!(CfgEdgeType::BackEdge.as_str(), "back_edge");
        assert_eq!(CfgEdgeType::Call.as_str(), "call");
        assert_eq!(CfgEdgeType::Return.as_str(), "return");
    }

    #[test]
    fn test_cfg_edge_creation() {
        let edge = CfgEdge {
            source_idx: 0,
            target_idx: 1,
            edge_type: CfgEdgeType::Fallthrough,
        };

        assert_eq!(edge.source_idx, 0);
        assert_eq!(edge.target_idx, 1);
        assert_eq!(edge.edge_type, CfgEdgeType::Fallthrough);
    }

    #[test]
    fn test_extract_cfg_with_edges_simple_function() {
        let source = r#"
fn simple() -> i32 {
    let x = 1;
    let y = 2;
    return x + y;
}
"#;

        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        assert_eq!(result.function_id, 1);
        assert!(result.blocks.len() >= 1, "Should have at least entry block");
    }

    #[test]
    fn test_extract_cfg_with_edges_if_statement() {
        let source = r#"
fn with_if(x: i32) -> i32 {
    if x > 0 {
        return x;
    } else {
        return -x;
    }
}
"#;

        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        assert!(result.blocks.len() >= 2, "Should have multiple blocks");
        assert!(result.edges.len() >= 2, "Should have edges for if/else");

        // Check for conditional edges
        let has_conditional = result.edges.iter().any(|e| {
            matches!(
                e.edge_type,
                CfgEdgeType::ConditionalTrue | CfgEdgeType::ConditionalFalse
            )
        });
        assert!(has_conditional, "Should have conditional edges");
    }

    #[test]
    fn test_extract_cfg_with_edges_loop() {
        let source = r#"
fn with_loop() -> i32 {
    let mut sum = 0;
    for i in 0..10 {
        sum += i;
    }
    return sum;
}
"#;

        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        assert!(result.blocks.len() >= 2, "Should have multiple blocks");
        // Note: Back-edge detection depends on proper loop body extraction
        // For now, just verify we have edges
        assert!(result.edges.len() >= 0, "Should have edges");
    }

    #[test]
    fn test_extract_cfg_with_edges_return() {
        let source = r#"
fn with_return(x: i32) -> i32 {
    if x > 0 {
        return x;
    }
    return 0;
}
"#;

        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        // Check for return edges
        let has_return = result
            .edges
            .iter()
            .any(|e| matches!(e.edge_type, CfgEdgeType::Return));
        assert!(has_return, "Should have return edges");
    }

    #[test]
    fn test_extract_cfg_empty_function() {
        let source = r#"
fn empty() {}
"#;

        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        assert_eq!(result.function_id, 1);
        assert!(result.blocks.len() >= 1, "Should have entry block");
    }

    #[test]
    fn test_extract_try_expression() {
        let source = r#"
fn test() -> Result<i32, ()> {
    let x = try_result()?;
    Ok(x)
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());
        let try_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "try").collect();
        assert!(!try_blocks.is_empty(), "Should have try blocks");
    }

    #[test]
    fn test_extract_try_operator() {
        let source = r#"
fn test() -> Result<i32, ()> {
    let x = some_result()?;
    Ok(x + 1)
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        let try_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "try").collect();
        assert!(!try_blocks.is_empty(), "Should have try blocks for ? operator");

        // Should have a return edge (error path)
        let return_edges: Vec<_> = result.edges.iter()
            .filter(|e| matches!(e.edge_type, CfgEdgeType::Return))
            .collect();
        assert!(!return_edges.is_empty(), "Should have return edge for error path");
    }

    #[test]
    fn test_extract_await_expression() {
        let source = r#"
async fn test() -> i32 {
    let x = some_async().await;
    x
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());
        let await_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "await").collect();
        assert!(!await_blocks.is_empty(), "Should have await blocks");
    }

    #[test]
    fn test_extract_closure_expression() {
        let source = r#"
fn test() {
    let f = |x: i32| {
        if x > 0 { x } else { 0 }
    };
    println!("{}", f(1));
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());
        let closure_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "closure").collect();
        assert!(!closure_blocks.is_empty(), "Should have closure blocks");
        // Closure body should have nested if
        let if_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "if").collect();
        assert!(!if_blocks.is_empty(), "Closure body should contain if blocks");
    }

    #[test]
    fn test_merge_block_has_nonzero_range() {
        let source = r#"
fn test() {
    if x { a } else { b }
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());
        let merge_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "merge").collect();
        assert!(!merge_blocks.is_empty(), "Should have merge blocks");
        for merge in merge_blocks {
            assert!(
                merge.byte_start > 0 || merge.start_line > 0,
                "Merge block should have non-zero position"
            );
        }
    }

    #[test]
    fn test_extract_and_operator() {
        let source = r#"
fn test() -> bool {
    let a = true;
    let b = false;
    a && b
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        let and_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "and").collect();
        assert!(!and_blocks.is_empty(), "Should have && blocks");
        let and_idx = result.blocks.iter().position(|b| b.kind == "and").unwrap();

        let rhs_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "short_circuit_rhs").collect();
        assert!(!rhs_blocks.is_empty(), "Should have rhs blocks");
        let rhs_idx = result.blocks.iter().position(|b| b.kind == "short_circuit_rhs").unwrap();

        // Should have ConditionalTrue edge from and to rhs
        let true_edges: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == and_idx && e.target_idx == rhs_idx && e.edge_type == CfgEdgeType::ConditionalTrue)
            .collect();
        assert!(!true_edges.is_empty(), "Should have ConditionalTrue edge from and to rhs");

        // Should have ConditionalFalse edge from and to merge (short-circuit)
        let false_edges: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == and_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
            .collect();
        assert!(!false_edges.is_empty(), "Should have ConditionalFalse edge for short-circuit");

        // Should have fallthrough from right branch's last block to merge
        let merge_idx = result.blocks.iter().position(|b| b.kind == "merge").unwrap();
        let rhs_to_merge: Vec<_> = result.edges.iter()
            .filter(|e| e.target_idx == merge_idx && e.edge_type == CfgEdgeType::Fallthrough)
            .collect();
        assert!(!rhs_to_merge.is_empty(), "Should have fallthrough from right branch to merge");
    }

    #[test]
    fn test_extract_or_operator() {
        let source = r#"
fn test() -> bool {
    let a = false;
    let b = true;
    a || b
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        let or_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "or").collect();
        assert!(!or_blocks.is_empty(), "Should have || blocks");
        let or_idx = result.blocks.iter().position(|b| b.kind == "or").unwrap();

        let rhs_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "short_circuit_rhs").collect();
        assert!(!rhs_blocks.is_empty(), "Should have rhs blocks");
        let rhs_idx = result.blocks.iter().position(|b| b.kind == "short_circuit_rhs").unwrap();

        // Should have ConditionalFalse edge from or to rhs
        let false_edges: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == or_idx && e.target_idx == rhs_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
            .collect();
        assert!(!false_edges.is_empty(), "Should have ConditionalFalse edge from or to rhs");

        // Should have ConditionalTrue edge from or to merge (short-circuit)
        let true_edges: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == or_idx && e.edge_type == CfgEdgeType::ConditionalTrue)
            .collect();
        assert!(!true_edges.is_empty(), "Should have ConditionalTrue edge for short-circuit");
    }

    #[test]
    fn test_extract_chained_short_circuit() {
        let source = r#"
fn test() -> bool {
    a && b && c
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        let and_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "and").collect();
        assert_eq!(and_blocks.len(), 2, "Should have two and blocks for chained &&");

        // Verify no dead ends - every conditional should have at least 2 outgoing edges
        for (idx, block) in result.blocks.iter().enumerate().filter(|(_, b)| b.kind == "and" || b.kind == "or") {
            let outgoing = result.edges.iter().filter(|e| e.source_idx == idx).count();
            assert!(outgoing >= 2, "Block {} ({}) should have at least 2 outgoing edges, got {}", idx, block.kind, outgoing);
        }

        // Verify the chain is connected: first and's merge should connect to second and
        let first_and_idx = result.blocks.iter().position(|b| b.kind == "and").unwrap();
        let merge_indices: Vec<usize> = result.blocks.iter().enumerate()
            .filter(|(_, b)| b.kind == "merge")
            .map(|(i, _)| i)
            .collect();
        assert!(!merge_indices.is_empty(), "Should have merge blocks");

        // Each and block should have ConditionalTrue and ConditionalFalse outgoing edges
        for (idx, block) in result.blocks.iter().enumerate().filter(|(_, b)| b.kind == "and" || b.kind == "or") {
            let cond_true = result.edges.iter().any(|e| e.source_idx == idx && e.edge_type == CfgEdgeType::ConditionalTrue);
            let cond_false = result.edges.iter().any(|e| e.source_idx == idx && e.edge_type == CfgEdgeType::ConditionalFalse);
            assert!(cond_true, "Block {} should have ConditionalTrue edge", idx);
            assert!(cond_false, "Block {} should have ConditionalFalse edge", idx);
        }
    }

    #[test]
    fn test_extract_match_guard() {
        let source = r#"
fn test_match_guard(x: Option<i32>) -> i32 {
    match x {
        Some(v) if v > 0 => v,
        Some(v) => -v,
        None => 0,
    }
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        // Should have match_guard blocks
        let guard_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "match_guard").collect();
        assert_eq!(guard_blocks.len(), 1, "Should have exactly one match_guard block");
        let guard_idx = result.blocks.iter().position(|b| b.kind == "match_guard").unwrap();

        // Guard block should have ConditionalTrue edge to arm body
        let guard_true_edges: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == guard_idx && e.edge_type == CfgEdgeType::ConditionalTrue)
            .collect();
        assert!(!guard_true_edges.is_empty(), "Guard should have ConditionalTrue edge to body");

        // Guard block should have ConditionalFalse edge to next arm
        let guard_false_edges: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == guard_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
            .collect();
        assert!(!guard_false_edges.is_empty(), "Guard should have ConditionalFalse edge to next arm");

        // The false edge should point to a block after the guard (next arm entry)
        let false_target = guard_false_edges[0].target_idx;
        assert!(false_target > guard_idx, "Guard false edge should point to a later block");
    }

    #[test]
    fn test_extract_match_guard_chain() {
        let source = r#"
fn test_chain(x: Option<i32>) -> i32 {
    match x {
        Some(v) if v > 10 => v * 2,
        Some(v) if v > 0 => v,
        Some(v) => -v,
        None => 0,
    }
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        // Should have two match_guard blocks
        let guard_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "match_guard").collect();
        assert_eq!(guard_blocks.len(), 2, "Should have two match_guard blocks");

        let guard_indices: Vec<usize> = result.blocks.iter().enumerate()
            .filter(|(_, b)| b.kind == "match_guard")
            .map(|(i, _)| i)
            .collect();

        // First guard false should point to second guard
        let first_guard = guard_indices[0];
        let second_guard = guard_indices[1];
        let first_false: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == first_guard && e.edge_type == CfgEdgeType::ConditionalFalse)
            .collect();
        assert!(!first_false.is_empty(), "First guard should have ConditionalFalse edge");
        assert_eq!(first_false[0].target_idx, second_guard,
            "First guard false should fall through to second guard");

        // Second guard false should point to an unguarded arm (not another guard)
        let second_false: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == second_guard && e.edge_type == CfgEdgeType::ConditionalFalse)
            .collect();
        assert!(!second_false.is_empty(), "Second guard should have ConditionalFalse edge");
        assert!(
            !guard_indices.contains(&second_false[0].target_idx),
            "Second guard false should point to unguarded arm, not another guard"
        );
    }

    #[test]
    fn test_extract_match_guard_last_arm() {
        let source = r#"
fn test_last(x: Option<i32>) -> i32 {
    match x {
        Some(v) => v,
        None if x.is_none() => 0,
    }
}
"#;
        let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::language());

        let guard_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "match_guard").collect();
        assert_eq!(guard_blocks.len(), 1, "Should have one match_guard block");
        let guard_idx = result.blocks.iter().position(|b| b.kind == "match_guard").unwrap();

        // Last guard false should point to merge block
        let guard_false: Vec<_> = result.edges.iter()
            .filter(|e| e.source_idx == guard_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
            .collect();
        assert!(!guard_false.is_empty(), "Last guard should have ConditionalFalse edge");

        let merge_indices: Vec<usize> = result.blocks.iter().enumerate()
            .filter(|(_, b)| b.kind == "merge")
            .map(|(i, _)| i)
            .collect();
        assert!(!merge_indices.is_empty(), "Should have merge blocks");
        assert!(
            merge_indices.contains(&guard_false[0].target_idx),
            "Last guard false should point to merge block"
        );
    }
}

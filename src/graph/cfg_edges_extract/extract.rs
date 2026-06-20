use super::*;
use tree_sitter::Node;

const CONTROL_FLOW_KINDS: &[&str] = &[
    "if_expression",
    "if_statement",
    "match_expression",
    "match_statement",
    "switch_statement",
    "expression_switch_statement",
    "type_switch_statement",
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
];

fn is_control_flow(kind: &str) -> bool {
    CONTROL_FLOW_KINDS.contains(&kind)
}

fn is_cfg_relevant_expression(kind: &str) -> bool {
    is_control_flow(kind)
        || matches!(
            kind,
            "binary_expression" | "call_expression" | "call_statement"
        )
}

/// Scan a node's children for control-flow expressions and extract them.
/// Used for let/assignment bindings where the RHS may contain control flow.
pub(super) fn extract_control_flow_children(
    node: &tree_sitter::Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_scope: LoopScope,
) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let kind = child.kind();
            if is_cfg_relevant_expression(kind) {
                extract_blocks_from_node_with_fallthrough(
                    &child,
                    function_id,
                    source,
                    blocks,
                    edges,
                    previous_block_idx,
                    loop_scope,
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
pub(super) fn extract_blocks_from_node_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_scope: LoopScope,
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
                loop_scope,
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
        "match_expression"
        | "switch_statement"
        | "expression_switch_statement"
        | "type_switch_statement" => {
            extract_match_blocks_with_fallthrough(
                node,
                function_id,
                source,
                blocks,
                edges,
                previous_block_idx,
                loop_scope,
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

            if let Some(exit) = loop_scope.exit {
                edges.push(CfgEdge {
                    source_idx,
                    target_idx: exit,
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

            if let Some(header) = loop_scope.header {
                edges.push(CfgEdge {
                    source_idx,
                    target_idx: header,
                    edge_type: CfgEdgeType::BackEdge,
                });
            }
        }

        // Block/compound statement - process each statement with fallthrough
        "block" | "compound_statement" | "statement_list" => {
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
                        loop_scope,
                    );
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }

        // Call expression
        "call_expression" | "call_statement" => {
            let block = create_block_from_node(node, function_id, source, "call", "call");
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

            edges.push(CfgEdge {
                source_idx,
                target_idx: source_idx,
                edge_type: CfgEdgeType::Call,
            });

            *previous_block_idx = Some(source_idx); // Call allows fallthrough
        }

        // Assignment, let binding, macro invocation, expression statement
        "let_declaration"
        | "let_statement"
        | "assignment_expression"
        | "assignment_statement"
        | "macro_invocation"
        | "declaration_statement"
        | "item" => {
            let kind = match node_kind {
                "let_declaration" | "let_statement" => "let",
                "assignment_expression" | "assignment_statement" => "assign",
                "macro_invocation" => "macro",
                _ => "stmt",
            };
            let let_else_alternative = matches!(node_kind, "let_declaration" | "let_statement")
                .then(|| node.child_by_field_name("alternative"))
                .flatten();

            if let Some(alternative) = let_else_alternative {
                let block =
                    create_block_from_node(node, function_id, source, "let_else", "conditional");
                let source_idx = blocks.len();
                blocks.push(block);

                if let Some(prev_idx) = *previous_block_idx {
                    edges.push(CfgEdge {
                        source_idx: prev_idx,
                        target_idx: source_idx,
                        edge_type: CfgEdgeType::Fallthrough,
                    });
                }

                let alternative_start_idx = blocks.len();
                let mut alternative_last_idx = None;
                extract_blocks_from_node_with_fallthrough(
                    &alternative,
                    function_id,
                    source,
                    blocks,
                    edges,
                    &mut alternative_last_idx,
                    loop_scope,
                );

                if alternative_start_idx < blocks.len() {
                    edges.push(CfgEdge {
                        source_idx,
                        target_idx: alternative_start_idx,
                        edge_type: CfgEdgeType::ConditionalFalse,
                    });
                }

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
                    cfg_condition: None,
                };
                let merge_idx = blocks.len();
                blocks.push(merge_block);

                edges.push(CfgEdge {
                    source_idx,
                    target_idx: merge_idx,
                    edge_type: CfgEdgeType::ConditionalTrue,
                });

                *previous_block_idx = Some(merge_idx);
            } else {
                let block = create_block_from_node(node, function_id, source, kind, "fallthrough");
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
                    loop_scope,
                );
            }
        }

        "statement" | "expression_statement" => {
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                let inner = cursor.node();
                if is_cfg_relevant_expression(inner.kind()) {
                    extract_blocks_from_node_with_fallthrough(
                        &inner,
                        function_id,
                        source,
                        blocks,
                        edges,
                        previous_block_idx,
                        loop_scope,
                    );
                } else {
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
                            &child,
                            function_id,
                            source,
                            blocks,
                            edges,
                            &mut Some(try_idx),
                            loop_scope,
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
                            loop_scope,
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
                    loop_scope,
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
                        node,
                        function_id,
                        source,
                        blocks,
                        edges,
                        previous_block_idx,
                        loop_scope,
                        true,
                    );
                }
                Some("||") => {
                    extract_short_circuit_blocks(
                        node,
                        function_id,
                        source,
                        blocks,
                        edges,
                        previous_block_idx,
                        loop_scope,
                        false,
                    );
                }
                _ => {
                    // Other binary operators (+, -, *, etc.) - no control flow
                    extract_default_blocks(
                        node,
                        function_id,
                        source,
                        blocks,
                        edges,
                        previous_block_idx,
                        loop_scope,
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
                            loop_scope,
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
///
/// - Block for `a` with conditional: true -> `b`, false -> merge
/// - Block for `b` with fallthrough -> merge
///
/// For `a || b`:
///
/// - Block for `a` with conditional: true -> merge, false -> `b`
/// - Block for `b` with fallthrough -> merge
#[allow(
    clippy::too_many_arguments,
    reason = "short-circuit extraction threads mutable CFG assembly state through recursion"
)]
pub(super) fn extract_short_circuit_blocks(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_scope: LoopScope,
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
        node,
        function_id,
        source,
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
            &left_node,
            function_id,
            source,
            blocks,
            edges,
            &mut Some(left_idx),
            loop_scope,
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
        cfg_condition: None,
    };
    let merge_idx = blocks.len();
    blocks.push(merge_block);

    // For &&: true branch goes to right operand, false branch skips to merge
    // For ||: true branch skips to merge, false branch goes to right operand
    if let Some(right_node) = right {
        let right_block = create_block_from_node(
            &right_node,
            function_id,
            source,
            "short_circuit_rhs",
            "fallthrough",
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
            &right_node,
            function_id,
            source,
            blocks,
            edges,
            &mut right_last_idx,
            loop_scope,
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
pub(super) fn extract_default_blocks(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_scope: LoopScope,
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
                    &child,
                    function_id,
                    source,
                    blocks,
                    edges,
                    &mut Some(idx),
                    loop_scope,
                );
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    *previous_block_idx = Some(idx);
}

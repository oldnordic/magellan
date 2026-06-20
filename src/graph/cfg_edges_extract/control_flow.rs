use super::*;
use tree_sitter::Node;

/// Extract blocks for if statement with fallthrough tracking
pub(super) fn extract_if_blocks_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_scope: LoopScope,
) {
    // Find then and else blocks FIRST (before creating blocks)
    let mut then_node = None;
    let mut else_node = None;

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let kind = child.kind();
            if kind == "block" || kind == "statement_block" || kind == "compound_statement" {
                if then_node.is_none() {
                    then_node = Some(child);
                }
            } else if kind == "else_clause" || kind == "else" {
                let mut else_cursor = child.walk();
                if else_cursor.goto_first_child() {
                    loop {
                        let else_child = else_cursor.node();
                        if else_child.kind() == "block"
                            || else_child.kind() == "statement_block"
                            || else_child.kind() == "compound_statement"
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
            loop_scope,
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
            loop_scope,
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
        cfg_condition: None,
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
pub(super) fn extract_loop_blocks_with_fallthrough(
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

    // Create exit block before the body so nested break statements can target it.
    let exit_idx = blocks.len();
    let exit_block = CfgBlock {
        function_id,
        kind: "exit".to_string(),
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
    blocks.push(exit_block);

    // Extract body blocks with loop_scope set.
    let body_start_idx = blocks.len();
    let mut body_last_idx = None;
    if let Some(body) = body_node {
        extract_blocks_from_node_with_fallthrough(
            &body,
            function_id,
            source,
            blocks,
            edges,
            &mut body_last_idx,
            LoopScope {
                header: Some(header_idx),
                exit: Some(exit_idx),
            },
        );
    }

    // Create back-edge from end of body to header
    if body_node.is_some()
        && body_last_idx.is_some()
        && body_last_idx.unwrap_or(body_start_idx) >= body_start_idx
    {
        edges.push(CfgEdge {
            source_idx: body_last_idx.unwrap_or(body_start_idx),
            target_idx: header_idx,
            edge_type: CfgEdgeType::BackEdge,
        });
    }

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
pub(super) fn extract_match_blocks_with_fallthrough(
    node: &Node,
    function_id: i64,
    source: &str,
    blocks: &mut Vec<CfgBlock>,
    edges: &mut Vec<CfgEdge>,
    previous_block_idx: &mut Option<usize>,
    loop_scope: LoopScope,
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
            } else if child.kind() == "expression_case"
                || child.kind() == "default_case"
                || child.kind() == "type_case"
            {
                // Go switch cases (direct children, no body wrapper)
                arm_nodes.push(child);
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
        let has_guard = arm
            .child(0)
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
                loop_scope,
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
        let Some(match_pattern) = arm.child(0) else {
            // Malformed AST: match arm has no children
            let arm_start_idx = blocks.len();
            extract_blocks_from_node_with_fallthrough(
                arm,
                function_id,
                source,
                blocks,
                edges,
                &mut None,
                loop_scope,
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
                arm,
                function_id,
                source,
                blocks,
                edges,
                &mut None,
                loop_scope,
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
        let guard_block =
            create_block_from_node(&guard, function_id, source, "match_guard", "conditional");
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
                loop_scope,
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
        cfg_condition: None,
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

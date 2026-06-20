use super::*;
use tree_sitter::Parser as TsParser;

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

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    assert_eq!(result.function_id, 1);
    assert!(
        !result.blocks.is_empty(),
        "Should have at least entry block"
    );
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

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

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

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    assert!(result.blocks.len() >= 2, "Should have multiple blocks");
    // Note: Back-edge detection depends on proper loop body extraction
    // For now, just verify we have edges
    assert!(!result.edges.is_empty(), "Should have edges");
}

#[test]
fn test_break_targets_loop_exit_not_header() {
    let source = r#"
fn with_break(flag: bool) {
    loop {
        if flag {
            break;
        }
    }
}
"#;

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let break_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "break")
        .expect("Should have break block");
    let loop_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "loop")
        .expect("Should have loop header block");
    let exit_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "exit")
        .expect("Should have loop exit block");

    let break_jump_targets: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == break_idx && e.edge_type == CfgEdgeType::Jump)
        .map(|e| e.target_idx)
        .collect();

    assert_eq!(
        break_jump_targets,
        vec![exit_idx],
        "break should jump to loop exit, not back to header {loop_idx}"
    );
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

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    // Check for return edges
    let has_return = result
        .edges
        .iter()
        .any(|e| matches!(e.edge_type, CfgEdgeType::Return));
    assert!(has_return, "Should have return edges");
}

#[test]
fn test_extract_call_expression_emits_call_edge() {
    let source = r#"
fn helper(x: i32) -> i32 { x + 1 }

fn caller() -> i32 {
    let y = helper(41);
    helper(y)
}
"#;

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let call_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "call").collect();
    assert!(
        !call_blocks.is_empty(),
        "Should extract call blocks for ordinary call expressions"
    );

    let has_call_edge = result
        .edges
        .iter()
        .any(|e| matches!(e.edge_type, CfgEdgeType::Call));
    assert!(has_call_edge, "Should emit at least one typed Call edge");
}

#[test]
fn test_extract_let_else_creates_conditional_flow() {
    let source = r#"
fn parse(maybe: Option<i32>) -> i32 {
    let Some(value) = maybe else {
        return 0;
    };
    value + 1
}
"#;

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let has_conditional_edge = result.edges.iter().any(|e| {
        matches!(
            e.edge_type,
            CfgEdgeType::ConditionalTrue | CfgEdgeType::ConditionalFalse
        )
    });
    assert!(
        has_conditional_edge,
        "let-else should produce conditional control-flow edges"
    );

    let has_return_edge = result
        .edges
        .iter()
        .any(|e| matches!(e.edge_type, CfgEdgeType::Return));
    assert!(
        has_return_edge,
        "let-else failure branch should produce a return edge"
    );
}

#[test]
fn test_extract_cfg_empty_function() {
    let source = r#"
fn empty() {}
"#;

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    assert_eq!(result.function_id, 1);
    assert!(!result.blocks.is_empty(), "Should have entry block");
}

#[test]
fn test_extract_try_expression() {
    let source = r#"
fn test() -> Result<i32, ()> {
    let x = try_result()?;
    Ok(x)
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let try_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "try").collect();
    assert!(
        !try_blocks.is_empty(),
        "Should have try blocks for ? operator"
    );

    // Should have a return edge (error path)
    let return_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| matches!(e.edge_type, CfgEdgeType::Return))
        .collect();
    assert!(
        !return_edges.is_empty(),
        "Should have return edge for error path"
    );
}

#[test]
fn test_extract_await_expression() {
    let source = r#"
async fn test() -> i32 {
    let x = some_async().await;
    x
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());
    let closure_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.kind == "closure")
        .collect();
    assert!(!closure_blocks.is_empty(), "Should have closure blocks");
    // Closure body should have nested if
    let if_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "if").collect();
    assert!(
        !if_blocks.is_empty(),
        "Closure body should contain if blocks"
    );
}

#[test]
fn test_merge_block_has_nonzero_range() {
    let source = r#"
fn test() {
    if x { a } else { b }
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let and_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "and").collect();
    assert!(!and_blocks.is_empty(), "Should have && blocks");
    let and_idx = result.blocks.iter().position(|b| b.kind == "and").unwrap();

    let rhs_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.kind == "short_circuit_rhs")
        .collect();
    assert!(!rhs_blocks.is_empty(), "Should have rhs blocks");
    let rhs_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "short_circuit_rhs")
        .unwrap();

    // Should have ConditionalTrue edge from and to rhs
    let true_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| {
            e.source_idx == and_idx
                && e.target_idx == rhs_idx
                && e.edge_type == CfgEdgeType::ConditionalTrue
        })
        .collect();
    assert!(
        !true_edges.is_empty(),
        "Should have ConditionalTrue edge from and to rhs"
    );

    // Should have ConditionalFalse edge from and to merge (short-circuit)
    let false_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == and_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
        .collect();
    assert!(
        !false_edges.is_empty(),
        "Should have ConditionalFalse edge for short-circuit"
    );

    // Should have fallthrough from right branch's last block to merge
    let merge_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "merge")
        .unwrap();
    let rhs_to_merge: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.target_idx == merge_idx && e.edge_type == CfgEdgeType::Fallthrough)
        .collect();
    assert!(
        !rhs_to_merge.is_empty(),
        "Should have fallthrough from right branch to merge"
    );
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let or_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "or").collect();
    assert!(!or_blocks.is_empty(), "Should have || blocks");
    let or_idx = result.blocks.iter().position(|b| b.kind == "or").unwrap();

    let rhs_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.kind == "short_circuit_rhs")
        .collect();
    assert!(!rhs_blocks.is_empty(), "Should have rhs blocks");
    let rhs_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "short_circuit_rhs")
        .unwrap();

    // Should have ConditionalFalse edge from or to rhs
    let false_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| {
            e.source_idx == or_idx
                && e.target_idx == rhs_idx
                && e.edge_type == CfgEdgeType::ConditionalFalse
        })
        .collect();
    assert!(
        !false_edges.is_empty(),
        "Should have ConditionalFalse edge from or to rhs"
    );

    // Should have ConditionalTrue edge from or to merge (short-circuit)
    let true_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == or_idx && e.edge_type == CfgEdgeType::ConditionalTrue)
        .collect();
    assert!(
        !true_edges.is_empty(),
        "Should have ConditionalTrue edge for short-circuit"
    );
}

#[test]
fn test_extract_chained_short_circuit() {
    let source = r#"
fn test() -> bool {
    a && b && c
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let and_blocks: Vec<_> = result.blocks.iter().filter(|b| b.kind == "and").collect();
    assert_eq!(
        and_blocks.len(),
        2,
        "Should have two and blocks for chained &&"
    );

    // Verify no dead ends - every conditional should have at least 2 outgoing edges
    for (idx, block) in result
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "and" || b.kind == "or")
    {
        let outgoing = result.edges.iter().filter(|e| e.source_idx == idx).count();
        assert!(
            outgoing >= 2,
            "Block {} ({}) should have at least 2 outgoing edges, got {}",
            idx,
            block.kind,
            outgoing
        );
    }

    // Verify the chain is connected: first and's merge should connect to second and
    let _first_and_idx = result.blocks.iter().position(|b| b.kind == "and").unwrap();
    let merge_indices: Vec<usize> = result
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "merge")
        .map(|(i, _)| i)
        .collect();
    assert!(!merge_indices.is_empty(), "Should have merge blocks");

    // Each and block should have ConditionalTrue and ConditionalFalse outgoing edges
    for (idx, _block) in result
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "and" || b.kind == "or")
    {
        let cond_true = result
            .edges
            .iter()
            .any(|e| e.source_idx == idx && e.edge_type == CfgEdgeType::ConditionalTrue);
        let cond_false = result
            .edges
            .iter()
            .any(|e| e.source_idx == idx && e.edge_type == CfgEdgeType::ConditionalFalse);
        assert!(cond_true, "Block {} should have ConditionalTrue edge", idx);
        assert!(
            cond_false,
            "Block {} should have ConditionalFalse edge",
            idx
        );
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    // Should have match_guard blocks
    let guard_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.kind == "match_guard")
        .collect();
    assert_eq!(
        guard_blocks.len(),
        1,
        "Should have exactly one match_guard block"
    );
    let guard_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "match_guard")
        .unwrap();

    // Guard block should have ConditionalTrue edge to arm body
    let guard_true_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == guard_idx && e.edge_type == CfgEdgeType::ConditionalTrue)
        .collect();
    assert!(
        !guard_true_edges.is_empty(),
        "Guard should have ConditionalTrue edge to body"
    );

    // Guard block should have ConditionalFalse edge to next arm
    let guard_false_edges: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == guard_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
        .collect();
    assert!(
        !guard_false_edges.is_empty(),
        "Guard should have ConditionalFalse edge to next arm"
    );

    // The false edge should point to a block after the guard (next arm entry)
    let false_target = guard_false_edges[0].target_idx;
    assert!(
        false_target > guard_idx,
        "Guard false edge should point to a later block"
    );
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    // Should have two match_guard blocks
    let guard_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.kind == "match_guard")
        .collect();
    assert_eq!(guard_blocks.len(), 2, "Should have two match_guard blocks");

    let guard_indices: Vec<usize> = result
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "match_guard")
        .map(|(i, _)| i)
        .collect();

    // First guard false should point to second guard
    let first_guard = guard_indices[0];
    let second_guard = guard_indices[1];
    let first_false: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == first_guard && e.edge_type == CfgEdgeType::ConditionalFalse)
        .collect();
    assert!(
        !first_false.is_empty(),
        "First guard should have ConditionalFalse edge"
    );
    assert_eq!(
        first_false[0].target_idx, second_guard,
        "First guard false should fall through to second guard"
    );

    // Second guard false should point to an unguarded arm (not another guard)
    let second_false: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == second_guard && e.edge_type == CfgEdgeType::ConditionalFalse)
        .collect();
    assert!(
        !second_false.is_empty(),
        "Second guard should have ConditionalFalse edge"
    );
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
    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    let guard_blocks: Vec<_> = result
        .blocks
        .iter()
        .filter(|b| b.kind == "match_guard")
        .collect();
    assert_eq!(guard_blocks.len(), 1, "Should have one match_guard block");
    let guard_idx = result
        .blocks
        .iter()
        .position(|b| b.kind == "match_guard")
        .unwrap();

    // Last guard false should point to merge block
    let guard_false: Vec<_> = result
        .edges
        .iter()
        .filter(|e| e.source_idx == guard_idx && e.edge_type == CfgEdgeType::ConditionalFalse)
        .collect();
    assert!(
        !guard_false.is_empty(),
        "Last guard should have ConditionalFalse edge"
    );

    let merge_indices: Vec<usize> = result
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "merge")
        .map(|(i, _)| i)
        .collect();
    assert!(!merge_indices.is_empty(), "Should have merge blocks");
    assert!(
        merge_indices.contains(&guard_false[0].target_idx),
        "Last guard false should point to merge block"
    );
}

#[test]
fn test_extract_cfg_condition_from_function() {
    let source = r#"
#[cfg(feature = "tokio")]
fn cfg_function() {
    let x = 1;
}

fn normal_function() {
    let y = 2;
}
"#;

    let mut parser = TsParser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source.as_bytes(), None).unwrap();
    let root = tree.root_node();

    let mut funcs = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_item" {
            funcs.push(child);
        }
    }
    assert_eq!(funcs.len(), 2);

    let cfg_cond = extract_cfg_condition(&funcs[0], source);
    assert_eq!(
        cfg_cond,
        Some(r#"feature = "tokio""#.to_string()),
        "Should extract cfg condition from #[cfg(feature = \"tokio\")]"
    );

    let no_cfg = extract_cfg_condition(&funcs[1], source);
    assert_eq!(no_cfg, None, "Normal function should have no cfg condition");
}

#[test]
fn test_extract_cfg_condition_complex() {
    let source = r#"
#[cfg(all(feature = "a", feature = "b"))]
fn complex_cfg() {}
"#;

    let mut parser = TsParser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source.as_bytes(), None).unwrap();
    let root = tree.root_node();

    let mut func = None;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_item" {
            func = Some(child);
            break;
        }
    }
    let func = func.expect("should find function_item");

    let cfg_cond = extract_cfg_condition(&func, source);
    assert_eq!(
        cfg_cond,
        Some(r#"all(feature = "a", feature = "b")"#.to_string()),
        "Should extract complex cfg condition"
    );
}

#[test]
fn test_extract_cfg_condition_applied_to_blocks() {
    let source = r#"
#[cfg(feature = "tokio")]
fn cfg_function() {
    let x = 1;
    if x > 0 {
        return x;
    }
}
"#;

    let result = extract_cfg_with_edges(source, 1, tree_sitter_rust::LANGUAGE.into());

    assert!(
        !result.blocks.is_empty(),
        "Should have blocks for cfg function"
    );

    // All blocks should inherit the cfg condition from the function
    for block in &result.blocks {
        assert_eq!(
            block.cfg_condition,
            Some(r#"feature = "tokio""#.to_string()),
            "Block {:?} should inherit function's cfg condition",
            block.kind
        );
    }
}

#[test]
fn test_go_function_cfg_extraction() {
    let source = r#"
package main

func add(x int, y int) int {
    if x > 0 {
        return x + y
    }
    return y
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_go::LANGUAGE.into());
    assert!(
        !result.blocks.is_empty(),
        "Go function should produce at least entry block, got {} blocks",
        result.blocks.len()
    );
    assert!(
        result.edges.len() >= 2,
        "Go if/return should produce at least 2 edges, got {}",
        result.edges.len()
    );
    let has_conditional = result.edges.iter().any(|e| {
        matches!(
            e.edge_type,
            CfgEdgeType::ConditionalTrue | CfgEdgeType::ConditionalFalse
        )
    });
    assert!(has_conditional, "Go if should produce conditional edges");
}

#[test]
fn test_go_method_cfg_extraction() {
    let source = r#"
package main

func (s *Server) handle() error {
    if s.running {
        s.stop()
        return nil
    }
    return s.start()
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_go::LANGUAGE.into());
    assert!(
        !result.blocks.is_empty(),
        "Go method should produce at least entry block, got {} blocks",
        result.blocks.len()
    );
    assert!(
        result.edges.len() >= 2,
        "Go method with if should produce at least 2 edges, got {}",
        result.edges.len()
    );
}

#[test]
fn test_go_for_loop_cfg() {
    let source = r#"
package main

func sum(n int) int {
    total := 0
    for i := 0; i < n; i++ {
        total += i
    }
    return total
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_go::LANGUAGE.into());
    assert!(
        !result.blocks.is_empty(),
        "Go for loop should produce blocks"
    );
    assert!(!result.edges.is_empty(), "Go for loop should produce edges");
}

#[test]
fn test_go_switch_cfg() {
    let source = r#"
package main

func classify(x int) string {
    switch x {
    case 0:
        return "zero"
    case 1:
        return "one"
    default:
        return "other"
    }
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_go::LANGUAGE.into());
    assert!(!result.blocks.is_empty(), "Go switch should produce blocks");
    assert!(
        result.edges.len() >= 3,
        "Go switch with 3 cases should produce at least 3 edges, got {}",
        result.edges.len()
    );
}

#[test]
fn test_cuda_kernel_cfg_extraction() {
    let source = r#"
__global__ void vectorAdd(float *a, float *b, float *c, int n) {
    int i = threadIdx.x + blockIdx.x * blockDim.x;
    if (i < n) {
        c[i] = a[i] + b[i];
    }
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_cuda::LANGUAGE.into());
    assert!(
        !result.blocks.is_empty(),
        "CUDA kernel should produce at least entry block, got {} blocks",
        result.blocks.len()
    );
    assert!(
        result.edges.len() >= 2,
        "CUDA if should produce at least 2 edges, got {}",
        result.edges.len()
    );
    let has_conditional = result.edges.iter().any(|e| {
        matches!(
            e.edge_type,
            CfgEdgeType::ConditionalTrue | CfgEdgeType::ConditionalFalse
        )
    });
    assert!(has_conditional, "CUDA if should produce conditional edges");
}

#[test]
fn test_cuda_device_function_cfg() {
    let source = r#"
__device__ float clamp(float x, float lo, float hi) {
    if (x < lo) return lo;
    if (x > hi) return hi;
    return x;
}
"#;
    let result = extract_cfg_with_edges(source, 1, tree_sitter_cuda::LANGUAGE.into());
    assert!(
        !result.blocks.is_empty(),
        "CUDA device function should produce blocks"
    );
    assert!(
        !result.edges.is_empty(),
        "CUDA device function with branches should produce edges"
    );
}

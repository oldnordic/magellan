//! Comprehensive tests for AST nodes functionality

use tempfile::tempdir;

use crate::graph::CodeGraph;

/// Test end-to-end: indexing creates AST nodes
#[test]
fn test_indexing_creates_ast_nodes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
            fn main() {
                if true {
                    println!("hello");
                } else {
                    println!("goodbye");
                }
                for i in 0..10 {
                    println!("{}", i);
                }
                while x > 0 {
                    x -= 1;
                }
                loop {
                    break;
                }
                return 42;
            }
        "#;

    graph.index_file("test.rs", source.as_bytes()).unwrap();

    // Verify AST nodes were created
    let count = graph.count_ast_nodes().unwrap();
    assert!(count > 0, "AST nodes should be created during indexing");

    // Verify specific node types exist
    let fn_count = graph.get_ast_nodes_by_kind("function_item").unwrap();
    assert!(!fn_count.is_empty(), "Should have function_item node");

    let if_count = graph.get_ast_nodes_by_kind("if_expression").unwrap();
    assert!(!if_count.is_empty(), "Should have if_expression node");

    let for_count = graph.get_ast_nodes_by_kind("for_expression").unwrap();
    assert!(!for_count.is_empty(), "Should have for_expression node");

    let while_count = graph.get_ast_nodes_by_kind("while_expression").unwrap();
    assert!(!while_count.is_empty(), "Should have while_expression node");

    let loop_count = graph.get_ast_nodes_by_kind("loop_expression").unwrap();
    assert!(!loop_count.is_empty(), "Should have loop_expression node");

    let return_count = graph.get_ast_nodes_by_kind("return_expression").unwrap();
    assert!(!return_count.is_empty(), "Should have return_expression node");
}

/// Test parent-child relationship queries
#[test]
fn test_parent_child_relationships() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let graph = CodeGraph::open(&db_path).unwrap();

    // Insert test hierarchy
    let conn = graph.chunks.connect().unwrap();
    conn.execute(
        "INSERT INTO ast_nodes (id, parent_id, kind, byte_start, byte_end)
         VALUES
            (1, NULL, 'function_item', 0, 200),
            (2, 1, 'block', 10, 190),
            (3, 2, 'if_expression', 20, 100),
            (4, 2, 'return_expression', 110, 130),
            (5, 3, 'block', 30, 50)",
        [],
    ).unwrap();

    // Get children of function (should be block)
    let children = graph.get_ast_children(1).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].kind, "block");

    // Get children of if_expression (should be then block)
    let children = graph.get_ast_children(3).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].kind, "block");

    // Get children of leaf node (should be empty)
    let children = graph.get_ast_children(5).unwrap();
    assert_eq!(children.len(), 0);
}

/// Test position-based queries
#[test]
fn test_position_based_queries() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let graph = CodeGraph::open(&db_path).unwrap();

    // Insert nodes with overlapping spans
    let conn = graph.chunks.connect().unwrap();
    conn.execute(
        "INSERT INTO ast_nodes (id, parent_id, kind, byte_start, byte_end)
         VALUES
            (1, NULL, 'block', 0, 200),
            (2, 1, 'if_expression', 50, 150),
            (3, 2, 'block', 60, 100)",
        [],
    ).unwrap();

    // Position 25 should match outer block
    let node = graph.get_ast_node_at_position("test.rs", 25).unwrap();
    assert!(node.is_some());
    assert_eq!(node.unwrap().kind, "block");

    // Position 75 should match innermost block (smallest containing node)
    let node = graph.get_ast_node_at_position("test.rs", 75).unwrap();
    assert!(node.is_some());
    assert_eq!(node.unwrap().kind, "block");

    // Position 120 should match if_expression (between blocks)
    let node = graph.get_ast_node_at_position("test.rs", 120).unwrap();
    assert!(node.is_some());
    assert_eq!(node.unwrap().kind, "if_expression");
}

/// Test re-indexing updates AST nodes
///
/// NOTE: This test verifies that re-indexing adds new AST nodes.
/// Due to current schema limitation (no file_id column), old nodes
/// from previous indexing are NOT deleted. This is a known limitation
/// documented in ops.rs around line 433-437.
#[test]
fn test_reindexing_updates_ast_nodes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Index initial version
    let source1 = b"fn main() { if true { } }";
    graph.index_file("test.rs", source1).unwrap();
    let count1 = graph.count_ast_nodes().unwrap();
    assert!(count1 > 0);

    let _initial_if_count = graph.get_ast_nodes_by_kind("if_expression").unwrap().len();

    // Re-index with different content
    let source2 = b"fn main() { loop { break; } }";
    graph.index_file("test.rs", source2).unwrap();
    let count2 = graph.count_ast_nodes().unwrap();
    assert!(count2 > 0);

    // Verify new nodes are added (count should increase)
    assert!(count2 >= count1, "Re-indexing should add or maintain nodes");

    // Verify we have new loop_expression nodes
    let loop_nodes = graph.get_ast_nodes_by_kind("loop_expression").unwrap();
    assert!(!loop_nodes.is_empty(), "New loop_expression should exist");

    // NOTE: Due to missing file_id column, old if_expression nodes
    // from first indexing are NOT deleted. This is expected behavior
    // with current schema. Once file_id is added to ast_nodes table,
    // this test can be updated to verify proper cleanup.
}

/// Test empty source file
#[test]
fn test_empty_source_file() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    graph.index_file("empty.rs", b"").unwrap();

    // Empty file should have no AST nodes
    let count = graph.count_ast_nodes().unwrap();
    assert_eq!(count, 0);
}

/// Test file with only comments
#[test]
fn test_file_with_only_comments() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"// This is a comment\n// Another comment\n";
    graph.index_file("comments.rs", source).unwrap();

    // Comment-only file should have no structural AST nodes
    let count = graph.count_ast_nodes().unwrap();
    assert_eq!(count, 0);
}

/// Test deeply nested control flow
#[test]
fn test_deeply_nested_control_flow() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
            fn test() {
                if a {
                    if b {
                        if c {
                            if d {
                                println!("deep");
                            }
                        }
                    }
                }
            }
        "#;

    graph.index_file("nested.rs", source.as_bytes()).unwrap();

    // Should find multiple nested if expressions
    let if_nodes = graph.get_ast_nodes_by_kind("if_expression").unwrap();
    assert_eq!(if_nodes.len(), 4, "Should have 4 nested if expressions");
}

/// Test match expression (has multiple arms)
#[test]
fn test_match_expression() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
            fn test(x: i32) {
                match x {
                    1 => println!("one"),
                    2 => println!("two"),
                    _ => println!("other"),
                }
            }
        "#;

    graph.index_file("match.rs", source.as_bytes()).unwrap();

    // Should find match expression
    let match_nodes = graph.get_ast_nodes_by_kind("match_expression").unwrap();
    assert!(!match_nodes.is_empty(), "Should have match_expression node");
}

/// Test get_ast_roots returns top-level nodes
#[test]
fn test_get_ast_roots() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let graph = CodeGraph::open(&db_path).unwrap();

    // Insert nodes with mixed parentage
    let conn = graph.chunks.connect().unwrap();
    conn.execute(
        "INSERT INTO ast_nodes (id, parent_id, kind, byte_start, byte_end)
         VALUES
            (1, NULL, 'function_item', 0, 100),
            (2, 1, 'block', 10, 90),
            (3, NULL, 'struct_item', 100, 200),
            (4, 3, 'block', 110, 190)",
        [],
    ).unwrap();

    let roots = graph.get_ast_roots().unwrap();
    assert_eq!(roots.len(), 2);

    let kinds: Vec<_> = roots.iter().map(|n| n.kind.as_str()).collect();
    assert!(kinds.contains(&"function_item"));
    assert!(kinds.contains(&"struct_item"));
}

/// Performance test for large file
#[test]
#[ignore]  // Run explicitly with cargo test -- --ignored
fn test_performance_large_file() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Generate a large source file (100 functions)
    let mut source = String::new();
    for i in 0..100 {
        source.push_str(&format!("fn func_{}() {{ if x {{ return {}; }} }}\n", i, i));
    }

    let start = std::time::Instant::now();
    graph.index_file("large.rs", source.as_bytes()).unwrap();
    let index_time = start.elapsed();

    // Should complete within reasonable time
    assert!(index_time.as_secs() < 5, "Indexing too slow: {:?}", index_time);

    let count = graph.count_ast_nodes().unwrap();
    assert!(count > 500, "Should have extracted many nodes (got {})", count);
}

/// Test get_ast_nodes_by_file returns nodes in order
#[test]
fn test_get_ast_nodes_by_file() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
        fn first() { }
        fn second() { }
        fn third() { }
    "#;

    graph.index_file("ordered.rs", source.as_bytes()).unwrap();

    let nodes = graph.get_ast_nodes_by_file("ordered.rs").unwrap();
    assert!(!nodes.is_empty(), "Should have AST nodes");

    // Verify nodes are ordered by byte_start
    for i in 1..nodes.len() {
        assert!(
            nodes[i].node.byte_start >= nodes[i - 1].node.byte_start,
            "Nodes should be ordered by byte_start"
        );
    }
}

/// Test multiple files maintain separate AST nodes
#[test]
fn test_multiple_files_separate_asts() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Index first file
    let source1 = b"fn foo() { if true { } }";
    graph.index_file("file1.rs", source1).unwrap();

    // Index second file
    let source2 = b"fn bar() { loop { break; } }";
    graph.index_file("file2.rs", source2).unwrap();

    // Both files should contribute to total count
    let total_count = graph.count_ast_nodes().unwrap();
    assert!(total_count > 0, "Should have AST nodes from both files");

    // Should have both if_expression and loop_expression
    let if_nodes = graph.get_ast_nodes_by_kind("if_expression").unwrap();
    let loop_nodes = graph.get_ast_nodes_by_kind("loop_expression").unwrap();

    assert!(!if_nodes.is_empty(), "Should have if_expression from file1");
    assert!(!loop_nodes.is_empty(), "Should have loop_expression from file2");
}

/// Test let_declaration is captured
#[test]
fn test_let_declaration_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn main() { let x = 42; }";
    graph.index_file("let_test.rs", source).unwrap();

    let let_nodes = graph.get_ast_nodes_by_kind("let_declaration").unwrap();
    assert!(!let_nodes.is_empty(), "Should capture let_declaration");
}

/// Test block expressions are captured
#[test]
fn test_block_expressions_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn main() { let x = { 1 + 2 }; }";
    graph.index_file("block_test.rs", source).unwrap();

    let block_nodes = graph.get_ast_nodes_by_kind("block").unwrap();
    assert!(!block_nodes.is_empty(), "Should capture block expressions");
}

/// Test call_expression is captured
#[test]
fn test_call_expression_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn main() { foo(); bar(); }";
    graph.index_file("call_test.rs", source).unwrap();

    let call_nodes = graph.get_ast_nodes_by_kind("call_expression").unwrap();
    assert!(!call_nodes.is_empty(), "Should capture call_expression");
}

/// Test assignment_expression is captured
#[test]
fn test_assignment_expression_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn main() { let mut x = 1; x = 2; }";
    graph.index_file("assign_test.rs", source).unwrap();

    let assign_nodes = graph.get_ast_nodes_by_kind("assignment_expression").unwrap();
    assert!(!assign_nodes.is_empty(), "Should capture assignment_expression");
}

/// Test struct and enum definitions are captured
#[test]
fn test_struct_enum_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
        struct Foo;
        enum Bar { A, B }
    "#;

    graph.index_file("definitions.rs", source.as_bytes()).unwrap();

    let struct_nodes = graph.get_ast_nodes_by_kind("struct_item").unwrap();
    assert!(!struct_nodes.is_empty(), "Should capture struct_item");

    let enum_nodes = graph.get_ast_nodes_by_kind("enum_item").unwrap();
    assert!(!enum_nodes.is_empty(), "Should capture enum_item");
}

/// Test impl and trait definitions are captured
#[test]
fn test_impl_trait_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
        trait MyTrait { }
        impl MyTrait for Foo { }
    "#;

    graph.index_file("trait_test.rs", source.as_bytes()).unwrap();

    let trait_nodes = graph.get_ast_nodes_by_kind("trait_item").unwrap();
    assert!(!trait_nodes.is_empty(), "Should capture trait_item");

    let impl_nodes = graph.get_ast_nodes_by_kind("impl_item").unwrap();
    assert!(!impl_nodes.is_empty(), "Should capture impl_item");
}

/// Test mod_item is captured
#[test]
fn test_mod_item_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"mod my_mod;";
    graph.index_file("mod_test.rs", source).unwrap();

    let mod_nodes = graph.get_ast_nodes_by_kind("mod_item").unwrap();
    assert!(!mod_nodes.is_empty(), "Should capture mod_item");
}

/// Test const and static items are captured
#[test]
fn test_const_static_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
        const MAX: i32 = 100;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
    "#;

    graph.index_file("const_test.rs", source.as_bytes()).unwrap();

    let const_nodes = graph.get_ast_nodes_by_kind("const_item").unwrap();
    assert!(!const_nodes.is_empty(), "Should capture const_item");

    let static_nodes = graph.get_ast_nodes_by_kind("static_item").unwrap();
    assert!(!static_nodes.is_empty(), "Should capture static_item");
}

/// Test break and continue expressions are captured
#[test]
fn test_break_continue_captured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn main() { loop { if x { break; } else { continue; } } }";
    graph.index_file("break_continue_test.rs", source).unwrap();

    let break_nodes = graph.get_ast_nodes_by_kind("break_expression").unwrap();
    assert!(!break_nodes.is_empty(), "Should capture break_expression");

    let continue_nodes = graph.get_ast_nodes_by_kind("continue_expression").unwrap();
    assert!(!continue_nodes.is_empty(), "Should capture continue_expression");
}

//! CFG extraction integration tests
//!
//! Tests for AST-based CFG extraction and storage.

use magellan::CodeGraph;
use tempfile::TempDir;

#[test]
fn test_cfg_extracted_from_rust_function() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn simple_function() {
    let x = 42;
    if x > 0 {
        println!("positive");
    } else {
        println!("non-positive");
    }
}

fn loop_function() {
    for i in 0..10 {
        if i == 5 {
            break;
        }
    }
}

fn match_function(x: i32) {
    match x {
        1 => println!("one"),
        2 => println!("two"),
        _ => println!("other"),
    }
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path = "/test.rs";
    let _ = graph.index_file(path, source.as_bytes());

    // Get symbols to find function IDs
    let _symbols = graph.symbols_in_file(path).unwrap();

    // Verify CFG was extracted - check via cfg_ops
    let all_cfg = graph.cfg_ops.get_cfg_for_file(path).unwrap();

    // Should have CFG blocks for at least one function
    assert!(!all_cfg.is_empty(), "CFG should be extracted for Rust files");

    // Count total CFG blocks across all functions
    let total_blocks: usize = all_cfg.iter().map(|(_, blocks): &(_, Vec<_>)| blocks.len()).sum();
    assert!(total_blocks > 0, "Should have at least one CFG block");

    // Verify some expected block kinds exist
    let all_blocks: Vec<_> = all_cfg
        .iter()
        .flat_map(|(_, blocks): &(_, Vec<_>)| blocks.iter())
        .collect();

    // Check for expected block kinds
    let block_kinds: Vec<_> = all_blocks.iter().map(|b| b.kind.as_str()).collect();
    assert!(block_kinds.contains(&"entry"), "Should have entry block");
}

#[test]
fn test_cfg_deleted_on_file_reindex() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source1 = r#"
fn test_function() {
    if true {
        return;
    }
}
"#;

    let source2 = r#"
fn test_function() {
    loop {
        break;
    }
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path = "/test.rs";

    // Index first version
    let _ = graph.index_file(path, source1.as_bytes());

    // Get CFG count after first index
    let cfg1 = graph.cfg_ops.get_cfg_for_file(path).unwrap();
    let initial_count: usize = cfg1.iter().map(|(_, blocks): &(_, Vec<_>)| blocks.len()).sum();

    // Re-index with different source
    let _ = graph.index_file(path, source2.as_bytes());

    // Get CFG count after re-index
    let cfg2 = graph.cfg_ops.get_cfg_for_file(path).unwrap();
    let after_count: usize = cfg2.iter().map(|(_, blocks): &(_, Vec<_>)| blocks.len()).sum();

    // CFG should be cleaned up and re-extracted
    assert!(initial_count > 0, "Should have CFG blocks after first index");
    assert!(after_count > 0, "Should have CFG blocks after re-index");

    // The block kinds should differ between versions
    let blocks1: Vec<_> = cfg1.iter().flat_map(|(_, b): &(_, Vec<_>)| b).map(|b| b.kind.as_str()).collect();
    let blocks2: Vec<_> = cfg2.iter().flat_map(|(_, b): &(_, Vec<_>)| b).map(|b| b.kind.as_str()).collect();

    // source1 has "if", source2 has "loop"
    assert!(blocks1.contains(&"if") || blocks1.contains(&"else"), "First version should have if/else blocks");
    assert!(blocks2.contains(&"loop"), "Second version should have loop block");
}

#[test]
fn test_cfg_deleted_on_file_delete() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn to_be_deleted() {
    return 42;
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path = "/test.rs";
    let _ = graph.index_file(path, source.as_bytes());

    // Verify CFG exists
    let cfg1 = graph.cfg_ops.get_cfg_for_file(path).unwrap();
    assert!(!cfg1.is_empty(), "CFG should exist after indexing");

    // Delete file
    let _ = graph.delete_file(path);

    // Verify CFG is deleted
    let cfg2 = graph.cfg_ops.get_cfg_for_file(path).unwrap();
    assert!(cfg2.is_empty(), "CFG should be deleted after file deletion");
}

#[test]
fn test_cfg_multiple_functions_same_file() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn func_one() {
    if true { return; }
}

fn func_two() {
    loop { break; }
}

fn func_three() {
    match 1 {
        1 => {},
        _ => {},
    }
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path = "/test.rs";
    let _ = graph.index_file(path, source.as_bytes());

    // Get all CFG for the file
    let all_cfg = graph.cfg_ops.get_cfg_for_file(path).unwrap();

    // Should have CFG for 3 functions
    assert_eq!(all_cfg.len(), 3, "Should have CFG for 3 functions");

    // Each function should have at least an entry block
    for (func_id, blocks) in &all_cfg {
        assert!(!blocks.is_empty(), "Function {} should have CFG blocks", func_id);
        assert!(blocks.iter().any(|b| b.kind == "entry"), "Function {} should have entry block", func_id);
    }
}

#[test]
fn test_cfg_for_simple_function() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn simple() {
    let x = 1;
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path = "/test.rs";
    let _ = graph.index_file(path, source.as_bytes());

    // Get all CFG for the file
    let all_cfg = graph.cfg_ops.get_cfg_for_file(path).unwrap();

    // Should have CFG for 1 function
    assert_eq!(all_cfg.len(), 1, "Should have CFG for 1 function");

    // Should have an entry block with fallthrough terminator
    let (_func_id, blocks) = &all_cfg[0];
    assert!(!blocks.is_empty(), "Function should have CFG blocks");
    assert!(blocks.iter().any(|b| b.kind == "entry"), "Should have entry block");
}

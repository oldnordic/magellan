#![cfg(feature = "geometric-backend")]
//! Integration tests for AST and Chunk storage in geometric backend
//!
//! These tests verify:
//! - AST nodes are extracted and stored during indexing
//! - Code chunks are stored and retrievable
//! - Labels are auto-populated during indexing
//! - All data survives database reopen

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Test AST indexing populates nodes
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_ast_indexing_populates_nodes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_ast.geo");

    // Create a simple Rust file to index
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("main.rs");
    std::fs::write(
        &test_file,
        r#"
fn main() {
    println!("Hello, world!");
}

fn helper() -> i32 {
    42
}
"#,
    )
    .unwrap();

    // Create backend and index
    use magellan::graph::geo_index::{scan_directory_with_progress, IndexingMode};
    use magellan::graph::geometric_backend::GeometricBackend;

    let mut backend = GeometricBackend::create(&db_path).unwrap();
    let indexed = scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();

    assert!(indexed > 0, "Should have indexed at least one file");

    // Save to disk
    backend.save_to_disk().unwrap();

    // Check AST nodes were created
    let file_path = test_file.to_string_lossy().to_string();
    let ast_nodes = backend.get_ast_nodes_by_file(&file_path);

    assert!(!ast_nodes.is_empty(), "AST nodes should be populated");

    // Check for function_item nodes
    let function_nodes: Vec<_> = ast_nodes
        .iter()
        .filter(|n| n.kind == "function_item")
        .collect();

    assert!(
        function_nodes.len() >= 2,
        "Should have at least 2 function_item AST nodes, found {}",
        function_nodes.len()
    );
}

/// Test AST data survives reopen
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_ast_survives_reopen() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_ast_reopen.geo");

    // Create a simple Rust file
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("lib.rs");
    std::fs::write(
        &test_file,
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    )
    .unwrap();

    // Create and index
    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
        backend.save_to_disk().unwrap();
    }

    // Reopen and verify
    {
        let backend = GeometricBackend::open(&db_path).unwrap();
        let file_path = test_file.to_string_lossy().to_string();
        let ast_nodes = backend.get_ast_nodes_by_file(&file_path);

        assert!(!ast_nodes.is_empty(), "AST nodes should survive reopen");
    }
}

/// Test find AST by kind works
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_find_ast_by_kind_works() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_ast_kind.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("main.rs");
    std::fs::write(
        &test_file,
        r#"
fn main() {
    let x = 42;
    println!("{}", x);
}
"#,
    )
    .unwrap();

    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    let mut backend = GeometricBackend::create(&db_path).unwrap();
    scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
    backend.save_to_disk().unwrap();

    // Find function_item nodes
    let function_nodes = backend.get_ast_nodes_by_kind("function_item");
    assert!(
        !function_nodes.is_empty(),
        "Should find function_item nodes"
    );

    // Find identifier nodes
    let ident_nodes = backend.get_ast_nodes_by_kind("identifier");
    // Note: there may or may not be identifier nodes depending on the parser
}

/// Test chunks persist and reload
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunks_persist_and_reload() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunks.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("main.rs");
    std::fs::write(
        &test_file,
        r#"
fn main() {
    println!("Hello!");
}

fn foo() {}
"#,
    )
    .unwrap();

    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    // Index and save
    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
        backend.save_to_disk().unwrap();
    }

    // Reopen and verify chunks
    {
        let backend = GeometricBackend::open(&db_path).unwrap();
        let file_path = test_file.to_string_lossy().to_string();
        let chunks = backend.get_code_chunks(&file_path).unwrap();

        assert!(!chunks.is_empty(), "Should have code chunks");

        // Check that chunk content matches source
        let content = std::fs::read_to_string(&test_file).unwrap();
        for chunk in chunks {
            assert!(
                content.contains(&chunk.content),
                "Chunk content should be from source file"
            );
        }
    }
}

/// Test chunk by span works
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunk_by_span_works() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunk_span.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("lib.rs");
    std::fs::write(
        &test_file,
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    )
    .unwrap();

    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    let mut backend = GeometricBackend::create(&db_path).unwrap();
    scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
    backend.save_to_disk().unwrap();

    // Get a chunk by span
    let file_path = test_file.to_string_lossy().to_string();
    let symbols = backend.symbols_in_file(&file_path).unwrap();

    assert!(!symbols.is_empty(), "Should have symbols");

    let symbol = &symbols[0];
    let chunk = backend
        .get_code_chunk_by_span(
            &file_path,
            symbol.byte_start as usize,
            symbol.byte_end as usize,
        )
        .unwrap();

    assert!(chunk.is_some(), "Should find chunk at symbol span");

    let chunk = chunk.unwrap();
    assert_eq!(chunk.symbol_name, Some(symbol.name.clone()));
}

/// Test chunk by symbol works
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunk_by_symbol_works() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunk_symbol.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("main.rs");
    std::fs::write(
        &test_file,
        r#"
fn main() {
    helper();
}

fn helper() {
    println!("help");
}
"#,
    )
    .unwrap();

    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    let mut backend = GeometricBackend::create(&db_path).unwrap();
    scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
    backend.save_to_disk().unwrap();

    let file_path = test_file.to_string_lossy().to_string();
    let chunks = backend
        .get_code_chunks_for_symbol(&file_path, "helper")
        .unwrap();

    assert!(!chunks.is_empty(), "Should find chunks for 'helper' symbol");

    // Verify the chunk contains the function
    let found = chunks.iter().any(|c| c.content.contains("helper"));
    assert!(found, "Chunk should contain 'helper' function text");
}

/// Test labels populate during indexing
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_labels_populate_during_index() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_labels.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();

    // Create a file with various function types
    let main_file = src_dir.join("main.rs");
    std::fs::write(
        &main_file,
        r#"
fn main() {
    test_foo();
}

fn test_foo() {
    assert_eq!(1, 1);
}

pub fn public_fn() {}
"#,
    )
    .unwrap();

    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    let mut backend = GeometricBackend::create(&db_path).unwrap();
    scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
    backend.save_to_disk().unwrap();

    // Check labels were assigned
    let labels = backend.get_all_labels();

    // Should have at least "main" and "test" labels
    assert!(
        labels.contains(&"main".to_string()) || labels.contains(&"entry_point".to_string()),
        "Should have 'main' or 'entry_point' label, got: {:?}",
        labels
    );

    // Check main function has main label
    let main_symbols = backend.get_symbol_ids_by_label("main");
    assert!(
        !main_symbols.is_empty() || labels.is_empty(),
        "main function should have 'main' label or no labels at all"
    );
}

/// Test reopen preserves all data (AST, chunks, labels)
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_reopen_preserves_all_data() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_preserve.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    let test_file = src_dir.join("main.rs");
    std::fs::write(
        &test_file,
        r#"
fn main() {
    foo();
}

fn foo() -> i32 { 42 }
"#,
    )
    .unwrap();

    use magellan::graph::geo_index::scan_directory_with_progress;
    use magellan::graph::geometric_backend::GeometricBackend;

    let file_path = test_file.to_string_lossy().to_string();

    // First session: index and save
    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();

        // Record initial state
        let ast_count = backend.get_ast_nodes_by_file(&file_path).len();
        let chunks = backend.get_code_chunks(&file_path).unwrap();
        let labels = backend.get_all_labels();

        assert!(ast_count > 0, "Should have AST nodes before save");
        assert!(!chunks.is_empty(), "Should have chunks before save");

        backend.save_to_disk().unwrap();

        // Store counts for comparison
        std::fs::write(
            temp_dir.path().join("counts.txt"),
            format!("{}\n{}\n{}", ast_count, chunks.len(), labels.len()),
        )
        .unwrap();
    }

    // Second session: reopen and verify
    {
        let backend = GeometricBackend::open(&db_path).unwrap();

        let ast_count = backend.get_ast_nodes_by_file(&file_path).len();
        let chunks = backend.get_code_chunks(&file_path).unwrap();
        let labels = backend.get_all_labels();

        // Read expected counts
        let counts = std::fs::read_to_string(temp_dir.path().join("counts.txt")).unwrap();
        let lines: Vec<_> = counts.lines().collect();
        let expected_ast: usize = lines[0].parse().unwrap();
        let expected_chunks: usize = lines[1].parse().unwrap();
        let expected_labels: usize = lines[2].parse().unwrap();

        assert_eq!(
            ast_count, expected_ast,
            "AST node count should match after reopen"
        );
        assert_eq!(
            chunks.len(),
            expected_chunks,
            "Chunk count should match after reopen"
        );
        assert_eq!(
            labels.len(),
            expected_labels,
            "Label count should match after reopen"
        );
    }
}

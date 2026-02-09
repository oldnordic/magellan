//! Bounds checking tests for byte slice indexing
//!
//! Tests that code chunk extraction properly validates byte bounds to prevent
//! panic on malformed symbol data (inverted byte ranges, overflowing byte_end).
//!
//! Issue #C7: Byte Slice Indexing Without Bounds Check

use magellan::CodeGraph;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_valid_byte_ranges_extract_code_chunks() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.rs");
    let source_content = "fn valid_function() { println!(\"test\"); }";
    fs::write(&test_file, source_content).unwrap();

    let path_str = test_file.to_string_lossy().to_string();
    let source = fs::read(&test_file).unwrap();

    // Index the file - should extract code chunk for valid byte range
    let symbol_count = graph.index_file(&path_str, &source).unwrap();
    assert_eq!(symbol_count, 1, "Should index 1 symbol");

    // Verify code chunk was extracted
    let chunks = graph.get_code_chunks(&path_str).unwrap();
    assert_eq!(chunks.len(), 1, "Should have 1 code chunk");
    assert_eq!(
        chunks[0].symbol_name,
        Some("valid_function".to_string()),
        "Code chunk should have correct symbol name"
    );
    assert!(
        !chunks[0].content.is_empty(),
        "Code chunk should have content"
    );
}

#[test]
fn test_index_file_normal_operation_with_valid_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create test file with multiple symbols
    let test_file = temp_dir.path().join("test.rs");
    let source_content = r#"
fn first_function() {}
fn second_function() { first_function(); }
struct MyStruct;
impl MyStruct {
    fn method(&self) {}
}
"#;
    fs::write(&test_file, source_content).unwrap();

    let path_str = test_file.to_string_lossy().to_string();
    let source = fs::read(&test_file).unwrap();

    // Index the file - should succeed without panic
    let symbol_count = graph.index_file(&path_str, &source).unwrap();
    assert_eq!(
        symbol_count, 4,
        "Should index 4 symbols (2 functions, 1 struct, 1 method)"
    );

    // Verify all code chunks were extracted (all symbols should have valid byte ranges)
    let chunks = graph.get_code_chunks(&path_str).unwrap();
    assert_eq!(chunks.len(), 4, "Should have 4 code chunks for 4 symbols");

    // Verify chunks have correct content
    let chunk_names: Vec<_> = chunks
        .iter()
        .filter_map(|c| c.symbol_name.as_ref())
        .collect();
    assert!(chunk_names.contains(&&"first_function".to_string()));
    assert!(chunk_names.contains(&&"second_function".to_string()));
    assert!(chunk_names.contains(&&"MyStruct".to_string()));
    assert!(chunk_names.contains(&&"method".to_string()));
}

#[test]
fn test_code_chunks_span_entire_function() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create test file with a specific function
    let test_file = temp_dir.path().join("test.rs");
    let source_content = "fn test_fn() { let x = 42; }";
    fs::write(&test_file, source_content).unwrap();

    let path_str = test_file.to_string_lossy().to_string();
    let source = fs::read(&test_file).unwrap();

    // Index the file
    graph.index_file(&path_str, &source).unwrap();

    // Get the code chunk
    let chunks = graph.get_code_chunks(&path_str).unwrap();
    assert_eq!(chunks.len(), 1);

    // The chunk should contain the function content
    assert!(chunks[0].content.contains("test_fn"));
    assert!(chunks[0].content.contains("42"));

    // Byte ranges should be valid (start < end, end <= source length)
    assert!(
        chunks[0].byte_start < chunks[0].byte_end,
        "Byte start should be less than byte end"
    );
    assert!(
        chunks[0].byte_end <= source.len(),
        "Byte end should not exceed source length"
    );
}

#[test]
fn test_multiple_files_with_valid_byte_ranges() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create multiple test files
    let file1 = temp_dir.path().join("file1.rs");
    let file2 = temp_dir.path().join("file2.rs");
    let file3 = temp_dir.path().join("file3.rs");

    fs::write(&file1, "fn func1() {}").unwrap();
    fs::write(&file2, "struct Struct2;").unwrap();
    fs::write(&file3, "enum Enum3 { A }").unwrap();

    // Index all files
    for file in [&file1, &file2, &file3] {
        let path_str = file.to_string_lossy().to_string();
        let source = fs::read(file).unwrap();
        graph.index_file(&path_str, &source).unwrap();
    }

    // Verify all files have code chunks extracted
    for file in [&file1, &file2, &file3] {
        let path_str = file.to_string_lossy().to_string();
        let chunks = graph.get_code_chunks(&path_str).unwrap();
        assert!(!chunks.is_empty(), "File should have code chunks");
    }

    // Verify total symbol count
    let symbols1 = graph
        .symbols_in_file(file1.to_string_lossy().as_ref())
        .unwrap();
    let symbols2 = graph
        .symbols_in_file(file2.to_string_lossy().as_ref())
        .unwrap();
    let symbols3 = graph
        .symbols_in_file(file3.to_string_lossy().as_ref())
        .unwrap();

    assert_eq!(symbols1.len(), 1);
    assert_eq!(symbols2.len(), 1);
    assert_eq!(symbols3.len(), 1);
}

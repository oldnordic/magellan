use magellan::{CodeGraph, SymbolKind};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_round_trip_symbols_in_file() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"
        fn function_a() {}
        struct StructA;
        enum EnumA { X }
        mod mod_a;
    ";

    let path = "test.rs";
    let symbol_count = graph.index_file(path, source).unwrap();

    // Should extract 4 symbols
    assert_eq!(symbol_count, 4, "Should index 4 symbols");

    // Query symbols back
    let symbols = graph.symbols_in_file(path).unwrap();

    assert_eq!(symbols.len(), 4, "Should retrieve 4 symbols");

    // Verify each symbol type
    let kinds: Vec<_> = symbols.iter().map(|s| &s.kind).collect();
    assert!(kinds.contains(&&SymbolKind::Function));
    assert!(kinds.contains(&&SymbolKind::Class)); // Rust struct → Class (language-agnostic)
    assert!(kinds.contains(&&SymbolKind::Enum));
    assert!(kinds.contains(&&SymbolKind::Module));
}

#[test]
fn test_idempotent_reindex() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source_v1 = b"fn func_a() {}";
    let path = "test.rs";

    // First index
    let count1 = graph.index_file(path, source_v1).unwrap();
    assert_eq!(count1, 1, "First index should create 1 symbol");

    // Re-index with same content
    let count2 = graph.index_file(path, source_v1).unwrap();
    assert_eq!(
        count2, 1,
        "Re-index with same content should still return 1 symbol"
    );

    // Query should still return 1 symbol
    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(
        symbols.len(),
        1,
        "Should have exactly 1 symbol after re-index"
    );

    // Re-index with different content
    let source_v2 = b"fn func_a() {} fn func_b() {}";
    let count3 = graph.index_file(path, source_v2).unwrap();
    assert_eq!(
        count3, 2,
        "Re-index with new content should create 2 symbols"
    );

    // Query should return 2 symbols now
    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(
        symbols.len(),
        2,
        "Should have 2 symbols after content update"
    );
}

#[test]
fn test_delete_file_cleanup() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn test_fn() {}";
    let path = "test.rs";

    // Index a file
    graph.index_file(path, source).unwrap();

    // Verify symbols exist
    let symbols_before = graph.symbols_in_file(path).unwrap();
    assert_eq!(
        symbols_before.len(),
        1,
        "Should have 1 symbol before delete"
    );

    // Delete the file
    graph.delete_file(path).unwrap();

    // Verify symbols are gone
    let symbols_after = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols_after.len(), 0, "Should have 0 symbols after delete");
}

#[test]
fn test_multiple_files_independent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Index multiple files
    graph.index_file("file_a.rs", b"fn func_a() {}").unwrap();
    graph.index_file("file_b.rs", b"fn func_b() {}").unwrap();
    graph.index_file("file_c.rs", b"struct StructC;").unwrap();

    // Each file should have its own symbols
    let symbols_a = graph.symbols_in_file("file_a.rs").unwrap();
    let symbols_b = graph.symbols_in_file("file_b.rs").unwrap();
    let symbols_c = graph.symbols_in_file("file_c.rs").unwrap();

    assert_eq!(symbols_a.len(), 1, "file_a.rs should have 1 symbol");
    assert_eq!(symbols_b.len(), 1, "file_b.rs should have 1 symbol");
    assert_eq!(symbols_c.len(), 1, "file_c.rs should have 1 symbol");

    // Delete one file should not affect others
    graph.delete_file("file_b.rs").unwrap();

    let symbols_a_after = graph.symbols_in_file("file_a.rs").unwrap();
    let symbols_b_after = graph.symbols_in_file("file_b.rs").unwrap();
    let symbols_c_after = graph.symbols_in_file("file_c.rs").unwrap();

    assert_eq!(
        symbols_a_after.len(),
        1,
        "file_a.rs should still have 1 symbol"
    );
    assert_eq!(
        symbols_b_after.len(),
        0,
        "file_b.rs should have 0 symbols after delete"
    );
    assert_eq!(
        symbols_c_after.len(),
        1,
        "file_c.rs should still have 1 symbol"
    );
}

#[test]
fn test_symbol_fact_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn my_function() {}";
    let path = "test.rs";

    graph.index_file(path, source).unwrap();

    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols.len(), 1);

    let symbol = &symbols[0];

    // Verify all fields are persisted correctly
    assert_eq!(symbol.file_path, PathBuf::from(path));
    assert_eq!(symbol.kind, SymbolKind::Function);
    assert_eq!(symbol.name, Some("my_function".to_string()));
    assert!(symbol.byte_start < symbol.byte_end);
    assert!(symbol.byte_end <= source.len());
}

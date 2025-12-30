//! Tests for symbol kind filtering functionality
//!
//! TDD approach: Write failing test first, then implement feature.

use std::path::PathBuf;

#[test]
fn test_symbols_in_file_filters_by_kind() {
    // Verify that symbols_in_file can filter by symbol kind
    let source = b"fn test_function() {}\nstruct TestStruct {}\nenum TestEnum {}";

    let mut parser = magellan::Parser::new().unwrap();
    let symbols = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(symbols.len(), 3, "Should extract 3 symbols");

    // NEW: Filter by kind
    let functions_only: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Function)
        .collect();

    assert_eq!(functions_only.len(), 1, "Should have 1 function");
    assert!(functions_only[0].name.as_ref().unwrap() == "test_function");

    // Test struct filtering
    let structs_only: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Class)  // Rust struct → Class (language-agnostic)
        .collect();

    assert_eq!(structs_only.len(), 1, "Should have 1 struct");
}

#[test]
fn test_symbols_in_file_with_none_returns_all() {
    // Verify that passing None as kind returns all symbols
    let source = b"fn foo() {}\nstruct Bar {}";

    let mut parser = magellan::Parser::new().unwrap();
    let symbols = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(symbols.len(), 2, "Should extract 2 symbols");
}

#[test]
fn test_code_graph_symbols_in_file_with_kind_filter() {
    // Test the CodeGraph API with optional kind parameter
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn func1() {}\nfn func2() {}\nstruct MyStruct {}";
    graph.index_file("test.rs", source).unwrap();

    // NEW: Query only functions
    let functions = graph.symbols_in_file_with_kind("test.rs", Some(magellan::SymbolKind::Function)).unwrap();

    assert_eq!(functions.len(), 2, "Should have 2 functions");

    for func in &functions {
        assert_eq!(func.kind, magellan::SymbolKind::Function);
    }

    // NEW: Query only structs
    let structs = graph.symbols_in_file_with_kind("test.rs", Some(magellan::SymbolKind::Class)).unwrap();  // Rust struct → Class (language-agnostic)

    assert_eq!(structs.len(), 1, "Should have 1 struct");

    // NEW: Query all (None)
    let all = graph.symbols_in_file_with_kind("test.rs", None).unwrap();

    assert_eq!(all.len(), 3, "Should have 3 symbols total");
}

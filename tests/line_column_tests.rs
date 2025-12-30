//! Tests for line/column span functionality
//!
//! TDD approach: Write failing test first, then implement feature.

use std::path::PathBuf;

#[test]
fn test_symbol_fact_contains_line_column_spans() {
    // Verify that SymbolFact now includes line/column information
    let source = b"fn test_function() {}\n\nstruct TestStruct {}";
    let mut parser = magellan::Parser::new().unwrap();

    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert!(!facts.is_empty(), "Should extract at least one symbol");

    let func = &facts[0];
    assert_eq!(func.kind, magellan::SymbolKind::Function);

    // NEW: Check for line/column fields
    // This test will fail until we add the fields
    let _start_line = func.start_line;
    let _start_col = func.start_col;
    let _end_line = func.end_line;
    let _end_col = func.end_col;

    // Verify values are correct (function starts at line 1, column 0)
    assert_eq!(func.start_line, 1, "Function should start at line 1");
    assert_eq!(func.start_col, 0, "Function should start at column 0 (0-indexed)");
}

#[test]
fn test_reference_fact_contains_line_column_spans() {
    // Verify that ReferenceFact includes line/column information
    let source = b"fn foo() {}\nfn bar() { foo(); }";

    let mut parser = magellan::Parser::new().unwrap();
    let symbols = parser.extract_symbols(PathBuf::from("test.rs"), source);

    let refs = parser.extract_references(PathBuf::from("test.rs"), source, &symbols);

    assert!(!refs.is_empty(), "Should extract at least one reference");

    let reference = &refs[0];

    // NEW: Check for line/column fields
    let _start_line = reference.start_line;
    let _start_col = reference.start_col;
    let _end_line = reference.end_line;
    let _end_col = reference.end_col;

    // Reference to foo() is on line 2
    assert_eq!(reference.start_line, 2, "Reference should be on line 2");
}

#[test]
fn test_symbol_node_persistence_includes_line_column() {
    // Verify that persisted SymbolNode includes line/column
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = b"fn example() {}";
    graph.index_file("test.rs", source).unwrap();

    // Query the symbol back
    let symbols = graph.symbols_in_file("test.rs").unwrap();
    assert!(!symbols.is_empty());

    let symbol = &symbols[0];

    // NEW: Check for line/column fields in queried symbols
    let _start_line = symbol.start_line;
    let _start_col = symbol.start_col;
    let _end_line = symbol.end_line;
    let _end_col = symbol.end_col;

    assert_eq!(symbol.start_line, 1, "Symbol should be on line 1");
}

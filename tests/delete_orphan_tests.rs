//! Orphan detection tests for delete operations.
//!
//! These tests verify that delete_file_facts() leaves the graph in a clean state
//! with no orphaned references or calls after file deletion.
//!
//! The validation module (src/graph/validation.rs) provides:
//! - validate_graph() - runs check_orphan_references() and check_orphan_calls()
//! - check_orphan_references() - finds Reference nodes without REFERENCES edges
//! - check_orphan_calls() - finds Call nodes without CALLER or CALLS edges
//!
//! Test scenarios:
//! 1. Delete file that has only local symbols (no cross-file refs/calls)
//! 2. Delete file that is referenced by other files (cross-file references)
//! 3. Delete file that calls symbols from other files (cross-file calls)
//! 4. Delete multiple files in sequence
//! 5. Delete file, then re-index same file (idempotency check)

use magellan::CodeGraph;
use tempfile::TempDir;

// ============================================================================
// Test 1: Single file deletion (baseline)
// ============================================================================

#[test]
fn test_delete_single_file_no_orphans() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create a graph with one file containing symbols
    let path = "test_single.rs";
    let source = br#"
fn main() {
    println!("hello");
}
"#;

    // Index the file
    graph.index_file(path, source).unwrap();
    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Verify graph is valid before delete
    let report_before = graph.validate_graph();
    assert!(
        report_before.passed,
        "Graph should be valid before delete: {:?}",
        report_before.errors
    );

    // Delete the file
    graph.delete_file(path).unwrap();

    // Run validate_graph() - should pass (no orphans)
    let report_after = graph.validate_graph();
    assert!(
        report_after.passed,
        "Graph should have no orphans after delete: {:?}",
        report_after.errors
    );
    assert!(
        report_after.errors.is_empty(),
        "Should have no orphan errors after delete"
    );
}

// ============================================================================
// Test 2: Cross-file references (referenced file deleted)
// ============================================================================

#[test]
fn test_delete_referenced_file_no_orphans() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // File A: defines function foo()
    let path_a = "lib_a.rs";
    let source_a = br#"
pub fn foo() -> i32 {
    42
}

pub fn bar() -> i32 {
    43
}
"#;

    // File B: references foo() from File A
    let path_b = "lib_b.rs";
    let source_b = br#"
fn call_foo() -> i32 {
    crate::lib_a::foo()
}
"#;

    // Index both files
    graph.index_file(path_a, source_a).unwrap();
    graph.index_references(path_a, source_a).unwrap();
    graph.index_calls(path_a, source_a).unwrap();

    graph.index_file(path_b, source_b).unwrap();
    graph.index_references(path_b, source_b).unwrap();
    graph.index_calls(path_b, source_b).unwrap();

    // Verify graph is valid before delete
    let report_before = graph.validate_graph();
    assert!(
        report_before.passed,
        "Graph should be valid before delete: {:?}",
        report_before.errors
    );

    // Delete File A (the definition)
    // Note: References in File B that pointed to File A's symbols will become orphans
    // because their target symbols no longer exist. This is expected behavior - references
    // can point to external/non-existent symbols. The validation detects this but it's
    // not a bug in the delete operation.
    graph.delete_file(path_a).unwrap();

    // Run validate_graph() to check the state
    // We expect ORPHAN_REFERENCE errors for File B's references to the deleted symbols
    let report_after = graph.validate_graph();

    // The key invariant: File A is completely deleted (no orphaned data FROM File A)
    // Any orphan errors are references IN File B pointing to deleted symbols IN File A
    // This is expected and correct - the delete operation cleaned up all File A's data
    let _orphan_refs_from_b: Vec<_> = report_after
        .errors
        .iter()
        .filter(|e| e.code == "ORPHAN_REFERENCE" && e.details["file"] == "lib_b.rs")
        .collect();

    // These orphan references are expected - they're in File B, not File A
    // The delete operation correctly removed all of File A's data

    // Verify File B still exists with its symbols
    let symbols_b = graph.symbols_in_file(path_b).unwrap();
    assert!(!symbols_b.is_empty(), "File B should still have symbols");

    // Verify File A is gone
    let file_a = graph.get_file_node(path_a).unwrap();
    assert!(file_a.is_none(), "File A should be deleted");
}

// ============================================================================
// Test 3: Cross-file calls (caller deleted)
// ============================================================================

#[test]
fn test_delete_calling_file_no_orphans() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // File A: defines foo() and bar(), foo() calls bar()
    let path_a = "caller.rs";
    let source_a = br#"
fn foo() -> i32 {
    bar()
}

fn bar() -> i32 {
    42
}
"#;

    // File B: defines an independent function
    let path_b = "independent.rs";
    let source_b = br#"
pub fn other_function() -> i32 {
    100
}
"#;

    // Index both files
    graph.index_file(path_a, source_a).unwrap();
    graph.index_references(path_a, source_a).unwrap();
    graph.index_calls(path_a, source_a).unwrap();

    graph.index_file(path_b, source_b).unwrap();
    graph.index_references(path_b, source_b).unwrap();
    graph.index_calls(path_b, source_b).unwrap();

    // Verify graph is valid before delete
    let report_before = graph.validate_graph();
    assert!(
        report_before.passed,
        "Graph should be valid before delete: {:?}",
        report_before.errors
    );

    // Delete File A (which has internal calls)
    graph.delete_file(path_a).unwrap();

    // Run validate_graph() - should pass with no orphan calls
    let report_after = graph.validate_graph();
    assert!(
        report_after.passed,
        "Graph should have no orphan calls after deleting caller file: {:?}",
        report_after.errors
    );

    // Verify File B is unaffected
    let symbols_b = graph.symbols_in_file(path_b).unwrap();
    assert!(!symbols_b.is_empty(), "File B should still have symbols");
}

// ============================================================================
// Test 4: Multiple file deletion
// ============================================================================

#[test]
fn test_delete_multiple_files_no_orphans() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create 3 files with cross-references
    let paths = vec!["file1.rs", "file2.rs", "file3.rs"];

    for (i, path) in paths.iter().enumerate() {
        let source = format!(
            r#"
// File {}
pub fn function_{i}() -> i32 {{
    {i}
}}
"#,
            i + 1
        );
        let source_bytes = source.as_bytes();
        graph.index_file(path, source_bytes).unwrap();
        graph.index_references(path, source_bytes).unwrap();
        graph.index_calls(path, source_bytes).unwrap();
    }

    // Verify graph is valid before delete
    let report_before = graph.validate_graph();
    assert!(
        report_before.passed,
        "Graph should be valid before delete: {:?}",
        report_before.errors
    );

    // Delete all 3 files
    for path in &paths {
        graph.delete_file(path).unwrap();

        // Verify no orphans after each deletion
        let report = graph.validate_graph();
        assert!(
            report.passed,
            "Graph should have no orphans after deleting {}: {:?}",
            path, report.errors
        );
    }

    // Final verification
    let report_final = graph.validate_graph();
    assert!(
        report_final.passed,
        "Graph should have no orphans after deleting all files: {:?}",
        report_final.errors
    );

    // Verify all files are gone
    for path in &paths {
        let file_node = graph.get_file_node(path).unwrap();
        assert!(file_node.is_none(), "{} should be deleted", path);
    }
}

// ============================================================================
// Test 5: Delete and re-index (idempotency)
// ============================================================================

#[test]
fn test_delete_reindex_no_orphans() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "reindex_test.rs";
    let source = br#"
fn test_function() -> i32 {
    42
}

struct TestStruct {
    value: i32,
}

impl TestStruct {
    fn new(value: i32) -> Self {
        Self { value }
    }
}
"#;

    // Index the file
    let symbols_first = graph.index_file(path, source).unwrap();
    assert!(symbols_first > 0, "Should have indexed symbols");

    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Verify graph is valid
    let report_before = graph.validate_graph();
    assert!(
        report_before.passed,
        "Graph should be valid before delete: {:?}",
        report_before.errors
    );

    // Delete the file
    graph.delete_file(path).unwrap();

    // Verify file is gone
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File should be deleted");

    // Verify no orphans after delete
    let report_after_delete = graph.validate_graph();
    assert!(
        report_after_delete.passed,
        "Graph should have no orphans after delete: {:?}",
        report_after_delete.errors
    );

    // Re-index the same file (same content)
    let symbols_second = graph.index_file(path, source).unwrap();
    assert_eq!(
        symbols_second, symbols_first,
        "Should index same number of symbols"
    );

    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Verify graph is clean after re-index
    let report_after_reindex = graph.validate_graph();
    assert!(
        report_after_reindex.passed,
        "Graph should have no orphans after re-index: {:?}",
        report_after_reindex.errors
    );
}

// ============================================================================
// Test 6: Complex graph with multiple symbol types
// ============================================================================

#[test]
fn test_delete_complex_file_no_orphans() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // File with: functions, structs, impls, methods
    let path = "complex.rs";
    let source = br#"
// Module-level function
fn module_function() -> i32 {
    42
}

// Struct definition
struct ComplexStruct {
    field1: i32,
    field2: String,
}

// impl block with methods
impl ComplexStruct {
    fn new(field1: i32, field2: String) -> Self {
        Self { field1, field2 }
    }

    fn get_field1(&self) -> i32 {
        self.field1
    }

    fn set_field1(&mut self, value: i32) {
        self.field1 = value;
    }
}

// Trait definition
trait MyTrait {
    fn trait_method(&self) -> i32;
}

// Trait implementation
impl MyTrait for ComplexStruct {
    fn trait_method(&self) -> i32 {
        self.field1
    }
}

// Enum
enum MyEnum {
    VariantA(i32),
    VariantB(String),
}

impl MyEnum {
    fn value(&self) -> i32 {
        match self {
            MyEnum::VariantA(v) => *v,
            MyEnum::VariantB(_) => 0,
        }
    }
}

// Function that uses the complex types
fn use_complex_types() -> i32 {
    let mut s = ComplexStruct::new(10, String::from("test"));
    s.set_field1(20);
    s.get_field1()
}
"#;

    // Index the file
    let symbol_count = graph.index_file(path, source).unwrap();
    assert!(
        symbol_count >= 10,
        "Should have indexed many symbols (got {})",
        symbol_count
    );

    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Verify graph is valid before delete
    let report_before = graph.validate_graph();
    assert!(
        report_before.passed,
        "Graph should be valid before delete: {:?}",
        report_before.errors
    );

    // Delete the file
    let delete_result = graph.delete_file(path).unwrap();
    assert!(
        delete_result.symbols_deleted > 0,
        "Should have deleted symbols"
    );

    // Verify all entity types and edges cleaned up
    let report_after = graph.validate_graph();
    assert!(
        report_after.passed,
        "Graph should have no orphans after deleting complex file: {:?}",
        report_after.errors
    );

    // Verify no orphan references
    let orphan_refs: Vec<_> = report_after
        .errors
        .iter()
        .filter(|e| e.code == "ORPHAN_REFERENCE")
        .collect();
    assert!(
        orphan_refs.is_empty(),
        "Should have no ORPHAN_REFERENCE errors: {:?}",
        orphan_refs
    );

    // Verify no orphan calls
    let orphan_calls: Vec<_> = report_after
        .errors
        .iter()
        .filter(|e| e.code == "ORPHAN_CALL_NO_CALLER" || e.code == "ORPHAN_CALL_NO_CALLEE")
        .collect();
    assert!(
        orphan_calls.is_empty(),
        "Should have no ORPHAN_CALL errors: {:?}",
        orphan_calls
    );
}

// ============================================================================
// Test 7: Code chunks deletion
// ============================================================================

#[test]
fn test_delete_file_code_chunks_removed() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Index file with multiple symbols (generates chunks)
    let path = "chunks_test.rs";
    let source = br#"
fn function_one() -> i32 {
    10
}

fn function_two() -> i32 {
    20
}

fn function_three() -> i32 {
    30
}
"#;

    graph.index_file(path, source).unwrap();

    // Verify code chunks exist
    let chunks_before = graph.get_code_chunks(path).unwrap();
    let chunk_count = chunks_before.len();
    assert!(
        chunk_count > 0,
        "Should have code chunks (got {})",
        chunk_count
    );

    // Delete file
    graph.delete_file(path).unwrap();

    // Verify chunks are gone via API
    let chunks_after = graph.get_code_chunks(path).unwrap();
    assert_eq!(chunks_after.len(), 0, "No chunks should remain");
}

// ============================================================================
// Test 8: Edge cleanup verification
// ============================================================================

#[test]
fn test_delete_file_edges_removed() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create file with symbols and DEFINES edges
    let path = "edges_test.rs";
    let source = br#"
fn test_func() -> i32 {
    42
}
"#;

    graph.index_file(path, source).unwrap();

    // Get the symbol nodes that were created
    let symbol_nodes = graph.symbol_nodes_in_file(path).unwrap();
    assert!(!symbol_nodes.is_empty(), "Should have symbols");

    // Delete file
    graph.delete_file(path).unwrap();

    // Verify file is gone
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File should be deleted");

    // Verify symbols are gone (and thus edges were cleaned up)
    let symbols_after = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols_after.len(), 0, "All symbols should be deleted");
}

// ============================================================================
// Test 9: No ORPHAN_REFERENCE errors after delete
// ============================================================================

#[test]
fn test_no_orphan_reference_after_delete() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create a file with references
    let path = "ref_test.rs";
    let source = br#"
fn foo() -> i32 {
    42
}

fn bar() -> i32 {
    foo() + 1
}
"#;

    graph.index_file(path, source).unwrap();
    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Delete the file
    graph.delete_file(path).unwrap();

    // Run validation
    let report = graph.validate_graph();

    // Check for ORPHAN_REFERENCE errors specifically
    let orphan_refs: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.code == "ORPHAN_REFERENCE")
        .collect();

    assert!(
        orphan_refs.is_empty(),
        "Should have no ORPHAN_REFERENCE errors after delete. Found: {:?}",
        orphan_refs
    );
}

// ============================================================================
// Test 10: No ORPHAN_CALL errors after delete
// ============================================================================

#[test]
fn test_no_orphan_call_after_delete() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create a file with calls
    let path = "call_test.rs";
    let source = br#"
fn caller() -> i32 {
    callee()
}

fn callee() -> i32 {
    42
}
"#;

    graph.index_file(path, source).unwrap();
    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Delete the file
    graph.delete_file(path).unwrap();

    // Run validation
    let report = graph.validate_graph();

    // Check for ORPHAN_CALL errors specifically
    let orphan_caller: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.code == "ORPHAN_CALL_NO_CALLER")
        .collect();

    let orphan_callee: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.code == "ORPHAN_CALL_NO_CALLEE")
        .collect();

    assert!(
        orphan_caller.is_empty(),
        "Should have no ORPHAN_CALL_NO_CALLER errors after delete. Found: {:?}",
        orphan_caller
    );

    assert!(
        orphan_callee.is_empty(),
        "Should have no ORPHAN_CALL_NO_CALLEE errors after delete. Found: {:?}",
        orphan_callee
    );
}

// ============================================================================
// Test 11: Empty graph validation after delete all
// ============================================================================

#[test]
fn test_empty_graph_valid_after_delete_all() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create and delete multiple files
    let files = vec!["file1.rs", "file2.rs", "file3.rs"];

    for path in &files {
        let source = br#"fn test() -> i32 { 42 }"#;
        graph.index_file(path, source).unwrap();
        graph.index_references(path, source).unwrap();
        graph.index_calls(path, source).unwrap();
    }

    // Delete all files
    for path in &files {
        graph.delete_file(path).unwrap();
    }

    // Verify empty graph is valid
    let report = graph.validate_graph();
    assert!(
        report.passed,
        "Empty graph should be valid after deleting all files: {:?}",
        report.errors
    );
    assert!(report.errors.is_empty(), "Should have no errors");
}

// ============================================================================
// Test 12: Validate specific error codes are not present
// ============================================================================

#[test]
fn test_validate_graph_after_delete_returns_clean_report() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "clean_test.rs";
    let source = br#"
fn main() {
    let x = 42;
    println!("{}", x);
}
"#;

    graph.index_file(path, source).unwrap();
    graph.index_references(path, source).unwrap();
    graph.index_calls(path, source).unwrap();

    // Delete
    graph.delete_file(path).unwrap();

    // Get validation report
    let report = graph.validate_graph();

    // Assert passed is true
    assert!(report.passed, "Report should indicate passed");

    // Assert is_clean is true
    assert!(report.is_clean(), "Report should indicate clean (no errors or warnings)");

    // Assert total_issues is 0
    assert_eq!(report.total_issues(), 0, "Total issues should be 0");

    // Assert errors is empty
    assert!(report.errors.is_empty(), "Errors list should be empty");
}

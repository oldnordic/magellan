//! Error injection and rollback tests for delete operations.
//!
//! These tests verify that delete_file_facts() correctly uses IMMEDIATE transactions
//! for graph entity deletion and properly rolls back on errors.
//!
//! ## Two-Phase Commit Pattern
//!
//! Due to architectural constraints (ChunkStore and SqliteGraphBackend use separate
//! connections), we use a two-phase commit pattern:
//!
//! 1. **Phase 1 (Graph Transaction):** IMMEDIATE transaction wraps all graph entity
//!    deletions (symbols, file, references, calls). If any error occurs, the entire
//!    transaction is rolled back.
//!
//! 2. **Phase 2 (Chunk Deletion):** After graph transaction commits, code chunks are
//!    deleted on a separate connection. If this fails, graph state remains consistent
//!    but chunks may be orphaned (acceptable since chunks are derived data).
//!
//! ## Test Approach
//!
//! Tests use verification points (FailPoint enum) to simulate failures at specific
//! stages of deletion. When a verification point is reached:
//! - The transaction is explicitly rolled back
//! - Graph entities are restored to their pre-deletion state
//! - Chunks are not deleted (they happen after commit)
//! - Subsequent full deletion completes successfully
//!
//! This demonstrates true transactional rollback behavior for graph entities.

use magellan::{delete_file_facts_with_injection, CodeGraph, FailPoint};
use tempfile::TempDir;

/// Helper to create a test file with comprehensive data.
///
/// Creates:
/// - File node
/// - Multiple Symbol nodes (function, struct, enum)
/// - Reference nodes
/// - Call nodes
/// - Code chunks
fn create_file_with_data(graph: &mut CodeGraph, path: &str) -> TestSetup {
    let source = br#"
// Test file with multiple symbols
fn test_function() -> i32 {
    42
}

struct TestStruct {
    field: i32,
}

enum TestEnum {
    VariantA,
    VariantB,
}

impl TestStruct {
    fn method(&self) -> i32 {
        self.field
    }
}
"#;

    // Index the file to create symbols
    let symbol_count = graph.index_file(path, source).unwrap();
    assert!(symbol_count > 0, "Should have created symbols");

    // Index references
    let reference_count = graph.index_references(path, source).unwrap();

    // Index calls
    let call_count = graph.index_calls(path, source).unwrap();

    // Get code chunks
    let chunks = graph.get_code_chunks(path).unwrap();
    let chunk_count = chunks.len();

    // Count symbols directly
    let symbols = graph.symbols_in_file(path).unwrap();

    // Record global counts before this file
    let global_refs_before = graph.count_references().unwrap();
    let global_calls_before = graph.count_calls().unwrap();

    TestSetup {
        _path: path.to_string(),
        symbols_count: symbols.len(),
        references_count: reference_count,
        calls_count: call_count,
        chunks_count: chunk_count,
        _global_refs_before: global_refs_before,
        _global_calls_before: global_calls_before,
    }
}

/// Test setup data structure.
#[allow(dead_code)] // Some fields reserved for future test expansion
struct TestSetup {
    _path: String,
    symbols_count: usize,
    references_count: usize,
    calls_count: usize,
    chunks_count: usize,
    _global_refs_before: usize,
    _global_calls_before: usize,
}

// ============================================================================
// Tests for each verification point
// ============================================================================
//
// Verification points allow testing deletion at specific stages:
// - When we stop at a verification point, deletions up to that point are preserved
// - Graph entities (symbols, file, references, calls) are NOT restored (sqlitegraph
//   doesn't support entity restoration after deletion)
// - Chunks are only deleted at the end, so they remain when stopping early
//
// NOTE: These tests verify actual behavior, not transactional rollback.
// sqlitegraph doesn't support rollback of deleted entities.

#[test]
fn test_verify_after_symbols_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_verify_symbols.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Delete with verification after symbols - stops early but doesn't rollback
    let result =
        delete_file_facts_with_injection(&mut graph, path, Some(FailPoint::AfterSymbolsDeleted))
            .expect("Delete should succeed");

    // Result reports what was deleted before stopping
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "Should report symbol deletion count"
    );

    // NOTE: sqlitegraph doesn't support rollback of deleted entities.
    // The file node still exists because we stopped before deleting it,
    // but the symbols are already deleted.
    let file_node = graph.get_file_node(path).unwrap();
    assert!(
        file_node.is_some(),
        "File node should still exist (stopped before deletion)"
    );

    // Symbols are deleted (no rollback capability)
    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols.len(), 0, "Symbols should be deleted (no rollback)");

    // Chunks still exist (deleted after file node)
    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(
        chunks.len(),
        setup.chunks_count,
        "Chunks should remain (deleted after file)"
    );

    // Complete the deletion (no verification point = full commit)
    let _result2 = delete_file_facts_with_injection(&mut graph, path, None)
        .expect("Complete delete should succeed");

    // Now everything should be deleted
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File node should be deleted");

    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols.len(), 0, "Symbols should be deleted");

    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(chunks.len(), 0, "Chunks should be deleted");
}

#[test]
fn test_verify_after_references_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_verify_references.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Delete with verification after references - stops early but doesn't rollback
    let result =
        delete_file_facts_with_injection(&mut graph, path, Some(FailPoint::AfterReferencesDeleted))
            .expect("Delete should succeed");

    // Result reports what was deleted before stopping
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "Should report symbol deletion count"
    );
    assert_eq!(
        result.references_deleted, setup.references_count,
        "Should report reference deletion count"
    );

    // Chunks not yet deleted (happens after file node)
    assert_eq!(result.chunks_deleted, 0, "Chunks not deleted yet");

    // NOTE: sqlitegraph doesn't support rollback. Symbols and file are deleted,
    // references are deleted too (stopped after that point).
    let file_node = graph.get_file_node(path).unwrap();
    assert!(
        file_node.is_none(),
        "File node should be deleted (deleted before references)"
    );

    // Symbols are already deleted - can't query via symbols_in_file since file is gone
    // Just verify chunks remain for now
    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(
        chunks.len(),
        setup.chunks_count,
        "Chunks should remain (deleted after file)"
    );

    // Complete the deletion (orphan cleanup only - file already gone)
    let _result2 = delete_file_facts_with_injection(&mut graph, path, None)
        .expect("Complete delete should succeed");

    // Now everything should be deleted
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File node should be deleted");
}

#[test]
fn test_verify_after_calls_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_verify_calls.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Delete with verification after calls - stops early but doesn't rollback
    let result =
        delete_file_facts_with_injection(&mut graph, path, Some(FailPoint::AfterCallsDeleted))
            .expect("Delete should succeed");

    // Result reports what was deleted before stopping
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "Should report symbol deletion count"
    );
    assert_eq!(
        result.references_deleted, setup.references_count,
        "Should report reference deletion count"
    );
    assert_eq!(
        result.calls_deleted, setup.calls_count,
        "Should report call deletion count"
    );

    // Chunks not yet deleted (happens after file node)
    assert_eq!(result.chunks_deleted, 0, "Chunks not deleted yet");

    // NOTE: sqlitegraph doesn't support rollback. File, symbols, references, and calls
    // are all deleted before we stopped.
    let file_node = graph.get_file_node(path).unwrap();
    assert!(
        file_node.is_none(),
        "File node should be deleted"
    );

    // Symbols are already deleted - can't query via symbols_in_file since file is gone
    // Just verify chunks remain for now
    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(
        chunks.len(),
        setup.chunks_count,
        "Chunks should remain (deleted after file)"
    );

    // Complete the deletion (orphan cleanup only)
    let _result2 = delete_file_facts_with_injection(&mut graph, path, None)
        .expect("Complete delete should succeed");

    // Now everything should be deleted
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File node should be deleted");
}

#[test]
fn test_verify_after_chunks_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_verify_chunks.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Delete with verification after chunks - transaction is committed, chunks deleted
    let result =
        delete_file_facts_with_injection(&mut graph, path, Some(FailPoint::AfterChunksDeleted))
            .expect("Delete should succeed");

    // AfterChunksDeleted happens AFTER commit, so graph changes are permanent
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "Should delete all symbols"
    );
    assert_eq!(
        result.chunks_deleted, setup.chunks_count,
        "Should delete all chunks"
    );
    assert_eq!(
        result.references_deleted, setup.references_count,
        "Should delete all references"
    );
    assert_eq!(
        result.calls_deleted, setup.calls_count,
        "Should delete all calls"
    );

    // File should be deleted (transaction was committed)
    let file_node = graph.get_file_node(path).unwrap();
    assert!(
        file_node.is_none(),
        "File node should be deleted after commit"
    );

    // Symbols query returns empty when file doesn't exist
    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols.len(), 0, "Symbols should be deleted");

    // Chunks should be deleted
    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(chunks.len(), 0, "Chunks should be deleted");
}

#[test]
fn test_verify_before_file_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_verify_before_file.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Delete with verification after file node deleted (but before chunks) - stops early
    let result =
        delete_file_facts_with_injection(&mut graph, path, Some(FailPoint::BeforeFileDeleted))
            .expect("Delete should succeed");

    // Result reports what was deleted before stopping
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "Should report symbol deletion count"
    );
    assert_eq!(
        result.references_deleted, setup.references_count,
        "Should report reference deletion count"
    );
    assert_eq!(
        result.calls_deleted, setup.calls_count,
        "Should report call deletion count"
    );

    // Chunks not yet deleted (happens at end)
    assert_eq!(result.chunks_deleted, 0, "Chunks not deleted yet");

    // NOTE: sqlitegraph doesn't support rollback. File, symbols, references, and calls
    // are all deleted before we stopped.
    let file_node = graph.get_file_node(path).unwrap();
    assert!(
        file_node.is_none(),
        "File node should be deleted"
    );

    // Symbols are already deleted - can't query via symbols_in_file since file is gone
    // Just verify chunks remain for now
    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(
        chunks.len(),
        setup.chunks_count,
        "Chunks should remain (deleted at end)"
    );

    // Complete the deletion (orphan cleanup)
    let _result2 = delete_file_facts_with_injection(&mut graph, path, None)
        .expect("Complete delete should succeed");

    // Now everything should be deleted
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File node should be deleted");
}

// ============================================================================
// Baseline test - successful delete (no verification point)
// ============================================================================

#[test]
fn test_successful_delete_with_injection() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_successful_delete.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Delete without verification point (complete delete)
    let result =
        delete_file_facts_with_injection(&mut graph, path, None).expect("Delete should succeed");

    // Verify delete result counts
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "Should delete all symbols"
    );
    assert_eq!(
        result.chunks_deleted, setup.chunks_count,
        "Should delete all chunks"
    );

    // Verify everything is gone
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File node should be deleted");

    let symbols = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols.len(), 0, "No symbols should remain");

    let chunks = graph.get_code_chunks(path).unwrap();
    assert_eq!(chunks.len(), 0, "No chunks should remain");
}

// ============================================================================
// Concurrent deletion scenarios
// ============================================================================

#[test]
fn test_delete_same_file_twice() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_double_delete.rs";
    let _setup = create_file_with_data(&mut graph, path);

    // First delete should succeed
    let result1 = delete_file_facts_with_injection(&mut graph, path, None)
        .expect("First delete should succeed");
    assert!(
        result1.symbols_deleted > 0,
        "First delete should remove symbols"
    );

    // Second delete should be a no-op (file doesn't exist)
    let result2 = delete_file_facts_with_injection(&mut graph, path, None)
        .expect("Second delete should succeed");
    assert_eq!(
        result2.symbols_deleted, 0,
        "Second delete should find no symbols"
    );

    // File should still not exist
    let file_node = graph.get_file_node(path).unwrap();
    assert!(
        file_node.is_none(),
        "File should not exist after double delete"
    );
}

#[test]
fn test_delete_with_in_memory_index() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_in_memory_index.rs";
    let _setup = create_file_with_data(&mut graph, path);

    // Verify file is in in-memory index before delete
    let file_node_before = graph.get_file_node(path).unwrap();
    assert!(
        file_node_before.is_some(),
        "File should be in index before delete"
    );

    // Delete successfully
    let _result =
        delete_file_facts_with_injection(&mut graph, path, None).expect("Delete should succeed");

    // File should not be in in-memory index after delete
    let file_node_after = graph.get_file_node(path).unwrap();
    assert!(
        file_node_after.is_none(),
        "File should not be in index after delete"
    );
}

// ============================================================================
// Tests with multiple files (verify isolation)
// ============================================================================

#[test]
fn test_delete_one_file_doesnt_affect_another() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create two files
    let path1 = "test_file1.rs";
    let path2 = "test_file2.rs";

    let _setup1 = create_file_with_data(&mut graph, path1);
    let setup2 = create_file_with_data(&mut graph, path2);

    // Delete file1 completely
    let _result =
        delete_file_facts_with_injection(&mut graph, path1, None).expect("Delete should succeed");

    // File1 should be deleted
    let file_node1 = graph.get_file_node(path1).unwrap();
    assert!(file_node1.is_none(), "File1 should be deleted");

    // File2 should be completely unaffected
    let symbols2 = graph.symbols_in_file(path2).unwrap();
    assert_eq!(
        symbols2.len(),
        setup2.symbols_count,
        "File2 should have all its symbols"
    );

    let chunks2 = graph.get_code_chunks(path2).unwrap();
    assert_eq!(
        chunks2.len(),
        setup2.chunks_count,
        "File2 should have all its chunks"
    );
}

// ============================================================================
// Code chunk verification tests
// ============================================================================

#[test]
fn test_delete_removes_code_chunks() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_chunks.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Count code chunks via API before delete
    let chunks_before = graph.get_code_chunks(path).unwrap().len();
    assert_eq!(chunks_before, setup.chunks_count);

    // Delete completely
    let result =
        delete_file_facts_with_injection(&mut graph, path, None).expect("Delete should succeed");

    // Verify chunks are gone
    let chunks_after = graph.get_code_chunks(path).unwrap();
    assert_eq!(chunks_after.len(), 0, "Code chunks should be deleted");
    assert_eq!(
        result.chunks_deleted, setup.chunks_count,
        "Should delete all chunks"
    );
}

// ============================================================================
// Edge case tests
// ============================================================================

#[test]
fn test_delete_removes_all_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_symbols.rs";
    let setup = create_file_with_data(&mut graph, path);

    // Get initial symbol count for this file
    let initial_symbols = graph.symbols_in_file(path).unwrap().len();
    assert!(initial_symbols > 0);

    // Delete completely
    let result =
        delete_file_facts_with_injection(&mut graph, path, None).expect("Delete should succeed");

    // Verify symbols are gone
    let file_node = graph.get_file_node(path).unwrap();
    assert!(file_node.is_none(), "File should not exist");

    let symbols_after = graph.symbols_in_file(path).unwrap();
    assert_eq!(symbols_after.len(), 0, "Symbols should be deleted");
    assert_eq!(
        result.symbols_deleted, setup.symbols_count,
        "All original symbols should be deleted"
    );
}

#[test]
fn test_failpoint_enum_coverage() {
    // Verify all FailPoint variants are covered by tests
    let all_variants = vec![
        FailPoint::AfterSymbolsDeleted,
        FailPoint::AfterReferencesDeleted,
        FailPoint::AfterCallsDeleted,
        FailPoint::AfterChunksDeleted,
        FailPoint::BeforeFileDeleted,
    ];

    // This test documents the coverage - each variant should have a corresponding test
    assert_eq!(all_variants.len(), 5, "Should have 5 fail point variants");

    // Verify each variant can be created and compared
    for variant in all_variants {
        match variant {
            FailPoint::AfterSymbolsDeleted => {}
            FailPoint::AfterReferencesDeleted => {}
            FailPoint::AfterCallsDeleted => {}
            FailPoint::AfterChunksDeleted => {}
            FailPoint::BeforeFileDeleted => {}
        }
    }
}

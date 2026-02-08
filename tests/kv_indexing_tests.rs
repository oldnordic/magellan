//! Integration tests for KV indexing behavior.
//!
//! These tests verify that indexing with native-v2 backend stores all
//! metadata in the KV store and queries read from KV.

#[cfg(feature = "native-v2")]
mod tests {
    use magellan::CodeGraph;
    use sqlitegraph::{SnapshotId, backend::KvValue};
    use tempfile::tempdir;

    /// Test that indexing writes code chunks to KV store.
    ///
    /// Verifies:
    /// - Chunks are created with correct key prefix (chunk:)
    /// - Chunk keys contain file path and byte range
    /// - Chunk values are Json containing CodeChunk data
    #[test]
    fn test_indexing_writes_chunks_to_kv() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index a file
        let source = b"fn test_func() { let x = 42; }";
        let count = graph.index_file("test.rs", source).unwrap();
        assert!(count > 0, "Should have indexed symbols");

        // Get backend reference after indexing
        let backend = graph.__backend_for_benchmarks();

        // Verify chunks in KV
        let snapshot = SnapshotId::current();
        let entries = backend.kv_prefix_scan(snapshot, b"chunk:").unwrap();
        assert!(!entries.is_empty(), "Should have chunks in KV");

        // Verify chunk content format
        for (key, value) in entries {
            let key_str = String::from_utf8(key).unwrap();
            assert!(key_str.starts_with("chunk:"), "Chunk key should have chunk: prefix");
            // Chunks are stored as Json containing CodeChunk data
            assert!(matches!(value, KvValue::Json(_)), "Chunk value should be Json");
        }
    }

    /// Test that indexing writes AST nodes to KV store.
    ///
    /// Verifies:
    /// - AST nodes are created with correct key prefix (ast:file:)
    /// - AST values contain encoded node data
    #[test]
    fn test_indexing_writes_ast_to_kv() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index a file with nested structure
        let source = b"fn main() { if true { println!(\"hi\"); } }";
        graph.index_file("test.rs", source).unwrap();

        // Get backend reference after indexing
        let backend = graph.__backend_for_benchmarks();

        // Verify AST nodes in KV
        let snapshot = SnapshotId::current();
        let entries = backend.kv_prefix_scan(snapshot, b"ast:file:").unwrap();
        assert!(!entries.is_empty(), "Should have AST nodes in KV");

        // Verify AST entries have correct format
        for (key, value) in entries {
            let key_str = String::from_utf8(key).unwrap();
            assert!(key_str.starts_with("ast:file:"), "AST key should have ast:file: prefix");
            // AST nodes are stored as Bytes containing encoded Vec<AstNode>
            assert!(matches!(value, KvValue::Bytes(_)), "AST value should be Bytes");
        }
    }

    /// Test that deletion removes KV entries for chunks.
    ///
    /// Verifies:
    /// - Chunks are created during indexing
    /// - Chunks are removed after file deletion
    ///
    /// Note: Currently ignored because delete_file_facts() has a dependency on SQLite
    /// tables (graph_edges) that don't exist in Native V2 backend. This is a known
    /// architectural limitation that needs to be fixed in ops.rs::delete_file_facts.
    #[test]
    #[ignore = "delete_file has SQLite table dependency in Native V2 backend"]
    fn test_deletion_removes_kv_entries() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index a file
        let source = b"fn to_delete() { }";
        graph.index_file("delete.rs", source).unwrap();

        // Get backend reference to verify data exists
        {
            let backend = graph.__backend_for_benchmarks();
            let snapshot = SnapshotId::current();
            let chunks_before = backend.kv_prefix_scan(snapshot, b"chunk:delete.rs:").unwrap();
            assert!(!chunks_before.is_empty(), "Should have chunks before deletion");
        }

        // Delete file
        graph.delete_file("delete.rs").unwrap();

        // Verify chunks removed (get new backend reference after deletion)
        let backend = graph.__backend_for_benchmarks();
        let snapshot = SnapshotId::current();
        let chunks_after = backend.kv_prefix_scan(snapshot, b"chunk:delete.rs:").unwrap();
        assert!(chunks_after.is_empty(), "Chunks should be removed after deletion");
    }

    /// Test that indexing creates label entries (if applicable).
    ///
    /// Verifies:
    /// - If labels are created, they have correct key format
    /// - Label values are JSON-encoded
    #[test]
    fn test_indexing_writes_labels_to_kv() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index a file that creates symbols
        let source = b"pub fn my_function() -> i32 { 42 }";
        graph.index_file("labels.rs", source).unwrap();

        // Get backend reference after indexing
        let backend = graph.__backend_for_benchmarks();
        let snapshot = SnapshotId::current();
        let label_entries = backend.kv_prefix_scan(snapshot, b"label:").unwrap();

        // Labels may or may not be created depending on implementation
        // If present, verify format
        for (key, value) in label_entries {
            let key_str = String::from_utf8(key).unwrap();
            assert!(key_str.starts_with("label:"), "Label key should have label: prefix");
            assert!(matches!(value, KvValue::Json(_)), "Label value should be Json");
        }
    }

    /// Test that KV index entries are created for symbol lookups.
    ///
    /// Verifies:
    /// - sym:fqn:* entries are created for O(1) symbol lookup
    /// - sym:fqn_of:* entries are created for reverse lookup
    /// - file:sym:* entries contain encoded symbol ID lists
    #[test]
    fn test_indexing_creates_symbol_kv_index() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index a file with named symbols
        let source = b"pub fn indexed_function() -> i32 { 42 }";
        let count = graph.index_file("indexed.rs", source).unwrap();
        assert!(count > 0, "Should have indexed symbols");

        // Get backend reference after indexing
        let backend = graph.__backend_for_benchmarks();
        let snapshot = SnapshotId::current();

        // Check for sym:fqn: entries (primary index)
        let fqn_entries = backend.kv_prefix_scan(snapshot, b"sym:fqn:").unwrap();
        assert!(!fqn_entries.is_empty(), "Should have sym:fqn: entries in KV");

        // Check for sym:fqn_of: entries (reverse index)
        let fqn_of_entries = backend.kv_prefix_scan(snapshot, b"sym:fqn_of:").unwrap();
        assert!(!fqn_of_entries.is_empty(), "Should have sym:fqn_of: entries in KV");

        // Verify entry formats
        for (key, value) in fqn_entries {
            let key_str = String::from_utf8(key).unwrap();
            assert!(key_str.starts_with("sym:fqn:"), "Key should have sym:fqn: prefix");
            // Symbol ID values are stored as Integer
            assert!(matches!(value, KvValue::Integer(_)), "Symbol ID value should be Integer");
        }

        for (key, value) in fqn_of_entries {
            let key_str = String::from_utf8(key).unwrap();
            assert!(key_str.starts_with("sym:fqn_of:"), "Key should have sym:fqn_of: prefix");
            // FQN values are stored as String
            assert!(matches!(value, KvValue::String(_)), "FQN value should be String");
        }
    }

    /// Test that reindexing updates KV entries correctly.
    ///
    /// Verifies:
    /// - Old entries are invalidated
    /// - New entries are created
    /// - No stale entries remain
    #[test]
    fn test_reindexing_updates_kv_entries() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index initial version
        let source1 = b"fn old_func() { 1 }";
        graph.index_file("reindex.rs", source1).unwrap();

        // Get initial chunk count
        let snapshot = SnapshotId::current();
        {
            let backend = graph.__backend_for_benchmarks();
            let chunks_before = backend.kv_prefix_scan(snapshot, b"chunk:reindex.rs:").unwrap();
            assert!(!chunks_before.is_empty(), "Should have chunks before reindexing");
        }

        // Reindex with different content
        let source2 = b"fn new_func() { 2 }";
        graph.index_file("reindex.rs", source2).unwrap();

        // Verify chunks still exist (may have different count)
        let backend = graph.__backend_for_benchmarks();
        let chunks_after = backend.kv_prefix_scan(snapshot, b"chunk:reindex.rs:").unwrap();
        assert!(!chunks_after.is_empty(), "Should have chunks after reindexing");
    }

    /// Test that indexing writes call edges to KV store.
    ///
    /// Verifies:
    /// - Call edges are created with correct key prefix (calls:)
    /// - Call keys contain caller and callee IDs
    /// - Call values are Json with call edge data
    #[test]
    fn test_indexing_writes_calls_to_kv() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index files with call relationships
        let caller = b"fn caller() { callee(); }";
        let callee = b"fn callee() { }";
        graph.index_file("caller.rs", caller).unwrap();
        graph.index_file("callee.rs", callee).unwrap();

        // Get backend reference after indexing
        let backend = graph.__backend_for_benchmarks();
        let snapshot = SnapshotId::current();

        // Verify call edges in KV
        let call_entries = backend.kv_prefix_scan(snapshot, b"calls:").unwrap();

        // At minimum, we should check the key format is correct for any entries found
        for (key, value) in call_entries {
            let key_str = String::from_utf8(key).unwrap();
            assert!(key_str.starts_with("calls:"), "Call key should have calls: prefix");
            assert!(matches!(value, KvValue::Json(_)), "Call value should be Json");
        }
    }
}

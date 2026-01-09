//! Tests for graph module

#[cfg(test)]
mod tests {

    #[test]
    fn test_hash_computation() {
        let graph = crate::CodeGraph::open(":memory:").unwrap();
        let source = b"fn test() {}";
        let hash = graph.files.compute_hash(source);

        // SHA-256 hash should be 64 hex characters
        assert_eq!(hash.len(), 64);

        // Same input should produce same hash
        let hash2 = graph.files.compute_hash(source);
        assert_eq!(hash, hash2);

        // Different input should produce different hash
        let hash3 = graph.files.compute_hash(b"different content");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_cross_file_references() {
        // This test demonstrates the bug: cross-file references are NOT created
        let mut graph = crate::CodeGraph::open(":memory:").unwrap();

        // File 1: defines a function
        let file1_path = "/test/file1.rs";
        let file1_source = b"pub fn defined_in_file1() -> i32 { 42 }";
        graph.index_file(file1_path, file1_source).unwrap();

        // File 2: calls the function from file1
        let file2_path = "/test/file2.rs";
        let file2_source = b"pub fn caller_in_file2() -> i32 { defined_in_file1() }";
        graph.index_file(file2_path, file2_source).unwrap();

        // Index references for both files
        graph.index_references(file1_path, file1_source).unwrap();
        graph.index_references(file2_path, file2_source).unwrap();

        // Verify symbols are indexed
        let symbols1 = graph.symbols_in_file(file1_path).unwrap();
        assert_eq!(symbols1.len(), 1);
        assert_eq!(symbols1[0].name, Some("defined_in_file1".to_string()));

        let symbols2 = graph.symbols_in_file(file2_path).unwrap();
        assert_eq!(symbols2.len(), 1);
        assert_eq!(symbols2[0].name, Some("caller_in_file2".to_string()));

        // Get the symbol ID for defined_in_file1
        let symbol_id = graph.symbol_id_by_name(file1_path, "defined_in_file1").unwrap();
        assert!(symbol_id.is_some(), "Symbol defined_in_file1 should exist");
        let symbol_id = symbol_id.unwrap();

        // Check for REFERENCES edges to defined_in_file1
        // This SHOULD find references from file2, but currently doesn't (BUG)
        let references = graph.references_to_symbol(symbol_id).unwrap();

        // THIS ASSERTION FAILS - demonstrates the bug
        // Expected: at least 1 reference from file2
        // Actual: 0 references (only same-file references are indexed)
        assert!(
            !references.is_empty(),
            "Cross-file references should be created. Found: {} references. \
             This demonstrates the bug: only same-file references are indexed.",
            references.len()
        );
    }
}

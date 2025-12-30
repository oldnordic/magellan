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
}

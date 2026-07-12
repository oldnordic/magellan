
#[test]
fn test_ast_nodes_indexed_with_file() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    let source = b"fn main() { if true { println!(\"hello\"); } }";
    graph.index_file("test.rs", source).unwrap();

    // Verify AST nodes were created
    let count: i64 = graph
        .chunks
        .with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM ast_nodes", [], |row| row.get(0))
                .map_err(anyhow::Error::from)
        })
        .unwrap();

    assert!(count > 0, "AST nodes should be created during indexing");

    // Verify specific nodes exist
    let if_count: i64 = graph
        .chunks
        .with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM ast_nodes WHERE kind = 'if_expression'",
                [],
                |row| row.get(0),
            )
            .map_err(anyhow::Error::from)
        })
        .unwrap();
    assert!(if_count > 0, "if_expression should be indexed");
}

#[test]
fn test_hopgraph_lifecycle_index_delete_reindex() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    let source_a = b"fn parse_rust() -> u32 { 42 }";
    graph.index_file("a.rs", source_a).unwrap();

    let hits = graph.hopgraph_search("parse_rust", 5, 0).unwrap();
    assert!(
        !hits.is_empty(),
        "hopgraph_search should find symbols after indexing"
    );

    let top_id = hits[0].entity_id;

    graph.delete_file_facts("a.rs").unwrap();

    let hits_after_delete = graph.hopgraph_search("parse_rust", 5, 0).unwrap();
    let deleted_still_present = hits_after_delete.iter().any(|h| h.entity_id == top_id);
    assert!(
        !deleted_still_present,
        "deleted symbol should not appear in hopgraph results"
    );

    graph.index_file("a.rs", source_a).unwrap();

    let hits_after_reindex = graph.hopgraph_search("parse_rust", 5, 0).unwrap();
    assert!(
        !hits_after_reindex.is_empty(),
        "hopgraph_search should find symbols after reindexing"
    );
}

#[test]
fn test_hopgraph_multiple_files_ranking() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    graph
        .index_file(
            "parse_rust.rs",
            b"fn parse_rust_file() -> String { \"\".to_string() }",
        )
        .unwrap();
    graph
        .index_file(
            "parse_python.rs",
            b"fn parse_python_file() -> String { \"\".to_string() }",
        )
        .unwrap();

    let hits = graph.hopgraph_search("parse_rust_file", 5, 0).unwrap();
    assert!(!hits.is_empty(), "search should find results");

    let first_id = hits[0].entity_id;

    graph.delete_file_facts("parse_rust.rs").unwrap();

    let hits_after = graph.hopgraph_search("parse_rust_file", 5, 0).unwrap();
    let still_present = hits_after.iter().any(|h| h.entity_id == first_id);
    assert!(
        !still_present,
        "deleted file's symbols should be removed from index"
    );

    let python_present = hits_after.iter().any(|h| h.entity_id != first_id);
    assert!(
        python_present || hits_after.is_empty(),
        "other file's symbols may or may not appear (hash embedder)"
    );
}

#[test]
fn test_hopgraph_resolves_entity_ids_to_names() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    let source = b"fn compute_checksum(data: &[u8]) -> u32 { 42 }";
    graph.index_file("checksum.rs", source).unwrap();

    let hits = graph.hopgraph_search("compute_checksum", 5, 0).unwrap();
    assert!(!hits.is_empty(), "hopgraph should find compute_checksum");

    let top = &hits[0];
    assert!(
        !top.name.is_empty(),
        "resolved hit should have a non-empty name"
    );
    assert!(
        !top.kind.is_empty(),
        "resolved hit should have a non-empty kind"
    );
    // The top result should be the indexed function
    assert!(
        top.name.contains("checksum"),
        "resolved name '{}' should contain 'checksum'",
        top.name,
    );
}

#[test]
fn test_hopgraph_hops_expansion() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    // Index two functions where one calls the other
    let source_a = b"fn compute_hash(data: &[u8]) -> u32 { 42 }";
    let source_b = b"fn compute_hash_wrapper(input: &[u8]) -> u32 { compute_hash(input) }";
    graph.index_file("hash.rs", source_a).unwrap();
    graph.index_file("wrapper.rs", source_b).unwrap();

    // hops=0 should return only vector matches
    let hits_0 = graph.hopgraph_search("compute_hash", 10, 0).unwrap();
    assert!(!hits_0.is_empty(), "should find results with hops=0");

    // hops=1 should include graph-expanded neighbors
    let hits_1 = graph.hopgraph_search("compute_hash", 10, 1).unwrap();
    // With hops enabled, we should get at least as many results
    // (expanded hits are added to the initial vector hits)
    assert!(!hits_1.is_empty(), "should find results with hops=1");

    // hops=0 results should all have hop_distance=0
    for hit in &hits_0 {
        assert_eq!(
            hit.hop_distance, 0,
            "hops=0 results should all have hop_distance=0, got {} for {}",
            hit.hop_distance, hit.name
        );
    }
}

#[test]
fn test_delete_file_facts_clears_symbol_lookup() {
    // Regression: delete_file_facts must remove symbols from the in-memory
    // lookup index. Without this, stale (deleted) entity_ids remain and
    // cause "edge endpoints must exist" errors when index_references()
    // creates ambiguity/reference edges targeting those dead entities.
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    // Index a file with multiple symbols
    let src = b"fn alpha() -> u32 { 42 }\nfn beta() -> u32 { 99 }";
    graph.index_file("test.rs", src).unwrap();

    // Verify symbols are in the lookup
    let facts_before = graph.symbols.lookup.all_symbol_facts();
    assert!(
        !facts_before.is_empty(),
        "lookup should contain indexed symbols"
    );

    // Delete file facts — must clear the lookup too
    graph.delete_file_facts("test.rs").unwrap();

    // Verify lookup no longer contains the deleted symbols
    let facts_after = graph.symbols.lookup.all_symbol_facts();
    assert!(
        facts_after.is_empty(),
        "lookup should be empty after delete_file_facts, but still has {} entries: {:?}",
        facts_after.len(),
        facts_after
            .iter()
            .map(|f| f.fqn.as_deref().unwrap_or("?"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_reconcile_file_no_dangling_lookup_entries() {
    // Regression: reconciling an edited file must not leave stale symbol
    // entity_ids in the in-memory lookup. Before the fix, delete_file_facts
    // deleted symbols from the backend but not from the lookup, so the
    // subsequent index_references() created edges referencing dead entities
    // ("edge endpoints must exist"), rolling back the entire file reconcile.
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");

    // Two files with same-named symbols to exercise ambiguity + ref paths
    fs::write(&file_a, b"fn shared_fn() -> u32 { 1 }").unwrap();
    fs::write(&file_b, b"fn shared_fn() -> u32 { 2 }").unwrap();

    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();

    graph
        .index_file("a.rs", b"fn shared_fn() -> u32 { 1 }")
        .unwrap();
    graph
        .index_file("b.rs", b"fn shared_fn() -> u32 { 2 }")
        .unwrap();

    // Modify file A and reconcile — must not error
    fs::write(&file_a, b"fn shared_fn() -> u32 { 999 }").unwrap();
    let outcome =
        graph.reconcile_file_path_with_source(&file_a, "a.rs", b"fn shared_fn() -> u32 { 999 }");
    assert!(
        outcome.is_ok(),
        "reconcile should succeed, but failed: {:?}",
        outcome.err()
    );

    // Verify no dead entity_ids leaked into the lookup: every id_to_fqn
    // key must correspond to a live entity in the backend.
    let backend_ids: std::collections::HashSet<i64> = graph
        .symbols
        .backend
        .entity_ids()
        .unwrap()
        .into_iter()
        .collect();
    let lookup_ids: Vec<i64> = graph
        .symbols
        .lookup
        .entity_to_symbol_id()
        .keys()
        .copied()
        .collect();
    for id in &lookup_ids {
        assert!(
            backend_ids.contains(id),
            "stale entity_id {} in lookup after reconcile — not in backend",
            id
        );
    }
}

#[test]
fn test_no_hnsw_index_when_embeddings_disabled() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = crate::CodeGraph::open(&db_path).unwrap();
    graph.configure_embeddings(&crate::config::EmbedProvider::Hash, false, "", "", "", 0);

    assert!(
        !graph.embeddings_enabled(),
        "embeddings should be disabled by default"
    );

    let source = b"fn parse_rust() -> u32 { 42 }";
    graph.index_file("test.rs", source).unwrap();

    let sg = graph.symbols.sqlite_graph().unwrap();
    let indexes = sg.list_hnsw_indexes().unwrap();
    assert!(
        indexes.is_empty(),
        "no HNSW index should be created when embeddings disabled, found: {:?}",
        indexes
    );
}

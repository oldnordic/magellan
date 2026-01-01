//! Exploration test for sqlitegraph features not currently used by Magellan
//!
//! Tests:
//! 1. HNSW vector embeddings (1536 dim for OpenAI)
//! 2. Graph labels for categorization
//! 3. Graph properties for metadata

use std::time::Instant;

#[test]
fn test_sqlitegraph_features() -> anyhow::Result<()> {
    println!("=== SQLiteGraph Features Exploration ===\n");

    test_hnsw_embeddings()?;
    test_graph_labels()?;
    test_graph_properties()?;

    println!("\n=== All Tests Passed ===");
    Ok(())
}

/// Test HNSW vector embeddings with 1536 dimensions (OpenAI format)
fn test_hnsw_embeddings() -> anyhow::Result<()> {
    println!("--- Test 1: HNSW Vector Embeddings (1536 dim) ---");

    use sqlitegraph::hnsw::{hnsw_config, DistanceMetric, HnswIndex};
    use serde_json::json;

    // Production configuration for OpenAI text-embedding-ada-002
    let config = hnsw_config()
        .dimension(1536)
        .m_connections(20)
        .ef_construction(400)
        .ef_search(100)
        .distance_metric(DistanceMetric::Cosine)
        .build()?;

    let mut hnsw = HnswIndex::new(config)?;

    // Create synthetic 1536-dim embeddings for code snippets
    let code_snippets = vec![
        ("fn parse_config(path: &str) -> Result<Config>", create_1536_embedding(1.0)),
        ("fn connect_database(url: &str) -> Result<Connection>", create_1536_embedding(2.0)),
        ("fn render_template(data: &Data) -> String", create_1536_embedding(3.0)),
        ("fn compress_data(input: &[u8]) -> Vec<u8>", create_1536_embedding(4.0)),
    ];

    // Insert with metadata
    let start = Instant::now();
    for (code, embedding) in &code_snippets {
        let metadata = json!({
            "symbol_name": code.split('(').next().unwrap().trim().replace("fn ", ""),
            "file_path": "src/example.rs",
            "code_snippet": *code,
            "language": "rust",
        });
        hnsw.insert_vector(embedding, Some(metadata))?;
    }
    let insert_duration = start.elapsed();

    println!("  Inserted {} vectors (1536 dim each) in {:?}", code_snippets.len(), insert_duration);

    // Test search
    let query = create_1536_embedding(1.05); // Similar to first snippet
    let start = Instant::now();
    let results = hnsw.search(&query, 3)?;
    let search_duration = start.elapsed();

    println!("  Search completed in {:?} (found {} results)", search_duration, results.len());

    for (vector_id, distance) in results {
        if let Some((_, metadata)) = hnsw.get_vector(vector_id)? {
            if let Some(name) = metadata.get("symbol_name").and_then(|v| v.as_str()) {
                println!("    [{}] distance={:.4}: {}", name, distance, vector_id);
            }
        }
    }

    // Test statistics
    let stats = hnsw.statistics()?;
    println!("  HNSW Index Stats:");
    println!("    Total vectors: {}", stats.vector_count);
    println!("    Layers: {}", stats.layer_count);

    println!("  ✓ HNSW embeddings working\n");

    Ok(())
}

/// Test graph labels for categorization
fn test_graph_labels() -> anyhow::Result<()> {
    println!("--- Test 2: Graph Labels ---");

    use sqlitegraph::{SqliteGraph, add_label};
    use std::path::PathBuf;

    let test_db = PathBuf::from("/tmp/magellan_labels_test.db");
    if test_db.exists() {
        std::fs::remove_file(&test_db)?;
    }

    let graph = SqliteGraph::open(&test_db)?;

    // Insert entity
    let entity = sqlitegraph::GraphEntity {
        id: 0,
        kind: "Function".to_string(),
        name: "process_request".to_string(),
        file_path: Some("src/api.rs".to_string()),
        data: serde_json::json!({"public": true}),
    };

    let entity_id = graph.insert_entity(&entity)?;

    // Add labels using the standalone function
    add_label(&graph, entity_id, "rust")?;
    add_label(&graph, entity_id, "public")?;
    add_label(&graph, entity_id, "async")?;
    add_label(&graph, entity_id, "api")?;

    println!("  Labeled entity {} with: rust, public, async, api", entity_id);

    // Query labels via raw SQL (workaround since get_entities_by_label is not exported)
    let labels = query_sql(
        &test_db,
        "SELECT label FROM graph_labels WHERE entity_id=?1 ORDER BY label",
        &[entity_id as usize]
    )?;
    println!("  All labels: {:?}", labels);

    println!("  ✓ Graph labels working\n");

    Ok(())
}

/// Test graph properties for metadata
fn test_graph_properties() -> anyhow::Result<()> {
    println!("--- Test 3: Graph Properties ---");

    use sqlitegraph::{SqliteGraph, add_property};
    use std::path::PathBuf;

    let test_db = PathBuf::from("/tmp/magellan_props_test.db");
    if test_db.exists() {
        std::fs::remove_file(&test_db)?;
    }

    let graph = SqliteGraph::open(&test_db)?;

    // Insert entity
    let entity = sqlitegraph::GraphEntity {
        id: 0,
        kind: "Function".to_string(),
        name: "complex_handler".to_string(),
        file_path: Some("src/handler.rs".to_string()),
        data: serde_json::json!({}),
    };

    let entity_id = graph.insert_entity(&entity)?;

    // Add properties using the standalone function
    add_property(&graph, entity_id, "complexity", "12")?;
    add_property(&graph, entity_id, "lines_of_code", "87")?;
    add_property(&graph, entity_id, "test_coverage", "65%")?;
    add_property(&graph, entity_id, "last_modified", "2025-01-01")?;

    println!("  Added 4 properties to entity {}", entity_id);

    // Query properties via raw SQL
    let props = query_sql(
        &test_db,
        "SELECT key || '=' || value FROM graph_properties WHERE entity_id=?1 ORDER BY key",
        &[entity_id as usize]
    )?;
    println!("  All properties: {:?}", props);

    println!("  ✓ Graph properties working\n");

    Ok(())
}

/// Create a synthetic 1536-dimension embedding
/// In production, this would be generated by OpenAI's text-embedding-ada-002
fn create_1536_embedding(seed: f32) -> Vec<f32> {
    (0..1536)
        .map(|i| (seed * (i as f32 / 1536.0)).sin())
        .collect()
}

/// Helper to query a single column from SQLite
fn query_sql(db_path: &std::path::PathBuf, sql: &str, params: &[usize]) -> anyhow::Result<Vec<String>> {
    use rusqlite::params_from_iter;

    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter().copied()), |row| row.get::<_, String>(0))?;
    let result: Result<Vec<_>, _> = rows.collect();
    Ok(result?)
}

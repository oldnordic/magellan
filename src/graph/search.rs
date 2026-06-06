use anyhow::Result;
use serde_json::json;
use sqlitegraph::hnsw::{DistanceMetric, HnswConfig};
use sqlitegraph::SqliteGraph;

use super::embed::{symbol_embed_text, TextEmbedder};

const SEARCH_INDEX_NAME: &str = "symbols";

fn search_config(dim: usize) -> Result<HnswConfig> {
    sqlitegraph::hnsw::HnswConfigBuilder::new()
        .dimension(dim)
        .distance_metric(DistanceMetric::Cosine)
        .ef_search(200)
        .enable_multilayer(true)
        .build()
        .map_err(|e| anyhow::anyhow!("HNSW config build failed: {}", e))
}

fn index_in_memory(graph: &SqliteGraph) -> bool {
    graph
        .get_hnsw_index(SEARCH_INDEX_NAME)
        .map(|opt| opt.is_some())
        .unwrap_or(false)
}

fn ensure_index_in_memory(graph: &SqliteGraph, dim: usize) -> Result<()> {
    if index_in_memory(graph) {
        return Ok(());
    }
    let config = search_config(dim)?;
    {
        let _guard = graph
            .hnsw_index_persistent(SEARCH_INDEX_NAME, config)
            .map_err(|e| anyhow::anyhow!("hnsw_index_persistent failed: {}", e))?;
    }
    Ok(())
}

pub fn ensure_search_index(
    graph: &SqliteGraph,
    embedder: &dyn TextEmbedder,
    entities: &[sqlitegraph::GraphEntity],
) -> Result<()> {
    if index_in_memory(graph) {
        return Ok(());
    }
    ensure_index_in_memory(graph, embedder.dimension())?;
    for entity in entities {
        let text = symbol_embed_text(entity, None);
        let vector = embedder.embed(&text)?;
        let entity_id = entity.id;
        let _ = graph.get_hnsw_index_mut(SEARCH_INDEX_NAME, move |idx| {
            idx.insert_vector(&vector, Some(json!({"entity_id": entity_id})))
        });
    }
    Ok(())
}

pub fn add_to_search_index(
    graph: &SqliteGraph,
    embedder: &dyn TextEmbedder,
    entity: &sqlitegraph::GraphEntity,
) -> Result<()> {
    ensure_index_in_memory(graph, embedder.dimension())?;
    let text = symbol_embed_text(entity, None);
    let vector = embedder.embed(&text)?;
    let entity_id = entity.id;
    graph
        .get_hnsw_index_mut(SEARCH_INDEX_NAME, move |idx| {
            idx.insert_vector(&vector, Some(json!({"entity_id": entity_id})))
        })
        .map_err(|e| anyhow::anyhow!("insert_vector failed: {}", e))?
        .map_err(|e| anyhow::anyhow!("hnsw insert failed: {}", e))?;
    Ok(())
}

pub fn add_to_search_index_with_vector(
    graph: &SqliteGraph,
    entity_id: i64,
    vector: &[f32],
) -> Result<()> {
    ensure_index_in_memory(graph, vector.len())?;
    let v = vector.to_vec();
    graph
        .get_hnsw_index_mut(SEARCH_INDEX_NAME, move |idx| {
            idx.insert_vector(&v, Some(json!({"entity_id": entity_id})))
        })
        .map_err(|e| anyhow::anyhow!("insert_vector failed: {}", e))?
        .map_err(|e| anyhow::anyhow!("hnsw insert failed: {}", e))?;
    Ok(())
}

/// Bulk-insert pre-computed vectors into the HNSW search index.
///
/// This acquires the index mutex once and calls `batch_insert_vectors`,
/// which persists topology to SQLite once for the entire batch instead of
/// after every individual insert. Use this instead of calling
/// `add_to_search_index_with_vector` in a loop.
pub fn bulk_add_to_search_index(graph: &SqliteGraph, entries: &[(i64, Vec<f32>)]) -> Result<usize> {
    if entries.is_empty() {
        return Ok(0);
    }
    let dim = entries[0].1.len();
    ensure_index_in_memory(graph, dim)?;

    let batch: Vec<(Vec<f32>, Option<serde_json::Value>)> = entries
        .iter()
        .map(|(entity_id, vec)| (vec.clone(), Some(json!({"entity_id": *entity_id}))))
        .collect();

    let count = batch.len();
    graph
        .get_hnsw_index_mut(SEARCH_INDEX_NAME, move |idx| {
            idx.batch_insert_vectors(&batch)
        })
        .map_err(|e| anyhow::anyhow!("batch_insert failed: {}", e))?
        .map_err(|e| anyhow::anyhow!("hnsw batch insert failed: {}", e))?;

    Ok(count)
}

pub fn remove_from_search_index(_graph: &SqliteGraph, _entity_id: i64) -> Result<()> {
    Ok(())
}

/// Clear all vectors from the 'symbols' HNSW index (both in-memory and persistent).
/// Used by `embed --force` to avoid duplicate vectors on re-embed.
pub fn clear_search_index(graph: &SqliteGraph) -> Result<()> {
    graph
        .delete_hnsw_index(SEARCH_INDEX_NAME)
        .map_err(|e| anyhow::anyhow!("failed to delete HNSW index: {}", e))?;
    Ok(())
}

pub fn search_symbols(
    graph: &SqliteGraph,
    embedder: &dyn TextEmbedder,
    query: &str,
    k: usize,
) -> Result<Vec<(i64, f32)>> {
    ensure_index_in_memory(graph, embedder.dimension())?;
    if !index_in_memory(graph) {
        return Ok(Vec::new());
    }
    let query_vec = embedder.embed(query)?;
    let hits = graph
        .get_hnsw_index_ref(SEARCH_INDEX_NAME, |idx| idx.search(&query_vec, k * 2))
        .map_err(|e| anyhow::anyhow!("hnsw search failed: {}", e))?
        .map_err(|e| anyhow::anyhow!("hnsw search error: {}", e))?;
    let mut results = Vec::with_capacity(hits.len());
    for (vector_id, score) in hits {
        if results.len() >= k {
            break;
        }
        let metadata = graph
            .get_hnsw_index_ref(SEARCH_INDEX_NAME, |idx| {
                idx.get_vector(vector_id).ok().flatten()
            })
            .map_err(|e| anyhow::anyhow!("get_vector failed: {}", e))?;
        if let Some((_vec, meta)) = metadata {
            if let Some(entity_id) = meta.get("entity_id").and_then(|v| v.as_i64()) {
                if graph.get_entity(entity_id).is_ok() {
                    results.push((entity_id, score));
                }
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::embed::HashEmbedder;

    #[test]
    fn test_search_config_builds() {
        let config = search_config(128).unwrap();
        assert_eq!(config.dimension, 128);
    }

    #[test]
    fn test_symbol_embed_text_extracts_fields() {
        let entity = sqlitegraph::GraphEntity {
            id: 1,
            kind: "Symbol".to_string(),
            name: "parse_rust".to_string(),
            file_path: Some("src/lib.rs".to_string()),
            data: serde_json::json!({
                "fqn": "magellan::parse_rust",
                "kind_normalized": "function",
            }),
        };
        let text = symbol_embed_text(&entity, None);
        assert!(text.contains("parse_rust"));
        assert!(text.contains("magellan::parse_rust"));
    }

    #[test]
    fn test_add_and_search_roundtrip() {
        let graph = SqliteGraph::open_in_memory().unwrap();
        let embedder = HashEmbedder::new(128);

        let e1 = sqlitegraph::GraphEntity {
            id: 0,
            kind: "Symbol".to_string(),
            name: "parse_rust".to_string(),
            file_path: None,
            data: serde_json::json!({"fqn": "magellan::parse_rust"}),
        };
        let e2 = sqlitegraph::GraphEntity {
            id: 0,
            kind: "Symbol".to_string(),
            name: "parse_python".to_string(),
            file_path: None,
            data: serde_json::json!({"fqn": "magellan::parse_python"}),
        };
        let id1 = graph.insert_entity(&e1).unwrap();
        let id2 = graph.insert_entity(&e2).unwrap();

        let entities: Vec<sqlitegraph::GraphEntity> = vec![
            sqlitegraph::GraphEntity { id: id1, ..e1 },
            sqlitegraph::GraphEntity { id: id2, ..e2 },
        ];

        ensure_search_index(&graph, &embedder, &entities).unwrap();

        let results = search_symbols(&graph, &embedder, "parse_rust", 2).unwrap();
        assert!(!results.is_empty(), "search should find results");
        assert_eq!(
            results[0].0, id1,
            "top result should be entity {} (parse_rust)",
            id1
        );
    }

    #[test]
    fn test_remove_from_index() {
        let graph = SqliteGraph::open_in_memory().unwrap();
        let embedder = HashEmbedder::new(128);

        let e1 = sqlitegraph::GraphEntity {
            id: 0,
            kind: "Symbol".to_string(),
            name: "parse_rust".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        };
        let e2 = sqlitegraph::GraphEntity {
            id: 0,
            kind: "Symbol".to_string(),
            name: "parse_python".to_string(),
            file_path: None,
            data: serde_json::json!({}),
        };
        let id1 = graph.insert_entity(&e1).unwrap();
        let id2 = graph.insert_entity(&e2).unwrap();

        let entities: Vec<sqlitegraph::GraphEntity> = vec![
            sqlitegraph::GraphEntity { id: id1, ..e1 },
            sqlitegraph::GraphEntity { id: id2, ..e2 },
        ];

        ensure_search_index(&graph, &embedder, &entities).unwrap();

        graph.delete_entity(id2).unwrap();

        let results = search_symbols(&graph, &embedder, "parse_python", 2).unwrap();
        let found_ids: Vec<i64> = results.iter().map(|(id, _)| *id).collect();
        assert!(
            !found_ids.contains(&id2),
            "deleted entity should not appear in results"
        );
    }
}

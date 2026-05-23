//! HNSW command implementation
//!
//! Create and query HNSW vector indexes via sqlitegraph.

use anyhow::{Context, Result};
use magellan::OutputFormat;
use serde_json::json;
use sqlitegraph::SqliteGraph;
use std::path::PathBuf;

/// Create an HNSW index
pub fn run_hnsw_create(
    db_path: PathBuf,
    name: String,
    dim: usize,
    m: usize,
    ef_construction: usize,
    ef_search: usize,
    _output_format: OutputFormat,
) -> Result<()> {
    use sqlitegraph::hnsw::config::HnswConfig;
    use sqlitegraph::hnsw::distance_metric::DistanceMetric;

    let graph = SqliteGraph::open(&db_path)
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

    let mut config = HnswConfig::new(dim, m, ef_construction, DistanceMetric::Cosine);
    config.ef_search = ef_search;

    let _guard = graph
        .hnsw_index(&name, config)
        .map_err(|e| anyhow::anyhow!("Failed to create HNSW index: {}", e))?;

    println!(
        "Created HNSW index '{}' (dim={}, m={}, ef_construction={}, ef_search={})",
        name, dim, m, ef_construction, ef_search
    );
    Ok(())
}

/// Query an HNSW index
pub fn run_hnsw_query(
    db_path: PathBuf,
    name: String,
    vector_str: String,
    k: usize,
    output_format: OutputFormat,
) -> Result<()> {
    let vector: Vec<f32> = serde_json::from_str(&vector_str).with_context(|| {
        format!(
            "Failed to parse vector as JSON array of f32: {}",
            vector_str
        )
    })?;

    let graph = SqliteGraph::open(&db_path)
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

    let results: Vec<(u64, f32)> = graph
        .get_hnsw_index_ref(&name, |idx| idx.search(&vector, k).unwrap_or_default())
        .map_err(|e| anyhow::anyhow!("HNSW query failed: {}", e))?;

    let rows: Vec<serde_json::Value> = results
        .into_iter()
        .enumerate()
        .map(|(i, (id, dist))| {
            json!({
                "rank": i + 1,
                "id": id,
                "distance": dist,
            })
        })
        .collect();

    match output_format {
        OutputFormat::Json => println!("{}", serde_json::to_string(&rows)?),
        OutputFormat::Pretty => println!("{}", serde_json::to_string_pretty(&rows)?),
        OutputFormat::Human => {
            println!("HNSW query results (index '{}', k={}):", name, k);
            for row in &rows {
                println!(
                    "  #{}: id={}, distance={:.6}",
                    row["rank"].as_u64().unwrap_or(0),
                    row["id"].as_u64().unwrap_or(0),
                    row["distance"].as_f64().unwrap_or(f64::NAN),
                );
            }
            println!("{} result(s)", rows.len());
        }
    }

    Ok(())
}

use anyhow::Result;
use magellan::graph::CodeGraph;
use magellan::output::{output_json, OutputFormat};
use serde_json::json;
use std::path::PathBuf;

pub fn run_hopgraph(
    db_path: PathBuf,
    query: String,
    k: usize,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let hits = graph.hopgraph_search(&query, k)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let rows: Vec<serde_json::Value> = hits
                .iter()
                .enumerate()
                .map(|(i, (entity_id, score))| {
                    json!({
                        "rank": i + 1,
                        "entity_id": entity_id,
                        "score": score,
                    })
                })
                .collect();
            output_json(&rows, output_format)?;
        }
        OutputFormat::Human => {
            println!("HopGraph results for '{}':", query);
            for (i, (entity_id, score)) in hits.iter().enumerate() {
                println!("  #{}: entity_id={}, score={:.6}", i + 1, entity_id, score);
            }
            println!("{} result(s)", hits.len());
        }
    }

    Ok(())
}

use anyhow::Result;
use magellan::graph::CodeGraph;
use magellan::output::{output_json, OutputFormat};
use serde_json::json;
use std::path::PathBuf;

pub fn run_hopgraph(
    db_path: PathBuf,
    query: String,
    k: usize,
    hops: u32,
    output_format: OutputFormat,
) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;
    let hits = graph.hopgraph_search(&query, k, hops)?;

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let rows: Vec<serde_json::Value> = hits
                .iter()
                .enumerate()
                .map(|(i, hit)| {
                    let mut obj = json!({
                        "rank": i + 1,
                        "entity_id": hit.entity_id,
                        "score": hit.score,
                        "name": hit.name,
                        "kind": hit.kind,
                        "file_path": hit.file_path,
                        "start_line": hit.start_line,
                    });
                    if hit.hop_distance > 0 {
                        obj["hop_distance"] = json!(hit.hop_distance);
                    }
                    obj
                })
                .collect();
            output_json(&rows, output_format)?;
        }
        OutputFormat::Human => {
            let hops_label = if hops > 0 {
                format!(" ({} hop expansion)", hops)
            } else {
                String::new()
            };
            println!("HopGraph results for '{}':{}", query, hops_label);
            for (i, hit) in hits.iter().enumerate() {
                let location = hit
                    .file_path
                    .as_ref()
                    .map(|p| {
                        let short = shorten_path(p);
                        format!(" [{}:{}]", short, hit.start_line)
                    })
                    .unwrap_or_default();
                let hop_label = if hit.hop_distance > 0 {
                    format!(" hop={}", hit.hop_distance)
                } else {
                    String::new()
                };
                println!(
                    "  #{}: {} {}{}{} score={:.6}",
                    i + 1,
                    hit.kind,
                    hit.name,
                    location,
                    hop_label,
                    hit.score,
                );
            }
            println!("{} result(s)", hits.len());
        }
    }

    Ok(())
}

/// Strip common path prefixes for human-readable output.
fn shorten_path(path: &str) -> &str {
    // Try to show just the src/... portion
    if let Some(idx) = path.find("/src/") {
        &path[idx + 1..]
    } else if let Some(idx) = path.rfind('/') {
        &path[idx + 1..]
    } else {
        path
    }
}

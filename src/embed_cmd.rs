//! `magellan embed` command implementation
//!
//! Reads already-indexed entities from the DB, finds those missing HNSW vectors,
//! and embeds them using the configured embedding provider. No re-parsing — purely
//! DB-driven. Uses CodeGraph::embed_from_db internally.

use anyhow::Result;
use magellan::graph::CodeGraph;
use magellan::output::OutputFormat;
use serde_json::json;
use std::path::PathBuf;

pub fn run_embed(
    db_path: PathBuf,
    force: bool,
    batch_size: Option<usize>,
    output_format: OutputFormat,
) -> Result<()> {
    let batch = batch_size.unwrap_or(64);
    let mut graph = CodeGraph::open(&db_path)?;

    if !graph.embeddings_enabled() {
        anyhow::bail!(
            "Embeddings are not enabled. Configure an embedding provider in \
             ~/.config/magellan/config.toml under [embeddings]."
        );
    }

    let (embedded, skipped, failed) =
        graph.embed_from_db(force, batch, |path, count, idx, total| {
            if matches!(output_format, OutputFormat::Human) {
                eprintln!("  [{}/{}] {} — {} symbols", idx + 1, total, path, count);
            }
        })?;

    match output_format {
        OutputFormat::Human => {
            if embedded == 0 && failed == 0 {
                println!(
                    "All symbols already embedded ({} total). Use --force to re-embed.",
                    skipped
                );
            } else {
                println!(
                    "Embedded {} / skipped {} (already indexed) / failed {}",
                    embedded, skipped, failed
                );
            }
        }
        OutputFormat::Json => {
            let result = json!({
                "embedded": embedded,
                "skipped": skipped,
                "failed": failed,
            });
            println!("{}", serde_json::to_string(&result)?);
        }
        OutputFormat::Pretty => {
            let result = json!({
                "embedded": embedded,
                "skipped": skipped,
                "failed": failed,
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

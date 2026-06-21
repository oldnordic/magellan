//! Blast score computation command
//!
//! Provides single-score impact analysis similar to codeindex's blast radius.

use anyhow::Result;
use magellan::context::compute_blast_score;
use magellan::{CodeGraph, OutputFormat};
use std::path::PathBuf;

/// Run blast score computation
pub fn run_blast_score(
    db_path: PathBuf,
    symbol: String,
    file: Option<String>,
    depth: usize,
    output: OutputFormat,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;

    let blast_score = compute_blast_score(&mut graph, &symbol, file.as_deref(), depth)?;

    match output {
        OutputFormat::Human => {
            println!(
                "Blast Score: {:.1} ({} direct · {} transitive) [{}]",
                blast_score.score,
                blast_score.direct_count,
                blast_score.transitive_count,
                blast_score.risk_level
            );

            if blast_score.total_impacted > 0 {
                println!(
                    "Impact: {:.1}% of codebase ({} files, {} symbols)",
                    blast_score.risk_percent,
                    blast_score.total_impacted,
                    blast_score.total_impacted
                );
            } else {
                println!("No impact found for symbol '{}'", symbol);
            }
        }
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = serde_json::json!({
                "command": "blast-score",
                "data": blast_score,
            });

            let formatted = if matches!(output, OutputFormat::Pretty) {
                serde_json::to_string_pretty(&response)?
            } else {
                serde_json::to_string(&response)?
            };

            println!("{}", formatted);
        }
    }

    Ok(())
}

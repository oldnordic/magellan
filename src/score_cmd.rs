//! Score command — rank symbols by interestingness
//!
//! Computes scores for symbols based on static, CFG, and temporal features.
//! Outputs ranked candidates for optimization, review, or vulnerability analysis.
//!
//! Usage:
//!   magellan score --db <db>                    — score all symbols
//!   magellan score --db <db> --top 10           — show top 10 candidates
//!   magellan score --db <db> --min-churn 5      — filter by minimum churn
//!   magellan score --db <db> --output json      — JSON output

use anyhow::{Context, Result};
use std::path::PathBuf;

use magellan::graph::scorer::{ScorerOps, ScorerRunSummary, ScoreFilters, SymbolScore};

/// Run the score command
///
/// # Arguments
/// * `db` - Database path
/// * `top` - Show top N candidates
/// * `min_score` - Filter by minimum score
/// * `min_churn` - Filter by minimum churn count
/// * `min_complexity` - Filter by minimum complexity
/// * `min_lifetime` - Filter by minimum lifetime
/// * `output_format` - Output format (human/json/pretty)
pub fn run_score(
    db: &PathBuf,
    top: Option<usize>,
    min_score: Option<f64>,
    min_churn: Option<i64>,
    min_complexity: Option<i64>,
    min_lifetime: Option<i64>,
    output_format: magellan::OutputFormat,
) -> Result<()> {
    // If no filters/top specified, default to scoring all
    let should_score_all = top.is_none()
        && min_score.is_none()
        && min_churn.is_none()
        && min_complexity.is_none()
        && min_lifetime.is_none();

    let mut ops = ScorerOps::from_db_path(db)
        .with_context(|| format!("Failed to open scorer operations for {}", db.display()))?;

    if should_score_all {
        // Score all symbols
        let summary = ops.score_all()
            .map_err(|e| {
                let mut msg = format!("Detailed error: {}", e);
                for cause in e.chain() {
                    msg.push_str(&format!("\n  caused by: {}", cause));
                }
                eprintln!("{}", msg);
                anyhow::anyhow!("Failed to score all symbols")
            })?;

        match output_format {
            magellan::OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            }
            magellan::OutputFormat::Human | magellan::OutputFormat::Pretty => {
                println!("Scored {} symbols in {:?}", summary.symbols_scored, summary.duration);
                println!("Scorer version: {}", summary.scorer_version);
                println!("Run ID: {}", summary.id);

                // Show top 10
                if let Ok(top) = ops.top_candidates(10) {
                    println!("\nTop 10 candidates:");
                    for (i, candidate) in top.iter().enumerate() {
                        println!(
                            "  {}. {} — score: {:.2}, churn: {}, complexity: {}",
                            i + 1,
                            candidate.stable_id,
                            candidate.score,
                            candidate.feature_churn_count,
                            candidate.feature_complexity
                        );
                    }
                }
            }
        }
    } else {
        // Query with filters
        let filters = ScoreFilters {
            min_score,
            min_churn,
            min_complexity,
            min_lifetime,
            limit: top,
        };

        let candidates = ops.query_candidates(&filters)
            .with_context(|| "Failed to query candidates")?;

        match output_format {
            magellan::OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&candidates)?);
            }
            magellan::OutputFormat::Human | magellan::OutputFormat::Pretty => {
                let limit = filters.limit.unwrap_or(candidates.len());
                println!("Showing {} of {} candidates:", limit, candidates.len());
                println!();

                for (i, candidate) in candidates.iter().enumerate() {
                    println!(
                        "{}. {} — score: {:.2}",
                        i + 1,
                        candidate.stable_id,
                        candidate.score
                    );
                    println!("   LOC: {}, fan_in: {}, fan_out: {}, complexity: {}",
                        candidate.feature_loc,
                        candidate.feature_fan_in,
                        candidate.feature_fan_out,
                        candidate.feature_complexity
                    );
                    println!("   CFG blocks: {}, CFG edges: {}, conditional_density: {:.2}",
                        candidate.feature_cfg_block_count,
                        candidate.feature_cfg_edge_count,
                        candidate.feature_conditional_density
                    );
                    println!("   Lifetime: {}, churn: {}",
                        candidate.feature_lifetime,
                        candidate.feature_churn_count
                    );
                    println!();
                }
            }
        }
    }

    Ok(())
}

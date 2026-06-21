//! Scorer schema for symbol ranking and candidate selection
//!
//! Stores computed scores and feature breakdowns for symbols,
//! enabling fast candidate queries for optimization, review,
//! or vulnerability analysis.

use serde::{Deserialize, Serialize};

/// Score cache row (one per symbol)
///
/// Computed by scorer and stored for fast ranking queries.
/// Features are explicitly stored (not JSON blob) to enable SQL filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolScore {
    /// Symbol ID (PRIMARY KEY, FOREIGN KEY to graph_entities)
    pub symbol_id: i64,

    /// Most recent snapshot for this symbol
    pub snapshot_id: i64,

    /// Stable temporal identifier (across snapshots)
    pub stable_id: String,

    /// Final ranking score
    pub score: f64,

    /// Global rank (computed post-query, NULL until ranked)
    pub rank: Option<i64>,

    /// Static features (from symbol_metrics)
    pub feature_loc: i64,
    pub feature_fan_in: i64,
    pub feature_fan_out: i64,
    pub feature_complexity: i64,

    /// CFG features (from cfg_blocks)
    pub feature_cfg_block_count: i64,
    pub feature_cfg_edge_count: i64,
    pub feature_conditional_density: f64,

    /// Temporal features (from symbol_versions)
    pub feature_lifetime: i64,
    pub feature_churn_count: i64,

    /// Model metadata
    pub scorer_version: String,
    pub scored_at: i64,
}

/// Feature importance tracking
///
/// Stores weights and metadata for scoring model features.
/// Enables model evolution and weight tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerFeature {
    /// Feature name (PRIMARY KEY)
    pub name: String,

    /// Weight in scoring model
    pub weight: f64,

    /// Whether feature is enabled (0 or 1)
    pub enabled: bool,

    /// Human-readable description
    pub description: String,
}

/// Scoring run audit trail
///
/// Records each scoring execution for traceability and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerRun {
    /// Run ID (PRIMARY KEY)
    pub id: i64,

    /// Scorer version identifier (e.g., "v1_ast_baseline")
    pub scorer_version: String,

    /// Unix timestamp when run started
    pub started_at: i64,

    /// Unix timestamp when run completed (NULL if in progress)
    pub completed_at: Option<i64>,

    /// Number of symbols scored
    pub symbols_scored: i64,

    /// Number of features used
    pub feature_count: i64,

    /// Additional metadata (JSON)
    pub metadata: Option<String>,
}

/// Create scorer schema tables
///
/// Creates three tables: symbol_scores, scorer_features, scorer_runs.
/// All tables use IF NOT EXISTS for safe repeated calls.
pub fn ensure_schema(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    // symbol_scores table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbol_scores (
            symbol_id INTEGER PRIMARY KEY,
            snapshot_id INTEGER NOT NULL,
            stable_id TEXT NOT NULL,
            score REAL NOT NULL,
            rank INTEGER,
            feature_loc INTEGER NOT NULL DEFAULT 0,
            feature_fan_in INTEGER NOT NULL DEFAULT 0,
            feature_fan_out INTEGER NOT NULL DEFAULT 0,
            feature_complexity INTEGER NOT NULL DEFAULT 0,
            feature_cfg_block_count INTEGER NOT NULL DEFAULT 0,
            feature_cfg_edge_count INTEGER NOT NULL DEFAULT 0,
            feature_conditional_density REAL NOT NULL DEFAULT 0.0,
            feature_lifetime INTEGER NOT NULL DEFAULT 0,
            feature_churn_count INTEGER NOT NULL DEFAULT 0,
            scorer_version TEXT NOT NULL,
            scored_at INTEGER NOT NULL,
            FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE,
            FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| anyhow::anyhow!("create symbol_scores table: {}", e))?;

    // Indexes for symbol_scores
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_scores_score ON symbol_scores(score DESC)",
        [],
    )
    .map_err(|e| anyhow::anyhow!("create score index: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_scores_stable ON symbol_scores(stable_id)",
        [],
    )
    .map_err(|e| anyhow::anyhow!("create stable_id index: {}", e))?;

    // scorer_features table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scorer_features (
            name TEXT PRIMARY KEY,
            weight REAL NOT NULL,
            enabled INTEGER NOT NULL,
            description TEXT
        )",
        [],
    )
    .map_err(|e| anyhow::anyhow!("create scorer_features table: {}", e))?;

    // scorer_runs table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scorer_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scorer_version TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            symbols_scored INTEGER NOT NULL,
            feature_count INTEGER NOT NULL,
            metadata TEXT
        )",
        [],
    )
    .map_err(|e| anyhow::anyhow!("create scorer_runs table: {}", e))?;

    Ok(())
}

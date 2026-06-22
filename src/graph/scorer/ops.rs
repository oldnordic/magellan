//! Scorer operations for symbol ranking
//!
//! Coordinates feature extraction, scoring, and database storage.
//! Provides API for scoring symbols and querying ranked candidates.

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::extract::{FeatureExtractor, SymbolFeatures};
use super::schema::SymbolScore;
use super::score::Scorer;

/// Summary of a scoring run
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScorerRunSummary {
    /// Run ID
    pub id: i64,

    /// Scorer version used
    pub scorer_version: String,

    /// Unix timestamp when run started
    pub started_at: i64,

    /// Unix timestamp when run completed
    pub completed_at: i64,

    /// Number of symbols scored
    pub symbols_scored: i64,

    /// Duration in seconds
    pub duration: i64,
}

/// Filters for querying scored symbols
#[derive(Debug, Clone, Default)]
pub struct ScoreFilters {
    /// Minimum score threshold
    pub min_score: Option<f64>,

    /// Minimum churn count
    pub min_churn: Option<i64>,

    /// Minimum complexity
    pub min_complexity: Option<i64>,

    /// Minimum lifetime
    pub min_lifetime: Option<i64>,

    /// Maximum number of results
    pub limit: Option<usize>,
}

/// Scorer operations
///
/// Coordinates feature extraction, scoring model, and database operations.
pub struct ScorerOps {
    db: Arc<Mutex<rusqlite::Connection>>,
    extractor: FeatureExtractor,
    scorer: Scorer,
}

impl ScorerOps {
    /// Create scorer operations from database path
    pub fn from_db_path(db_path: &std::path::Path) -> Result<Self> {
        let conn = rusqlite::Connection::open(db_path)?;
        Self::with_connection(Arc::new(Mutex::new(conn)))
    }

    /// Create scorer operations with shared connection
    pub fn with_connection(db: Arc<Mutex<rusqlite::Connection>>) -> Result<Self> {
        // Ensure schema exists
        {
            let conn = db.lock();
            super::schema::ensure_schema(&conn)?;
        }

        let extractor = FeatureExtractor::new(db.clone());
        let scorer = Scorer::new_ast_baseline();

        Ok(Self {
            db,
            extractor,
            scorer,
        })
    }

    /// Create scorer operations with custom scorer
    pub fn with_scorer(db: Arc<Mutex<rusqlite::Connection>>, scorer: Scorer) -> Result<Self> {
        {
            let conn = db.lock();
            super::schema::ensure_schema(&conn)?;
        }

        let extractor = FeatureExtractor::new(db.clone());

        Ok(Self {
            db,
            extractor,
            scorer,
        })
    }

    /// Score all symbols in the database
    ///
    /// 1. Start scorer_run row
    /// 2. FOR each symbol: extract features → compute score → store
    /// 3. Complete scorer_run row
    /// 4. Compute global ranks
    pub fn score_all(&mut self) -> Result<ScorerRunSummary> {
        let started_at = Self::now_timestamp();

        // Start scorer_run row
        let run_id = {
            let conn = self.db.lock();
            conn.execute(
                "INSERT INTO scorer_runs (scorer_version, started_at, symbols_scored, feature_count)
                 VALUES (?1, ?2, 0, 9)",
                params![self.scorer.version(), started_at],
            )?;
            conn.last_insert_rowid()
        };

        // Extract all features
        let all_features = self.extractor.extract_all()?;
        let symbols_scored = all_features.len() as i64;

        // Score and store each symbol
        {
            let conn = self.db.lock();

            for features in &all_features {
                let score = self.scorer.score(features);

                // Check if symbol already exists (update) or is new (insert)
                let exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM symbol_scores WHERE symbol_id = ?1",
                        params![features.symbol_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0)
                    > 0;

                if exists {
                    conn.execute(
                        "UPDATE symbol_scores SET
                            snapshot_id = ?1, stable_id = ?2, score = ?3,
                            feature_loc = ?4, feature_fan_in = ?5, feature_fan_out = ?6,
                            feature_complexity = ?7, feature_cfg_block_count = ?8,
                            feature_cfg_edge_count = ?9, feature_conditional_density = ?10,
                            feature_lifetime = ?11, feature_churn_count = ?12,
                            scorer_version = ?13, scored_at = ?14
                         WHERE symbol_id = ?15",
                        params![
                            features.snapshot_id,
                            &features.stable_id,
                            score,
                            features.loc,
                            features.fan_in,
                            features.fan_out,
                            features.complexity,
                            features.cfg_block_count,
                            features.cfg_edge_count,
                            features.conditional_density,
                            features.lifetime,
                            features.churn_count,
                            self.scorer.version(),
                            started_at,
                            features.symbol_id,
                        ],
                    )?;
                } else {
                    conn.execute(
                        "INSERT INTO symbol_scores (
                            symbol_id, snapshot_id, stable_id, score,
                            feature_loc, feature_fan_in, feature_fan_out,
                            feature_complexity, feature_cfg_block_count,
                            feature_cfg_edge_count, feature_conditional_density,
                            feature_lifetime, feature_churn_count,
                            scorer_version, scored_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                        params![
                            features.symbol_id,
                            features.snapshot_id,
                            &features.stable_id,
                            score,
                            features.loc,
                            features.fan_in,
                            features.fan_out,
                            features.complexity,
                            features.cfg_block_count,
                            features.cfg_edge_count,
                            features.conditional_density,
                            features.lifetime,
                            features.churn_count,
                            self.scorer.version(),
                            started_at,
                        ],
                    )?;
                }
            }

            // Compute global ranks (ROW_NUMBER() OVER ORDER BY score DESC)
            conn.execute(
                "WITH ranked AS (
                    SELECT symbol_id, ROW_NUMBER() OVER (ORDER BY score DESC) as rn
                    FROM symbol_scores
                )
                UPDATE symbol_scores SET rank = ranked.rn
                FROM ranked WHERE symbol_scores.symbol_id = ranked.symbol_id",
                [],
            )?;
        }

        let completed_at = Self::now_timestamp();

        // Complete scorer_run row
        {
            let conn = self.db.lock();
            conn.execute(
                "UPDATE scorer_runs SET completed_at = ?1, symbols_scored = ?2 WHERE id = ?3",
                params![completed_at, symbols_scored, run_id],
            )?;
        }

        Ok(ScorerRunSummary {
            id: run_id,
            scorer_version: self.scorer.version().to_string(),
            started_at,
            completed_at,
            symbols_scored,
            duration: completed_at - started_at,
        })
    }

    /// Score specific symbols only
    pub fn score_symbols(&mut self, symbol_ids: &[i64]) -> Result<Vec<SymbolScore>> {
        let started_at = Self::now_timestamp();

        let mut results = Vec::new();

        for &symbol_id in symbol_ids {
            if let Ok(features) = self.extractor.extract_for_symbol(symbol_id) {
                let score = self.scorer.score(&features);
                let stable_id = features.stable_id.clone();

                let symbol_score = SymbolScore {
                    symbol_id: features.symbol_id,
                    snapshot_id: features.snapshot_id,
                    stable_id: stable_id.clone(),
                    score,
                    rank: None,
                    feature_loc: features.loc,
                    feature_fan_in: features.fan_in,
                    feature_fan_out: features.fan_out,
                    feature_complexity: features.complexity,
                    feature_cfg_block_count: features.cfg_block_count,
                    feature_cfg_edge_count: features.cfg_edge_count,
                    feature_conditional_density: features.conditional_density,
                    feature_lifetime: features.lifetime,
                    feature_churn_count: features.churn_count,
                    scorer_version: self.scorer.version().to_string(),
                    scored_at: started_at,
                };

                results.push(symbol_score);

                // Store in database
                let conn = self.db.lock();
                let exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM symbol_scores WHERE symbol_id = ?1",
                        params![symbol_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0)
                    > 0;

                if exists {
                    conn.execute(
                        "UPDATE symbol_scores SET
                            snapshot_id = ?1, stable_id = ?2, score = ?3,
                            scorer_version = ?4, scored_at = ?5
                         WHERE symbol_id = ?6",
                        params![
                            features.snapshot_id,
                            &stable_id,
                            score,
                            self.scorer.version(),
                            started_at,
                            symbol_id,
                        ],
                    )?;
                } else {
                    conn.execute(
                        "INSERT INTO symbol_scores (
                            symbol_id, snapshot_id, stable_id, score,
                            scorer_version, scored_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            symbol_id,
                            features.snapshot_id,
                            &features.stable_id,
                            score,
                            self.scorer.version(),
                            started_at,
                        ],
                    )?;
                }
            }
        }

        Ok(results)
    }

    /// Get top-N candidates by score
    pub fn top_candidates(&self, limit: usize) -> Result<Vec<SymbolScore>> {
        let conn = self.db.lock();

        let mut stmt = conn.prepare(
            "SELECT
                symbol_id, snapshot_id, stable_id, score, rank,
                feature_loc, feature_fan_in, feature_fan_out, feature_complexity,
                feature_cfg_block_count, feature_cfg_edge_count, feature_conditional_density,
                feature_lifetime, feature_churn_count,
                scorer_version, scored_at
             FROM symbol_scores
             ORDER BY score DESC
             LIMIT ?1",
        )?;

        let results = stmt
            .query_map([limit as i64], |row| {
                Ok(SymbolScore {
                    symbol_id: row.get(0)?,
                    snapshot_id: row.get(1)?,
                    stable_id: row.get(2)?,
                    score: row.get(3)?,
                    rank: row.get(4)?,
                    feature_loc: row.get(5)?,
                    feature_fan_in: row.get(6)?,
                    feature_fan_out: row.get(7)?,
                    feature_complexity: row.get(8)?,
                    feature_cfg_block_count: row.get(9)?,
                    feature_cfg_edge_count: row.get(10)?,
                    feature_conditional_density: row.get(11)?,
                    feature_lifetime: row.get(12)?,
                    feature_churn_count: row.get(13)?,
                    scorer_version: row.get(14)?,
                    scored_at: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("query candidates: {}", e))?;

        Ok(results)
    }

    /// Query candidates with filters
    pub fn query_candidates(&self, filters: &ScoreFilters) -> Result<Vec<SymbolScore>> {
        let conn = self.db.lock();

        let mut query = String::from(
            "SELECT
                symbol_id, snapshot_id, stable_id, score, rank,
                feature_loc, feature_fan_in, feature_fan_out, feature_complexity,
                feature_cfg_block_count, feature_cfg_edge_count, feature_conditional_density,
                feature_lifetime, feature_churn_count,
                scorer_version, scored_at
             FROM symbol_scores
             WHERE 1=1",
        );

        let mut param_count = 0;
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(min_score) = filters.min_score {
            param_count += 1;
            query.push_str(&format!(" AND score >= ?{param_count}"));
            params.push(Box::new(min_score));
        }

        if let Some(min_churn) = filters.min_churn {
            param_count += 1;
            query.push_str(&format!(" AND feature_churn_count >= ?{param_count}"));
            params.push(Box::new(min_churn));
        }

        if let Some(min_complexity) = filters.min_complexity {
            param_count += 1;
            query.push_str(&format!(" AND feature_complexity >= ?{param_count}"));
            params.push(Box::new(min_complexity));
        }

        if let Some(min_lifetime) = filters.min_lifetime {
            param_count += 1;
            query.push_str(&format!(" AND feature_lifetime >= ?{param_count}"));
            params.push(Box::new(min_lifetime));
        }

        param_count += 1;
        let limit = filters.limit.unwrap_or(100) as i64;
        query.push_str(&format!(" ORDER BY score DESC LIMIT ?{param_count}"));
        params.push(Box::new(limit));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&query)?;

        let results = stmt
            .query_map(&*param_refs, |row| {
                Ok(SymbolScore {
                    symbol_id: row.get(0)?,
                    snapshot_id: row.get(1)?,
                    stable_id: row.get(2)?,
                    score: row.get(3)?,
                    rank: row.get(4)?,
                    feature_loc: row.get(5)?,
                    feature_fan_in: row.get(6)?,
                    feature_fan_out: row.get(7)?,
                    feature_complexity: row.get(8)?,
                    feature_cfg_block_count: row.get(9)?,
                    feature_cfg_edge_count: row.get(10)?,
                    feature_conditional_density: row.get(11)?,
                    feature_lifetime: row.get(12)?,
                    feature_churn_count: row.get(13)?,
                    scorer_version: row.get(14)?,
                    scored_at: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("query candidates: {}", e))?;

        Ok(results)
    }

    /// Get current timestamp (seconds since Unix epoch)
    fn now_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_filters_default() {
        let filters = ScoreFilters::default();
        assert!(filters.min_score.is_none());
        assert!(filters.min_churn.is_none());
        assert_eq!(filters.limit, None);
    }

    #[test]
    fn test_score_filters_with_values() {
        let filters = ScoreFilters {
            min_score: Some(0.5),
            min_churn: Some(3),
            min_complexity: Some(5),
            min_lifetime: Some(10),
            limit: Some(20),
        };

        assert_eq!(filters.min_score, Some(0.5));
        assert_eq!(filters.min_churn, Some(3));
        assert_eq!(filters.min_complexity, Some(5));
        assert_eq!(filters.min_lifetime, Some(10));
        assert_eq!(filters.limit, Some(20));
    }
}

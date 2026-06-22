//! Feature extraction for symbol scoring
//!
//! Extracts 13 features per symbol from existing tables:
//! - Static features from symbol_metrics
//! - CFG features from cfg_blocks/cfg_edges
//! - Temporal features from symbol_versions
//! - Derived features (conditional_density, etc.)

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension};
use std::sync::Arc;

/// Feature set for a single symbol
///
/// Contains all 13 features used by the scoring model.
#[derive(Debug, Clone)]
pub struct SymbolFeatures {
    /// Symbol ID (links to graph_entities)
    pub symbol_id: i64,

    /// Stable temporal identifier
    pub stable_id: String,

    /// Most recent snapshot for this symbol
    pub snapshot_id: i64,

    // Static features (from symbol_metrics)
    /// Lines of code
    pub loc: i64,

    /// Incoming reference/call edges
    pub fan_in: i64,

    /// Outgoing call edges
    pub fan_out: i64,

    /// Cyclomatic complexity
    pub complexity: i64,

    // CFG features (from cfg_blocks)
    /// Number of basic blocks in CFG
    pub cfg_block_count: i64,

    /// Number of edges in CFG
    pub cfg_edge_count: i64,

    /// Derived: conditional_blocks / total_blocks
    pub conditional_density: f64,

    // Temporal features (from symbol_versions)
    /// Number of snapshots where this symbol appears
    pub lifetime: i64,

    /// Number of versions (edits) this symbol has had
    pub churn_count: i64,
}

impl SymbolFeatures {
    /// Get a feature value by name (for scoring model)
    pub fn get(&self, name: &str) -> Option<f64> {
        match name {
            "loc" => Some(self.loc as f64),
            "fan_in" => Some(self.fan_in as f64),
            "fan_out" => Some(self.fan_out as f64),
            "complexity" => Some(self.complexity as f64),
            "cfg_block_count" => Some(self.cfg_block_count as f64),
            "cfg_edge_count" => Some(self.cfg_edge_count as f64),
            "conditional_density" => Some(self.conditional_density),
            "lifetime" => Some(self.lifetime as f64),
            "churn_count" => Some(self.churn_count as f64),
            _ => None,
        }
    }
}

/// Feature extraction engine
///
/// Queries database tables to compute features for symbols.
pub struct FeatureExtractor {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl FeatureExtractor {
    /// Create a new feature extractor
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    /// Extract features for a single symbol
    pub fn extract_for_symbol(&self, symbol_id: i64) -> Result<SymbolFeatures> {
        let conn = self.db.lock();

        // Get static features from symbol_metrics
        let (loc, fan_in, fan_out, complexity, stable_id) = conn
            .query_row(
                "SELECT loc, fan_in, fan_out, cyclomatic_complexity, symbol_name
                 FROM symbol_metrics
                 WHERE symbol_id = ?1",
                params![symbol_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("Symbol {} not found in symbol_metrics", symbol_id))?;

        // Get CFG block count
        let cfg_block_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cfg_blocks WHERE function_id = ?1",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Get CFG edge count
        let cfg_edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cfg_edges WHERE function_id = ?1",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count conditional blocks (if, match, while, etc.)
        let conditional_count: i64 = if cfg_block_count > 0 {
            conn.query_row(
                "SELECT COUNT(*) FROM cfg_blocks
                 WHERE function_id = ?1
                 AND kind IN ('if', 'match', 'while', 'for', 'loop')",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0)
        } else {
            0
        };

        // Derived: conditional density
        let conditional_density = if cfg_block_count > 0 {
            conditional_count as f64 / cfg_block_count as f64
        } else {
            0.0
        };

        // Get temporal features from symbol_versions
        // Use most recent snapshot_id for this symbol to satisfy FK constraint
        let (snapshot_id, lifetime, churn_count) = conn
            .query_row(
                "SELECT MAX(snapshot_id), COUNT(DISTINCT snapshot_id), COUNT(*)
                 FROM symbol_versions
                 WHERE name = ?1
                 GROUP BY name",
                params![stable_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or((0, 0, 0));

        // If no temporal data exists, use most recent repo snapshot (avoid FK violation)
        let snapshot_id = if snapshot_id == 0 {
            conn.query_row("SELECT MAX(id) FROM repo_snapshots", [], |row| row.get(0))
                .unwrap_or(0)
        } else {
            snapshot_id
        };

        drop(conn); // Release lock before returning

        Ok(SymbolFeatures {
            symbol_id,
            stable_id,
            snapshot_id,
            loc,
            fan_in,
            fan_out,
            complexity,
            cfg_block_count,
            cfg_edge_count,
            conditional_density,
            lifetime,
            churn_count,
        })
    }

    /// Extract features for multiple symbols in batch
    pub fn extract_batch(&self, symbol_ids: &[i64]) -> Result<Vec<SymbolFeatures>> {
        let mut results = Vec::with_capacity(symbol_ids.len());

        for &symbol_id in symbol_ids {
            match self.extract_for_symbol(symbol_id) {
                Ok(features) => results.push(features),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to extract features for symbol {}: {}",
                        symbol_id, e
                    );
                }
            }
        }

        Ok(results)
    }

    /// Extract features for all symbols in symbol_metrics
    pub fn extract_all(&self) -> Result<Vec<SymbolFeatures>> {
        let conn = self.db.lock();

        // Only select symbol_ids that exist in graph_entities (foreign key constraint)
        let symbol_ids: Vec<i64> = conn
            .prepare(
                "SELECT sm.symbol_id FROM symbol_metrics sm
                     INNER JOIN graph_entities ge ON sm.symbol_id = ge.id",
            )?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        drop(conn); // Release lock before batch extraction

        self.extract_batch(&symbol_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_features_get() {
        let features = SymbolFeatures {
            symbol_id: 1,
            stable_id: "test::symbol".to_string(),
            snapshot_id: 100,
            loc: 100,
            fan_in: 5,
            fan_out: 3,
            complexity: 8,
            cfg_block_count: 10,
            cfg_edge_count: 15,
            conditional_density: 0.4,
            lifetime: 50,
            churn_count: 3,
        };

        assert_eq!(features.get("loc"), Some(100.0));
        assert_eq!(features.get("fan_in"), Some(5.0));
        assert_eq!(features.get("conditional_density"), Some(0.4));
        assert_eq!(features.get("unknown"), None);
    }
}

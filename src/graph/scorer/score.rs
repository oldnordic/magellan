//! Scoring model for symbol ranking
//!
//! Implements weighted linear model (v1_ast_baseline).
//! Future: GNN model inference with same feature interface.

use std::collections::HashMap;
use super::extract::SymbolFeatures;

/// Scoring model
///
/// Computes scores from feature sets using weighted linear combination.
/// Currently implements AST baseline (v1). Future: GNN inference.
pub struct Scorer {
    feature_weights: HashMap<String, f64>,
    version: String,
}

impl Scorer {
    /// Create AST baseline scorer (v1)
    ///
    /// Weights based on heuristic importance:
    /// - High weight: complexity (0.25), fan_in (0.2), churn (0.2)
    /// - Medium weight: fan_out (0.15), conditional_density (0.15)
    /// - Low weight: loc, cfg counts (0.1 each)
    /// - Negative weight: lifetime (-0.05, stable code = less interesting)
    pub fn new_ast_baseline() -> Self {
        let mut weights = HashMap::new();
        weights.insert("loc".to_string(), 0.1);
        weights.insert("fan_in".to_string(), 0.2);
        weights.insert("fan_out".to_string(), 0.15);
        weights.insert("complexity".to_string(), 0.25);
        weights.insert("cfg_block_count".to_string(), 0.1);
        weights.insert("cfg_edge_count".to_string(), 0.1);
        weights.insert("conditional_density".to_string(), 0.15);
        weights.insert("lifetime".to_string(), -0.05);
        weights.insert("churn_count".to_string(), 0.2);

        Self {
            feature_weights: weights,
            version: "v1_ast_baseline".to_string(),
        }
    }

    /// Create scorer with custom weights
    pub fn with_weights(mut weights: HashMap<String, f64>) -> Self {
        // Ensure required features exist
        let required = vec![
            "loc", "fan_in", "fan_out", "complexity",
            "cfg_block_count", "cfg_edge_count", "conditional_density",
            "lifetime", "churn_count",
        ];

        for feature in required {
            if !weights.contains_key(feature) {
                weights.insert(feature.to_string(), 0.0);
            }
        }

        Self {
            feature_weights: weights,
            version: "v1_custom".to_string(),
        }
    }

    /// Compute score for a single symbol
    ///
    /// Score = sum(weight * feature_value)
    pub fn score(&self, features: &SymbolFeatures) -> f64 {
        self.feature_weights
            .iter()
            .map(|(name, weight)| features.get(name).unwrap_or(0.0) * weight)
            .sum()
    }

    /// Compute scores for multiple symbols
    pub fn score_batch(&self, batch: &[SymbolFeatures]) -> Vec<f64> {
        batch.iter().map(|f| self.score(f)).collect()
    }

    /// Get scorer version identifier
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get feature weight (for inspection/debugging)
    pub fn get_weight(&self, feature: &str) -> Option<f64> {
        self.feature_weights.get(feature).copied()
    }

    /// Get all feature weights (for export/inspection)
    pub fn weights(&self) -> &HashMap<String, f64> {
        &self.feature_weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_baseline_creation() {
        let scorer = Scorer::new_ast_baseline();
        assert_eq!(scorer.version(), "v1_ast_baseline");
        assert!(scorer.get_weight("complexity").is_some());
        assert!(scorer.get_weight("unknown").is_none());
    }

    #[test]
    fn test_score_computation() {
        let scorer = Scorer::new_ast_baseline();
        let features = SymbolFeatures {
            symbol_id: 1,
            stable_id: "test".to_string(),
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

        let score = scorer.score(&features);

        // Score should be non-zero for typical input
        assert!(score > 0.0);

        // Score computation is deterministic
        let score2 = scorer.score(&features);
        assert_eq!(score, score2);
    }

    #[test]
    fn test_batch_scoring() {
        let scorer = Scorer::new_ast_baseline();
        let features = vec![
            SymbolFeatures {
                symbol_id: 1,
                stable_id: "test1".to_string(),
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
            },
            SymbolFeatures {
                symbol_id: 2,
                stable_id: "test2".to_string(),
                snapshot_id: 100,
                loc: 50,
                fan_in: 2,
                fan_out: 1,
                complexity: 4,
                cfg_block_count: 5,
                cfg_edge_count: 7,
                conditional_density: 0.3,
                lifetime: 30,
                churn_count: 1,
            },
        ];

        let scores = scorer.score_batch(&features);
        assert_eq!(scores.len(), 2);

        // First symbol should score higher (more complex, more churn)
        assert!(scores[0] > scores[1]);
    }

    #[test]
    fn test_custom_weights() {
        let mut weights = HashMap::new();
        weights.insert("complexity".to_string(), 1.0);
        weights.insert("loc".to_string(), 0.5);

        let scorer = Scorer::with_weights(weights);
        assert_eq!(scorer.version(), "v1_custom");
        assert_eq!(scorer.get_weight("complexity"), Some(1.0));
        assert_eq!(scorer.get_weight("loc"), Some(0.5));

        // Missing features should default to 0.0
        assert_eq!(scorer.get_weight("fan_in"), Some(0.0));
    }
}

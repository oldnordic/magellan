//! Symbol scorer for ranking candidates
//!
//! Computes scores for symbols based on static, CFG, and temporal features.
//! Enables fast candidate selection for optimization, review, or vulnerability analysis.

pub mod extract;
pub mod ops;
pub mod schema;
pub mod score;

pub use extract::{FeatureExtractor, SymbolFeatures};
pub use ops::{ScorerOps, ScorerRunSummary, ScoreFilters};
pub use schema::{SymbolScore, ScorerFeature, ScorerRun};
pub use score::Scorer;

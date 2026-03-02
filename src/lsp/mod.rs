//! LSP CLI Analyzer module
//!
//! Uses language server tools (rust-analyzer, jdtls, clangd) as CLI data sources
//! to enrich Magellan symbols with type signatures and documentation.
//!
//! Unlike LSP server mode, this approach:
//! - Runs LSP tools as CLI commands (like Splice does)
//! - Parses their output for type information
//! - Stores enriched data in magellan.db
//! - Provides better LLM context without LSP complexity

pub mod analyzer;
pub mod enrich;

pub use analyzer::{AnalyzerKind, AnalyzerResult, detect_available_analyzers};
pub use enrich::{enrich_symbols, EnrichConfig, EnrichResult};

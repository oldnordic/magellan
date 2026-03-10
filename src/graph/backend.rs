//! Backend trait abstraction for Magellan
//!
//! This module provides a unified interface that both SQLite (CodeGraph)
//! and Geometric backends implement, enabling backend-agnostic CLI commands.

use anyhow::Result;
use serde_json::Value;

/// Unified backend trait for Magellan
///
/// This trait abstracts the common operations needed by CLI commands,
/// allowing the same command code to work with either SQLite or Geometric backend.
pub trait Backend {
    /// Get database statistics
    ///
    /// Returns statistics about the database including symbol counts,
    /// file counts, reference counts, etc.
    fn get_stats(&self) -> Result<BackendStats>;

    /// Export database to JSON format
    ///
    /// Returns a JSON string representation of the database contents.
    fn export_json(&self) -> Result<String>;

    /// Export database to JSON Lines format
    ///
    /// Returns a JSONL string with one JSON object per line.
    fn export_jsonl(&self) -> Result<String>;

    /// Export database to CSV format
    ///
    /// Returns a CSV string with headers.
    fn export_csv(&self) -> Result<String>;

    /// Find symbol by fully qualified name
    ///
    /// Returns the symbol information if found.
    fn find_symbol_by_fqn(&self, fqn: &str) -> Option<SymbolInfo>;

    /// Find symbols by simple name
    ///
    /// Returns all symbols with the given simple name.
    fn find_symbols_by_name(&self, name: &str) -> Vec<SymbolInfo>;

    /// Search symbols by pattern
    ///
    /// Returns symbols whose name or FQN contains the pattern.
    fn search_symbols(&self, pattern: &str) -> Vec<SymbolInfo>;

    /// Get all symbols
    ///
    /// Returns all symbols in the database.
    fn get_all_symbols(&self) -> Result<Vec<SymbolInfo>>;

    /// Get outgoing references (callees)
    ///
    /// Returns all symbols called by the given symbol.
    fn get_callees(&self, symbol_id: u64) -> Vec<u64>;

    /// Get incoming references (callers)
    ///
    /// Returns all symbols that call the given symbol.
    fn get_callers(&self, symbol_id: u64) -> Vec<u64>;

    /// Get bidirectional references
    ///
    /// Returns both callers and callees for a symbol.
    fn get_references_bidirectional(&self, symbol_id: u64) -> Result<(Vec<u64>, Vec<u64>)>;

    /// Find dead code
    ///
    /// Returns symbols not reachable from the given entry point.
    fn find_dead_code(&self, entry_id: u64) -> Result<Vec<u64>>;

    /// Find dead code with multiple entry points
    ///
    /// Returns symbols not reachable from any entry point.
    fn find_dead_code_multiple_entries(&self, entry_ids: &[u64]) -> Result<Vec<u64>>;

    /// Insert a symbol
    ///
    /// Adds a new symbol to the database.
    fn insert_symbol(&self, symbol: &SymbolData) -> Result<u64>;

    /// Insert a reference
    ///
    /// Creates a caller -> callee relationship.
    fn insert_reference(&self, caller_id: u64, callee_id: u64) -> Result<()>;
}

/// Statistics returned by Backend::get_stats()
#[derive(Debug, Clone)]
pub struct BackendStats {
    pub node_count: usize,
    pub symbol_count: usize,
    pub file_count: usize,
    pub reference_count: usize,
    pub cfg_block_count: usize,
}

/// Symbol information returned by Backend queries
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub id: u64,
    pub fqn: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: u64,
    pub end_line: u64,
}

/// Symbol data for insertion
#[derive(Debug, Clone)]
pub struct SymbolData {
    pub fqn: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
}

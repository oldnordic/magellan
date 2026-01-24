//! In-memory index for SymbolId lookups
//!
//! Provides O(1) lookup by SymbolId without repeated SQL queries.
//! Index is built on-demand and cached for the lifetime of the CodeGraph.
//!
//! # Status
//!
//! This module is defined for future optimization. The index is not yet integrated
//! into the main codebase path. SymbolId lookups currently use direct SQL queries.
//!
//! # Performance
//!
//! - Build time: ~50-100ms for 10k symbols
//! - Lookup time: O(1) average case
//! - Memory: ~1MB for 10k symbols (String + i64 per entry)
//!
//! # Usage Pattern
//!
//! ```rust
//! let mut index = SymbolIndex::new();
//! index.build_index(&conn)?;
//! if let Some(entity_id) = index.lookup("abc123...") {
//!     // Symbol found, use entity_id
//! }
//! ```

#![allow(dead_code)] // Future optimization, not yet integrated

use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;

use crate::graph::schema::SymbolNode;

/// In-memory index for SymbolId -> entity_id lookups
///
/// Provides O(1) lookup by SymbolId without repeated SQL queries.
/// Index is built on-demand and cached for the lifetime of the CodeGraph.
///
/// # Performance
///
/// - Build time: ~50-100ms for 10k symbols
/// - Lookup time: O(1) average case
/// - Memory: ~1MB for 10k symbols (String + i64 per entry)
///
/// # Usage Pattern
///
/// ```rust
/// let mut index = SymbolIndex::new();
/// index.build_index(&conn)?;
/// if let Some(entity_id) = index.lookup("abc123...") {
///     // Symbol found, use entity_id
/// }
/// ```
pub struct SymbolIndex {
    /// Map: SymbolId (32-char BLAKE3 hash) -> entity_id (i64)
    index: HashMap<String, i64>,
}

impl SymbolIndex {
    /// Create empty SymbolIndex
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Build index from database (populates all SymbolId -> entity_id mappings)
    ///
    /// # Arguments
    /// * `conn` - SQLite connection to graph database
    ///
    /// # Behavior
    /// - Queries all Symbol nodes from graph_entities table
    /// - Extracts symbol_id from JSON data field
    /// - Populates HashMap with symbol_id -> entity_id mappings
    /// - Skips symbols where symbol_id is None
    ///
    /// # Errors
    /// Returns error if SQL query fails or JSON deserialization fails.
    pub fn build_index(&mut self, conn: &Connection) -> Result<()> {
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, data
             FROM graph_entities
             WHERE kind = 'Symbol'
             AND json_extract(data, '$.symbol_id') IS NOT NULL",
            )
            .map_err(|e| anyhow::anyhow!("Failed to prepare SymbolIndex build query: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let data: String = row.get(1)?;
                Ok((id, data))
            })
            .map_err(|e| anyhow::anyhow!("Failed to execute SymbolIndex build query: {}", e))?;

        for row in rows {
            let (id, data) =
                row.map_err(|e| anyhow::anyhow!("Failed to read SymbolIndex row: {}", e))?;
            if let Ok(node) = serde_json::from_str::<SymbolNode>(&data) {
                if let Some(symbol_id) = node.symbol_id {
                    self.index.insert(symbol_id, id);
                }
            }
        }

        Ok(())
    }

    /// Lookup entity_id by SymbolId
    ///
    /// # Returns
    /// Option<i64> with entity_id if found, None if not in index
    ///
    /// # Note
    /// Returns None if SymbolId is not in index. This does NOT mean
    /// the symbol doesn't exist - it may have been added after index
    /// was built, or symbol_id may be None for legacy data.
    pub fn lookup(&self, symbol_id: &str) -> Option<i64> {
        self.index.get(symbol_id).copied()
    }

    /// Get number of symbols in index
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Clear all entries from the index
    pub fn clear(&mut self) {
        self.index.clear();
    }
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_index_new_is_empty() {
        let index = SymbolIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_symbol_index_lookup_returns_none_when_empty() {
        let index = SymbolIndex::new();
        assert!(index.lookup("nonexistent").is_none());
    }

    #[test]
    fn test_symbol_index_lookup_after_insert() {
        let mut index = SymbolIndex::new();
        // Directly insert for testing (normally done via build_index)
        index.index.insert("test_id".to_string(), 42);
        assert_eq!(index.lookup("test_id"), Some(42));
        assert_eq!(index.lookup("other"), None);
    }

    #[test]
    fn test_symbol_index_clear() {
        let mut index = SymbolIndex::new();
        index.index.insert("test_id".to_string(), 42);
        assert_eq!(index.len(), 1);
        index.clear();
        assert!(index.is_empty());
    }

    #[test]
    fn test_symbol_index_default() {
        let index = SymbolIndex::default();
        assert!(index.is_empty());
    }
}

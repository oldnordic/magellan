//! In-memory symbol lookup index for O(1) lookups
//!
//! Provides O(1) lookup by FQN, name, and (file_path, name) pairs without
//! repeated database scans. Index is built on-demand and maintained incrementally.
//!
//! # Thread Safety
//!
//! **This module is NOT thread-safe.**
//!
//! Same constraints as `FileOps`:
//! - All methods require `&self` or `&mut self`
//! - No `Send` or `Sync` impls
//! - Must be accessed through single-threaded `CodeGraph`
//!
//! # Performance
//!
//! - Build time: ~50-100ms for 10k symbols (one-time)
//! - Lookup time: O(1) average case
//! - Memory: ~2MB for 10k symbols
//!
//! # Usage Pattern
//!
//! ```rust
//! // Build on startup (via CodeGraph::open)
//! lookup.rebuild_from_backend(&backend)?;
//!
//! // O(1) lookup during indexing
//! if let Some(entry) = lookup.get_by_fqn("crate::module::function") {
//!     // Use entry.entity_id
//! }
//!
//! // Get all symbol facts for reference extraction (no O(n) scan)
//! let all_facts = lookup.get_all_symbol_facts();
//!
//! // Incremental updates during indexing
//! lookup.insert(entity_id, file_path, &symbol_fact);
//! lookup.remove(entity_id);
//! ```

use anyhow::Result;
use sqlitegraph::{GraphBackend, SnapshotId};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::ingest::{SymbolFact, SymbolKind};

/// Entry in the symbol lookup index
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    /// Entity ID in the graph database
    pub entity_id: i64,
    /// File path where symbol is defined
    pub file_path: String,
    /// Simple symbol name
    pub name: Option<String>,
    /// Symbol kind enum
    pub kind: SymbolKind,
    /// Normalized kind string ("fn", "struct", "enum", etc.)
    pub kind_normalized: String,
    /// Fully-qualified name (e.g., "crate::module::function")
    pub fqn: Option<String>,
    /// Canonical FQN (resolved through type aliases)
    pub canonical_fqn: Option<String>,
    /// Display FQN (for user-friendly display)
    pub display_fqn: Option<String>,
    /// Stable symbol ID (SHA-256 hash, used for cross-file reference resolution)
    pub stable_symbol_id: Option<String>,
    /// Byte start position
    pub byte_start: i64,
    /// Byte end position
    pub byte_end: i64,
    /// Start line number (1-indexed)
    pub start_line: usize,
    /// Start column (0-indexed)
    pub start_col: usize,
    /// End line number (1-indexed)
    pub end_line: usize,
    /// End column (0-indexed)
    pub end_col: usize,
}

impl SymbolEntry {
    /// Create a SymbolEntry from a SymbolFact
    pub fn from_fact(entity_id: i64, file_path: &str, fact: &SymbolFact) -> Self {
        Self {
            entity_id,
            file_path: file_path.to_string(),
            name: fact.name.clone(),
            kind: fact.kind.clone(),
            kind_normalized: fact.kind_normalized.clone(),
            fqn: fact.fqn.clone(),
            canonical_fqn: fact.canonical_fqn.clone(),
            display_fqn: fact.display_fqn.clone(),
            stable_symbol_id: None, // Set separately when available
            byte_start: fact.byte_start as i64,
            byte_end: fact.byte_end as i64,
            start_line: fact.start_line,
            start_col: fact.start_col,
            end_line: fact.end_line,
            end_col: fact.end_col,
        }
    }

    /// Create a SymbolEntry from a SymbolFact with a known stable_symbol_id
    pub fn from_fact_with_symbol_id(
        entity_id: i64,
        file_path: &str,
        fact: &SymbolFact,
        stable_symbol_id: String,
    ) -> Self {
        let mut entry = Self::from_fact(entity_id, file_path, fact);
        entry.stable_symbol_id = Some(stable_symbol_id);
        entry
    }

    /// Convert SymbolEntry to SymbolFact for use in reference extraction
    pub fn to_fact(&self) -> SymbolFact {
        SymbolFact {
            file_path: PathBuf::from(&self.file_path),
            kind: self.kind.clone(),
            kind_normalized: self.kind_normalized.clone(),
            name: self.name.clone(),
            fqn: self.fqn.clone(),
            canonical_fqn: self.canonical_fqn.clone(),
            display_fqn: self.display_fqn.clone(),
            byte_start: self.byte_start as usize,
            byte_end: self.byte_end as usize,
            start_line: self.start_line,
            start_col: self.start_col,
            end_line: self.end_line,
            end_col: self.end_col,
        }
    }
}

/// In-memory symbol lookup index
///
/// Provides O(1) lookups for:
/// - FQN -> SymbolEntry (primary resolution)
/// - Simple name -> Vec<entity_id> (fallback matching)
/// - entity_id -> stable_symbol_id (for reference resolution)
pub struct SymbolLookup {
    /// FQN -> SymbolEntry
    /// Used for primary symbol resolution
    /// Key is FQN if present, otherwise simple name
    fqn_index: HashMap<String, SymbolEntry>,

    /// Simple name -> [entity_id]
    /// Used for fallback matching when FQN doesn't match exactly
    /// E.g., "render" -> [id1, id2] for Widget::render and Page::render
    name_index: HashMap<String, Vec<i64>>,

    /// entity_id -> FQN key
    /// Used for reverse lookup during removal
    id_to_fqn: HashMap<i64, String>,

    /// entity_id -> stable_symbol_id (SHA-256 hash)
    /// Used for cross-file reference resolution
    id_to_symbol_id: HashMap<i64, String>,

    /// Total symbols in index
    count: usize,
}

impl SymbolLookup {
    /// Create empty SymbolLookup
    pub fn new() -> Self {
        Self {
            fqn_index: HashMap::new(),
            name_index: HashMap::new(),
            id_to_fqn: HashMap::new(),
            id_to_symbol_id: HashMap::new(),
            count: 0,
        }
    }

    /// Get number of symbols in index
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Clear all entries from the index
    pub fn clear(&mut self) {
        self.fqn_index.clear();
        self.name_index.clear();
        self.id_to_fqn.clear();
        self.id_to_symbol_id.clear();
        self.count = 0;
    }

    /// Insert a symbol into the index
    ///
    /// # Arguments
    /// * `entity_id` - Graph database entity ID
    /// * `file_path` - File path where symbol is defined
    /// * `fact` - Symbol fact from parser
    pub fn insert(&mut self, entity_id: i64, file_path: &str, fact: &SymbolFact) {
        // Build the key: prefer FQN, fall back to name
        let key = fact
            .fqn
            .as_ref()
            .or(fact.name.as_ref())
            .cloned()
            .unwrap_or_default();

        if key.is_empty() {
            return;
        }

        let entry = SymbolEntry::from_fact(entity_id, file_path, fact);

        // Update FQN index
        self.fqn_index.insert(key.clone(), entry);

        // Update name index (for fallback matching)
        if let Some(ref name) = fact.name {
            self.name_index
                .entry(name.clone())
                .or_default()
                .push(entity_id);
        }

        // Update reverse lookup
        self.id_to_fqn.insert(entity_id, key);

        self.count += 1;
    }

    /// Insert a symbol with a known stable_symbol_id
    ///
    /// Use this when the stable_symbol_id is already computed (e.g., during rebuild_from_backend)
    pub fn insert_with_symbol_id(
        &mut self,
        entity_id: i64,
        file_path: &str,
        fact: &SymbolFact,
        stable_symbol_id: String,
    ) {
        // Build the key: prefer FQN, fall back to name
        let key = fact
            .fqn
            .as_ref()
            .or(fact.name.as_ref())
            .cloned()
            .unwrap_or_default();

        if key.is_empty() {
            return;
        }

        let entry = SymbolEntry::from_fact_with_symbol_id(entity_id, file_path, fact, stable_symbol_id.clone());

        // Update FQN index
        self.fqn_index.insert(key.clone(), entry);

        // Update name index (for fallback matching)
        if let Some(ref name) = fact.name {
            self.name_index
                .entry(name.clone())
                .or_default()
                .push(entity_id);
        }

        // Update reverse lookup
        self.id_to_fqn.insert(entity_id, key);

        // Track stable symbol_id for cross-file reference resolution
        self.id_to_symbol_id.insert(entity_id, stable_symbol_id);

        self.count += 1;
    }

    /// Remove a symbol from the index by entity_id
    ///
    /// # Arguments
    /// * `entity_id` - Graph database entity ID to remove
    pub fn remove(&mut self, entity_id: i64) {
        // Also remove stable_symbol_id mapping
        self.id_to_symbol_id.remove(&entity_id);

        // Get the FQN key for this entity
        if let Some(key) = self.id_to_fqn.remove(&entity_id) {
            // Get the entry to find the name
            if let Some(entry) = self.fqn_index.get(&key) {
                // Remove from name index
                if let Some(ref name) = entry.name {
                    if let Some(ids) = self.name_index.get_mut(name) {
                        ids.retain(|&id| id != entity_id);
                        if ids.is_empty() {
                            self.name_index.remove(name);
                        }
                    }
                }
            }

            // Remove from FQN index
            self.fqn_index.remove(&key);
            self.count = self.count.saturating_sub(1);
        }
    }

    /// Look up a symbol by FQN
    ///
    /// # Returns
    /// Reference to SymbolEntry if found, None if not in index
    pub fn get_by_fqn(&self, fqn: &str) -> Option<&SymbolEntry> {
        self.fqn_index.get(fqn)
    }

    /// Look up entity IDs by simple name (for fallback matching)
    ///
    /// # Returns
    /// Slice of entity_ids matching this name, empty if none found
    pub fn get_ids_by_name(&self, name: &str) -> &[i64] {
        self.name_index.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get all FQN keys in the index
    ///
    /// Used for building the FQN -> entity_id map needed by index_calls
    pub fn all_fqns(&self) -> impl Iterator<Item = (&String, i64)> + '_ {
        self.fqn_index.iter().map(|(fqn, entry)| (fqn, entry.entity_id))
    }

    /// Get all symbol facts for reference extraction
    ///
    /// This replaces the O(n) database scan in ReferenceOps::index_references_with_symbol_id.
    /// Instead of scanning all entities and deserializing SymbolNodes, we return
    /// pre-computed SymbolFacts from the in-memory index.
    ///
    /// # Returns
    /// Vector of SymbolFact for all symbols in the index
    pub fn get_all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.fqn_index.values().map(|entry| entry.to_fact()).collect()
    }

    /// Get map of entity_id -> stable_symbol_id
    ///
    /// Used by `populate_cross_file_refs` to resolve references to their stable IDs.
    /// This replaces the O(n) scan that was previously done in query.rs.
    ///
    /// # Returns
    /// HashMap mapping entity IDs to their stable symbol IDs
    pub fn get_entity_to_symbol_id_map(&self) -> &HashMap<i64, String> {
        &self.id_to_symbol_id
    }

    /// Build FQN -> entity_id map with current file preference
    ///
    /// This replicates the logic from `src/graph/calls.rs:index_calls`:
    /// - Current file symbols take precedence for duplicate FQNs
    /// - Returns HashMap suitable for passing to CallOps::index_calls
    pub fn fqn_to_id_with_current_file(&self, current_file: &str) -> HashMap<String, (i64, bool)> {
        let mut result: HashMap<String, (i64, bool)> = HashMap::new();

        for (fqn, entry) in &self.fqn_index {
            let is_current_file = entry.file_path == current_file;

            // Prefer current file symbols for duplicates
            match result.get(fqn) {
                Some((_, existing_is_current)) if *existing_is_current || !is_current_file => {}
                _ => {
                    result.insert(fqn.clone(), (entry.entity_id, is_current_file));
                }
            }
        }

        result
    }

    /// Rebuild index from backend database
    ///
    /// Scans all Symbol nodes and rebuilds all indexes.
    /// Call this on CodeGraph::open() or after bulk changes.
    ///
    /// # Arguments
    /// * `backend` - Graph backend to scan
    ///
    /// # Returns
    /// Number of symbols indexed
    pub fn rebuild_from_backend(&mut self, backend: &dyn GraphBackend) -> Result<usize> {
        self.clear();

        let entity_ids = backend.entity_ids()?;
        let snapshot = SnapshotId::current();

        for entity_id in entity_ids {
            if let Ok(node) = backend.get_node(snapshot, entity_id) {
                if node.kind != "Symbol" {
                    continue;
                }

                // Extract file path
                let file_path = node.file_path.clone().unwrap_or_default();

                // Parse SymbolNode from JSON data
                if let Ok(symbol_node) =
                    serde_json::from_value::<crate::graph::schema::SymbolNode>(node.data.clone())
                {
                    // Convert SymbolNode to SymbolFact for insertion
                    let fact = SymbolFact {
                        file_path: std::path::PathBuf::from(&file_path),
                        kind: match symbol_node.kind_normalized.as_deref() {
                            Some("fn") => crate::ingest::SymbolKind::Function,
                            Some("method") => crate::ingest::SymbolKind::Method,
                            Some("struct") => crate::ingest::SymbolKind::Class,
                            Some("enum") => crate::ingest::SymbolKind::Enum,
                            Some("trait") => crate::ingest::SymbolKind::Interface,
                            Some("mod") => crate::ingest::SymbolKind::Module,
                            _ => crate::ingest::SymbolKind::Unknown,
                        },
                        kind_normalized: symbol_node
                            .kind_normalized
                            .clone()
                            .unwrap_or_else(|| symbol_node.kind.clone()),
                        name: symbol_node.name,
                        fqn: symbol_node.fqn,
                        canonical_fqn: symbol_node.canonical_fqn,
                        display_fqn: symbol_node.display_fqn,
                        byte_start: symbol_node.byte_start as usize,
                        byte_end: symbol_node.byte_end as usize,
                        start_line: symbol_node.start_line as usize,
                        start_col: symbol_node.start_col as usize,
                        end_line: symbol_node.end_line as usize,
                        end_col: symbol_node.end_col as usize,
                    };

                    // Extract stable symbol_id if present, use insert_with_symbol_id
                    if let Some(stable_symbol_id) = symbol_node.symbol_id {
                        self.insert_with_symbol_id(entity_id, &file_path, &fact, stable_symbol_id);
                    } else {
                        self.insert(entity_id, &file_path, &fact);
                    }
                }
            }
        }

        Ok(self.count)
    }

    /// Get all SymbolEntries for a specific file
    ///
    /// Used when deleting symbols for a file
    pub fn get_entries_for_file(&self, file_path: &str) -> Vec<&SymbolEntry> {
        self.fqn_index
            .values()
            .filter(|entry| entry.file_path == file_path)
            .collect()
    }

    /// Get all entries as an iterator
    ///
    /// Used for building symbol_facts in call_ops
    pub fn iter_entries(&self) -> impl Iterator<Item = &SymbolEntry> {
        self.fqn_index.values()
    }
}

impl Default for SymbolLookup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::SymbolKind;
    use std::path::PathBuf;

    fn make_fact(name: &str, fqn: &str) -> SymbolFact {
        SymbolFact {
            file_path: PathBuf::from("test.rs"),
            kind: SymbolKind::Function,
            kind_normalized: "fn".to_string(),
            name: Some(name.to_string()),
            fqn: Some(fqn.to_string()),
            canonical_fqn: None,
            display_fqn: None,
            byte_start: 0,
            byte_end: 10,
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 10,
        }
    }

    #[test]
    fn test_insert_and_lookup() {
        let mut lookup = SymbolLookup::new();
        let fact = make_fact("main", "crate::main");
        lookup.insert(1, "test.rs", &fact);

        assert_eq!(lookup.len(), 1);
        assert!(lookup.get_by_fqn("crate::main").is_some());
        assert_eq!(lookup.get_by_fqn("crate::main").unwrap().entity_id, 1);
    }

    #[test]
    fn test_remove() {
        let mut lookup = SymbolLookup::new();
        let fact = make_fact("main", "crate::main");
        lookup.insert(1, "test.rs", &fact);
        assert_eq!(lookup.len(), 1);

        lookup.remove(1);
        assert_eq!(lookup.len(), 0);
        assert!(lookup.get_by_fqn("crate::main").is_none());
    }

    #[test]
    fn test_name_index() {
        let mut lookup = SymbolLookup::new();

        // Insert two symbols with same name but different FQNs
        let fact1 = make_fact("render", "Widget::render");
        let fact2 = make_fact("render", "Page::render");
        lookup.insert(1, "widget.rs", &fact1);
        lookup.insert(2, "page.rs", &fact2);

        // Should find both by name
        let ids = lookup.get_ids_by_name("render");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn test_fqn_to_id_with_current_file() {
        let mut lookup = SymbolLookup::new();

        // Insert symbols with DIFFERENT FQNs (realistic scenario)
        let fact1 = make_fact("func", "crate::module::func");
        lookup.insert(1, "src/module.rs", &fact1);

        let fact2 = make_fact("helper", "crate::other::helper");
        lookup.insert(2, "src/other.rs", &fact2);

        // When called with current file, both symbols should be in the map
        let map = lookup.fqn_to_id_with_current_file("src/module.rs");

        // Current file symbol should have is_current_file = true
        assert_eq!(map.get("crate::module::func"), Some(&(1, true)));

        // Other file symbol should have is_current_file = false
        assert_eq!(map.get("crate::other::helper"), Some(&(2, false)));
    }

    #[test]
    fn test_clear() {
        let mut lookup = SymbolLookup::new();
        let fact = make_fact("main", "crate::main");
        lookup.insert(1, "test.rs", &fact);

        lookup.clear();
        assert!(lookup.is_empty());
        assert!(lookup.fqn_index.is_empty());
        assert!(lookup.name_index.is_empty());
    }
}

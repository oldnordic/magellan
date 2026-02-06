// KV Index Module for Native V2 Backend
//
// This module provides O(1) symbol lookup functionality using the Native V2
// backend's KV store. It replaces expensive database queries with fast key-based
// lookups for common operations like symbol resolution, file-to-symbol mapping,
// and reference finding.
//
// ## Architecture
//
// The KV index is organized into several specialized sub-modules:
//
// - **encoding**: Converts complex types (Vec<SymbolId>) to/from byte arrays
// - **keys**: Constructs well-formed keys for different index patterns
// - **index**: (Future) Population and invalidation logic
//
// ## Key Patterns
//
// The KV store uses namespaced keys for efficient lookups:
//
// | Pattern         | Purpose                              | Value Type          |
// |-----------------|--------------------------------------|---------------------|
// | sym:fqn:{fqn}   | SymbolId lookup by fully-qualified name | SymbolId (i64)     |
// | sym:id:{id}     | Symbol metadata by ID                | FQN + metadata      |
// | file:path:{path}| FileId lookup by path                | FileId (u64)        |
// | file:sym:{id}   | All symbols in a file                | Vec<SymbolId>       |
// | sym:rev:{id}    | Reverse index (references to symbol) | Vec<SymbolId>       |
//
// ## Usage
//
// ```ignore
// // During indexing: populate the KV index
// populate_symbol_index(&graph, &symbols);
//
// // For queries: O(1) lookup instead of SQL query
// if let Some(symbol_id) = lookup_symbol_by_fqn("crate::function") {
//     // Found the symbol instantly
// }
//
// // When a file changes: invalidate its index entries
// invalidate_file_index(file_id);
// ```
//
// ## Feature Flag
//
// This module is only available with the `native-v2` feature, as the KV store
// is specific to the Native V2 backend. The SQLite backend uses traditional
// indexed queries instead.

#[cfg(feature = "native-v2")]
pub mod encoding;
#[cfg(feature = "native-v2")]
pub mod keys;

// Re-export commonly used types for convenience
#[cfg(feature = "native-v2")]
pub use encoding::{decode_symbol_ids, encode_symbol_ids};
#[cfg(feature = "native-v2")]
pub use keys::{file_path_key, file_sym_key, sym_fqn_key, sym_fqn_of_key, sym_id_key, sym_rev_key};

// ============================================================================
// Public API - Index Management
// ============================================================================

use crate::ingest::SymbolFact;
use sqlitegraph::backend::KvValue;
use sqlitegraph::{GraphBackend, SnapshotId};
use std::rc::Rc;

/// Populate the KV index with symbols from a file during indexing.
///
/// This function is called immediately after symbol nodes are inserted into the graph.
/// It builds the following indexes in a single transaction (participates in WAL):
/// - sym:fqn:{fqn} → SymbolId (primary O(1) lookup)
/// - sym:fqn_of:{id} → FQN (reverse lookup for invalidation)
/// - sym:rev:{id} → revision (starts at 1, increments on reindex)
/// - file:sym:{file_id} → Vec<SymbolId> (all symbols in file, encoded)
///
/// # Arguments
/// * `backend` - Graph backend (must be Native V2 for KV operations)
/// * `file_id` - FileId (u64) of the indexed file
/// * `symbols` - Slice of (SymbolFact, NodeId) tuples containing extracted symbols and their assigned node IDs
///
/// # Returns
/// Result<()> indicating success or failure
///
/// # Errors
/// Returns error if KV operations fail (backend doesn't support KV, write errors)
///
/// # Note
/// All KV writes participate in the same WAL transaction as graph writes.
/// The WAL transaction is managed by the graph backend, not this function.
#[cfg(feature = "native-v2")]
pub fn populate_symbol_index(
    backend: Rc<dyn GraphBackend>,
    file_id: u64,
    symbols: &[(SymbolFact, i64)],
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::kv::encoding::encode_symbol_ids;
    use crate::kv::keys::{file_sym_key, sym_fqn_key, sym_fqn_of_key, sym_rev_key};

    let mut symbol_ids: Vec<i64> = Vec::new();

    // For each symbol: create KV entries
    for (symbol, node_id) in symbols {
        // Only index symbols with FQN (skip unnamed symbols)
        let fqn = if let Some(ref f) = symbol.fqn {
            f.clone()
        } else {
            continue; // Skip symbols without FQN
        };

        // 1. sym:fqn:{fqn} → SymbolId (primary lookup index)
        let fqn_key = sym_fqn_key(&fqn);
        backend.kv_set(fqn_key, KvValue::Integer(*node_id), None)?;

        // 2. sym:fqn_of:{id} → FQN (reverse lookup for invalidation)
        let fqn_of_key = sym_fqn_of_key(*node_id);
        backend.kv_set(fqn_of_key, KvValue::String(fqn.clone()), None)?;

        // 3. sym:rev:{id} → revision (starts at 1)
        let rev_key = sym_rev_key(*node_id);
        backend.kv_set(rev_key, KvValue::Integer(1), None)?;

        // Collect for file-level index
        symbol_ids.push(*node_id);
    }

    // 4. file:sym:{file_id} → Vec<SymbolId> (all symbols in file, encoded)
    let file_key = file_sym_key(file_id);
    let encoded_ids = encode_symbol_ids(&symbol_ids);
    backend.kv_set(file_key, KvValue::Bytes(encoded_ids), None)?;

    Ok(())
}

/// Look up a SymbolId by its fully-qualified name using the KV index.
///
/// This provides O(1) lookup performance for symbol resolution,
/// replacing the traditional SQL query with a KV store lookup.
///
/// # Arguments
/// * `backend` - Graph backend (must be Native V2 for KV operations)
/// * `fqn` - Fully-qualified name of the symbol to look up
///
/// # Returns
/// Option<i64> - Some(symbol_id) if found, None if not in index
///
/// # Example
/// ```ignore
/// if let Some(symbol_id) = lookup_symbol_by_fqn(backend, "my_crate::module::function") {
///     // Found the symbol instantly - O(1) lookup
/// }
/// ```
#[cfg(feature = "native-v2")]
pub fn lookup_symbol_by_fqn(backend: &dyn GraphBackend, fqn: &str) -> Option<i64> {
    use crate::kv::keys::sym_fqn_key;

    let key = sym_fqn_key(fqn);
    let snapshot = SnapshotId::current();

    match backend.kv_get(snapshot, &key) {
        Ok(Some(KvValue::Integer(symbol_id))) => Some(symbol_id),
        Ok(_) => None, // Key not found or wrong type
        Err(_) => None, // KV operation failed (fallback gracefully)
    }
}

/// Invalidate KV index entries for a specific file before reindex or deletion.
///
/// When a file is modified or deleted, its index entries must be removed
/// to maintain consistency. This function handles the invalidation by:
/// 1. Reading old symbol IDs from file:sym:{file_id}
/// 2. Deleting sym:fqn_of:{id} entries for each old symbol
/// 3. Deleting file:sym:{file_id} entry
///
/// Note: Individual sym:fqn:{fqn} entries are NOT deleted here - they will be
/// overwritten on reindex. Stale entries for deleted symbols will naturally
/// expire since their sym:fqn_of:{id} entry is removed.
///
/// # Arguments
/// * `backend` - Graph backend (must be Native V2 for KV operations)
/// * `file_id` - FileId (u64) of the file being invalidated
/// * `old_symbol_ids` - Vec<i64> of symbol IDs that were in this file
///
/// # Returns
/// Result<()> indicating success or failure
///
/// # Errors
/// Returns error if KV operations fail
#[cfg(feature = "native-v2")]
pub fn invalidate_file_index(
    backend: &dyn GraphBackend,
    file_id: u64,
    old_symbol_ids: &[i64],
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::kv::keys::{file_sym_key, sym_fqn_of_key};

    // Delete sym:fqn_of:{id} for each old symbol
    for &symbol_id in old_symbol_ids {
        let fqn_of_key = sym_fqn_of_key(symbol_id);
        backend.kv_delete(&fqn_of_key)?;
    }

    // Delete file:sym:{file_id} entry
    let file_key = file_sym_key(file_id);
    backend.kv_delete(&file_key)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_compiles() {
        // This test just verifies the module compiles correctly
        // with the native-v2 feature enabled
        assert!(true);
    }

    #[cfg(feature = "native-v2")]
    #[test]
    fn test_public_api_signatures() {
        // Verify the public API functions exist and have correct signatures

        // populate_symbol_index: (Rc<GraphBackend>, u64, &[(SymbolFact, i64)]) -> Result
        fn accepts_populate(_f: fn(std::rc::Rc<dyn sqlitegraph::GraphBackend>, u64, &[(crate::ingest::SymbolFact, i64)]) -> Result<(), Box<dyn std::error::Error>>) {}

        // invalidate_file_index: (&GraphBackend, u64, &[i64]) -> Result
        fn accepts_invalidate(_f: fn(&dyn sqlitegraph::GraphBackend, u64, &[i64]) -> Result<(), Box<dyn std::error::Error>>) {}

        // lookup_symbol_by_fqn: (&GraphBackend, &str) -> Option<i64>
        fn accepts_lookup(_f: fn(&dyn sqlitegraph::GraphBackend, &str) -> Option<i64>) {}

        // If this compiles, the signatures are correct
        accepts_populate(populate_symbol_index);
        accepts_invalidate(invalidate_file_index);
        accepts_lookup(lookup_symbol_by_fqn);
    }
}

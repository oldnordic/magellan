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
pub use keys::{file_path_key, file_sym_key, sym_fqn_key, sym_id_key, sym_rev_key};

// ============================================================================
// Public API - Index Management (Stubs for 48-02 implementation)
// ============================================================================

/// Populate the KV index with symbols from the graph.
///
/// This function scans all symbols in the graph and builds the following indexes:
/// - FQN → SymbolId mapping (for fast resolution)
/// - File → Symbols mapping (for file-scoped queries)
/// - Reverse index (for "find usages" queries)
///
/// # Arguments
/// * `graph` - Reference to the graph backend
///
/// # Returns
/// Result<()> indicating success or failure
///
/// # Note
/// This is a stub - actual implementation will be in plan 48-02.
#[cfg(feature = "native-v2")]
pub fn populate_symbol_index(_graph: &dyn sqlitegraph::GraphBackend) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement in plan 48-02
    // 1. Query all symbols from graph
    // 2. For each symbol: store sym:fqn:{fqn} → symbol_id
    // 3. For each symbol: store sym:id:{id} → fqn
    // 4. Group symbols by file: store file:sym:{file_id} → Vec<SymbolId>
    // 5. Build reverse index: store sym:rev:{symbol_id} → Vec<referencing_id>
    Ok(())
}

/// Invalidate KV index entries for a specific file.
///
/// When a file is modified or deleted, its index entries must be removed
/// to maintain consistency. This function handles the invalidation.
///
/// # Arguments
/// * `file_id` - The FileId whose entries should be invalidated
///
/// # Returns
/// Result<()> indicating success or failure
///
/// # Note
/// This is a stub - actual implementation will be in plan 48-02.
#[cfg(feature = "native-v2")]
pub fn invalidate_file_index(_file_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement in plan 48-02
    // 1. Get all symbols for this file from file:sym:{file_id}
    // 2. For each symbol: remove sym:fqn:{fqn} entry
    // 3. For each symbol: remove sym:id:{id} entry
    // 4. Remove file:sym:{file_id} entry
    // 5. Update reverse index: remove this symbol from sym:rev entries
    Ok(())
}

/// Look up a SymbolId by its fully-qualified name.
///
/// This provides O(1) lookup performance for symbol resolution,
/// replacing the traditional SQL query with a KV store lookup.
///
/// # Arguments
/// * `fqn` - Fully-qualified name of the symbol to look up
///
/// # Returns
/// Option<SymbolId> - Some(id) if found, None if not in index
///
/// # Note
/// This is a stub - actual implementation will be in plan 48-02.
#[cfg(feature = "native-v2")]
pub fn lookup_symbol_by_fqn(_fqn: &str) -> Option<i64> {
    // TODO: Implement in plan 48-02
    // 1. Construct key: sym:fqn:{fqn}
    // 2. Query KV store
    // 3. Decode and return SymbolId if present
    None
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
    fn test_public_api_stubs_exist() {
        // Verify the public API functions exist and have correct signatures
        // These are stubs - actual functionality will be added in 48-02

        // populate_symbol_index should accept GraphBackend trait object
        fn accepts_graph(_f: fn(&dyn sqlitegraph::GraphBackend) -> Result<(), Box<dyn std::error::Error>>) {}

        // invalidate_file_index should accept u64 file_id
        fn accepts_file_id(_f: fn(u64) -> Result<(), Box<dyn std::error::Error>>) {}

        // lookup_symbol_by_fqn should accept &str and return Option<i64>
        fn accepts_fqn_lookup(_f: fn(&str) -> Option<i64>) {}

        // If this compiles, the signatures are correct
        accepts_graph(populate_symbol_index);
        accepts_file_id(invalidate_file_index);
        accepts_fqn_lookup(lookup_symbol_by_fqn);
    }
}

// KV Key Construction Helpers for Native V2 Backend
//
// This module provides functions for constructing well-formed keys for the
// KV store index. Each key pattern serves a specific lookup purpose:
//
// - sym:fqn:{fqn}       → O(1) lookup of SymbolId by fully-qualified name
// - sym:id:{id}         → O(1) lookup of symbol metadata by ID
// - file:path:{path}    → O(1) lookup of FileId by file path
// - file:sym:{file_id}  → O(1) lookup of all symbols in a file
// - sym:rev:{symbol_id} → O(1) lookup of references TO a symbol (reverse index)
//
// Key design principles:
// - Namespaced prefixes (sym:, file:) prevent key collisions
// - Colon-separated format for readability and efficient prefix scans
// - Consistent encoding (Vec<u8>) for KvStore API compatibility

/// Construct a KV store key for looking up a SymbolId by fully-qualified name.
///
/// The key format is: b"sym:fqn:{fqn}"
///
/// This enables O(1) symbol lookup when you have the fully-qualified name
/// (e.g., "crate::module::function_name").
///
/// # Arguments
/// * `fqn` - Fully-qualified name of the symbol
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = sym_fqn_key("my_crate::module::function");
/// // Returns: b"sym:fqn:my_crate::module::function"
/// ```
pub fn sym_fqn_key(fqn: &str) -> Vec<u8> {
    format!("sym:fqn:{}", fqn).into_bytes()
}

/// Construct a KV store key for looking up symbol metadata by ID.
///
/// The key format is: b"sym:id:{id}"
///
/// This enables O(1) lookup of symbol metadata when you have the SymbolId.
/// The stored value contains the symbol's canonical FQN and other metadata.
///
/// # Arguments
/// * `id` - SymbolId (i64) to look up
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = sym_id_key(12345);
/// // Returns: b"sym:id:12345"
/// ```
pub fn sym_id_key(id: i64) -> Vec<u8> {
    format!("sym:id:{}", id).into_bytes()
}

/// Construct a KV store key for looking up a FileId by file path.
///
/// The key format is: b"file:path:{path}"
///
/// This enables O(1) file lookup when you have the file path.
/// Useful for validating if a file is indexed and getting its FileId.
///
/// # Arguments
/// * `path` - File path (as string)
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = file_path_key("src/main.rs");
/// // Returns: b"file:path:src/main.rs"
/// ```
pub fn file_path_key(path: &str) -> Vec<u8> {
    format!("file:path:{}", path).into_bytes()
}

/// Construct a KV store key for looking up all symbols in a file.
///
/// The key format is: b"file:sym:{file_id}"
///
/// This enables O(1) retrieval of all SymbolId values that belong to a file.
/// The stored value is an encoded Vec<SymbolId> (see encoding.rs).
///
/// # Arguments
/// * `file_id` - FileId (u64) of the file
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = file_sym_key(999);
/// // Returns: b"file:sym:999"
/// // Value would be encoded Vec<SymbolId> containing all symbols in file
/// ```
pub fn file_sym_key(file_id: u64) -> Vec<u8> {
    format!("file:sym:{}", file_id).into_bytes()
}

/// Construct a KV store key for the reverse index (references TO a symbol).
///
/// The key format is: b"sym:rev:{symbol_id}"
///
/// This enables O(1) lookup of all symbols that reference this symbol.
/// Used for "find usages" and impact analysis.
/// The stored value is an encoded Vec<SymbolId> of referencing symbols.
///
/// # Arguments
/// * `symbol_id` - SymbolId (i64) of the target symbol
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = sym_rev_key(12345);
/// // Returns: b"sym:rev:12345"
/// // Value would be encoded Vec<SymbolId> of symbols referencing 12345
/// ```
pub fn sym_rev_key(symbol_id: i64) -> Vec<u8> {
    format!("sym:rev:{}", symbol_id).into_bytes()
}

/// Construct a KV store key for reverse FQN lookup by symbol ID.
///
/// The key format is: b"sym:fqn_of:{symbol_id}"
///
/// This enables O(1) lookup of a symbol's FQN when you have the SymbolId.
/// Used during cache invalidation to delete sym:fqn:* entries without
/// querying the graph.
///
/// # Arguments
/// * `symbol_id` - SymbolId (i64) to look up
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = sym_fqn_of_key(12345);
/// // Returns: b"sym:fqn_of:12345"
/// // Value would be the FQN string (e.g., "my_crate::module::function")
/// ```
pub fn sym_fqn_of_key(symbol_id: i64) -> Vec<u8> {
    format!("sym:fqn_of:{}", symbol_id).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sym_fqn_key_format() {
        let key = sym_fqn_key("my_crate::module::function");
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "sym:fqn:my_crate::module::function");
    }

    #[test]
    fn test_sym_id_key_format() {
        let key = sym_id_key(12345);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "sym:id:12345");
    }

    #[test]
    fn test_sym_id_key_negative() {
        let key = sym_id_key(-1);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "sym:id:-1");
    }

    #[test]
    fn test_file_path_key_format() {
        let key = file_path_key("src/main.rs");
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "file:path:src/main.rs");
    }

    #[test]
    fn test_file_sym_key_format() {
        let key = file_sym_key(999);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "file:sym:999");
    }

    #[test]
    fn test_sym_rev_key_format() {
        let key = sym_rev_key(54321);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "sym:rev:54321");
    }

    #[test]
    fn test_sym_fqn_of_key_format() {
        let key = sym_fqn_of_key(12345);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "sym:fqn_of:12345");
    }

    #[test]
    fn test_keys_are_vec_u8() {
        // All key functions should return Vec<u8> for KvStore API compatibility
        let _: Vec<u8> = sym_fqn_key("test");
        let _: Vec<u8> = sym_id_key(1);
        let _: Vec<u8> = file_path_key("test.rs");
        let _: Vec<u8> = file_sym_key(1);
        let _: Vec<u8> = sym_rev_key(1);
        let _: Vec<u8> = sym_fqn_of_key(1);
    }

    #[test]
    fn test_key_namespaces() {
        // Verify all keys use proper namespace prefixes
        let fqn_key = String::from_utf8(sym_fqn_key("test")).unwrap();
        let id_key = String::from_utf8(sym_id_key(1)).unwrap();
        let path_key = String::from_utf8(file_path_key("test")).unwrap();
        let sym_key = String::from_utf8(file_sym_key(1)).unwrap();
        let rev_key = String::from_utf8(sym_rev_key(1)).unwrap();

        assert!(fqn_key.starts_with("sym:fqn:"));
        assert!(id_key.starts_with("sym:id:"));
        assert!(path_key.starts_with("file:path:"));
        assert!(sym_key.starts_with("file:sym:"));
        assert!(rev_key.starts_with("sym:rev:"));
    }
}

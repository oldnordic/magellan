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

/// Construct a KV store key for a code chunk.
///
/// The key format is: b"chunk:{file_path}:{byte_start}:{byte_end}"
///
/// This enables O(1) lookup of code chunks by file path and byte range.
/// File paths containing colons are escaped with "::" to avoid collisions.
///
/// # Arguments
/// * `file_path` - Path to the file containing the chunk
/// * `byte_start` - Start byte offset of the chunk
/// * `byte_end` - End byte offset of the chunk
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = chunk_key("src/main.rs", 100, 200);
/// // Returns: b"chunk:src/main.rs:100:200"
/// ```
pub fn chunk_key(file_path: &str, byte_start: usize, byte_end: usize) -> Vec<u8> {
    let escaped_path = file_path.replace(':', "::");
    format!("chunk:{}:{}:{}", escaped_path, byte_start, byte_end).into_bytes()
}

/// Construct a KV store key for an execution log entry.
///
/// The key format is: b"execlog:{execution_id}"
///
/// This enables O(1) lookup of execution records by execution ID.
/// Used for tracking Magellan CLI command executions.
///
/// # Arguments
/// * `execution_id` - UUID string identifying the execution
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = execution_log_key("550e8400-e29b-41d4-a716-446655440000");
/// // Returns: b"execlog:550e8400-e29b-41d4-a716-446655440000"
/// ```
pub fn execution_log_key(execution_id: &str) -> Vec<u8> {
    format!("execlog:{}", execution_id).into_bytes()
}

/// Construct a KV store key for file-level metrics.
///
/// The key format is: b"metrics:file:{file_path}"
///
/// This enables O(1) lookup of file metrics (complexity, line counts, etc.).
/// File paths containing colons are escaped with "::" to avoid collisions.
///
/// # Arguments
/// * `file_path` - Path to the file
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = file_metrics_key("src/main.rs");
/// // Returns: b"metrics:file:src/main.rs"
/// ```
pub fn file_metrics_key(file_path: &str) -> Vec<u8> {
    let escaped_path = file_path.replace(':', "::");
    format!("metrics:file:{}", escaped_path).into_bytes()
}

/// Construct a KV store key for symbol-level metrics.
///
/// The key format is: b"metrics:symbol:{symbol_id}"
///
/// This enables O(1) lookup of symbol metrics (complexity, cyclomatic, etc.).
///
/// # Arguments
/// * `symbol_id` - SymbolId (i64) of the symbol
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = symbol_metrics_key(12345);
/// // Returns: b"metrics:symbol:12345"
/// ```
pub fn symbol_metrics_key(symbol_id: i64) -> Vec<u8> {
    format!("metrics:symbol:{}", symbol_id).into_bytes()
}

/// Construct a KV store key for CFG blocks of a function.
///
/// The key format is: b"cfg:func:{function_id}"
///
/// This enables O(1) lookup of control flow graph blocks for a function.
/// The stored value is an encoded Vec<CfgBlock>.
///
/// # Arguments
/// * `function_id` - SymbolId (i64) of the function
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = cfg_blocks_key(12345);
/// // Returns: b"cfg:func:12345"
/// // Value would be encoded Vec<CfgBlock>
/// ```
pub fn cfg_blocks_key(function_id: i64) -> Vec<u8> {
    format!("cfg:func:{}", function_id).into_bytes()
}

/// Construct a KV store key for AST nodes of a file.
///
/// The key format is: b"ast:file:{file_id}"
///
/// This enables O(1) lookup of abstract syntax tree nodes for a file.
/// The stored value is an encoded Vec<AstNode>.
///
/// # Arguments
/// * `file_id` - FileId (u64) of the file
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = ast_nodes_key(999);
/// // Returns: b"ast:file:999"
/// // Value would be encoded Vec<AstNode>
/// ```
pub fn ast_nodes_key(file_id: u64) -> Vec<u8> {
    format!("ast:file:{}", file_id).into_bytes()
}

/// Construct a KV store key for a graph label.
///
/// The key format is: b"label:{name}"
///
/// This enables O(1) lookup of label metadata by name.
/// Labels are used for canonical FQN mappings and symbol category lookups.
///
/// # Arguments
/// * `name` - Label name
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = label_key("canonical_fqn");
/// // Returns: b"label:canonical_fqn"
/// ```
pub fn label_key(name: &str) -> Vec<u8> {
    format!("label:{}", name).into_bytes()
}

/// Construct a KV store key for a call edge between symbols.
///
/// Call edges are stored with multiple key patterns for different access patterns:
/// - `calls:{caller_id}:{callee_id}` - Specific edge for existence check
/// - `calls:from:{caller_id}` - All calls from a symbol
/// - `calls:to:{callee_id}` - All calls to a symbol
///
/// # Arguments
/// * `caller_id` - The symbol ID making the call (u64)
/// * `callee_id` - The symbol ID being called (u64)
///
/// # Returns
/// Vec<u8> containing the formatted key
///
/// # Example
/// ```ignore
/// let key = calls_key(123, 456);
/// // Returns: b"calls:123:456"
/// ```
pub fn calls_key(caller_id: u64, callee_id: u64) -> Vec<u8> {
    format!("calls:{}:{}", caller_id, callee_id).into_bytes()
}

/// Construct a KV store key prefix for caller lookups.
///
/// Used to find all calls made by a specific symbol.
///
/// # Arguments
/// * `caller_id` - The symbol ID making the call (u64)
///
/// # Returns
/// Vec<u8> containing the prefix key
///
/// # Example
/// ```ignore
/// let key = calls_from_key(123);
/// // Returns: b"calls:from:123:"
/// ```
pub fn calls_from_key(caller_id: u64) -> Vec<u8> {
    format!("calls:from:{}:", caller_id).into_bytes()
}

/// Construct a KV store key prefix for callee lookups.
///
/// Used to find all calls to a specific symbol.
///
/// # Arguments
/// * `callee_id` - The symbol ID being called (u64)
///
/// # Returns
/// Vec<u8> containing the prefix key
///
/// # Example
/// ```ignore
/// let key = calls_to_key(456);
/// // Returns: b"calls:to:456:"
/// ```
pub fn calls_to_key(callee_id: u64) -> Vec<u8> {
    format!("calls:to:{}:", callee_id).into_bytes()
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

    #[test]
    fn test_chunk_key_format() {
        let key = chunk_key("src/main.rs", 100, 200);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "chunk:src/main.rs:100:200");
    }

    #[test]
    fn test_chunk_key_with_colon_path() {
        // Paths with colons should be escaped
        let key = chunk_key("src/module:name/file.rs", 0, 100);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "chunk:src/module::name/file.rs:0:100");
    }

    #[test]
    fn test_execution_log_key_format() {
        let exec_id = "550e8400-e29b-41d4-a716-446655440000";
        let key = execution_log_key(exec_id);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, format!("execlog:{}", exec_id));
    }

    #[test]
    fn test_file_metrics_key_format() {
        let key = file_metrics_key("src/lib.rs");
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "metrics:file:src/lib.rs");
    }

    #[test]
    fn test_file_metrics_key_with_colon_path() {
        // Paths with colons should be escaped
        let key = file_metrics_key("src/test:module/file.rs");
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "metrics:file:src/test::module/file.rs");
    }

    #[test]
    fn test_symbol_metrics_key_format() {
        let key = symbol_metrics_key(12345);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "metrics:symbol:12345");
    }

    #[test]
    fn test_cfg_blocks_key_format() {
        let key = cfg_blocks_key(999);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "cfg:func:999");
    }

    #[test]
    fn test_ast_nodes_key_format() {
        let key = ast_nodes_key(12345);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "ast:file:12345");
    }

    #[test]
    fn test_label_key_format() {
        let key = label_key("canonical_fqn");
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "label:canonical_fqn");
    }

    #[test]
    fn test_calls_key_format() {
        let key = calls_key(123, 456);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "calls:123:456");
    }

    #[test]
    fn test_calls_from_key_format() {
        let key = calls_from_key(123);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "calls:from:123:");
    }

    #[test]
    fn test_calls_to_key_format() {
        let key = calls_to_key(456);
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "calls:to:456:");
    }

    #[test]
    fn test_metadata_key_namespaces() {
        // Verify metadata keys use proper namespace prefixes
        let chunk_key_str = String::from_utf8(chunk_key("test.rs", 0, 10)).unwrap();
        let exec_key_str = String::from_utf8(execution_log_key("exec-123")).unwrap();
        let file_metrics_str = String::from_utf8(file_metrics_key("test.rs")).unwrap();
        let sym_metrics_str = String::from_utf8(symbol_metrics_key(1)).unwrap();
        let cfg_str = String::from_utf8(cfg_blocks_key(1)).unwrap();
        let ast_str = String::from_utf8(ast_nodes_key(1)).unwrap();
        let calls_str = String::from_utf8(calls_key(1, 2)).unwrap();
        let calls_from_str = String::from_utf8(calls_from_key(1)).unwrap();
        let calls_to_str = String::from_utf8(calls_to_key(1)).unwrap();

        assert!(chunk_key_str.starts_with("chunk:"));
        assert!(exec_key_str.starts_with("execlog:"));
        assert!(file_metrics_str.starts_with("metrics:file:"));
        assert!(sym_metrics_str.starts_with("metrics:symbol:"));
        assert!(cfg_str.starts_with("cfg:func:"));
        assert!(ast_str.starts_with("ast:file:"));
        assert!(calls_str.starts_with("calls:"));
        assert!(calls_from_str.starts_with("calls:from:"));
        assert!(calls_to_str.starts_with("calls:to:"));
    }

    #[test]
    fn test_no_key_namespace_collisions() {
        // Verify all key namespace patterns are distinct
        // We check the prefix up to and including the first value separator
        let all_keys = vec![
            ("sym:fqn:", sym_fqn_key("test")),
            ("sym:id:", sym_id_key(1)),
            ("file:path:", file_path_key("test")),
            ("file:sym:", file_sym_key(1)),
            ("sym:rev:", sym_rev_key(1)),
            ("sym:fqn_of:", sym_fqn_of_key(1)),
            ("chunk:", chunk_key("test.rs", 0, 10)),
            ("execlog:", execution_log_key("exec-123")),
            ("metrics:file:", file_metrics_key("test.rs")),
            ("metrics:symbol:", symbol_metrics_key(1)),
            ("cfg:func:", cfg_blocks_key(1)),
            ("ast:file:", ast_nodes_key(1)),
            ("label:", label_key("test_label")),
            ("calls:", calls_key(1, 2)),
            ("calls:from:", calls_from_key(1)),
            ("calls:to:", calls_to_key(1)),
        ];

        // Extract actual prefixes from generated keys
        let prefixes: Vec<_> = all_keys
            .iter()
            .map(|(expected_prefix, key)| {
                let key_str = String::from_utf8(key.clone()).unwrap();
                // Verify key starts with expected prefix
                assert!(
                    key_str.starts_with(expected_prefix),
                    "Key '{:?}' should start with '{}'",
                    key_str,
                    expected_prefix
                );
                // Return the expected prefix for uniqueness check
                expected_prefix.to_string()
            })
            .collect();

        // Check for duplicates
        let unique_prefixes: std::collections::HashSet<_> =
            prefixes.iter().cloned().collect();
        assert_eq!(
            unique_prefixes.len(),
            prefixes.len(),
            "All key namespace prefixes should be unique"
        );
    }
}

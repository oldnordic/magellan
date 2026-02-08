//! CLI integration tests for magellan get command.
//!
//! Tests verify that the underlying `get_chunks_for_symbol()` method works
//! identically on both SQLite and Native-V2 backends, which is what the
//! `magellan get --file <path> --symbol <name>` command relies on.
//!
//! Verification test (TDD GREEN phase): Should PASS immediately because:
//! 1. get_chunks_for_symbol() already has KV support (lines 592-640 in generation/mod.rs)
//! 2. Prefix scan with symbol_name filter works (lines 606-625)
//! 3. run_get() uses graph.get_code_chunks_for_symbol() (line 162 in get_cmd.rs)

#[cfg(feature = "native-v2")]
use magellan::generation::schema::CodeChunk;
#[cfg(feature = "native-v2")]
use magellan::{ChunkStore, CodeGraph};
#[cfg(feature = "native-v2")]
use sqlitegraph::{NativeGraphBackend, GraphBackend};
#[cfg(feature = "native-v2")]
use std::rc::Rc;
#[cfg(feature = "native-v2")]
use tempfile::TempDir;

/// Test: ChunkStore::get_chunks_for_symbol() works on Native-V2 backend.
///
/// This test verifies that `magellan get --file <path> --symbol <name>` can
/// retrieve code chunks from a Native-V2 KV backend. The run_get() function
/// (line 129 in get_cmd.rs) calls graph.get_code_chunks_for_symbol() which
/// uses get_chunks_for_symbol() with KV support (lines 592-640 in generation/mod.rs).
#[cfg(feature = "native-v2")]
#[test]
fn test_magellan_get_command() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_native.db");

    // Create Native-V2 backend
    let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
    let mut chunks_store = ChunkStore::with_kv_backend(backend);

    // Add test chunks for a symbol
    let chunks = vec![CodeChunk::new(
        "src/test.rs".to_string(),
        0,
        30,
        "pub fn my_function() {}".to_string(),
        Some("my_function".to_string()),
        Some("Function".to_string()),
    )];

    for chunk in &chunks {
        chunks_store.store_chunk(chunk).unwrap();
    }

    // Query chunks by symbol (this is what run_get() does internally)
    let result = chunks_store
        .get_chunks_for_symbol("src/test.rs", "my_function")
        .unwrap();

    assert!(!result.is_empty(), "Should retrieve chunks for symbol");
    assert_eq!(result.len(), 1, "Should retrieve 1 chunk");
    assert_eq!(
        result[0].symbol_name, Some("my_function".to_string()),
        "Symbol name should match"
    );
    assert_eq!(result[0].content, "pub fn my_function() {}", "Content should match");
}

/// Test: ChunkStore::get_chunks_for_symbol() produces identical output on SQLite and Native-V2 backends.
///
/// This cross-backend parity test ensures that both backends return the same
/// code chunks when querying by symbol name.
#[cfg(feature = "native-v2")]
#[test]
fn test_magellan_get_cross_backend_parity() {
    let temp_dir = TempDir::new().unwrap();
    let sqlite_db = temp_dir.path().join("test_sqlite.db");
    let native_db = temp_dir.path().join("test_native.db");

    // Create SQLite backend with schema
    let sqlite_chunks = ChunkStore::new(&sqlite_db);
    sqlite_chunks.ensure_schema().unwrap();

    // Create Native-V2 backend
    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Add same chunks to both backends
    let chunks = vec![CodeChunk::new(
        "src/test.rs".to_string(),
        0,
        40,
        "pub fn test_symbol() { println!(\"test\"); }".to_string(),
        Some("test_symbol".to_string()),
        Some("Function".to_string()),
    )];

    // Store in SQLite
    for chunk in &chunks {
        sqlite_chunks.store_chunk(chunk).unwrap();
    }

    // Store in Native-V2
    for chunk in &chunks {
        native_chunks.store_chunk(chunk).unwrap();
    }

    // Query from both backends
    let sqlite_results = sqlite_chunks
        .get_chunks_for_symbol("src/test.rs", "test_symbol")
        .unwrap();
    let native_results = native_chunks
        .get_chunks_for_symbol("src/test.rs", "test_symbol")
        .unwrap();

    // Verify parity
    assert_eq!(
        sqlite_results.len(),
        native_results.len(),
        "Both backends should return same number of chunks"
    );

    assert_eq!(
        sqlite_results[0].symbol_name, native_results[0].symbol_name,
        "Symbol names should match"
    );

    assert_eq!(
        sqlite_results[0].content, native_results[0].content,
        "Content should match"
    );

    assert_eq!(
        sqlite_results[0].byte_start, native_results[0].byte_start,
        "Byte spans should match"
    );
}

/// Test: ChunkStore::get_chunks_for_symbol() handles empty results gracefully.
///
/// Edge case test: verifies behavior when querying a non-existent symbol.
#[cfg(feature = "native-v2")]
#[test]
fn test_magellan_get_empty_result() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_native.db");

    // Create empty Native-V2 database
    let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
    let chunks_store = ChunkStore::with_kv_backend(backend);

    // Query chunks from non-existent symbol
    let result = chunks_store.get_chunks_for_symbol("test.rs", "nonexistent");

    assert!(result.is_ok(), "Should handle empty results gracefully");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.len(), 0, "Should return empty vec for non-existent symbol");
}

/// Test: ChunkStore::get_chunks_for_symbol() filters by symbol_name correctly.
///
/// This test verifies that the KV prefix scan with symbol_name filter
/// (lines 606-625 in generation/mod.rs) correctly filters chunks.
#[cfg(feature = "native-v2")]
#[test]
fn test_magellan_get_filters_by_symbol_name() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_native.db");

    let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
    let mut chunks_store = ChunkStore::with_kv_backend(backend);

    // Add multiple chunks with different symbols in the same file
    let chunks = vec![
        CodeChunk::new(
            "src/test.rs".to_string(),
            0,
            20,
            "fn func1() {}".to_string(),
            Some("func1".to_string()),
            Some("Function".to_string()),
        ),
        CodeChunk::new(
            "src/test.rs".to_string(),
            20,
            40,
            "fn func2() {}".to_string(),
            Some("func2".to_string()),
            Some("Function".to_string()),
        ),
        CodeChunk::new(
            "src/test.rs".to_string(),
            40,
            60,
            "struct MyStruct {}".to_string(),
            Some("MyStruct".to_string()),
            Some("Struct".to_string()),
        ),
    ];

    for chunk in &chunks {
        chunks_store.store_chunk(chunk).unwrap();
    }

    // Query for func1 - should only return the first chunk
    let result = chunks_store.get_chunks_for_symbol("src/test.rs", "func1").unwrap();

    assert_eq!(result.len(), 1, "Should return only 1 chunk");
    assert_eq!(
        result[0].symbol_name, Some("func1".to_string()),
        "Should return func1"
    );

    // Query for func2 - should only return the second chunk
    let result = chunks_store.get_chunks_for_symbol("src/test.rs", "func2").unwrap();

    assert_eq!(result.len(), 1, "Should return only 1 chunk");
    assert_eq!(
        result[0].symbol_name, Some("func2".to_string()),
        "Should return func2"
    );

    // Query for MyStruct - should only return the third chunk
    let result = chunks_store
        .get_chunks_for_symbol("src/test.rs", "MyStruct")
        .unwrap();

    assert_eq!(result.len(), 1, "Should return only 1 chunk");
    assert_eq!(
        result[0].symbol_name, Some("MyStruct".to_string()),
        "Should return MyStruct"
    );
}

/// Test: ChunkStore::get_chunks_for_symbol() works with colon-escaped paths.
///
/// File paths containing colons (e.g., "src/module:name/file.rs") must be
/// escaped in KV keys to prevent collisions. The prefix scan should handle
/// this correctly (line 609 in generation/mod.rs: replace(':', "::")).
#[cfg(feature = "native-v2")]
#[test]
fn test_magellan_get_with_colon_path() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_colon.db");

    let backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn GraphBackend>;
    let mut chunks_store = ChunkStore::with_kv_backend(backend);

    // Create chunk for file with colon in path
    let colon_path = "src/module:name/file.rs";
    let chunk = CodeChunk::new(
        colon_path.to_string(),
        0,
        50,
        "fn colon_test() {}".to_string(),
        Some("colon_test".to_string()),
        Some("Function".to_string()),
    );

    chunks_store.store_chunk(&chunk).unwrap();

    // Retrieve should work despite colon in path
    let result = chunks_store.get_chunks_for_symbol(colon_path, "colon_test").unwrap();

    assert_eq!(result.len(), 1, "Should retrieve 1 chunk");
    assert_eq!(
        result[0].file_path, colon_path,
        "File path with colon should be preserved"
    );
    assert_eq!(
        result[0].symbol_name, Some("colon_test".to_string()),
        "Symbol name should match"
    );
}

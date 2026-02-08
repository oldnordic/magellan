//! Cross-backend integration tests for ChunkStore operations.
//!
//! Tests the Native-V2 KV backend parity with SQLite for ChunkStore methods.
//! Initially fails (RED phase), then passes after KV support is added.

#[cfg(feature = "native-v2")]
use magellan::generation::{ChunkStore, CodeChunk};
#[cfg(feature = "native-v2")]
use sqlitegraph::NativeGraphBackend;
#[cfg(feature = "native-v2")]
use std::rc::Rc;
use tempfile::TempDir;

/// Test: get_chunks_for_file() returns same results on SQLite and Native-V2 backends.
///
/// This test demonstrates the bug: get_chunks_for_file() works on SQLite but
/// fails on Native-V2 because it lacks KV prefix scan support.
///
/// **RED phase**: This test fails because get_chunks_for_file() doesn't have KV support.
/// **GREEN phase**: After adding KV support, test passes.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunks_for_file_cross_backend() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_native.db");

    // Create Native-V2 backend
    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create test chunks for src/test.rs
    let test_chunks = vec![
        CodeChunk::new(
            "src/test.rs".to_string(),
            0,
            100,
            "fn test() {}".to_string(),
            Some("test".to_string()),
            Some("Function".to_string()),
        ),
        CodeChunk::new(
            "src/test.rs".to_string(),
            100,
            200,
            "fn main() {}".to_string(),
            Some("main".to_string()),
            Some("Function".to_string()),
        ),
    ];

    // Store chunks in KV backend
    for chunk in &test_chunks {
        native_chunks.store_chunk(chunk).unwrap();
    }

    // Verify Native-V2 backend can retrieve chunks
    // This WILL FAIL initially because get_chunks_for_file() doesn't have KV support
    let native_result = native_chunks.get_chunks_for_file("src/test.rs").unwrap();

    assert_eq!(native_result.len(), 2, "Should retrieve 2 chunks from KV backend");

    // Verify content matches
    assert_eq!(native_result[0].content, "fn test() {}", "First chunk content should match");
    assert_eq!(native_result[1].content, "fn main() {}", "Second chunk content should match");

    // Verify symbol names
    assert_eq!(native_result[0].symbol_name, Some("test".to_string()));
    assert_eq!(native_result[1].symbol_name, Some("main".to_string()));
}

/// Test: get_chunks_for_file() handles files with colons in path.
///
/// File paths containing colons (e.g., "src/module:name/file.rs") must be
/// escaped in KV keys to prevent collisions.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunks_for_file_with_colon_path() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_colon.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

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

    native_chunks.store_chunk(&chunk).unwrap();

    // Retrieve should work despite colon in path
    let result = native_chunks.get_chunks_for_file(colon_path).unwrap();

    assert_eq!(result.len(), 1, "Should retrieve 1 chunk");
    assert_eq!(result[0].content, "fn colon_test() {}", "Content should match");
}

/// Test: get_chunks_for_file() returns empty vec for non-existent file.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunks_for_file_empty_result() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_empty.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Query for non-existent file should return empty vec, not error
    let result = native_chunks.get_chunks_for_file("nonexistent.rs").unwrap();

    assert_eq!(result.len(), 0, "Should return empty vec for non-existent file");
}

/// Test: get_chunks_for_file() returns all chunks for a file in byte order.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunks_for_file_byte_order() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_order.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create chunks in random order
    let chunks = vec![
        CodeChunk::new(
            "src/order.rs".to_string(),
            100,
            150,
            "// middle".to_string(),
            None,
            None,
        ),
        CodeChunk::new(
            "src/order.rs".to_string(),
            0,
            50,
            "// first".to_string(),
            None,
            None,
        ),
        CodeChunk::new(
            "src/order.rs".to_string(),
            50,
            100,
            "// second".to_string(),
            None,
            None,
        ),
    ];

    // Store in random order
    native_chunks.store_chunk(&chunks[0]).unwrap();
    native_chunks.store_chunk(&chunks[1]).unwrap();
    native_chunks.store_chunk(&chunks[2]).unwrap();

    // Retrieve should return chunks in byte_start order
    let result = native_chunks.get_chunks_for_file("src/order.rs").unwrap();

    assert_eq!(result.len(), 3, "Should retrieve all 3 chunks");
    assert_eq!(result[0].byte_start, 0, "First chunk should start at 0");
    assert_eq!(result[1].byte_start, 50, "Second chunk should start at 50");
    assert_eq!(result[2].byte_start, 100, "Third chunk should start at 100");
}

/// Test: get_chunk_by_span() retrieves chunk by exact byte span on KV backend.
///
/// This is a verification test: get_chunk_by_span() already has KV support
/// (lines 461-485 in src/generation/mod.rs). This test proves it works correctly.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunk_by_span_cross_backend() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_span.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create test chunks for src/span_test.rs
    let test_chunks = vec![
        CodeChunk::new(
            "src/span_test.rs".to_string(),
            0,
            100,
            "fn first() {}".to_string(),
            Some("first".to_string()),
            Some("Function".to_string()),
        ),
        CodeChunk::new(
            "src/span_test.rs".to_string(),
            100,
            200,
            "fn second() {}".to_string(),
            Some("second".to_string()),
            Some("Function".to_string()),
        ),
        CodeChunk::new(
            "src/span_test.rs".to_string(),
            200,
            300,
            "fn third() {}".to_string(),
            Some("third".to_string()),
            Some("Function".to_string()),
        ),
    ];

    // Store all chunks
    for chunk in &test_chunks {
        native_chunks.store_chunk(chunk).unwrap();
    }

    // Verify O(1) retrieval by exact span
    let result = native_chunks.get_chunk_by_span("src/span_test.rs", 100, 200).unwrap();

    assert!(result.is_some(), "Should retrieve chunk at span 100-200");
    let chunk = result.unwrap();
    assert_eq!(chunk.file_path, "src/span_test.rs");
    assert_eq!(chunk.byte_start, 100);
    assert_eq!(chunk.byte_end, 200);
    assert_eq!(chunk.content, "fn second() {}", "Content should match");
    assert_eq!(chunk.symbol_name, Some("second".to_string()));
    assert_eq!(chunk.symbol_kind, Some("Function".to_string()));

    // Verify retrieval of first chunk
    let first_result = native_chunks.get_chunk_by_span("src/span_test.rs", 0, 100).unwrap();
    assert!(first_result.is_some(), "Should retrieve first chunk");
    assert_eq!(first_result.unwrap().symbol_name, Some("first".to_string()));

    // Verify retrieval of last chunk
    let last_result = native_chunks.get_chunk_by_span("src/span_test.rs", 200, 300).unwrap();
    assert!(last_result.is_some(), "Should retrieve last chunk");
    assert_eq!(last_result.unwrap().symbol_name, Some("third".to_string()));
}

/// Test: get_chunk_by_span() handles files with colons in path.
///
/// File paths containing colons (e.g., "src/module:name/file.rs") must be
/// escaped in KV keys to prevent collisions. The chunk_key function escapes
/// colons with "::" to ensure unique keys.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunk_by_span_with_colon_path() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_colon_span.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create chunks for files with colons in paths
    let colon_path_1 = "src/module:name/file.rs";
    let colon_path_2 = "src/other::path.rs";

    let chunk1 = CodeChunk::new(
        colon_path_1.to_string(),
        0,
        50,
        "fn colon_test_1() {}".to_string(),
        Some("colon_test_1".to_string()),
        Some("Function".to_string()),
    );

    let chunk2 = CodeChunk::new(
        colon_path_2.to_string(),
        0,
        50,
        "fn colon_test_2() {}".to_string(),
        Some("colon_test_2".to_string()),
        Some("Function".to_string()),
    );

    native_chunks.store_chunk(&chunk1).unwrap();
    native_chunks.store_chunk(&chunk2).unwrap();

    // Retrieve first chunk with colon path
    let result1 = native_chunks.get_chunk_by_span(colon_path_1, 0, 50).unwrap();
    assert!(result1.is_some(), "Should retrieve chunk with colon in path");
    assert_eq!(result1.unwrap().symbol_name, Some("colon_test_1".to_string()));

    // Retrieve second chunk with colon path
    let result2 = native_chunks.get_chunk_by_span(colon_path_2, 0, 50).unwrap();
    assert!(result2.is_some(), "Should retrieve chunk with double colon in path");
    assert_eq!(result2.unwrap().symbol_name, Some("colon_test_2".to_string()));
}

/// Test: get_chunk_by_span() returns None for non-existent chunk.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunk_by_span_empty_result() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_empty_span.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create and store a chunk
    let chunk = CodeChunk::new(
        "src/exists.rs".to_string(),
        0,
        100,
        "fn exists() {}".to_string(),
        Some("exists".to_string()),
        Some("Function".to_string()),
    );
    native_chunks.store_chunk(&chunk).unwrap();

    // Query for non-existent chunk should return Ok(None), not an error
    let result = native_chunks.get_chunk_by_span("src/exists.rs", 200, 300).unwrap();
    assert!(result.is_none(), "Should return None for non-existent span");

    // Query for non-existent file should also return Ok(None)
    let result = native_chunks.get_chunk_by_span("src/nonexistent.rs", 0, 100).unwrap();
    assert!(result.is_none(), "Should return None for non-existent file");
}

/// Test: get_chunk_by_span() handles zero-length span (byte_start == byte_end).
///
/// Edge case: A zero-length span represents an empty position in the file.
/// The KV store should still handle this correctly.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunk_by_span_zero_length() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_zero_span.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create chunk with zero-length span (position 100)
    let chunk = CodeChunk::new(
        "src/zero.rs".to_string(),
        100,
        100,
        "".to_string(),
        Some("zero".to_string()),
        Some("Function".to_string()),
    );

    native_chunks.store_chunk(&chunk).unwrap();

    // Retrieve zero-length chunk
    let result = native_chunks.get_chunk_by_span("src/zero.rs", 100, 100).unwrap();

    assert!(result.is_some(), "Should retrieve zero-length chunk");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.file_path, "src/zero.rs");
    assert_eq!(retrieved.byte_start, 100, "Start should be 100");
    assert_eq!(retrieved.byte_end, 100, "End should be 100 (zero-length)");
    assert_eq!(retrieved.content, "", "Content should be empty");
    assert_eq!(retrieved.symbol_name, Some("zero".to_string()));
}

/// Test: get_chunk_by_span() with multiple chunks in same file.
///
/// Verify that exact span matching retrieves the correct chunk when
/// multiple chunks exist in the same file.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunk_by_span_multiple_chunks_same_file() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_multi_span.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create multiple chunks in the same file
    let chunks = vec![
        CodeChunk::new(
            "src/multi.rs".to_string(),
            0,
            50,
            "// first".to_string(),
            Some("first".to_string()),
            Some("Comment".to_string()),
        ),
        CodeChunk::new(
            "src/multi.rs".to_string(),
            50,
            150,
            "fn second() {}".to_string(),
            Some("second".to_string()),
            Some("Function".to_string()),
        ),
        CodeChunk::new(
            "src/multi.rs".to_string(),
            150,
            300,
            "fn third() { body }".to_string(),
            Some("third".to_string()),
            Some("Function".to_string()),
        ),
    ];

    // Store all chunks
    for chunk in &chunks {
        native_chunks.store_chunk(chunk).unwrap();
    }

    // Retrieve each chunk by exact span
    let first = native_chunks.get_chunk_by_span("src/multi.rs", 0, 50).unwrap();
    assert!(first.is_some(), "Should retrieve first chunk");
    assert_eq!(first.unwrap().symbol_name, Some("first".to_string()));

    let second = native_chunks.get_chunk_by_span("src/multi.rs", 50, 150).unwrap();
    assert!(second.is_some(), "Should retrieve second chunk");
    assert_eq!(second.unwrap().symbol_name, Some("second".to_string()));

    let third = native_chunks.get_chunk_by_span("src/multi.rs", 150, 300).unwrap();
    assert!(third.is_some(), "Should retrieve third chunk");
    assert_eq!(third.unwrap().symbol_name, Some("third".to_string()));
}

/// Test: get_chunk_by_span() requires exact span match.
///
/// Verify that partial or overlapping span matches return None.
/// Only an exact match on (file_path, byte_start, byte_end) returns a chunk.
#[cfg(feature = "native-v2")]
#[test]
fn test_get_chunk_by_span_exact_match_required() {
    let temp_dir = TempDir::new().unwrap();
    let native_db = temp_dir.path().join("test_exact_span.db");

    let native_backend = Rc::new(NativeGraphBackend::new(&native_db).unwrap()) as Rc<dyn sqlitegraph::GraphBackend>;
    let native_chunks = ChunkStore::with_kv_backend(native_backend);

    // Create and store a chunk with span 100:200
    let chunk = CodeChunk::new(
        "src/exact.rs".to_string(),
        100,
        200,
        "fn exact() {}".to_string(),
        Some("exact".to_string()),
        Some("Function".to_string()),
    );
    native_chunks.store_chunk(&chunk).unwrap();

    // Exact match should succeed
    let exact = native_chunks.get_chunk_by_span("src/exact.rs", 100, 200).unwrap();
    assert!(exact.is_some(), "Exact match should succeed");
    assert_eq!(exact.unwrap().symbol_name, Some("exact".to_string()));

    // Off-by-one start should fail
    let off_start = native_chunks.get_chunk_by_span("src/exact.rs", 99, 200).unwrap();
    assert!(off_start.is_none(), "Off-by-one start should return None");

    // Off-by-one end should fail
    let off_end = native_chunks.get_chunk_by_span("src/exact.rs", 100, 201).unwrap();
    assert!(off_end.is_none(), "Off-by-one end should return None");

    // Completely different span should fail
    let different = native_chunks.get_chunk_by_span("src/exact.rs", 0, 50).unwrap();
    assert!(different.is_none(), "Different span should return None");

    // Overlapping but not exact should fail
    let overlap = native_chunks.get_chunk_by_span("src/exact.rs", 150, 250).unwrap();
    assert!(overlap.is_none(), "Overlapping span should return None");

    // Superset span should fail
    let superset = native_chunks.get_chunk_by_span("src/exact.rs", 50, 250).unwrap();
    assert!(superset.is_none(), "Superset span should return None");
}

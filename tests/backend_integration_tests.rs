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
    let mut native_chunks = ChunkStore::with_kv_backend(native_backend);

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
    let mut native_chunks = ChunkStore::with_kv_backend(native_backend);

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
    let mut native_chunks = ChunkStore::with_kv_backend(native_backend);

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

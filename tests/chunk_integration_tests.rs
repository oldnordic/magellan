//! Integration tests for chunk storage during indexing.
//!
//! These tests verify that the chunk storage pipeline works correctly:
//! - Chunks are stored during indexing with correct content and metadata
//! - Chunks are deleted when files are deleted
//! - Content hash deduplication works as expected
//! - Chunk queries by symbol and span work correctly
//!
//! ## Chunk Storage Flow
//!
//! During `index_file()`:
//! 1. Parser extracts SymbolFacts with byte spans
//! 2. For each symbol, extract source content using byte span
//! 3. Create CodeChunk with content, byte span, symbol metadata
//! 4. Compute SHA-256 hash of content (for deduplication)
//! 5. Store all chunks in a single transaction
//!
//! ## Deletion Flow
//!
//! During `delete_file_facts()`:
//! 1. Graph entities deleted in IMMEDIATE transaction
//! 2. Chunks deleted on separate connection (two-phase commit)
//! 3. chunks_deleted count returned in DeleteResult

use magellan::CodeGraph;
use tempfile::TempDir;

/// Helper to create a temporary test database.
fn setup_test_db() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

/// Helper to create test source code with multiple symbols.
///
/// Returns source code with:
/// - Function: `process_data`
/// - Struct: `DataProcessor`
/// - Impl block with method: `process`
fn create_test_source() -> &'static str {
    r#"
/// Process data with validation
pub fn process_data(input: i32) -> i32 {
    if input > 0 {
        input * 2
    } else {
        0
    }
}

/// Data processor struct
pub struct DataProcessor {
    threshold: i32,
}

impl DataProcessor {
    /// Create new processor
    pub fn new(threshold: i32) -> Self {
        Self { threshold }
    }

    /// Process value
    pub fn process(&self, value: i32) -> Option<i32> {
        if value >= self.threshold {
            Some(value * 2)
        } else {
            None
        }
    }
}
"#
}

/// Test: Chunk storage during indexing stores correct content and metadata.
#[test]
fn test_chunk_storage_during_indexing() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_storage.rs";
    let source = create_test_source().as_bytes();

    // Index the file
    let symbol_count = graph.index_file(path, source).unwrap();
    assert!(symbol_count > 0, "Should have indexed symbols");

    // Query chunks
    let chunks = graph.get_code_chunks(path).unwrap();
    assert!(!chunks.is_empty(), "Should have stored chunks");

    // Verify each chunk
    let source_str = create_test_source();
    for chunk in &chunks {
        // Verify file path
        assert_eq!(chunk.file_path, path, "File path should match");

        // Verify byte spans are valid
        assert!(
            chunk.byte_start < chunk.byte_end,
            "Byte span should be valid: {} < {}",
            chunk.byte_start,
            chunk.byte_end
        );

        // Verify content matches actual source for that span
        let expected_content = &source_str[chunk.byte_start..chunk.byte_end];
        assert_eq!(
            chunk.content, expected_content,
            "Content should match source at byte span {}..{}",
            chunk.byte_start, chunk.byte_end
        );

        // Verify symbol metadata exists
        assert!(
            chunk.symbol_name.is_some(),
            "Symbol name should be set"
        );
        assert!(
            chunk.symbol_kind.is_some(),
            "Symbol kind should be set"
        );

        // Verify content hash is non-empty (SHA-256 = 64 hex chars)
        assert_eq!(
            chunk.content_hash.len(),
            64,
            "Content hash should be SHA-256 (64 hex chars)"
        );

        // Verify hash is lowercase hex
        assert!(
            chunk.content_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Content hash should be valid hex"
        );
    }

    // Verify we have chunks for the main symbols
    let symbol_names: Vec<_> = chunks
        .iter()
        .filter_map(|c| c.symbol_name.as_ref())
        .collect();

    assert!(
        symbol_names.contains(&&"process_data".to_string()),
        "Should have chunk for process_data function"
    );
    assert!(
        symbol_names.contains(&&"DataProcessor".to_string()),
        "Should have chunk for DataProcessor struct"
    );
}

/// Test: Chunk deletion on file delete removes all chunks.
#[test]
fn test_chunk_deletion_on_file_delete() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "test_deletion.rs";
    let source = create_test_source().as_bytes();

    // Index the file
    graph.index_file(path, source).unwrap();

    // Verify chunks exist before deletion
    let chunks_before = graph.get_code_chunks(path).unwrap();
    let chunk_count_before = chunks_before.len();
    assert!(chunk_count_before > 0, "Should have chunks before deletion");

    // Delete the file
    let result = graph.delete_file_facts(path).unwrap();

    // Verify chunks are deleted
    let chunks_after = graph.get_code_chunks(path).unwrap();
    assert_eq!(
        chunks_after.len(),
        0,
        "All chunks should be deleted after file deletion"
    );

    // Verify DeleteResult includes chunks_deleted count
    assert_eq!(
        result.chunks_deleted, chunk_count_before,
        "DeleteResult should report correct chunks_deleted count"
    );
}

/// Test: Content hash deduplication works for identical content.
#[test]
fn test_content_hash_deduplication() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create two files with identical function content
    let identical_function = r#"
pub fn helper_function(x: i32) -> i32 {
    x + 42
}
"#;

    let path1 = "file1.rs";
    let path2 = "file2.rs";

    // Index both files
    graph.index_file(path1, identical_function.as_bytes()).unwrap();
    graph.index_file(path2, identical_function.as_bytes()).unwrap();

    // Get chunks from both files
    let chunks1 = graph.get_code_chunks(path1).unwrap();
    let chunks2 = graph.get_code_chunks(path2).unwrap();

    assert!(!chunks1.is_empty(), "File1 should have chunks");
    assert!(!chunks2.is_empty(), "File2 should have chunks");

    // Find the helper_function chunk in each file
    let chunk1 = chunks1
        .iter()
        .find(|c| c.symbol_name.as_deref() == Some("helper_function"))
        .expect("Should find helper_function in file1");

    let chunk2 = chunks2
        .iter()
        .find(|c| c.symbol_name.as_deref() == Some("helper_function"))
        .expect("Should find helper_function in file2");

    // Verify content_hash is identical for matching content
    assert_eq!(
        chunk1.content_hash, chunk2.content_hash,
        "Identical content should have identical content_hash"
    );

    // Verify content is actually identical
    assert_eq!(
        chunk1.content, chunk2.content,
        "Content should be identical"
    );

    // Verify different file paths (chunks are stored separately)
    assert_eq!(chunk1.file_path, path1, "Chunk1 should have path1");
    assert_eq!(chunk2.file_path, path2, "Chunk2 should have path2");

    // Verify they are different chunks (different IDs)
    assert_ne!(
        chunk1.id, chunk2.id,
        "Chunks should have different IDs (stored separately)"
    );
}

/// Test: Chunk query by symbol name returns correct chunks.
#[test]
fn test_chunk_by_symbol_query() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create source with overloaded-like symbols (same base name, different contexts)
    let source = r#"
pub fn process(input: i32) -> i32 {
    input + 1
}

pub mod utils {
    pub fn process(input: &str) -> String {
        input.to_uppercase()
    }
}

pub struct Processor {
    value: i32,
}

impl Processor {
    pub fn process(&self) -> i32 {
        self.value
    }
}
"#;

    let path = "test_symbol_query.rs";
    graph.index_file(path, source.as_bytes()).unwrap();

    // Query chunks by symbol name "process"
    let process_chunks = graph.get_code_chunks_for_symbol(path, "process").unwrap();

    // We should have multiple chunks for "process" (module-level, method, etc.)
    assert!(!process_chunks.is_empty(), "Should find chunks for 'process'");

    // Verify all returned chunks have matching symbol name
    for chunk in &process_chunks {
        assert_eq!(
            chunk.symbol_name.as_deref(),
            Some("process"),
            "All chunks should have symbol_name 'process'"
        );
        assert_eq!(chunk.file_path, path, "All chunks should be from the same file");
    }

    // Query for non-existent symbol
    let nonexistent_chunks = graph.get_code_chunks_for_symbol(path, "nonexistent").unwrap();
    assert_eq!(
        nonexistent_chunks.len(),
        0,
        "Should return empty vector for non-existent symbol"
    );
}

/// Test: Chunk query by exact byte span returns correct chunk.
#[test]
fn test_chunk_by_span_query() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = create_test_source();
    let path = "test_span_query.rs";
    graph.index_file(path, source.as_bytes()).unwrap();

    // Get all chunks to find a valid span
    let all_chunks = graph.get_code_chunks(path).unwrap();
    assert!(!all_chunks.is_empty(), "Should have chunks");

    // Test querying for each chunk's exact span
    for chunk in &all_chunks {
        // Query by exact span
        let found = graph
            .get_code_chunk_by_span(path, chunk.byte_start, chunk.byte_end)
            .unwrap();

        assert!(found.is_some(), "Should find chunk by exact span");

        let found_chunk = found.unwrap();
        assert_eq!(
            found_chunk.id, chunk.id,
            "Should return the exact same chunk (same ID)"
        );
        assert_eq!(
            found_chunk.content, chunk.content,
            "Content should match"
        );
        assert_eq!(
            found_chunk.byte_start, chunk.byte_start,
            "Byte start should match"
        );
        assert_eq!(found_chunk.byte_end, chunk.byte_end, "Byte end should match");
    }

    // Test querying for non-existent span
    let nonexistent = graph
        .get_code_chunk_by_span(path, 99999, 100000)
        .unwrap();
    assert!(
        nonexistent.is_none(),
        "Should return None for non-existent span"
    );

    // Test querying for invalid span (start > end)
    let invalid_span = graph.get_code_chunk_by_span(path, 100, 50).unwrap();
    assert!(
        invalid_span.is_none(),
        "Should return None for invalid span (start > end)"
    );
}

/// Test: Chunk count matches symbol count for typical file.
#[test]
fn test_chunk_count_matches_symbol_count() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = create_test_source();
    let path = "test_count.rs";

    // Index the file
    let symbol_count = graph.index_file(path, source.as_bytes()).unwrap();

    // Count symbols directly
    let symbols = graph.symbols_in_file(path).unwrap();

    // Count chunks
    let chunks = graph.get_code_chunks(path).unwrap();

    // For simple files without impl blocks, chunk count should be close to symbol count
    // (impl blocks may create extra SymbolFacts but we only chunk functions/structs/enums)
    assert!(chunks.len() > 0, "Should have at least some chunks");
    assert!(chunks.len() <= symbols.len(), "Chunks should not exceed symbols");

    // All chunks should have valid symbol metadata
    for chunk in &chunks {
        assert!(
            chunk.symbol_name.is_some(),
            "Chunk should have symbol_name"
        );
        assert!(
            chunk.symbol_kind.is_some(),
            "Chunk should have symbol_kind"
        );
    }
}

/// Test: Chunk content hash is deterministic.
#[test]
fn test_chunk_content_hash_deterministic() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
pub fn deterministic_function(x: i32) -> i32 {
    x * 2
}
"#;

    let path = "test_deterministic.rs";

    // Index the same content twice
    graph.index_file(path, source.as_bytes()).unwrap();
    let chunks1 = graph.get_code_chunks(path).unwrap();

    // Re-index (should replace chunks)
    graph.index_file(path, source.as_bytes()).unwrap();
    let chunks2 = graph.get_code_chunks(path).unwrap();

    assert!(!chunks1.is_empty(), "First index should create chunks");
    assert!(!chunks2.is_empty(), "Second index should create chunks");

    // Find the function chunk in both sets
    let chunk1 = chunks1
        .iter()
        .find(|c| c.symbol_name.as_deref() == Some("deterministic_function"))
        .expect("Should find function in first index");

    let chunk2 = chunks2
        .iter()
        .find(|c| c.symbol_name.as_deref() == Some("deterministic_function"))
        .expect("Should find function in second index");

    // Content hash should be identical (same content = same hash)
    assert_eq!(
        chunk1.content_hash, chunk2.content_hash,
        "Content hash should be deterministic for identical content"
    );
}

/// Test: Chunk byte spans are within source file bounds.
#[test]
fn test_chunk_byte_spans_within_bounds() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = create_test_source();
    let source_len = source.len();
    let path = "test_bounds.rs";

    graph.index_file(path, source.as_bytes()).unwrap();
    let chunks = graph.get_code_chunks(path).unwrap();

    // Verify all byte spans are within source bounds
    for chunk in &chunks {
        assert!(
            chunk.byte_start <= source_len,
            "byte_start {} should be within source length {}",
            chunk.byte_start,
            source_len
        );
        assert!(
            chunk.byte_end <= source_len,
            "byte_end {} should be within source length {}",
            chunk.byte_end,
            source_len
        );
        assert!(
            chunk.byte_start < chunk.byte_end,
            "byte_start {} should be less than byte_end {}",
            chunk.byte_start,
            chunk.byte_end
        );

        // Verify content can be extracted using byte span
        let _ = &source[chunk.byte_start..chunk.byte_end];
    }
}

/// Test: Chunk content matches source for all symbols.
#[test]
fn test_chunk_content_matches_source() {
    let temp_dir = setup_test_db();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = create_test_source();
    let path = "test_content_match.rs";

    graph.index_file(path, source.as_bytes()).unwrap();
    let chunks = graph.get_code_chunks(path).unwrap();

    // Verify chunk content exactly matches source at byte span
    for chunk in &chunks {
        let expected_content = &source[chunk.byte_start..chunk.byte_end];
        assert_eq!(
            chunk.content, expected_content,
            "Chunk content should match source at span {}..{}",
            chunk.byte_start, chunk.byte_end
        );

        // Verify byte length matches content length
        let expected_len = chunk.byte_end - chunk.byte_start;
        assert_eq!(
            chunk.content.len(), expected_len,
            "Content length should match byte span length"
        );
    }
}

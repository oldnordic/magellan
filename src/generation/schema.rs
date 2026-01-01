//! Schema definitions for code generation module.
//!
//! This module defines the types for storing and retrieving source code chunks
//! with their byte spans, enabling token-efficient queries without re-reading
//! entire files.

use serde::{Deserialize, Serialize};

/// A code chunk with source text and metadata.
///
/// Stores a contiguous span of source code with its byte offsets, enabling
/// efficient retrieval of specific code sections without reading entire files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeChunk {
    /// Unique identifier (auto-incremented)
    pub id: Option<i64>,

    /// File path containing this chunk
    pub file_path: String,

    /// Byte offset where chunk starts in source file
    pub byte_start: usize,

    /// Byte offset where chunk ends in source file
    pub byte_end: usize,

    /// Source code content for this span
    pub content: String,

    /// SHA-256 hash of the content (for deduplication)
    pub content_hash: String,

    /// Optional: symbol name this chunk represents
    pub symbol_name: Option<String>,

    /// Optional: symbol kind (fn, struct, etc.)
    pub symbol_kind: Option<String>,

    /// Unix timestamp when chunk was created/updated
    pub created_at: i64,
}

impl CodeChunk {
    /// Create a new code chunk.
    pub fn new(
        file_path: String,
        byte_start: usize,
        byte_end: usize,
        content: String,
        symbol_name: Option<String>,
        symbol_kind: Option<String>,
    ) -> Self {
        let content_hash = Self::compute_hash(&content);
        let created_at = Self::now();

        Self {
            id: None,
            file_path,
            byte_start,
            byte_end,
            content,
            content_hash,
            symbol_name,
            symbol_kind,
            created_at,
        }
    }

    /// Compute SHA-256 hash of content.
    fn compute_hash(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let hash = hasher.finalize();
        hex::encode(hash)
    }

    /// Get current Unix timestamp in seconds.
    fn now() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Get the byte length of this chunk.
    pub fn byte_len(&self) -> usize {
        self.byte_end - self.byte_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_chunk_hash() {
        let chunk1 = CodeChunk::new(
            "test.rs".to_string(),
            0,
            10,
            "fn main() {}".to_string(),
            Some("main".to_string()),
            Some("fn".to_string()),
        );

        let chunk2 = CodeChunk::new(
            "test.rs".to_string(),
            0,
            10,
            "fn main() {}".to_string(),
            Some("main".to_string()),
            Some("fn".to_string()),
        );

        assert_eq!(chunk1.content_hash, chunk2.content_hash);
    }

    #[test]
    fn test_code_chunk_byte_len() {
        let chunk = CodeChunk::new(
            "test.rs".to_string(),
            100,
            200,
            "x".repeat(100),
            None,
            None,
        );

        assert_eq!(chunk.byte_len(), 100);
    }

    #[test]
    fn test_code_chunk_with_symbols() {
        let chunk = CodeChunk::new(
            "src/main.rs".to_string(),
            42,
            100,
            "fn my_function() {}".to_string(),
            Some("my_function".to_string()),
            Some("fn".to_string()),
        );

        assert_eq!(chunk.symbol_name, Some("my_function".to_string()));
        assert_eq!(chunk.symbol_kind, Some("fn".to_string()));
        assert_eq!(chunk.byte_start, 42);
        assert_eq!(chunk.byte_end, 100);
    }
}

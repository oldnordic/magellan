//! Rich span extensions for optional metadata
//!
//! Provides optional fields for spans including:
//! - Context lines (before/after/selected)
//! - Relationships (callers/callees/imports/exports)
//! - Semantic information (kind, language)
//! - Checksums (content hash, file hash)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Context lines around a span
///
/// Provides source code context for better understanding of span location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpanContext {
    /// Lines before the span
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,

    /// Lines within the span
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<String>,

    /// Lines after the span
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

/// Symbol reference for relationships
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolReference {
    /// File containing the symbol
    pub file: String,

    /// Symbol name
    pub symbol: String,

    /// Byte start position
    pub byte_start: usize,

    /// Byte end position
    pub byte_end: usize,

    /// Line number (1-indexed)
    pub line: usize,
}

/// Relationship information for a span
///
/// Provides call graph and import/export relationships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SpanRelationships {
    /// Callers (functions that call this symbol)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<SymbolReference>,

    /// Callees (functions called by this symbol)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callees: Vec<SymbolReference>,

    /// Imports brought in by this span
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<SymbolReference>,

    /// Exports provided by this span
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<SymbolReference>,
}

/// Semantic information about a span
///
/// Groups symbol kind and programming language together.
/// This keeps the Span struct clean by using a single Option<SpanSemantics>
/// instead of separate Option<String> fields for kind and language.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpanSemantics {
    /// Symbol kind (function, variable, type, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Programming language
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl SpanSemantics {
    /// Create semantic info with both kind and language
    pub fn new(kind: String, language: String) -> Self {
        SpanSemantics {
            kind: Some(kind),
            language: Some(language),
        }
    }

    /// Create semantic info with only kind
    pub fn kind_only(kind: String) -> Self {
        SpanSemantics {
            kind: Some(kind),
            language: None,
        }
    }

    /// Create semantic info with only language
    pub fn language_only(language: String) -> Self {
        SpanSemantics {
            kind: None,
            language: Some(language),
        }
    }
}

/// Checksum information for content verification
///
/// Provides SHA-256 checksums for span content and entire file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpanChecksums {
    /// SHA-256 hash of span content (with "sha256:" prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_before: Option<String>,

    /// SHA-256 hash of entire file (with "sha256:" prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_checksum_before: Option<String>,
}

impl SpanContext {
    /// Extract context lines from a file
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the source file
    /// * `start_line` - Line where span starts (1-indexed)
    /// * `end_line` - Line where span ends (1-indexed)
    /// * `context_lines` - Number of context lines before/after
    pub fn extract(
        file_path: &str,
        start_line: usize,
        end_line: usize,
        context_lines: usize,
    ) -> Option<Self> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(file_path).ok()?;
        let reader = BufReader::new(file);

        let mut before = Vec::new();
        let mut selected = Vec::new();
        let mut after = Vec::new();

        let context_start = start_line.saturating_sub(context_lines);
        let context_end = end_line + context_lines;

        for (line_num, line) in reader.lines().enumerate() {
            let line_num = line_num + 1; // Convert to 1-indexed

            if line_num < context_start {
                continue;
            }

            if line_num > context_end {
                break;
            }

            let line = line.ok()?;

            if line_num < start_line {
                before.push(line);
            } else if line_num >= start_line && line_num <= end_line {
                selected.push(line);
            } else {
                after.push(line);
            }
        }

        Some(SpanContext {
            before,
            selected,
            after,
        })
    }
}

impl SpanChecksums {
    /// Safely extract bytes from a file for checksum computation
    ///
    /// This helper function extracts a byte slice from a file with bounds checking.
    /// It operates on raw bytes (not UTF-8 strings), so it's safe for any file content.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the source file
    /// * `byte_start` - Start byte offset of span
    /// * `byte_end` - End byte offset of span
    ///
    /// # Returns
    /// Some(Vec<u8>) with the extracted bytes, or None if:
    /// - File cannot be read
    /// - Offsets are out of bounds
    fn extract_from_bytes(file_path: &str, byte_start: usize, byte_end: usize) -> Option<Vec<u8>> {
        let content = std::fs::read(file_path).ok()?;

        // Validate bounds
        if byte_start > byte_end || byte_end > content.len() {
            return None;
        }

        // Extract the byte slice
        content.get(byte_start..byte_end).map(|s| s.to_vec())
    }

    /// Compute SHA-256 checksum of span content
    ///
    /// This function computes a checksum of raw bytes, not UTF-8 strings.
    /// It's safe for any file content, including files with multi-byte UTF-8 characters.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the source file
    /// * `byte_start` - Start byte offset of span
    /// * `byte_end` - End byte offset of span
    pub fn compute_span_checksum(
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Option<String> {
        let span_bytes = Self::extract_from_bytes(file_path, byte_start, byte_end)?;

        let mut hasher = Sha256::new();
        hasher.update(&span_bytes);
        let result = hasher.finalize();

        Some(format!("sha256:{}", hex::encode(result)))
    }

    /// Compute SHA-256 checksum of entire file
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the source file
    pub fn compute_file_checksum(file_path: &str) -> Option<String> {
        let content = std::fs::read(file_path).ok()?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let result = hasher.finalize();

        Some(format!("sha256:{}", hex::encode(result)))
    }

    /// Compute both span and file checksums
    pub fn compute(file_path: &str, byte_start: usize, byte_end: usize) -> Self {
        SpanChecksums {
            checksum_before: Self::compute_span_checksum(file_path, byte_start, byte_end),
            file_checksum_before: Self::compute_file_checksum(file_path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_context_serialization() {
        let context = SpanContext {
            before: vec!["line 1".to_string()],
            selected: vec!["line 2".to_string()],
            after: vec!["line 3".to_string()],
        };

        let json = serde_json::to_string(&context).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["before"][0], "line 1");
        assert_eq!(value["selected"][0], "line 2");
        assert_eq!(value["after"][0], "line 3");
    }

    #[test]
    fn test_empty_context_skips_empty_arrays() {
        let context = SpanContext {
            before: vec![],
            selected: vec!["line 1".to_string()],
            after: vec![],
        };

        let json = serde_json::to_string(&context).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Empty arrays should be skipped (or empty when present)
        assert!(value["before"].as_array().map_or(true, |a| a.is_empty()));
        assert!(!value["selected"].as_array().map_or(true, |a| a.is_empty()));
        assert!(value["after"].as_array().map_or(true, |a| a.is_empty()));
    }

    #[test]
    fn test_span_relationships_default() {
        let rel = SpanRelationships::default();
        assert!(rel.callers.is_empty());
        assert!(rel.callees.is_empty());
        assert!(rel.imports.is_empty());
        assert!(rel.exports.is_empty());
    }

    #[test]
    fn test_symbol_reference_serialization() {
        let ref_ = SymbolReference {
            file: "test.rs".to_string(),
            symbol: "main".to_string(),
            byte_start: 0,
            byte_end: 10,
            line: 1,
        };

        let json = serde_json::to_string(&ref_).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["file"], "test.rs");
        assert_eq!(value["symbol"], "main");
        assert_eq!(value["byte_start"], 0);
        assert_eq!(value["line"], 1);
    }

    #[test]
    fn test_span_semantics_serialization() {
        let sem = SpanSemantics::new("function".to_string(), "rust".to_string());

        let json = serde_json::to_string(&sem).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["kind"], "function");
        assert_eq!(value["language"], "rust");
    }

    #[test]
    fn test_span_semantics_kind_only() {
        let sem = SpanSemantics::kind_only("function".to_string());

        let json = serde_json::to_string(&sem).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["kind"], "function");
        // language should not appear or be null
        assert!(value.get("language").is_none() || value["language"].is_null());
    }

    #[test]
    fn test_span_semantics_none_skips_fields() {
        let sem = SpanSemantics {
            kind: None,
            language: None,
        };

        let json = serde_json::to_string(&sem).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        // None fields should be skipped
        assert!(value.get("kind").is_none() || value["kind"].is_null());
        assert!(value.get("language").is_none() || value["language"].is_null());
    }
}

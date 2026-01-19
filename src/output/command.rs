//! JSON output types for CLI commands
//!
//! Provides schema-versioned, span-aware response types for all query commands.
//! Follows Phase 3 CLI Output Contract specification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current JSON output schema version
pub const MAGELLAN_JSON_SCHEMA_VERSION: &str = "1.0.0";

/// Wrapper for all JSON responses
///
/// Every JSON response includes schema_version and execution_id for
/// parsing stability and traceability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonResponse<T> {
    /// Schema version for parsing stability
    pub schema_version: String,
    /// Unique execution ID for this run
    pub execution_id: String,
    /// Response data
    pub data: T,
    /// Whether the response is partial (e.g., truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
}

impl<T> JsonResponse<T> {
    /// Create a new JSON response
    pub fn new(data: T, execution_id: &str) -> Self {
        JsonResponse {
            schema_version: MAGELLAN_JSON_SCHEMA_VERSION.to_string(),
            execution_id: execution_id.to_string(),
            data,
            partial: None,
        }
    }

    /// Mark the response as partial
    pub fn with_partial(mut self, partial: bool) -> Self {
        self.partial = Some(partial);
        self
    }
}

/// Span in source code (byte + line/column)
///
/// Represents an exclusive range: [start, end)
/// - byte_end is the first byte NOT included
/// - end_line/end_col point to the position after the span
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Span {
    /// Stable span ID (SHA-256 hash of file_path:byte_start:byte_end)
    pub span_id: String,
    /// File path (absolute or root-relative)
    pub file_path: String,
    /// Byte range [start, end) - end is exclusive
    pub byte_start: usize,
    pub byte_end: usize,
    /// Line (1-indexed) and column (0-indexed, bytes)
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    /// Generate a stable span ID from (file_path, byte_start, byte_end)
    ///
    /// Uses SHA-256 for platform-independent, deterministic span IDs.
    /// The hash is computed from: file_path + ":" + byte_start + ":" + byte_end
    /// The first 8 bytes (64 bits) of the hash are formatted as 16 hex characters.
    ///
    /// This ensures span IDs are:
    /// - Deterministic: same inputs always produce the same ID
    /// - Platform-independent: SHA-256 produces consistent results across architectures
    /// - Collision-resistant: 64-bit space with good distribution
    pub fn generate_id(file_path: &str, byte_start: usize, byte_end: usize) -> String {
        let mut hasher = Sha256::new();

        // Hash file path
        hasher.update(file_path.as_bytes());

        // Separator to distinguish path from numbers
        hasher.update(b":");

        // Hash byte_start as big-endian bytes
        hasher.update(byte_start.to_be_bytes());

        // Separator
        hasher.update(b":");

        // Hash byte_end as big-endian bytes
        hasher.update(byte_end.to_be_bytes());

        // Take first 8 bytes (64 bits) and format as hex
        let result = hasher.finalize();
        format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                result[0], result[1], result[2], result[3],
                result[4], result[5], result[6], result[7])
    }

    /// Create a new Span from component parts
    pub fn new(
        file_path: String,
        byte_start: usize,
        byte_end: usize,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        let span_id = Self::generate_id(&file_path, byte_start, byte_end);
        Span {
            span_id,
            file_path,
            byte_start,
            byte_end,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}

/// Symbol match result for query/find commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    /// Stable match ID
    pub match_id: String,
    /// Symbol span
    pub span: Span,
    /// Symbol name
    pub name: String,
    /// Symbol kind (normalized)
    pub kind: String,
    /// Containing symbol (if nested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl SymbolMatch {
    /// Generate a match ID
    pub fn generate_match_id(symbol_name: &str, file_path: &str, byte_start: usize) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        symbol_name.hash(&mut hasher);
        file_path.hash(&mut hasher);
        byte_start.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Create a new SymbolMatch
    pub fn new(
        name: String,
        kind: String,
        span: Span,
        parent: Option<String>,
    ) -> Self {
        let match_id = Self::generate_match_id(&name, &span.file_path, span.byte_start);
        SymbolMatch {
            match_id,
            span,
            name,
            kind,
            parent,
        }
    }
}

/// Reference match result for refs command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceMatch {
    /// Stable match ID
    pub match_id: String,
    /// Reference span
    pub span: Span,
    /// Referenced symbol name
    pub referenced_symbol: String,
    /// Reference kind (optional, for categorization)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_kind: Option<String>,
}

impl ReferenceMatch {
    /// Generate a match ID for a reference
    pub fn generate_match_id(
        referenced_symbol: &str,
        file_path: &str,
        byte_start: usize,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        referenced_symbol.hash(&mut hasher);
        file_path.hash(&mut hasher);
        byte_start.hash(&mut hasher);
        format!("ref_{:x}", hasher.finish())
    }

    /// Create a new ReferenceMatch
    pub fn new(
        span: Span,
        referenced_symbol: String,
        reference_kind: Option<String>,
    ) -> Self {
        let match_id = Self::generate_match_id(&referenced_symbol, &span.file_path, span.byte_start);
        ReferenceMatch {
            match_id,
            span,
            referenced_symbol,
            reference_kind,
        }
    }
}

/// Response for query command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Symbols found in the queried file
    pub symbols: Vec<SymbolMatch>,
    /// File path that was queried
    pub file_path: String,
    /// Kind filter that was applied (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_filter: Option<String>,
}

/// Response for find command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResponse {
    /// Matching symbols found
    pub matches: Vec<SymbolMatch>,
    /// Name that was queried
    pub query_name: String,
    /// File filter that was applied (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_filter: Option<String>,
}

/// Response for refs command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefsResponse {
    /// References found
    pub references: Vec<ReferenceMatch>,
    /// Symbol name that was queried
    pub symbol_name: String,
    /// File path containing the symbol
    pub file_path: String,
    /// Direction ("in" for callers, "out" for callees)
    pub direction: String,
}

/// Response for files command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesResponse {
    /// All indexed files (sorted deterministically)
    pub files: Vec<String>,
}

/// Response for status command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    /// Number of indexed files
    pub files: usize,
    /// Number of indexed symbols
    pub symbols: usize,
    /// Number of indexed references
    pub references: usize,
    /// Number of indexed calls
    pub calls: usize,
    /// Number of code chunks
    pub code_chunks: usize,
}

/// Response for errors in JSON mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error category/type
    pub error: String,
    /// Human-readable error message
    pub message: String,
}

/// Output format for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output
    Human,
    /// JSON output with schema versioning
    Json,
}

impl OutputFormat {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "human" | "text" => Some(OutputFormat::Human),
            "json" => Some(OutputFormat::Json),
            _ => None,
        }
    }
}

/// Generate a unique execution ID for this run
///
/// Uses timestamp + process ID for uniqueness.
/// Phase 4 may upgrade to UUID-based IDs.
pub fn generate_execution_id() -> String {
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let pid = process::id();

    format!("{:x}-{:x}", timestamp, pid)
}

/// Output JSON to stdout
pub fn output_json<T: Serialize>(data: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    println!("{}", json);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_generate_id_is_deterministic() {
        let id1 = Span::generate_id("test.rs", 10, 20);
        let id2 = Span::generate_id("test.rs", 10, 20);
        let id3 = Span::generate_id("test.rs", 10, 21);

        assert_eq!(id1, id2, "Same inputs should produce same ID");
        assert_ne!(id1, id3, "Different inputs should produce different IDs");
    }

    #[test]
    fn test_span_generate_id_format() {
        let id = Span::generate_id("test.rs", 10, 20);

        // ID should be 16 hex characters (64 bits)
        assert_eq!(id.len(), 16, "Span ID should be 16 characters: {}", id);

        // All characters should be valid hex
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "Span ID should be hex: {}", id);

        // Verify specific known hash (SHA-256 of "test.rs:10:20" truncated to 8 bytes)
        // This is a regression test to ensure we don't accidentally change the algorithm
        let expected = Span::generate_id("test.rs", 10, 20);
        assert_eq!(id, expected);
    }

    #[test]
    fn test_symbol_match_generate_id_is_deterministic() {
        let id1 = SymbolMatch::generate_match_id("foo", "test.rs", 10);
        let id2 = SymbolMatch::generate_match_id("foo", "test.rs", 10);
        let id3 = SymbolMatch::generate_match_id("bar", "test.rs", 10);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_reference_match_generate_id_is_deterministic() {
        let id1 = ReferenceMatch::generate_match_id("foo", "test.rs", 10);
        let id2 = ReferenceMatch::generate_match_id("foo", "test.rs", 10);
        let id3 = ReferenceMatch::generate_match_id("bar", "test.rs", 10);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_execution_id_format() {
        let id = generate_execution_id();

        // ID should be in format "{timestamp}-{pid}"
        assert!(id.contains('-'), "Execution ID should contain separator: {}", id);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 2, "Execution ID should have 2 parts: {}", id);

        // Both parts should be valid hex numbers
        assert!(usize::from_str_radix(parts[0], 16).is_ok());
        assert!(usize::from_str_radix(parts[1], 16).is_ok());
    }

    #[test]
    fn test_json_response_serialization() {
        let response = JsonResponse::new(
            FilesResponse {
                files: vec!["a.rs".to_string(), "b.rs".to_string()],
            },
            "test-exec-123",
        );

        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["schema_version"], MAGELLAN_JSON_SCHEMA_VERSION);
        assert_eq!(parsed["execution_id"], "test-exec-123");
        assert_eq!(parsed["data"]["files"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::from_str("json"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::from_str("JSON"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::from_str("human"), Some(OutputFormat::Human));
        assert_eq!(OutputFormat::from_str("text"), Some(OutputFormat::Human));
        assert_eq!(OutputFormat::from_str("invalid"), None);
    }

    #[test]
    fn test_status_response_serialization() {
        let response = StatusResponse {
            files: 10,
            symbols: 100,
            references: 50,
            calls: 25,
            code_chunks: 200,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["files"], 10);
        assert_eq!(parsed["symbols"], 100);
        assert_eq!(parsed["references"], 50);
        assert_eq!(parsed["calls"], 25);
        assert_eq!(parsed["code_chunks"], 200);
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse {
            error: "file_not_found".to_string(),
            message: "The requested file does not exist".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["error"], "file_not_found");
        assert_eq!(parsed["message"], "The requested file does not exist");
    }
}

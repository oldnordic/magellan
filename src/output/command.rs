//! JSON output types for CLI commands
//!
//! Provides schema-versioned, span-aware response types for all query commands.
//! Follows Phase 3 CLI Output Contract specification.
//!
//! # Span Model
//!
//! A [`Span`] represents a half-open range in source code with stable identification.
//! Spans are the foundation for all symbol locations, references, and call graph data.
//!
//! ## Range Semantics
//!
//! Spans use **half-open ranges** `[start, end)` where:
//! - `start` is **inclusive** — the first byte INCLUDED in the span
//! - `end` is **exclusive** — the first byte NOT included in the span
//!
//! ### Example
//!
//! ```text
//! Source: "fn main() {}"
//!          0123456789...
//!
//! Span for "main": byte_start=3, byte_end=7
//!   - start=3 points to 'm' (INCLUDED)
//!   - end=7 points to '(' (NOT included)
//!   - Length = 7 - 3 = 4
//!   - Slice: source[3..7] == "main"
//! ```
//!
//! Half-open ranges enable:
//! - Simple length calculation: `length = end - start`
//! - Adjacent spans without overlap: `[0, 5)` and `[5, 10)` are contiguous
//! - Empty spans: `start == end` represents a zero-width position
//!
//! ## UTF-8 Byte Offsets
//!
//! All offsets are **UTF-8 byte offsets**, not character indices. This matches:
//! - Tree-sitter's byte-based API (`start_byte()`, `end_byte()`)
//! - Rust's string slicing (`&str[start..end]`)
//! - The SCIP protocol's UTF-8 encoding option
//!
//! ### Column Convention
//!
//! - `start_col` and `end_col` are **byte offsets within the line** (not character columns)
//! - Multi-byte UTF-8 characters (emoji, CJK) occupy multiple column positions
//!
//! Example: In `"let x = 🚀;"`, the emoji occupies 4 bytes in UTF-8.
//!
//! ## Line Numbering
//!
//! - Lines are **1-indexed** for user-friendliness (matches editor line numbers)
//! - Tree-sitter internally uses 0-indexed lines, so we add 1 during conversion
//!
//! ## Span ID Generation
//!
//! Each [`Span`] has a stable `span_id` generated via SHA-256:
//!
//! ```text
//! input = file_path + ":" + byte_start + ":" + byte_end
//! hash = SHA256(input)
//! span_id = first 8 bytes of hash (16 hex characters)
//! ```
//!
//! ### Stability Guarantees
//!
//! The span ID is **position-based only** (not content-based):
//!
//! - **Stable across:** Content changes at the same position, whitespace changes elsewhere
//! - **Changes when:** The position shifts (edits before the span), file path changes
//! - **Never depends on:** The actual source code content
//!
//! This design ensures the span ID identifies "the span at position X in file Y,"
//! which is appropriate for static analysis tools.
//!
//! ## Usage Examples
//!
//! ### Extracting Text from a Span
//!
//! ```rust
//! use magellan::output::command::Span;
//!
//! let source = "fn main() { println!(\"Hello\"); }";
//! let span = Span::new("main.rs".into(), 3, 7, 1, 3, 1, 7);
//!
//! // Safe extraction using get()
//! let text = source.get(span.byte_start..span.byte_end).unwrap();
//! assert_eq!(text, "main");
//! ```
//!
//! ### Validating Spans
//!
//! ```rust
//! use magellan::output::command::Span;
//!
//! fn validate_span(source: &str, span: &Span) -> bool {
//!     if span.byte_start > span.byte_end {
//!         return false;
//!     }
//!     if span.byte_end > source.len() {
//!         return false;
//!     }
//!     // Check UTF-8 boundaries
//!     source.is_char_boundary(span.byte_start)
//!         && source.is_char_boundary(span.byte_end)
//! }
//! ```
//!
//! ### Serialization
//!
//! [`Span`] implements `Serialize` and `Deserialize` for JSON output:
//!
//! ```rust
//! # use magellan::output::command::Span;
//! let span = Span::new("file.rs".into(), 10, 20, 2, 0, 2, 10);
//! let json = serde_json::to_string(&span).unwrap();
//! ```
//!
//! ## Standards Alignment
//!
//! Magellan's span model aligns with industry standards:
//!
//! | Aspect | Magellan | LSP | SCIP | Tree-sitter |
//! |--------|----------|-----|------|-------------|
//! | Range | Half-open `[start, end)` | Half-open | Half-open | Half-open |
//! | Offset basis | UTF-8 bytes | UTF-16 units | Configurable | UTF-8 bytes |
//! | Lines | 1-indexed | 0-indexed | 0-indexed | 0-indexed |
//! | Columns | Byte-based | UTF-16 units | Configurable | Byte-based |
//!
//! ## Further Reading
//!
//! - Phase 4 Research: `.planning/phases/04-canonical-span-model/04-RESEARCH.md`
//! - LSP Specification: <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/>
//! - SCIP Protocol: <https://github.com/sourcegraph/scip>

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::output::rich::{SpanContext, SpanRelationships, SpanSemantics, SpanChecksums};

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
            tool: Some("magellan".to_string()),
            timestamp: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
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
/// Represents a **half-open range** `[start, end)` where:
/// - `byte_start` is inclusive (first byte INCLUDED)
/// - `byte_end` is exclusive (first byte NOT included)
///
/// All offsets are UTF-8 byte positions. Lines are 1-indexed for user-friendliness.
/// Columns are 0-indexed byte offsets within each line.
///
/// # Examples
///
/// Creating a span and extracting text:
///
/// ```
/// use magellan::output::command::Span;
///
/// let source = "fn main() { println!(\"Hello\"); }";
/// let span = Span::new(
///     "main.rs".into(),  // file_path
///     3,   // byte_start (points to 'm')
///     7,   // byte_end (points to '(')
///     1,   // start_line (1-indexed)
///     3,   // start_col (byte offset in line)
///     1,   // end_line
///     7,   // end_col
/// );
///
/// // Extract text using the span
/// let text = source.get(span.byte_start..span.byte_end).unwrap();
/// assert_eq!(text, "main");
/// ```
///
/// # Safety
///
/// **Always use `.get()` for UTF-8 safe slicing:**
///
/// ```
/// # use magellan::output::command::Span;
/// # let source = "fn main() {}";
/// # let span = Span::new("test.rs".into(), 3, 7, 1, 3, 1, 7);
/// // SAFE: Returns Option<&str>, None if out of bounds
/// let text = source.get(span.byte_start..span.byte_end);
///
/// // UNSAFE: Can panic on invalid UTF-8 boundaries
/// // let text = &source[span.byte_start..span.byte_end];
/// ```
///
/// # Serialization
///
/// `Span` implements `Serialize` and `Deserialize` for JSON output.
/// All fields are public and included in serialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Span {
    /// Stable span ID (SHA-256 hash of file_path:byte_start:byte_end)
    ///
    /// This ID is deterministic and platform-independent.
    /// See [`Span::generate_id`] for the algorithm details.
    pub span_id: String,
    /// File path (absolute or root-relative)
    ///
    /// Use consistent paths for stable IDs. The path is included
    /// in the span ID hash, so different representations of the same
    /// file (e.g., `./main.rs` vs `main.rs`) produce different IDs.
    pub file_path: String,
    /// Byte range start (inclusive, first byte INCLUDED)
    ///
    /// UTF-8 byte offset from the start of the file.
    pub byte_start: usize,
    /// Byte range end (exclusive, first byte NOT included)
    ///
    /// UTF-8 byte offset. The span covers `[byte_start, byte_end)`.
    /// Length is `byte_end - byte_start`.
    pub byte_end: usize,
    /// Start line (1-indexed)
    ///
    /// Line number where the span starts, counting from 1.
    /// Matches editor line numbers.
    pub start_line: usize,
    /// Start column (0-indexed, byte-based)
    ///
    /// Byte offset within `start_line` where the span begins.
    /// This is a byte offset, not a character offset.
    pub start_col: usize,
    /// End line (1-indexed)
    ///
    /// Line number where the span ends.
    pub end_line: usize,
    /// End column (0-indexed, byte-based)
    ///
    /// Byte offset within `end_line` where the span ends (exclusive).
    pub end_col: usize,

    // Rich span extensions (optional, opt-in via CLI flags)

    /// Context lines around the span
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<SpanContext>,

    /// Semantic information (kind, language) - grouped in a single struct
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantics: Option<SpanSemantics>,

    /// Relationship information (callers, callees, imports, exports)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationships: Option<SpanRelationships>,

    /// Checksums for content verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksums: Option<SpanChecksums>,
}

impl Span {
    /// Generate a stable span ID from (file_path, byte_start, byte_end)
    ///
    /// Uses SHA-256 for platform-independent, deterministic span IDs.
    ///
    /// # Algorithm
    ///
    /// The hash is computed from: `file_path + ":" + byte_start + ":" + byte_end`
    /// The first 8 bytes (64 bits) of the hash are formatted as 16 hex characters.
    ///
    /// # Properties
    ///
    /// This ensures span IDs are:
    /// - **Deterministic**: same inputs always produce the same ID
    /// - **Platform-independent**: SHA-256 produces consistent results across architectures
    /// - **Collision-resistant**: 64-bit space with good distribution
    ///
    /// # Stability
    ///
    /// The span ID format is part of Magellan's stable API contract.
    /// IDs generated by this function will remain consistent across versions.
    ///
    /// # Examples
    ///
    /// ```
    /// use magellan::output::command::Span;
    ///
    /// let id1 = Span::generate_id("main.rs", 10, 20);
    /// let id2 = Span::generate_id("main.rs", 10, 20);
    /// let id3 = Span::generate_id("main.rs", 10, 21);
    ///
    /// assert_eq!(id1, id2);  // Same inputs = same ID
    /// assert_ne!(id1, id3);  // Different inputs = different ID
    /// assert_eq!(id1.len(), 16);  // Always 16 hex characters
    /// ```
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
    ///
    /// Constructs a [`Span`] with a stable [`span_id`](Self::span_id) automatically
    /// generated using [`Span::generate_id`].
    ///
    /// # Parameters
    ///
    /// - `file_path`: Path to the source file (absolute or root-relative)
    /// - `byte_start`: UTF-8 byte offset where the span starts (inclusive)
    /// - `byte_end`: UTF-8 byte offset where the span ends (exclusive)
    /// - `start_line`: Line number where the span starts (1-indexed)
    /// - `start_col`: Byte offset within `start_line` where the span starts (0-indexed)
    /// - `end_line`: Line number where the span ends (1-indexed)
    /// - `end_col`: Byte offset within `end_line` where the span ends (0-indexed, exclusive)
    ///
    /// # Half-Open Convention
    ///
    /// The span uses half-open range semantics `[byte_start, byte_end)`:
    /// - `byte_start` is **inclusive** (first byte included)
    /// - `byte_end` is **exclusive** (first byte NOT included)
    ///
    /// # Examples
    ///
    /// ```
    /// use magellan::output::command::Span;
    ///
    /// let span = Span::new(
    ///     "main.rs".into(),  // file_path
    ///     3,   // byte_start (inclusive)
    ///     7,   // byte_end (exclusive)
    ///     1,   // start_line (1-indexed)
    ///     3,   // start_col (byte offset, 0-indexed)
    ///     1,   // end_line
    ///     7,   // end_col (byte offset, 0-indexed)
    /// );
    ///
    /// assert_eq!(span.byte_end - span.byte_start, 4);  // Length
    /// assert_eq!(span.span_id.len(), 16);  // Stable ID
    /// ```
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
            context: None,
            semantics: None,
            relationships: None,
            checksums: None,
        }
    }
}

/// Symbol match result for query/find commands
///
/// Represents a symbol found during a query, including its location ([`Span`]),
/// name, kind (function, variable, type, etc.), and optional parent symbol
/// for nested definitions.
///
/// # Examples
///
/// Creating a symbol match:
///
/// ```
/// use magellan::output::command::{Span, SymbolMatch};
///
/// let span = Span::new("main.rs".into(), 3, 7, 1, 3, 1, 7);
/// let symbol = SymbolMatch::new(
///     "main".into(),    // name
///     "Function".into(), // kind
///     span,
///     None,             // no parent
///     None,             // no symbol_id
/// );
///
/// assert_eq!(symbol.name, "main");
/// assert_eq!(symbol.kind, "Function");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    /// Stable match ID
    ///
    /// Generated from symbol name, file path, and byte position.
    /// See [`SymbolMatch::generate_match_id`] for details.
    pub match_id: String,
    /// Symbol span (location in source code)
    pub span: Span,
    /// Symbol name
    pub name: String,
    /// Symbol kind (normalized)
    ///
    /// Examples: "Function", "Variable", "Struct", "Enum", "Method", etc.
    pub kind: String,
    /// Containing symbol (if nested)
    ///
    /// For nested symbols like methods inside structs or closures,
    /// this field contains the parent symbol's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Stable symbol ID
    ///
    /// Generated from language, fully-qualified name, and defining span.
    /// Corresponds to the symbol's stable identifier across runs.
    /// This ID is computed by [`crate::graph::schema::generate_symbol_id`]
    /// and stored in the graph's SymbolNode data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
}

impl SymbolMatch {
    /// Generate a stable match ID for a symbol
    ///
    /// Uses `DefaultHasher` to combine the symbol name, file path, and byte position
    /// into a unique hexadecimal identifier.
    ///
    /// # Algorithm
    ///
    /// The hash combines:
    /// - Symbol name (e.g., "main")
    /// - File path (e.g., "src/main.rs")
    /// - Byte start position (e.g., 42)
    ///
    /// # Examples
    ///
    /// ```
    /// use magellan::output::command::SymbolMatch;
    ///
    /// let id1 = SymbolMatch::generate_match_id("main", "main.rs", 3);
    /// let id2 = SymbolMatch::generate_match_id("main", "main.rs", 3);
    /// let id3 = SymbolMatch::generate_match_id("foo", "main.rs", 3);
    ///
    /// assert_eq!(id1, id2);  // Same inputs = same ID
    /// assert_ne!(id1, id3);  // Different symbol name = different ID
    /// ```
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
    ///
    /// Constructs a [`SymbolMatch`] with a stable [`match_id`](Self::match_id)
    /// automatically generated using [`SymbolMatch::generate_match_id`].
    ///
    /// # Parameters
    ///
    /// - `name`: The symbol name (e.g., "main", "MyStruct")
    /// - `kind`: The symbol kind (e.g., "Function", "Struct", "Variable")
    /// - `span`: Location of the symbol in source code
    /// - `parent`: Optional parent symbol name for nested definitions
    /// - `symbol_id`: Optional stable symbol ID from the graph (computed from
    ///   language, fully-qualified name, and span)
    ///
    /// # Examples
    ///
    /// ```
    /// use magellan::output::command::{Span, SymbolMatch};
    ///
    /// let span = Span::new("main.rs".into(), 3, 7, 1, 3, 1, 7);
    /// let symbol = SymbolMatch::new(
    ///     "main".into(),
    ///     "Function".into(),
    ///     span,
    ///     None,
    ///     Some("a1b2c3d4e5f6g7h8".into()),  // symbol_id
    /// );
    ///
    /// assert_eq!(symbol.name, "main");
    /// assert!(!symbol.match_id.is_empty());
    /// assert_eq!(symbol.symbol_id, Some("a1b2c3d4e5f6g7h8".into()));
    /// ```
    ///
    /// # Symbol ID Stability
    ///
    /// The `symbol_id` field provides a stable identifier for the symbol across
    /// different indexing runs. When present, it can be used to correlate the
    /// same symbol across different database snapshots or execution runs.
    ///
    /// The ID is computed from:
    /// - Language (e.g., "rust", "python")
    /// - Fully-qualified name (FQN)
    /// - Span ID (stable position-based identifier)
    ///
    /// See [`crate::graph::schema::generate_symbol_id`] for details.
    pub fn new(
        name: String,
        kind: String,
        span: Span,
        parent: Option<String>,
        symbol_id: Option<String>,
    ) -> Self {
        let match_id = Self::generate_match_id(&name, &span.file_path, span.byte_start);
        SymbolMatch {
            match_id,
            span,
            name,
            kind,
            parent,
            symbol_id,
        }
    }
}

/// Reference match result for refs command
///
/// Represents a reference to a symbol, including the location of the reference
/// ([`Span`]), the name of the symbol being referenced, an optional reference
/// kind for categorization (e.g., "call", "read", "write"), and the stable
/// symbol ID of the referenced symbol for cross-run correlation.
///
/// # Examples
///
/// Creating a reference match:
///
/// ```
/// use magellan::output::command::{Span, ReferenceMatch};
///
/// let span = Span::new("main.rs".into(), 10, 14, 2, 4, 2, 8);
/// let reference = ReferenceMatch::new(
///     span,
///     "println".into(),  // referenced_symbol
///     Some("call".into()), // reference_kind
///     Some("abc123def456".into()), // target_symbol_id
/// );
///
/// assert_eq!(reference.referenced_symbol, "println");
/// assert_eq!(reference.reference_kind, Some("call".into()));
/// assert_eq!(reference.target_symbol_id, Some("abc123def456".into()));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceMatch {
    /// Stable match ID
    ///
    /// Generated from referenced symbol, file path, and byte position.
    /// See [`ReferenceMatch::generate_match_id`] for details.
    pub match_id: String,
    /// Reference span (location where the reference occurs)
    pub span: Span,
    /// Referenced symbol name
    ///
    /// The name of the symbol being referenced (e.g., a function or variable name).
    pub referenced_symbol: String,
    /// Reference kind (optional, for categorization)
    ///
    /// Examples: "call", "read", "write", "type_ref", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_kind: Option<String>,
    /// Stable symbol ID of the referenced symbol
    ///
    /// This is the stable identifier (computed from language, FQN, and span) of the
    /// symbol being referenced. When present, it enables stable correlation across
    /// different indexing runs and database snapshots.
    ///
    /// This field is optional for backward compatibility with existing JSON consumers.
    /// Symbols indexed before this feature was added will have `None` here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_symbol_id: Option<String>,
}

impl ReferenceMatch {
    /// Generate a stable match ID for a reference
    ///
    /// Uses `DefaultHasher` to combine the referenced symbol name, file path,
    /// and byte position into a unique hexadecimal identifier with a "ref_" prefix.
    ///
    /// # Algorithm
    ///
    /// The hash combines:
    /// - Referenced symbol name (e.g., "println")
    /// - File path (e.g., "src/main.rs")
    /// - Byte start position (e.g., 42)
    ///
    /// The result is prefixed with "ref_" to distinguish reference IDs from symbol IDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use magellan::output::command::ReferenceMatch;
    ///
    /// let id1 = ReferenceMatch::generate_match_id("println", "main.rs", 10);
    /// let id2 = ReferenceMatch::generate_match_id("println", "main.rs", 10);
    /// let id3 = ReferenceMatch::generate_match_id("foo", "main.rs", 10);
    ///
    /// assert_eq!(id1, id2);  // Same inputs = same ID
    /// assert_ne!(id1, id3);  // Different symbol = different ID
    /// assert!(id1.starts_with("ref_"));  // Has prefix
    /// ```
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
    ///
    /// Constructs a [`ReferenceMatch`] with a stable [`match_id`](Self::match_id)
    /// automatically generated using [`ReferenceMatch::generate_match_id`].
    ///
    /// # Parameters
    ///
    /// - `span`: Location where the reference occurs in source code
    /// - `referenced_symbol`: Name of the symbol being referenced
    /// - `reference_kind`: Optional kind categorization (e.g., "call", "read", "write")
    /// - `target_symbol_id`: Optional stable symbol ID of the referenced symbol
    ///
    /// # Examples
    ///
    /// ```
    /// use magellan::output::command::{Span, ReferenceMatch};
    ///
    /// let span = Span::new("main.rs".into(), 10, 14, 2, 4, 2, 8);
    /// let reference = ReferenceMatch::new(
    ///     span,
    ///     "println".into(),
    ///     Some("call".into()),
    ///     Some("abc123def456".into()),
    /// );
    ///
    /// assert_eq!(reference.referenced_symbol, "println");
    /// assert!(!reference.match_id.is_empty());
    /// assert_eq!(reference.target_symbol_id, Some("abc123def456".into()));
    /// ```
    pub fn new(
        span: Span,
        referenced_symbol: String,
        reference_kind: Option<String>,
        target_symbol_id: Option<String>,
    ) -> Self {
        let match_id = Self::generate_match_id(&referenced_symbol, &span.file_path, span.byte_start);
        ReferenceMatch {
            match_id,
            span,
            referenced_symbol,
            reference_kind,
            target_symbol_id,
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
    /// Symbol count per file (optional, when --symbols flag is used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_counts: Option<std::collections::HashMap<String, usize>>,
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

/// Response for validation command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    /// Whether validation passed
    pub passed: bool,
    /// Number of errors found
    pub error_count: usize,
    /// Detailed error information
    pub errors: Vec<ValidationError>,
    /// Number of warnings found
    pub warning_count: usize,
    /// Detailed warning information
    pub warnings: Vec<ValidationWarning>,
}

/// A validation error with structured data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Machine-readable error code (SCREAMING_SNAKE_CASE)
    pub code: String,
    /// Human-readable error description
    pub message: String,
    /// Related stable symbol_id if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

/// A validation warning with structured data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Machine-readable warning code (SCREAMING_SNAKE_CASE)
    pub code: String,
    /// Human-readable warning description
    pub message: String,
    /// Related stable symbol_id if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

// Conversion from internal validation report to public response type
impl From<crate::graph::validation::ValidationReport> for ValidationResponse {
    fn from(report: crate::graph::validation::ValidationReport) -> Self {
        ValidationResponse {
            passed: report.passed,
            error_count: report.errors.len(),
            errors: report.errors.into_iter().map(|e| ValidationError {
                code: e.code,
                message: e.message,
                entity_id: e.entity_id,
                details: e.details,
            }).collect(),
            warning_count: report.warnings.len(),
            warnings: report.warnings.into_iter().map(|w| ValidationWarning {
                code: w.code,
                message: w.message,
                entity_id: w.entity_id,
                details: w.details,
            }).collect(),
        }
    }
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
                symbol_counts: None,
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

    // === Task 04-02.1: Span ID determinism and uniqueness tests ===

    #[test]
    fn test_span_id_deterministic_multiple_calls() {
        // Call generate_id() 100 times with same inputs, verify all equal
        let file_path = "src/main.rs";
        let byte_start = 42;
        let byte_end = 100;

        let first_id = Span::generate_id(file_path, byte_start, byte_end);

        for _ in 0..100 {
            let id = Span::generate_id(file_path, byte_start, byte_end);
            assert_eq!(id, first_id,
                "generate_id() must return identical ID for same inputs every time");
        }
    }

    #[test]
    fn test_span_id_unique_different_files() {
        // Same position in different files produces different IDs
        let byte_start = 10;
        let byte_end = 20;

        let id1 = Span::generate_id("src/main.rs", byte_start, byte_end);
        let id2 = Span::generate_id("lib/main.rs", byte_start, byte_end);
        let id3 = Span::generate_id("src/helper.rs", byte_start, byte_end);

        assert_ne!(id1, id2, "Different file paths should produce different IDs");
        assert_ne!(id1, id3, "Different file paths should produce different IDs");
        assert_ne!(id2, id3, "Different file paths should produce different IDs");
    }

    #[test]
    fn test_span_id_unique_different_positions() {
        // Same file, different positions produce different IDs
        let file_path = "test.rs";

        let id1 = Span::generate_id(file_path, 0, 10);
        let id2 = Span::generate_id(file_path, 10, 20);
        let id3 = Span::generate_id(file_path, 0, 20);
        let id4 = Span::generate_id(file_path, 5, 15);

        assert_ne!(id1, id2, "Different positions should produce different IDs");
        assert_ne!(id1, id3, "Different span lengths should produce different IDs");
        assert_ne!(id2, id3, "Different positions should produce different IDs");
        assert_ne!(id1, id4, "Different start positions should produce different IDs");
    }

    #[test]
    fn test_span_id_zero_length_span() {
        // Span where start == end is valid and produces stable ID
        let file_path = "test.rs";
        let position = 50;

        let id1 = Span::generate_id(file_path, position, position);
        let id2 = Span::generate_id(file_path, position, position);

        assert_eq!(id1.len(), 16, "Zero-length span ID should still be 16 hex characters");
        assert_eq!(id1, id2, "Zero-length span ID should be stable");
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()), "Zero-length span ID should be valid hex");
    }

    #[test]
    fn test_span_id_case_sensitive() {
        // File paths are case-sensitive
        let byte_start = 10;
        let byte_end = 20;

        let id_lower = Span::generate_id("test.rs", byte_start, byte_end);
        let id_upper = Span::generate_id("TEST.rs", byte_start, byte_end);
        let id_mixed = Span::generate_id("Test.rs", byte_start, byte_end);

        assert_ne!(id_lower, id_upper, "File path case should affect span ID");
        assert_ne!(id_lower, id_mixed, "File path case should affect span ID");
        assert_ne!(id_upper, id_mixed, "File path case should affect span ID");
    }

    #[test]
    fn test_span_id_large_offsets() {
        // Verify large byte offsets (common in big files) work correctly
        let file_path = "large_file.rs";

        let id1 = Span::generate_id(file_path, 1_000_000, 1_000_100);
        let id2 = Span::generate_id(file_path, 1_000_000, 1_000_100);

        assert_eq!(id1, id2, "Large offsets should produce stable IDs");
        assert_eq!(id1.len(), 16, "Large offset span ID should be 16 characters");

        // Different large offsets produce different IDs
        let id3 = Span::generate_id(file_path, 1_000_001, 1_000_100);
        assert_ne!(id1, id3, "Different start positions with large offsets should differ");
    }

    // === Task 04-02.2: UTF-8 safety tests ===

    #[test]
    fn test_span_id_utf8_file_path() {
        // Non-ASCII characters in file path handled correctly
        let byte_start = 0;
        let byte_end = 10;

        // UTF-8 encoded paths with non-ASCII characters
        let id1 = Span::generate_id("src/test.rs", byte_start, byte_end);
        let id2 = Span::generate_id("src/test.rs", byte_start, byte_end);
        let id3 = Span::generate_id("src/test文件.rs", byte_start, byte_end);  // Chinese characters
        let id4 = Span::generate_id("src/testфайл.rs", byte_start, byte_end);  // Cyrillic characters

        assert_eq!(id1, id2, "ASCII path should produce stable ID");
        assert_eq!(id1.len(), 16, "ASCII path span ID should be 16 characters");
        assert_eq!(id3.len(), 16, "Chinese path span ID should be 16 characters");
        assert_eq!(id4.len(), 16, "Cyrillic path span ID should be 16 characters");

        assert_ne!(id1, id3, "Different paths (ASCII vs Chinese) should produce different IDs");
        assert_ne!(id1, id4, "Different paths (ASCII vs Cyrillic) should produce different IDs");
        assert_ne!(id3, id4, "Different paths (Chinese vs Cyrillic) should produce different IDs");
    }

    #[test]
    fn test_span_id_multibyte_characters() {
        // Emoji, CJK characters in file path
        let byte_start = 5;
        let byte_end = 15;

        // Emoji in path (rocket is 3 bytes in UTF-8)
        let id_emoji = Span::generate_id("src/test.rs", byte_start, byte_end);
        let id_with_emoji = Span::generate_id("src/test-test.rs", byte_start, byte_end);

        assert_eq!(id_emoji.len(), 16, "Span ID with emoji path should be 16 characters");
        assert_eq!(id_with_emoji.len(), 16, "Span ID with emoji in name should be 16 characters");
        assert_ne!(id_emoji, id_with_emoji, "Different paths should produce different IDs");

        // CJK (Chinese/Japanese/Korean) characters are multi-byte
        let id_cjk = Span::generate_id("src/テスト.rs", byte_start, byte_end);
        assert_eq!(id_cjk.len(), 16, "CJK path span ID should be 16 characters");

        // Korean characters
        let id_korean = Span::generate_id("src/테스트.rs", byte_start, byte_end);
        assert_eq!(id_korean.len(), 16, "Korean path span ID should be 16 characters");

        // All different
        assert_ne!(id_emoji, id_cjk, "Different paths (ASCII vs CJK) should differ");
        assert_ne!(id_cjk, id_korean, "Different paths (Japanese vs Korean) should differ");
    }

    #[test]
    fn test_utf8_safe_extraction() {
        // Demonstrate using source.get(byte_start..byte_end) for safe slicing
        let source = "fn main() { let x = 42; }";

        // Safe extraction using get() returns Option<&str>
        let byte_start = 3;
        let byte_end = 7;

        let extracted = source.get(byte_start..byte_end);
        assert_eq!(extracted, Some("main"), "Safe extraction should work for valid UTF-8");

        // Out of bounds returns None instead of panic
        let out_of_bounds = source.get(10..1000);
        assert_eq!(out_of_bounds, None, "Out of bounds extraction should return None");
    }

    #[test]
    fn test_utf8_validation() {
        // Use source.is_char_boundary() to validate offsets
        // Use a multi-byte Unicode character (e with acute accent: \u{e9} = 0xc3 0xa9 in UTF-8)
        let source = "Hello\u{e9}";  // "Hello" (5) + "é" (2) = 7 bytes

        // Valid boundaries
        assert!(source.is_char_boundary(0), "Byte 0 is always a valid boundary");
        assert!(source.is_char_boundary(5), "After 'Hello' is valid (start of multi-byte char)");
        assert!(source.is_char_boundary(7), "After 'é' is valid (end of string)");
        assert!(source.is_char_boundary(source.len()), "End of string is valid boundary");

        // Invalid boundaries (middle of the 2-byte 'é' character)
        assert!(!source.is_char_boundary(6), "Byte 6 is in the middle of the 2-byte 'é'");
    }

    #[test]
    fn test_utf8_validation_three_byte_char() {
        // Test with a 3-byte UTF-8 character (CJK)
        let source = "test\u{4e2d}";  // "test" (4) + "" (3) = 7 bytes

        assert!(source.is_char_boundary(0), "Start is boundary");
        assert!(source.is_char_boundary(4), "After 'test' is boundary");
        assert!(source.is_char_boundary(7), "After Chinese char is boundary");
        assert!(source.is_char_boundary(source.len()), "End of string is valid");

        // Middle of the 3-byte Chinese character
        assert!(!source.is_char_boundary(5), "Byte 5 is in the middle of 3-byte char");
        assert!(!source.is_char_boundary(6), "Byte 6 is in the middle of 3-byte char");
    }

    #[test]
    fn test_span_id_unicode_normalization_difference() {
        // Different Unicode representations of the same visual character
        // produce different span IDs (by design - we use bytes as-is)
        let byte_start = 0;
        let byte_end = 10;

        // "cafe" with combining acute accent (e + combining acute)
        let decomposed = "cafe\u{0301}.rs";  // 5 bytes for "cafe" + 2 for combining acute + 3 for ".rs"
        let id1 = Span::generate_id(decomposed, byte_start, byte_end);

        // "cafe" with precomposed 'é' character
        let precomposed = "caf\u{e9}.rs";  // 4 bytes for "caf" + 2 for é + 3 for ".rs"
        let id2 = Span::generate_id(precomposed, byte_start, byte_end);

        assert_ne!(id1, id2,
            "Different Unicode representations should produce different span IDs (by design)");
    }

    #[test]
    fn test_span_id_with_path_separator_variants() {
        // Different path representations produce different IDs
        // (Important: users should canonicalize paths before use)
        let byte_start = 10;
        let byte_end = 20;

        let id1 = Span::generate_id("src/test.rs", byte_start, byte_end);
        let id2 = Span::generate_id("./src/test.rs", byte_start, byte_end);
        let id3 = Span::generate_id("/abs/path/src/test.rs", byte_start, byte_end);

        assert_ne!(id1, id2, "Relative vs explicit path should differ");
        assert_ne!(id1, id3, "Relative vs absolute path should differ");
        assert_ne!(id2, id3, "Different path forms should differ");
    }

    // === Task 05-03.5: SymbolMatch symbol_id tests ===

    #[test]
    fn test_symbol_match_with_symbol_id() {
        // Verify SymbolMatch includes symbol_id when present
        let span = Span::new("main.rs".into(), 3, 7, 1, 3, 1, 7);
        let symbol_id = Some("a1b2c3d4e5f6g7h8".to_string());

        let symbol = SymbolMatch::new(
            "main".into(),
            "Function".into(),
            span,
            None,
            symbol_id.clone(),
        );

        assert_eq!(symbol.symbol_id, symbol_id);
        assert_eq!(symbol.name, "main");
        assert_eq!(symbol.kind, "Function");
    }

    #[test]
    fn test_symbol_match_without_symbol_id() {
        // Verify SymbolMatch works without symbol_id
        let span = Span::new("lib.rs".into(), 10, 20, 2, 5, 2, 10);

        let symbol = SymbolMatch::new(
            "helper".into(),
            "Function".into(),
            span,
            None,
            None,
        );

        assert_eq!(symbol.symbol_id, None);
        assert_eq!(symbol.name, "helper");
    }

    #[test]
    fn test_symbol_match_symbol_id_serialization_includes_when_present() {
        // Verify symbol_id is included in JSON when present
        let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10);
        let symbol = SymbolMatch::new(
            "foo".into(),
            "Function".into(),
            span,
            None,
            Some("abc123def456".to_string()),
        );

        let json = serde_json::to_string(&symbol).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["symbol_id"].is_string());
        assert_eq!(parsed["symbol_id"], "abc123def456");
    }

    #[test]
    fn test_symbol_match_symbol_id_serialization_skips_when_none() {
        // Verify symbol_id is not included in JSON when None (skip_serializing_if)
        let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10);
        let symbol = SymbolMatch::new(
            "foo".into(),
            "Function".into(),
            span,
            None,
            None,
        );

        let json = serde_json::to_string(&symbol).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // symbol_id key should not be present when None
        assert!(parsed.get("symbol_id").is_none());
    }

    #[test]
    fn test_symbol_match_symbol_id_deserialization() {
        // Verify SymbolMatch can be deserialized with symbol_id
        let json_with_id = r#"{
            "match_id": "12345",
            "span": {
                "span_id": "abcd1234",
                "file_path": "main.rs",
                "byte_start": 3,
                "byte_end": 7,
                "start_line": 1,
                "start_col": 3,
                "end_line": 1,
                "end_col": 7
            },
            "name": "main",
            "kind": "Function",
            "symbol_id": "xyz789"
        }"#;

        let symbol: SymbolMatch = serde_json::from_str(json_with_id).unwrap();
        assert_eq!(symbol.symbol_id, Some("xyz789".to_string()));
        assert_eq!(symbol.name, "main");
    }

    #[test]
    fn test_symbol_match_symbol_id_deserialization_without_id() {
        // Verify SymbolMatch can be deserialized without symbol_id (backward compatible)
        let json_without_id = r#"{
            "match_id": "12345",
            "span": {
                "span_id": "abcd1234",
                "file_path": "main.rs",
                "byte_start": 3,
                "byte_end": 7,
                "start_line": 1,
                "start_col": 3,
                "end_line": 1,
                "end_col": 7
            },
            "name": "main",
            "kind": "Function"
        }"#;

        let symbol: SymbolMatch = serde_json::from_str(json_without_id).unwrap();
        assert_eq!(symbol.symbol_id, None);
        assert_eq!(symbol.name, "main");
    }
}

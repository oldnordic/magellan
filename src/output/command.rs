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

use crate::output::rich::{SpanChecksums, SpanContext, SpanRelationships, SpanSemantics};

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
    /// Tool name (e.g., "magellan")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// ISO 8601 timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Whether the response is partial (e.g., truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    /// Estimated token count (chars / 4 heuristic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_estimated: Option<usize>,
    /// Whether output was truncated to fit token budget
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
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
            tokens_estimated: None,
            truncated: None,
        }
    }

    /// Mark the response as partial
    pub fn with_partial(mut self, partial: bool) -> Self {
        self.partial = Some(partial);
        self
    }

    /// Set estimated token count
    pub fn with_tokens(mut self, tokens: usize) -> Self {
        self.tokens_estimated = Some(tokens);
        self
    }

    /// Mark the response as truncated
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = Some(truncated);
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
        format!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            result[0], result[1], result[2], result[3], result[4], result[5], result[6], result[7]
        )
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
    /// Set context on the span
    pub fn with_context(mut self, context: SpanContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Set semantic information on the span
    pub fn with_semantics(mut self, semantics: SpanSemantics) -> Self {
        self.semantics = Some(semantics);
        self
    }

    /// Set semantic information from kind and language strings
    pub fn with_semantics_from(mut self, kind: String, language: String) -> Self {
        self.semantics = Some(SpanSemantics::new(kind, language));
        self
    }

    /// Set relationships on the span
    pub fn with_relationships(mut self, relationships: SpanRelationships) -> Self {
        self.relationships = Some(relationships);
        self
    }

    /// Set checksums on the span
    pub fn with_checksums(mut self, checksums: SpanChecksums) -> Self {
        self.checksums = Some(checksums);
        self
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
    /// This ID is computed by crate::graph::schema::generate_symbol_id
    /// and stored in the graph's SymbolNode data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    /// Functions that call this symbol (cross-file callers)
    ///
    /// When requested via --with-callers, this contains the list of functions
    /// that call this symbol, along with their file paths and locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callers: Option<Vec<CallerInfo>>,
    /// Functions that this symbol calls (cross-file callees)
    ///
    /// When requested via --with-callees, this contains the list of functions
    /// that this symbol calls, along with their file paths and locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callees: Option<Vec<CalleeInfo>>,
}

/// Information about a function that calls a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerInfo {
    /// Name of the calling function
    pub name: String,
    /// File containing the call
    pub file_path: String,
    /// Line where call occurs
    pub line: usize,
    /// Column where call occurs
    pub column: usize,
}

/// Information about a function that a symbol calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalleeInfo {
    /// Name of the called function
    pub name: String,
    /// File containing the callee definition
    pub file_path: String,
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
    /// See crate::graph::schema::generate_symbol_id for details.
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
            callers: None,
            callees: None,
        }
    }

    /// Set caller and callee information for this symbol
    ///
    /// Used by the query command to populate cross-file call relationships
    /// when --with-callers or --with-callees flags are provided.
    pub fn with_callers_and_callees(
        mut self,
        callers: Option<Vec<CallerInfo>>,
        callees: Option<Vec<CalleeInfo>>,
    ) -> Self {
        self.callers = callers;
        self.callees = callees;
        self
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
        let match_id =
            Self::generate_match_id(&referenced_symbol, &span.file_path, span.byte_start);
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

/// Caller information with project attribution for cross-project queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCallerInfo {
    /// Source project name (derived from DB filename)
    pub project: String,
    /// Name of the calling function
    pub name: String,
    /// File containing the call
    pub file_path: String,
    /// Line where call occurs
    pub line: usize,
    /// Column where call occurs
    pub column: usize,
    /// Hop depth for recursive traversal (None = direct)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

/// Callee information with project attribution for cross-project queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCalleeInfo {
    /// Source project name (derived from DB filename)
    pub project: String,
    /// Name of the called function
    pub name: String,
    /// File containing the callee definition
    pub file_path: String,
    /// Line number where the call occurs
    pub line: u32,
    /// Hop depth for recursive traversal (None = direct)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

/// Symbol match with project attribution for multi-DB context queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSymbolMatch {
    /// Source project name (derived from DB filename)
    pub project: String,
    /// Stable match ID
    pub match_id: String,
    /// Symbol span (location in source code)
    pub span: Span,
    /// Symbol name
    pub name: String,
    /// Symbol kind (normalized)
    pub kind: String,
    /// Containing symbol (if nested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Stable symbol ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    /// Functions that call this symbol (cross-project)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callers: Option<Vec<ProjectCallerInfo>>,
    /// Functions that this symbol calls (cross-project)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callees: Option<Vec<ProjectCalleeInfo>>,
    /// Source code snippet (when --with-source is used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Response for context command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    /// Name that was queried
    pub query: String,
    /// Projects searched
    pub projects: Vec<String>,
    /// Matching symbols found across all projects
    pub matches: Vec<ProjectSymbolMatch>,
}

/// Collision candidate details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionCandidate {
    pub entity_id: i64,
    pub symbol_id: Option<String>,
    pub canonical_fqn: Option<String>,
    pub display_fqn: Option<String>,
    pub name: Option<String>,
    pub file_path: Option<String>,
}

/// Collision group response entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionGroup {
    pub field: String,
    pub value: String,
    pub count: usize,
    pub candidates: Vec<CollisionCandidate>,
}

/// Response for collisions command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionsResponse {
    pub field: String,
    pub groups: Vec<CollisionGroup>,
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
    /// Coverage data (always present for stable JSON shape)
    pub coverage: CoverageInfo,
}

/// Coverage information with stable JSON shape.
///
/// All fields are always serialized so consumers can rely on a fixed schema.
/// Use `available` to distinguish "no coverage data" from "zero coverage".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageInfo {
    /// Whether any coverage data has been ingested
    pub available: bool,
    /// Number of covered CFG blocks
    pub covered_blocks: usize,
    /// Number of covered CFG edges
    pub covered_edges: usize,
    /// Source kind (e.g. "lcov"), when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Source revision (e.g. git commit hash), when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Ingestion timestamp (RFC 3339), when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingested_at: Option<String>,
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

/// Response for migrate command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateResponse {
    /// Whether migration succeeded
    pub success: bool,
    /// Path to backup file (if created)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// Previous schema version
    pub old_version: i64,
    /// New schema version
    pub new_version: i64,
    /// Human-readable message
    pub message: String,
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
            errors: report
                .errors
                .into_iter()
                .map(|e| ValidationError {
                    code: e.code,
                    message: e.message,
                    entity_id: e.entity_id,
                    details: e.details,
                })
                .collect(),
            warning_count: report.warnings.len(),
            warnings: report
                .warnings
                .into_iter()
                .map(|w| ValidationWarning {
                    code: w.code,
                    message: w.message,
                    entity_id: w.entity_id,
                    details: w.details,
                })
                .collect(),
        }
    }
}

/// Response for errors in JSON mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Machine-readable error code (e.g., "MAG-REF-001")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Error category/type
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Related span for context (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Suggested remediation steps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

/// Output format for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output
    Human,
    /// JSON output (raw, compact)
    Json,
    /// JSON output (formatted with indentation)
    Pretty,
}

/// Response for program slicing command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceResponse {
    /// Target symbol being sliced
    pub target: SymbolMatch,
    /// Slice direction: "backward" or "forward"
    pub direction: String,
    /// Symbols included in the slice
    pub included_symbols: Vec<SymbolMatch>,
    /// Statistics about the slice
    pub statistics: SliceStats,
}

/// Statistics for program slice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceStats {
    /// Total number of symbols in the slice
    pub total_symbols: usize,
    /// Number of data dependencies (0 for call-graph fallback)
    pub data_dependencies: usize,
    /// Number of control dependencies
    pub control_dependencies: usize,
}

impl OutputFormat {
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "human" | "text" => Some(OutputFormat::Human),
            "json" => Some(OutputFormat::Json),
            "pretty" => Some(OutputFormat::Pretty),
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
        .unwrap_or_default()
        .as_secs();
    let pid = process::id();

    format!("{:x}-{:x}", timestamp, pid)
}

/// Output JSON to stdout
pub fn output_json<T: Serialize>(data: &T, format: OutputFormat) -> anyhow::Result<()> {
    let json = match format {
        OutputFormat::Json => serde_json::to_string(data)?,
        OutputFormat::Pretty => serde_json::to_string_pretty(data)?,
        OutputFormat::Human => anyhow::bail!("Human format not supported for JSON output"),
    };
    println!("{}", json);
    Ok(())
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;

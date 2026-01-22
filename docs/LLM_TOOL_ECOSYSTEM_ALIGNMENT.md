# LLM Tool Ecosystem Alignment

**Created:** 2026-01-22
**Purpose:** Align JSON formats and data structures across the LLM-native toolset (Magellan, Splice, llmtransform, llmsearch, llmastsearch, llmfilewrite) to create a unified feedback loop where tools provide LSP/AST information to prevent errors and reduce token waste.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Tool Inventory](#tool-inventory)
3. [Current State Analysis](#current-state-analysis)
4. [Common Patterns](#common-patterns)
5. [Key Differences](#key-differences)
6. [Alignment Recommendations](#alignment-recommendations)
7. [Proposed Standard Schema](#proposed-standard-schema)
8. [Implementation Roadmap](#implementation-roadmap)

---

## Executive Summary

### The Problem

The LLM tool ecosystem currently has inconsistent JSON output formats:
- Each tool uses different field names for the same concepts
- Coordinate representations vary (line_start vs start_line vs row)
- Error handling structures are not compatible
- Missing standardized metadata across tools
- This forces LLMs to re-read files and waste tokens on format translation

### The Vision

Create a unified feedback loop where:
1. Tools provide LSP/AST feedback in a **standard schema**
2. LLMs can consume output from any tool without format-specific parsing
3. Spans are consistent across all tools (byte + line/col)
4. Errors are structured and actionable
5. Execution IDs enable traceability across the entire workflow

**Result:** The LLM never edits or reads files blindly, never commits errors, and doesn't waste tokens re-reading files.

### Tools in Scope

| Tool | Purpose | Location | Status |
|------|---------|----------|--------|
| **Magellan** | Code indexing and symbol query | `/home/feanor/Projects/magellan` | Production |
| **Splice** | Span-safe refactoring with validation | `/home/feanor/Projects/splice` | Production |
| **llmtransform** | Span-safe text mutation | `/home/feanor/Projects/llmtransform` | Production |
| **llmsearch** | Structured text search | `/home/feanor/Projects/llmsearch` | Production |
| **llmastsearch** | AST-based code search (tree-sitter) | `/home/feanor/Projects/llmastsearch` | Production |
| **llmfilewrite** | Code creation with LSP validation | `/home/feanor/Projects/llmfilewrite` | Design Phase |

---

## Tool Inventory

### 1. Magellan (Code Indexing)

**Location:** `/home/feanor/Projects/magellan/src/output/command.rs`

**Key Structures:**

```rust
// Wrapper for all JSON responses
pub struct JsonResponse<T> {
    pub schema_version: String,      // "1.0"
    pub execution_id: String,         // UUID
    pub data: T,                      // Generic data payload
    pub partial: Option<bool>,        // Partial results indicator
}

// Span representation
pub struct Span {
    pub span_id: String,              // UUID
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,            // 1-indexed
    pub start_col: usize,             // 0-indexed
    pub end_line: usize,              // 1-indexed
    pub end_col: usize,               // 0-indexed
}

// Symbol match result
pub struct SymbolMatch {
    pub match_id: String,             // UUID
    pub span: Span,
    pub name: String,
    pub kind: String,                 // "function", "class", etc.
    pub parent: Option<String>,       // Parent symbol name
    pub symbol_id: Option<String>,    // Stable symbol identifier
}

// Reference match result
pub struct ReferenceMatch {
    pub match_id: String,
    pub span: Span,
    pub reference_type: String,
    def_symbol_id: String,
}
```

**Output Format:**
```json
{
  "schema_version": "1.0",
  "execution_id": "uuid-v4",
  "data": {
    "matches": [...]
  },
  "partial": false
}
```

---

### 2. Splice (Refactoring)

**Location:** `/home/feanor/Projects/splice/src/output.rs`

**Key Structures:**

```rust
// Top-level operation result
pub struct OperationResult {
    pub version: String,              // "2.0"
    pub operation_id: String,         // UUID
    pub operation_type: String,       // "patch", "delete", "rename"
    pub status: String,               // "success", "failure"
    pub message: String,
    pub timestamp: String,            // ISO 8601
    pub workspace: Option<String>,
    pub result: Option<OperationData>,
    pub error: Option<ErrorDetails>,
}

// Span result with checksums
pub struct SpanResult {
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,            // 1-indexed
    pub line_end: usize,              // 1-indexed
    pub col_start: usize,             // 0-indexed
    pub col_end: usize,               // 0-indexed
    pub span_id: String,              // UUID
    pub match_id: Option<String>,     // UUID
    pub content_checksum: String,     // SHA-256 of content
    pub surrounding_checksum: String, // SHA-256 with context
}

// Structured error payload
pub struct DiagnosticPayload {
    pub tool: String,                 // "splice"
    pub level: String,                // "error", "warning", "note"
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub code: Option<String>,         // SPLICE_E001, etc.
    pub note: Option<String>,
    pub remediation: Option<String>,  // How to fix
}
```

**Output Format:**
```json
{
  "version": "2.0",
  "operation_id": "uuid-v4",
  "operation_type": "patch",
  "status": "success",
  "message": "...",
  "timestamp": "2026-01-22T10:00:00Z",
  "result": {
    "spans": [...],
    "checksums": {...}
  }
}
```

---

### 3. llmtransform (Text Mutation)

**Location:** `/home/feanor/Projects/llmtransform/llm-transform/src/json.rs`

**Key Structures:**

```rust
// Edit request (input)
pub struct EditRequest {
    pub execution_id: String,
    pub file_path: String,
    pub expected_checksum: String,    // SHA-256
    pub edits: Vec<EditJson>,
}

// Individual edit
pub struct EditJson {
    pub byte_start: usize,
    pub byte_end: usize,
    pub new_content: String,
}

// Edit response (output)
pub struct EditResponse {
    pub execution_id: String,         // Same as request
    pub success: bool,
    pub final_checksum: String,       // SHA-256 after edits
    pub total_byte_shift: i64,        // For adjusting subsequent edits
    pub applied_count: usize,
    pub skipped_count: usize,
    pub error_count: usize,
    pub edits: Vec<PerEditResultJson>,
    pub error: Option<String>,
}

// Per-edit result
pub struct PerEditResultJson {
    pub byte_start: usize,
    pub byte_end: usize,
    pub status: String,               // "applied", "skipped", "error"
    pub error: Option<String>,
}
```

**Output Format:**
```json
{
  "execution_id": "uuid-v4",
  "success": true,
  "final_checksum": "sha256-hash",
  "total_byte_shift": 12,
  "applied_count": 2,
  "skipped_count": 0,
  "error_count": 0,
  "edits": [...]
}
```

---

### 4. llmsearch (Text Search)

**Location:** `/home/feanor/Projects/llmsearch/src/main.rs`

**Key Structures:**

```rust
// Single match
struct Match {
    match_id: String,                // UUID
    file: String,                    // NOT "file_path"
    byte_start: usize,
    byte_end: usize,
    matched_text: String,
    line_number: usize,              // NOT "line_start"
    column_number: usize,            // NOT "col_start"
    context_before: String,
    context_after: String,
}

// Search output
struct SearchOutput {
    execution_id: String,            // UUID
    pattern: String,
    matches: Vec<Match>,
    match_count: usize,
}
```

**Output Format:**
```json
{
  "execution_id": "uuid-v4",
  "pattern": "search_pattern",
  "matches": [...],
  "match_count": 5
}
```

---

### 5. llmastsearch (AST Query)

**Locations:**
- `/home/feanor/Projects/llmastsearch/src/query/result.rs`
- `/home/feanor/Projects/llmastsearch/src/query/ast.rs`
- `/home/feanor/Projects/llmastsearch/src/query/position.rs`

**Key Structures:**

```rust
// Query result wrapper
pub struct QueryResult {
    pub execution_id: Uuid,           // UUID type (not String)
    pub file_path: PathBuf,           // PathBuf type (not String)
    pub language: Language,           // Enum: Rust, Python, C, etc.
    pub query: String,
    pub matches: Vec<Match>,
    pub match_count: usize,
}

// Single match
pub struct Match {
    pub pattern_index: u32,
    pub captures: Vec<Capture>,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start: Position,              // Custom Position struct
    pub end: Position,
}

// Capture (named pattern match)
pub struct Capture {
    pub index: u32,
    pub name: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

// Position (line:column)
pub struct Position {
    pub row: usize,                   // 1-indexed line (NOT "line")
    pub column: usize,                // 0-indexed column in line
}
```

**Output Format:**
```json
{
  "execution_id": "uuid-v4",
  "file_path": "src/main.rs",
  "language": "Rust",
  "query": "(function_item) @func",
  "matches": [
    {
      "pattern_index": 0,
      "captures": [...],
      "byte_start": 10,
      "byte_end": 50,
      "start": {"row": 2, "column": 0},
      "end": {"row": 5, "column": 1}
    }
  ],
  "match_count": 1
}
```

---

### 6. llmfilewrite (Code Creation)

**Location:** `/home/feanor/Projects/llmfilewrite/docs/llmfilewrite.md`

**Planned Structures:**

```rust
// Write request (proposed)
pub struct WriteRequest {
    pub path: String,
    pub language: String,             // or "auto"
    pub content: String,
    pub overwrite: bool,
}

// LSP diagnostic (proposed)
pub struct LspDiagnostic {
    pub severity: String,             // "error", "warning"
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub code: String,                 // Error code from LSP
}

// Write response (proposed)
pub struct WriteResponse {
    pub execution_id: String,
    pub path: String,
    pub language: String,
    pub lsp_used: String,             // LSP server name
    pub diagnostics: Vec<LspDiagnostic>,
    pub status: String,               // "approved", "rejected"
    pub magellan_hints: Option<Vec<MagellanHint>>,
}
```

**Planned Output Format:**
```json
{
  "execution_id": "uuid-v4",
  "path": "src/main.rs",
  "language": "Rust",
  "lsp_used": "rust-analyzer",
  "diagnostics": [],
  "status": "approved"
}
```

---

## Current State Analysis

### Coordinate Field Names

| Tool | Line Start | Line End | Col Start | Col End | Byte Start | Byte End |
|------|-----------|---------|-----------|---------|------------|----------|
| Magellan | `start_line` | `end_line` | `start_col` | `end_col` | `byte_start` | `byte_end` |
| Splice | `line_start` | `line_end` | `col_start` | `col_end` | `byte_start` | `byte_end` |
| llmtransform | N/A | N/A | N/A | N/A | `byte_start` | `byte_end` |
| llmsearch | `line_number` | N/A | `column_number` | N/A | `byte_start` | `byte_end` |
| llmastsearch | `start.row` | `end.row` | `start.column` | `end.column` | `byte_start` | `byte_end` |

**Issue:** 5 different conventions for the same concept.

### Path Field Names

| Tool | Field Name | Type |
|------|-----------|------|
| Magellan | `file_path` | String |
| Splice | `file_path` | String |
| llmtransform | `file_path` | String |
| llmsearch | `file` | String |
| llmastsearch | `file_path` | PathBuf |

**Issue:** Inconsistent naming (`file` vs `file_path`) and types (String vs PathBuf).

### Execution ID

| Tool | Field Name | Type |
|------|-----------|------|
| Magellan | `execution_id` | String |
| Splice | `operation_id` | String |
| llmtransform | `execution_id` | String |
| llmsearch | `execution_id` | String |
| llmastsearch | `execution_id` | Uuid |

**Issue:** Splice uses `operation_id` instead of `execution_id`. llmastsearch uses Uuid type instead of String.

### Match/Span IDs

| Tool | ID Field | ID Type |
|------|----------|---------|
| Magellan | `match_id`, `span_id` | String (UUID) |
| Splice | `span_id`, `match_id` | String (UUID) |
| llmsearch | `match_id` | String (UUID) |
| llmastsearch | None | N/A |

**Issue:** llmastsearch lacks stable IDs for matches.

### Error Handling

| Tool | Error Structure | Severity Levels |
|------|----------------|-----------------|
| Magellan | Not documented | Unknown |
| Splice | `DiagnosticPayload` | error/warning/note |
| llmtransform | `error: Option<String>` | Boolean (success/error) |
| llmsearch | Not documented | Unknown |
| llmastsearch | `QueryError` enum | InvalidQuery/ParseError |

**Issue:** Only Splice has structured, actionable error diagnostics.

### Metadata

| Tool | Schema Version | Timestamp | Language |
|------|----------------|-----------|----------|
| Magellan | Yes (`schema_version`) | No | No |
| Splice | Yes (`version`) | Yes (`timestamp`) | No |
| llmtransform | No | No | No |
| llmsearch | No | No | No |
| llmastsearch | No | No | Yes (`language`) |

**Issue:** Inconsistent metadata across tools.

---

## Common Patterns

### 1. UUID Execution IDs

All tools (except Splice's naming) use `execution_id` with UUID v4 format.
**Recommendation:** Standardize on `execution_id: String` (UUID v4).

### 2. Byte Offset Coordinates

All tools use `byte_start` and `byte_end` for UTF-8 byte offsets.
**Recommendation:** Continue this pattern.

### 3. Half-Open Ranges

All tools use half-open ranges `[start, end)` consistent with Rust.
**Recommendation:** Document this explicitly.

### 4. Line/Column Conventions

Most tools use:
- **Lines:** 1-indexed (for human-friendly display)
- **Columns:** 0-indexed (byte offset within line)

**Recommendation:** Standardize and document.

### 5. JSON Serialization

All tools use `serde` for JSON serialization with explicit `Serialize` derives.
**Recommendation:** Continue this pattern.

---

## Key Differences

### 1. Position Representation

**Three incompatible patterns:**

```rust
// Pattern 1: Magellan, Splice (flat fields)
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
    pub start_col: usize,
    pub end_col: usize,
}

// Pattern 2: llmsearch (single line)
struct Match {
    pub line_number: usize,
    pub column_number: usize,
}

// Pattern 3: llmastsearch (nested Position)
pub struct Match {
    pub start: Position,  // { row, column }
    pub end: Position,
}
```

**Alignment:** Adopt Pattern 1 (flat fields) for maximum compatibility.

### 2. Nested Data vs Wrapper Objects

**Magellan:**
```json
{
  "schema_version": "1.0",
  "execution_id": "...",
  "data": { "matches": [...] }  // Nested
}
```

**llmsearch:**
```json
{
  "execution_id": "...",
  "matches": [...]  // Flat
}
```

**Alignment:** Use flat structure for simplicity (llmsearch pattern).

### 3. Error Depth

**Splice:** Rich diagnostic payload with remediation hints
**Others:** Simple error string or enum

**Alignment:** All tools should adopt Splice's `DiagnosticPayload` pattern.

---

## Alignment Recommendations

### Priority 1: Coordinate Standardization (High Impact)

**Standard field names:**
```rust
pub struct SpanCoordinates {
    // Byte offsets (UTF-8)
    pub byte_start: usize,
    pub byte_end: usize,

    // Line numbers (1-indexed)
    pub line_start: usize,
    pub line_end: usize,

    // Column numbers (0-indexed, byte offset in line)
    pub col_start: usize,
    pub col_end: usize,
}
```

**Migration required:**
- Magellan: Rename `start_line` → `line_start`, `start_col` → `col_start`
- llmsearch: Rename `line_number` → `line_start`, `column_number` → `col_start`
- llmastsearch: Flatten `start.row` → `line_start`, `start.column` → `col_start`

### Priority 2: Path Field Standardization

**Standard:** `file_path: String` (not `file`, not `PathBuf`)

**Migration required:**
- llmsearch: `file` → `file_path`
- llmastsearch: `PathBuf` → `String`

### Priority 3: Execution ID Standardization

**Standard:** `execution_id: String` (UUID v4 format)

**Migration required:**
- Splice: `operation_id` → `execution_id`
- llmastsearch: `Uuid` type → `String`

### Priority 4: Match ID Standardization

All match results should have stable IDs:

```rust
pub struct IdentifiableMatch {
    pub match_id: String,  // UUID v4
    // ... other fields
}
```

**Migration required:**
- llmastsearch: Add `match_id` to `Match` struct

### Priority 5: Error Structure Alignment

Adopt Splice's `DiagnosticPayload` pattern across all tools:

```rust
pub struct DiagnosticPayload {
    pub tool: String,              // Tool name
    pub level: String,             // "error" | "warning" | "note"
    pub message: String,           // Primary message
    pub file: Option<String>,      // File path
    pub line: Option<usize>,       // 1-indexed
    pub column: Option<usize>,     // 0-indexed
    pub code: Option<String>,      // Stable error code
    pub note: Option<String>,      // Additional context
    pub remediation: Option<String>, // How to fix
}
```

### Priority 6: Metadata Standardization

All tools should include:

```rust
pub struct ResponseMetadata {
    pub schema_version: String,     // "1.0"
    pub execution_id: String,       // UUID v4
    pub timestamp: Option<String>,  // ISO 8601
    pub language: Option<String>,   // For code tools
}
```

---

## Proposed Standard Schema

### Unified Span Type

```rust
/// Standard span representation for all LLM tools.
///
/// Coordinate conventions:
/// - byte_start, byte_end: UTF-8 byte offsets, half-open range [start, end)
/// - line_start, line_end: 1-indexed line numbers
/// - col_start, col_end: 0-indexed byte offsets within each line
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardSpan {
    /// Stable identifier for this span (UUID v4)
    pub span_id: String,

    /// File path (relative to workspace root when possible)
    pub file_path: String,

    /// UTF-8 byte offset of span start (inclusive)
    pub byte_start: usize,

    /// UTF-8 byte offset of span end (exclusive)
    pub byte_end: usize,

    /// Line number where span starts (1-indexed)
    pub line_start: usize,

    /// Line number where span ends (1-indexed)
    pub line_end: usize,

    /// Column number at span start (0-indexed, byte offset in line)
    pub col_start: usize,

    /// Column number at span end (0-indexed, byte offset in line)
    pub col_end: usize,
}
```

### Unified Match Type

```rust
/// Standard match result for search/query tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardMatch {
    /// Stable identifier for this match (UUID v4)
    pub match_id: String,

    /// Span information
    pub span: StandardSpan,

    /// Matched text content (for search) or capture name (for AST)
    pub content: String,

    /// Additional context (tool-specific)
    pub context: Option<MatchContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchContext {
    /// Lines before the match
    pub before: Option<Vec<String>>,

    /// Lines after the match
    pub after: Option<Vec<String>>,

    /// Symbol kind (for AST queries: "function", "class", etc.)
    pub kind: Option<String>,

    /// Parent symbol name (for AST queries)
    pub parent: Option<String>,
}
```

### Unified Response Wrapper

```rust
/// Standard response wrapper for all LLM tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardResponse<T> {
    /// Schema version (for backward compatibility)
    pub schema_version: String,

    /// Execution identifier (UUID v4)
    pub execution_id: String,

    /// Tool name that generated this response
    pub tool: String,

    /// Response timestamp (ISO 8601)
    pub timestamp: String,

    /// Response status
    pub status: ResponseStatus,

    /// Primary response data
    pub data: T,

    /// Diagnostics (errors, warnings, notes)
    pub diagnostics: Vec<DiagnosticPayload>,

    /// Partial results indicator (for streaming)
    pub partial: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseStatus {
    #[serde(rename = "success")]
    Success,

    #[serde(rename = "failure")]
    Failure,

    #[serde(rename = "partial")]
    Partial,
}
```

### Unified Diagnostic

```rust
/// Standard diagnostic payload for all tools.
///
/// Used for errors, warnings, and informational notes.
/// Designed to be both human-readable and LLM-parsable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticPayload {
    /// Tool that generated this diagnostic
    pub tool: String,

    /// Severity level
    pub level: DiagnosticLevel,

    /// Primary message
    pub message: String,

    /// File path (if applicable)
    pub file: Option<String>,

    /// Line number (1-indexed, if applicable)
    pub line: Option<usize>,

    /// Column number (0-indexed, if applicable)
    pub column: Option<usize>,

    /// Stable error code (e.g., "SPLICE_E001", "E0382")
    pub code: Option<String>,

    /// Additional context or notes
    pub note: Option<String>,

    /// Suggested remediation or fix
    pub remediation: Option<String>,

    /// Related span (if applicable)
    pub span: Option<StandardSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    #[serde(rename = "error")]
    Error,

    #[serde(rename = "warning")]
    Warning,

    #[serde(rename = "note")]
    Note,
}
```

---

## Implementation Roadmap

### Phase 1: Create Shared Types Crate (Week 1)

**Location:** New crate at `/home/feanor/Projects/llm-types` (or similar)

**Deliverables:**
1. `llm-types` crate with standard types
2. Published to local git registry or crates.io
3. Documentation of conventions

**Files:**
```
llm-types/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── span.rs
│   ├── match.rs
│   ├── response.rs
│   └── diagnostic.rs
└── README.md
```

### Phase 2: Migrate Tools (Weeks 2-3)

**Order:**
1. **llmsearch** (simplest, good starting point)
2. **llmastsearch** (medium complexity)
3. **llmtransform** (medium complexity)
4. **Magellan** (higher complexity, more consumers)
5. **Splice** (highest complexity, critical path)
6. **llmfilewrite** (new, implement with standard from start)

**Migration per tool:**
1. Add `llm-types` dependency
2. Update structs to use standard types
3. Add compatibility layer for old output format (deprecation period)
4. Update tests
5. Update documentation

### Phase 3: Validation & Testing (Week 4)

1. Cross-tool integration tests
2. LLM consumption tests (verify Claude/other LLMs can consume)
3. Performance regression tests
4. Documentation review

### Phase 4: Deprecate Old Formats (Week 6+)

1. Announce deprecation timeline
2. Add migration warnings to old output formats
3. Remove compatibility layers after deprecation period

---

## Implementation Details by Tool

### llmsearch

**Changes:**
```rust
// Before
struct Match {
    match_id: String,
    file: String,
    byte_start: usize,
    byte_end: usize,
    line_number: usize,
    column_number: usize,
    // ...
}

// After
struct Match {
    match_id: String,
    span: StandardSpan,
    matched_text: String,
    // ...
}
```

### llmastsearch

**Changes:**
```rust
// Before
pub struct Match {
    pub pattern_index: u32,
    pub captures: Vec<Capture>,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start: Position,
    pub end: Position,
}

// After
pub struct Match {
    pub match_id: String,  // NEW
    pub pattern_index: u32,
    pub captures: Vec<Capture>,
    pub span: StandardSpan,  // REPLACES byte_start, byte_end, start, end
}
```

**Position type:** Keep for internal use, but convert to `StandardSpan` for JSON output.

### Magellan

**Changes:**
```rust
// Before
pub struct Span {
    pub span_id: String,
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,  // RENAME
    pub start_col: usize,   // RENAME
    pub end_line: usize,    // RENAME
    pub end_col: usize,     // RENAME
}

// After
pub struct Span {
    pub span_id: String,
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,  // RENAMED
    pub line_end: usize,    // RENAMED
    pub col_start: usize,   // RENAMED
    pub col_end: usize,     // RENAMED
}
```

### Splice

**Changes:**
```rust
// Before
pub struct OperationResult {
    pub version: String,
    pub operation_id: String,  // RENAME
    // ...
}

// After
pub struct OperationResult {
    pub schema_version: String,  // RENAME for consistency
    pub execution_id: String,    // RENAMED
    // ...
}
```

Add `DiagnosticPayload` export for use by other tools.

### llmfilewrite

**Implementation:** Use standard types from day one.
```rust
use llm_types::{StandardResponse, DiagnosticPayload, StandardSpan};

pub struct WriteResponse {
    pub execution_id: String,
    pub path: String,
    pub language: String,
    pub lsp_used: String,
    pub diagnostics: Vec<DiagnosticPayload>,  // Standard type
    pub status: WriteStatus,
}
```

---

## Success Criteria

### Technical

- [ ] All tools use `execution_id: String` (UUID v4)
- [ ] All tools use `file_path: String`
- [ ] All tools use `line_start`, `line_end`, `col_start`, `col_end`
- [ ] All tools use `byte_start`, `byte_end`
- [ ] All tools include `schema_version` in responses
- [ ] All tools use `DiagnosticPayload` for errors
- [ ] All matches have stable `match_id` (UUID v4)

### LLM Experience

- [ ] LLM can consume output from any tool without format-specific parsing
- [ ] LLM can correlate matches across tools (e.g., Magellan symbol → llmsearch result)
- [ ] LLM gets actionable error messages with remediation hints
- [ ] LLM doesn't need to re-read files (spans are consistent and accurate)
- [ ] Token waste is minimized (no format translation)

### Developer Experience

- [ ] Shared types crate is easy to include
- [ ] Documentation is clear and examples are provided
- [ ] Breaking changes are minimized via compatibility layers
- [ ] Migration path is clear for existing consumers

---

## Open Questions

1. **Shared crate location:** Should we create a separate repo or use a workspace?
   - **Recommendation:** Separate repo with git dependency for stability

2. **Deprecation timeline:** How long to maintain compatibility layers?
   - **Recommendation:** 3 months for major version bump

3. **Versioning strategy:** How to coordinate versions across tools?
   - **Recommendation:** Semantic versioning with coordinated releases

4. **Error code namespace:** How to avoid collisions between tools?
   - **Recommendation:** Prefix with tool name: `SPLICE_E001`, `MAGELLAN_E001`

---

## Appendix: Coordinate Semantics

### Half-Open Ranges

All tools use half-open ranges `[start, end)`:

```
Text: "hello world"
      ^     ^
      0     5

byte_start = 0
byte_end = 5
Content = "hello" (does NOT include byte at offset 5)
```

This is consistent with Rust's `Range<usize>` and slice indexing.

### UTF-8 Safety

Byte offsets are UTF-8 byte offsets, not character counts.

For converting between bytes and characters, use:
```rust
let text = "Hello 世界";  // 12 bytes, 8 characters
let byte_start = 6;       // Start of "世"
let char_start = text[..byte_start].chars().count();  // 2
```

### Line/Column Calculation

Lines are 1-indexed (for human-friendly display).
Columns are 0-indexed (byte offset within the line).

```
Line 1: "hello\n"   column 0-5
Line 2: "world\n"   column 0-5
```

When `byte_start = 6` (start of "world"):
- `line_start = 2`
- `col_start = 0`

---

*Document created: 2026-01-22*
*Last updated: 2026-01-22*
*Status: Ready for review and implementation*

# Unified JSON Schema for LLM Toolset

**Created:** 2026-01-22
**Status:** Design Specification
**Applies to:** magellan, splice, llmtransform, llmsearch, llmastsearch, llmfilewrite

---

## Executive Summary

This document defines a unified JSON output schema across all LLM-native tools, building on the proven patterns from **Magellan** and **Splice**. The goal is to create a consistent, LLM-parsable output format that enables:

1. **Tool chaining** - Output from one tool can be consumed by another
2. **Traceability** - All operations tracked with stable IDs
3. **Precision** - Byte-accurate spans with UTF-8 safety
4. **Validation** - Checksums for integrity verification
5. **Extensibility** - Schema versioning for backward compatibility

---

## Table of Contents

1. [Design Principles](#design-principles)
2. [Core Types](#core-types)
3. [Response Wrappers](#response-wrappers)
4. [Span Model](#span-model)
5. [Rich Span Extensions](#rich-span-extensions)
6. [Tool-Specific Variants](#tool-specific-variants)
7. [Error Diagnostics](#error-diagnostics)
8. [Implementation Guide](#implementation-guide)
9. [Migration Path](#migration-path)
10. [CLI Flags for Optional Features](#cli-flags)

---

## Design Principles

### 1. Schema Versioning

All responses include a `schema_version` field following semantic versioning:

```json
{
  "schema_version": "1.0.0",
  ...
}
```

**Magellan's pattern (adopted):** Use `schema_version` instead of `version` for clarity.

### 2. Execution Tracking

All operations include a stable `execution_id` (UUID v4):

```json
{
  "execution_id": "550e8400-e29b-41d4-a716-446655440000",
  ...
}
```

**Splice's inconsistency (to fix):** Splice uses `operation_id` instead of `execution_id`.

### 3. Half-Open Ranges

All spans use half-open ranges `[start, end)`:

- `byte_start` is **inclusive**
- `byte_end` is **exclusive**
- Length = `byte_end - byte_start`

**Magellan's documentation (adopted):** Extensive comments explaining half-open semantics.

### 4. UTF-8 Byte Offsets

All positions are UTF-8 byte offsets (not character indices):
- Matches tree-sitter's API
- Matches Rust's string slicing
- Safe for all Unicode content

### 5. Stable Identifiers

All entities have stable IDs:
- `execution_id`: UUID v4 for operation tracking
- `span_id`: SHA-256 hash of `file_path:byte_start:byte_end` (Magellan's pattern)
- `match_id`: UUID v4 for individual matches

---

## Core Types

### Span (Canonical Definition)

**Source:** Magellan's `src/output/command.rs:176-389`

```rust
/// Span in source code (byte + line/column)
///
/// Represents a **half-open range** [start, end) where:
/// - byte_start is inclusive (first byte INCLUDED)
/// - byte_end is exclusive (first byte NOT included)
///
/// All offsets are UTF-8 byte positions. Lines are 1-indexed for user-friendliness.
/// Columns are 0-indexed byte offsets within each line.
pub struct Span {
    /// Stable span ID (SHA-256 hash of file_path:byte_start:byte_end)
    pub span_id: String,

    /// File path (absolute or root-relative)
    pub file_path: String,

    /// Byte range start (inclusive, first byte INCLUDED)
    pub byte_start: usize,

    /// Byte range end (exclusive, first byte NOT included)
    pub byte_end: usize,

    /// Start line (1-indexed)
    pub start_line: usize,

    /// Start column (0-indexed, byte-based)
    pub start_col: usize,

    /// End line (1-indexed)
    pub end_line: usize,

    /// End column (0-indexed, byte-based)
    pub end_col: usize,
}
```

**Span ID Generation (Magellan's algorithm):**

```rust
pub fn generate_span_id(file_path: &str, byte_start: usize, byte_end: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    hasher.update(b":");
    hasher.update(byte_start.to_be_bytes());
    hasher.update(b":");
    hasher.update(byte_end.to_be_bytes());

    let result = hasher.finalize();
    // First 8 bytes as 16 hex characters
    format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            result[0], result[1], result[2], result[3],
            result[4], result[5], result[6], result[7])
}
```

**Properties:**
- Deterministic: Same inputs always produce same ID
- Platform-independent: SHA-256 consistent across architectures
- Position-based: Not affected by content changes at same position

### Match Result

```rust
/// Match result with symbol information
pub struct Match {
    /// Stable match ID (UUID v4)
    pub match_id: String,

    /// Span information
    pub span: Span,

    /// Matched content (for text search) or capture name (for AST)
    pub content: String,

    /// Symbol kind (for AST queries): "function", "class", etc.
    pub kind: Option<String>,

    /// Parent symbol name (for nested definitions)
    pub parent: Option<String>,

    /// Stable symbol ID (for cross-reference)
    pub symbol_id: Option<String>,
}
```

---

## Response Wrappers

### Query/Search Response Wrapper

**Magellan's pattern (adopted for all query tools):**

```rust
/// Wrapper for all JSON responses
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
```

**Usage:**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "550e8400-e29b-41d4-a716-446655440000",
  "data": {
    "matches": [...]
  },
  "partial": false
}
```

### Mutation Response Wrapper

**Splice's pattern (adopted for mutation tools):**

```rust
/// Top-level operation result for mutations
pub struct OperationResult {
    /// Schema version
    pub schema_version: String,

    /// Unique operation ID (UUID v4)
    pub execution_id: String,

    /// Operation type: "patch", "delete", "rename", "apply_files"
    pub operation_type: String,

    /// Status: "ok", "error", "partial"
    pub status: String,

    /// Human-readable message
    pub message: String,

    /// Timestamp (ISO 8601)
    pub timestamp: String,

    /// Workspace root (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>],

    /// Primary result data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OperationData>,

    /// Error details if status is "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetails>,
}
```

**Usage:**

```json
{
  "schema_version": "2.0.0",
  "execution_id": "550e8400-e29b-41d4-a716-446655440000",
  "operation_type": "patch",
  "status": "ok",
  "message": "Successfully patched 1 symbol in 1 file",
  "timestamp": "2026-01-22T10:00:00Z",
  "workspace": "/path/to/project",
  "result": {
    "type": "patch",
    "file": "src/lib.rs",
    "spans": [...]
  }
}
```

---

## Span Model

### Coordinate Field Names

**Current inconsistencies:**

| Tool | Line Start | Line End | Col Start | Col End |
|------|-----------|---------|-----------|---------|
| Magellan | `start_line` | `end_line` | `start_col` | `end_col` |
| Splice | `line_start` | `line_end` | `col_start` | `col_end` |
| llmsearch | `line_number` | N/A | `column_number` | N/A |
| llmastsearch | `start.row` | `end.row` | `start.column` | `end.column` |

**Standardized format (Magellan's convention):**

```rust
pub struct SpanCoordinates {
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,    // NOT line_start
    pub end_line: usize,      // NOT line_end
    pub start_col: usize,     // NOT col_start
    pub end_col: usize,       // NOT col_end
}
```

**Rationale for Magellan's convention:**
- More explicit (start_line vs line_start reads better)
- Matches semantic naming (start/end pair)
- Consistent with LSP's startPosition/endPosition pattern

### Span Serialization

**All tools must serialize spans as:**

```json
{
  "span_id": "a1b2c3d4e5f6g7h8",
  "file_path": "src/main.rs",
  "byte_start": 100,
  "byte_end": 200,
  "start_line": 5,
  "start_col": 4,
  "end_line": 10,
  "end_col": 2
}
```

**Minimal span (line/col unknown):**

```json
{
  "span_id": "...",
  "file_path": "src/main.rs",
  "byte_start": 100,
  "byte_end": 200,
  "start_line": 0,
  "end_line": 0,
  "start_col": 0,
  "end_col": 0
}
```

---

## Rich Span Extensions (Optional Fields)

The following OPTIONAL fields extend spans with semantic and contextual information. All fields are opt-in via flags like `--with-context`, `--with-relationships`, etc.

### 1. Context Field

**Purpose:** Provide surrounding lines without additional file reads.

```rust
pub struct SpanContext {
    /// Lines before the span (default: 3 lines)
    pub before: Vec<String>,

    /// The actual span content (for verification)
    pub selected: Vec<String>,

    /// Lines after the span (default: 3 lines)
    pub after: Vec<String>,
}
```

**Usage:**

```json
{
  "span_id": "...",
  "file_path": "src/main.rs",
  "byte_start": 100,
  "byte_end": 200,
  "start_line": 5,
  "start_col": 4,
  "end_line": 10,
  "end_col": 2,
  "context": {
    "before": [
      "/// Documentation comment",
      "use std::collections::HashMap;",
      ""
    ],
    "selected": [
      "fn process(data: &str) -> Result<String> {",
      "    // implementation",
      "}"
    ],
    "after": [
      "",
      "fn main() {",
      "    process(\"hello\").unwrap();",
      "}"
    ]
  }
}
```

**Why this matters:**
- LLM stops guessing surrounding structure
- Splice patches become safer
- No need for additional Magellan calls
- Multi-line patches become deterministic

### 2. Semantic Kind and Language

**Purpose:** Add semantic meaning beyond syntactic position.

```rust
pub struct SpanSemantics {
    /// Semantic kind: "function", "class", "method", "struct", "enum", "trait", etc.
    pub semantic_kind: String,

    /// Programming language: "rust", "python", "typescript", etc.
    pub language: String,
}
```

**Usage:**

```json
{
  "span": { /* ... */ },
  "semantic_kind": "function",
  "language": "rust"
}
```

**Why this matters:**
- Enables smarter transforms
- Safe symbol-level operations
- Lets Splice enforce AST boundaries
- Enables multi-language refactors
- LLM can infer correct patch structure

**Supported semantic kinds by language:**

| Language | Kinds |
|----------|-------|
| Rust | function, method, struct, enum, trait, impl, mod, const, static, type, macro |
| Python | function, method, class, async_function, decorator, module, variable |
| TypeScript | function, method, class, interface, type, enum, namespace, variable |
| JavaScript | function, method, class, variable, statement |

### 3. Relationships Block

**Purpose:** Embed call graph information directly into spans.

```rust
pub struct SpanRelationships {
    /// Functions that call this symbol
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<SymbolReference>,

    /// Functions called by this symbol
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callees: Vec<SymbolReference>,

    /// Import statements referencing this symbol
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<SymbolReference>,

    /// Export statements exposing this symbol
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<SymbolReference>,
}

pub struct SymbolReference {
    pub file: String,
    pub symbol: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: usize,
}
```

**Usage:**

```json
{
  "span": { /* ... */ },
  "relationships": {
    "callers": [
      {"file": "src/a.rs", "symbol": "main", "byte_start": 120, "byte_end": 124, "line": 8}
    ],
    "callees": [
      {"file": "src/b.rs", "symbol": "helper", "byte_start": 50, "byte_end": 56, "line": 3},
      {"file": "vendor/lib.rs", "symbol": "parse", "byte_start": 200, "byte_end": 205, "line": 15}
    ],
    "imports": [
      {"file": "src/lib.rs", "symbol": "use crate::helper", "byte_start": 5, "byte_end": 20, "line": 1}
    ],
    "exports": []
  }
}
```

**Why this matters:**
- LLM can analyze impact before patching
- Removes need for separate Magellan queries
- Enables safe bulk refactors
- Helps Splice verify patch safety
- Foundation of blast radius analysis

### 4. Checksums (Race Condition Protection)

**Purpose:** Verify file/symbol hasn't changed before applying patches.

```rust
pub struct SpanChecksums {
    /// SHA-256 of span content before patch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_before: Option<String>,

    /// SHA-256 of span content after patch (for verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_after: Option<String>,

    /// SHA-256 of entire file before operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_checksum_before: Option<String>,
}
```

**Usage:**

```json
{
  "span": { /* ... */ },
  "checksums": {
    "checksum_before": "sha256:a1b2c3d4e5f6...",
    "file_checksum_before": "sha256:fedcba987654..."
  }
}
```

**Splice validation flow:**

1. Read file, compute SHA-256
2. Compare with `file_checksum_before`
3. If mismatch: reject patch (file changed)
4. Read span, compute SHA-256
5. Compare with `checksum_before`
6. If mismatch: reject span (symbol shifted)
7. Apply patch
8. Compute `checksum_after` for verification

**Why this matters:**
- Prevents applying outdated patches
- Avoids Magellan race conditions
- Prevents partial patch corruption
- Same pattern used by Google/Meta for automated refactoring

### 5. Suggested Action (Future-Proofing)

**Purpose:** Enable intelligent batching and self-repair.

```rust
pub struct SuggestedAction {
    /// Action type: "rename", "delete", "extract", "inline", etc.
    pub action_type: String,

    /// Action parameters (varies by type)
    pub params: serde_json::Value,
}
```

**Usage:**

```json
{
  "span": { /* ... */ },
  "suggested_action": {
    "action_type": "rename",
    "params": {
      "from": "foo",
      "to": "bar"
    }
  }
}
```

**Why this matters (future):**
- Automatic merge of multi-step refactors
- Intelligent batching of operations
- LLM self-repair suggestions
- Opportunistic optimizations

### 6. Tool Hints (Behavior Guidance)

**Purpose:** Let tools coordinate behavior automatically.

```rust
pub struct ToolHints {
    /// Whether this span requires full file context for safe patching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_full_context: Option<bool>,

    /// Whether this operation must be atomic (all-or-nothing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apply_atomically: Option<bool>,

    /// Search sensitivity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_case_sensitive: Option<bool>,

    /// Language-specific hints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_hints: Option<serde_json::Value>,
}
```

**Usage:**

```json
{
  "span": { /* ... */ },
  "tool_hints": {
    "requires_full_context": false,
    "apply_atomically": true,
    "search_case_sensitive": true,
    "language_hints": {
      "rust_macro_expansion": true
    }
  }
}
```

**Why this matters:**
- Rust macros require full context
- Some patches must be atomic (no partial application)
- Tools can adapt automatically
- Turns schema into shared contract

### 7. Unified Error Codes

**Purpose:** Machine-readable error codes for automated repair.

**Error code format:** `{TOOL}-{CATEGORY}-{NUMBER}`

| Tool | Prefix | Examples |
|------|--------|----------|
| Magellan | `MAG` | `MAG-REF-001`, `MAG-QRY-002` |
| Splice | `SPL` | `SPL-E001`, `SPL-V002` |
| llmsearch | `LMS` | `LMS-IO-001`, `LMS-QRY-002` |
| llmastsearch | `LMA` | `LMA-AST-001`, `LMA-QRY-002` |
| llmtransform | `LMT` | `LMT-IO-001`, `LMT-CSUM-002` |

**Usage:**

```json
{
  "diagnostics": [
    {
      "code": "MAG-REF-001",
      "tool": "magellan",
      "severity": "error",
      "message": "Symbol not found",
      "span": null,
      "remediation": "Check spelling or use 'magellan find' to search"
    }
  ]
}
```

**Error categories:**

| Category | Suffix | Examples |
|----------|--------|----------|
| IO | `-IO-` | File not found, permission denied |
| QUERY | `-QRY-` | Invalid query, parse error |
| REF | `-REF-` | Symbol not found, undefined reference |
| VALIDATION | `-V-` | Checksum mismatch, span invalid |
| AST | `-AST-` | Parse error, invalid syntax |

**Why this matters:**
- Automatic repair strategies
- LLM retry logic without hallucination
- Splice ↔ Magellan ↔ LLM integrated debugging
- Real agent workflows

---

## Complete Rich Span Example

Combining all optional fields:

```json
{
  "schema_version": "1.1.0",
  "execution_id": "550e8400-e29b-41d4-a716-446655440000",
  "tool": "magellan",
  "timestamp": "2026-01-22T10:30:00Z",
  "data": {
    "matches": [{
      "match_id": "bbb22222-3333-4444-5555-666666666666",
      "span": {
        "span_id": "a1b2c3d4e5f6g7h8",
        "file_path": "src/lib.rs",
        "byte_start": 100,
        "byte_end": 200,
        "start_line": 10,
        "start_col": 4,
        "end_line": 15,
        "end_col": 2,
        "context": {
          "before": ["/// Process input data", "use std::collections;", ""],
          "selected": ["fn process(data: &str) -> Result<String> {", "    data.trim().to_string()", "}"],
          "after": ["", "fn main() {", "    process(\"hello\").unwrap();", "}"]
        },
        "semantic_kind": "function",
        "language": "rust",
        "relationships": {
          "callers": [
            {"file": "src/main.rs", "symbol": "main", "line": 5}
          ],
          "callees": [
            {"file": "src/lib.rs", "symbol": "trim", "line": 11},
            {"file": "src/lib.rs", "symbol": "to_string", "line": 11}
          ]
        },
        "checksums": {
          "checksum_before": "sha256:abc123...",
          "file_checksum_before": "sha256:def456..."
        },
        "tool_hints": {
          "requires_full_context": false,
          "apply_atomically": true
        }
      },
      "name": "process",
      "kind": "function"
    }],
    "count": 1
  },
  "diagnostics": []
}
```

---

## Tool-Specific Variants

### 1. Magellan (Symbol Query)

**Current state:** Reference implementation

**Response format:**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "data": {
    "matches": [
      {
        "match_id": "...",
        "span": { /* Span */ },
        "name": "function_name",
        "kind": "function",
        "parent": "module_name",
        "symbol_id": "stable-symbol-id"
      }
    ],
    "count": 1
  },
  "partial": false
}
```

### 2. Splice (Refactoring)

**Changes needed:**
1. Rename `operation_id` → `execution_id`
2. Rename `version` → `schema_version`
3. Adopt Magellan's `start_line`/`start_col` convention
4. Use SHA-256 span_id instead of UUID

**Updated response format:**

```json
{
  "schema_version": "2.0.0",
  "execution_id": "...",
  "operation_type": "patch",
  "status": "ok",
  "message": "Successfully patched 1 symbol in 1 file",
  "timestamp": "2026-01-22T10:00:00Z",
  "result": {
    "type": "patch",
    "file": "src/lib.rs",
    "symbol": "function_name",
    "kind": "function",
    "spans": [
      {
        "span_id": "...",
        "file_path": "src/lib.rs",
        "byte_start": 100,
        "byte_end": 200,
        "start_line": 5,
        "start_col": 4,
        "end_line": 10,
        "end_col": 2,
        "before_hash": "abc123",
        "after_hash": "def456"
      }
    ],
    "lines_added": 5,
    "lines_removed": 3
  }
}
```

### 3. llmsearch (Text Search)

**Changes needed:**
1. Add `JsonResponse` wrapper
2. Rename `file` → `file_path`
3. Rename `line_number` → `start_line`
4. Rename `column_number` → `start_col`
5. Add `Span` structure
6. Add `match_id` (UUID v4)

**Updated response format:**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "data": {
    "pattern": "search_pattern",
    "matches": [
      {
        "match_id": "...",
        "span": {
          "span_id": "...",
          "file_path": "src/main.rs",
          "byte_start": 100,
          "byte_end": 116,
          "start_line": 5,
          "start_col": 0,
          "end_line": 5,
          "end_col": 16
        },
        "matched_text": "search_pattern",
        "context_before": ["line before"],
        "context_after": ["line after"]
      }
    ],
    "match_count": 1
  },
  "partial": false
}
```

### 4. llmastsearch (AST Query)

**Changes needed:**
1. Add `JsonResponse` wrapper
2. Flatten `Position` structure into `Span`
3. Add `match_id` (UUID v4)
4. Convert `PathBuf` → `String`
5. Convert `Uuid` → `String` for execution_id
6. Add `Span` structure

**Updated response format:**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "data": {
    "file_path": "src/main.rs",
    "language": "Rust",
    "query": "(function_item) @func",
    "matches": [
      {
        "match_id": "...",
        "span": {
          "span_id": "...",
          "file_path": "src/main.rs",
          "byte_start": 0,
          "byte_end": 50,
          "start_line": 1,
          "start_col": 0,
          "end_line": 5,
          "end_col": 1
        },
        "pattern_index": 0,
        "captures": [
          {
            "name": "func",
            "byte_start": 3,
            "byte_end": 7,
            "content": "main"
          }
        ]
      }
    ],
    "match_count": 1
  },
  "partial": false
}
```

### 5. llmtransform (Text Mutation)

**Changes needed:**
1. Add `OperationResult` wrapper
2. Add `Span` to each edit result
3. Add checksums for validation

**Updated response format:**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "operation_type": "edit",
  "status": "ok",
  "message": "Successfully applied 2 edits",
  "timestamp": "2026-01-22T10:00:00Z",
  "result": {
    "type": "edit",
    "file_path": "src/main.rs",
    "final_checksum": "def456...",
    "total_byte_shift": 12,
    "applied_count": 2,
    "skipped_count": 0,
    "error_count": 0,
    "edits": [
      {
        "match_id": "...",
        "span": {
          "span_id": "...",
          "file_path": "src/main.rs",
          "byte_start": 10,
          "byte_end": 20,
          "start_line": 1,
          "start_col": 10,
          "end_line": 1,
          "end_col": 20
        },
        "status": "applied",
        "before_checksum": "abc123",
        "after_checksum": "def456"
      }
    ]
  }
}
```

### 6. llmfilewrite (Code Creation)

**Design (new tool):**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "operation_type": "create",
  "status": "approved",
  "message": "File created and validated",
  "timestamp": "2026-01-22T10:00:00Z",
  "result": {
    "type": "create",
    "path": "src/main.rs",
    "language": "Rust",
    "lsp_used": "rust-analyzer",
    "checksum": "abc123...",
    "diagnostics": []
  }
}
```

**With diagnostics:**

```json
{
  "status": "rejected",
  "message": "LSP validation failed",
  "result": {
    "path": "src/main.rs",
    "diagnostics": [
      {
        "level": "error",
        "message": "expected identifier, found `;`",
        "file": "src/main.rs",
        "start_line": 5,
        "start_col": 10,
        "end_line": 5,
        "end_col": 11,
        "code": "E0382"
      }
    ]
  }
}
```

---

## Error Diagnostics

### Unified Error Structure

**Splice's `DiagnosticPayload` (enhanced and adopted):**

```rust
/// Standard diagnostic payload for all tools
pub struct DiagnosticPayload {
    /// Tool that generated this diagnostic
    pub tool: String,

    /// Severity level
    pub level: DiagnosticLevel,

    /// Primary message
    pub message: String,

    /// File path (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,

    /// Span information (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,

    /// Stable error code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Additional context or notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Suggested remediation or fix
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
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

**Error response format:**

```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "operation_type": "patch",
  "status": "error",
  "message": "Operation failed",
  "timestamp": "2026-01-22T10:00:00Z",
  "error": {
    "tool": "splice",
    "level": "error",
    "message": "Symbol not found: 'nonexistent_function'",
    "file": "src/main.rs",
    "span": {
      "span_id": "...",
      "file_path": "src/main.rs",
      "byte_start": 100,
      "byte_end": 120,
      "start_line": 10,
      "start_col": 0,
      "end_line": 10,
      "end_col": 20
    },
    "code": "SPLICE_E001",
    "note": "The symbol could not be resolved in the code graph",
    "remediation": "Use 'splice query' to list available symbols"
  }
}
```

---

## Implementation Guide

### Shared Types Crate

Create a new crate at `/home/feanor/Projects/llm-types/`:

```
llm-types/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── span.rs          // Span, SpanCoordinates
    ├── match.rs         // Match, MatchResult
    ├── response.rs      // JsonResponse, OperationResult
    ├── diagnostic.rs    // DiagnosticPayload
    └── error.rs         // Error types
```

**Cargo.toml:**

```toml
[package]
name = "llm-types"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sha2 = "0.10"
uuid = { version = "1.0", features = ["serde", "v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

### Migration Steps per Tool

**For each tool (llmsearch, llmastsearch, llmtransform, Splice):**

1. Add dependency:
   ```toml
   [dependencies]
   llm-types = { path = "../llm-types" }
   ```

2. Update output types:
   ```rust
   // Old
   pub struct Match {
       pub file: String,
       pub line_number: usize,
       ...
   }

   // New
   use llm_types::{JsonResponse, Match, Span};

   pub type JsonResponse<T> = llm_types::JsonResponse<T>;
   ```

3. Update field names (compatibility layer):
   ```rust
   // For backward compatibility during transition
   #[serde(alias = "file")]
   pub file_path: String,

   #[serde(alias = "line_number")]
   pub start_line: usize,
   ```

4. Update tests to use new format

5. Update documentation

**For Magellan:**
- Already reference implementation
- Export types to `llm-types` crate
- Add compatibility shims if needed

**For llmfilewrite:**
- Implement from scratch using `llm-types`

---

## Migration Path

### Phase 1: Create Shared Types (Week 1)

1. Create `llm-types` crate
2. Add all core types from Magellan/Splice
3. Publish to local registry or git dependency

### Phase 2: Update Simple Tools (Week 2)

1. **llmsearch** - Simplest, good starting point
2. Add `llm-types` dependency
3. Replace internal types
4. Add compatibility layer
5. Update tests

### Phase 3: Update Complex Tools (Weeks 3-4)

1. **llmastsearch** - Medium complexity
2. **llmtransform** - Medium complexity
3. Follow same process as llmsearch

### Phase 4: Update Splice (Week 5)

1. **Splice** - Highest complexity
2. Rename `operation_id` → `execution_id`
3. Rename `version` → `schema_version`
4. Adopt Magellan span format
5. Extensive testing required

### Phase 5: Deprecation (Week 8+)

1. Announce old format deprecation
2. Add migration warnings
3. Remove compatibility layers after 3 months

---

## Summary of Changes

| Tool | Key Changes | Priority |
|------|-------------|----------|
| Magellan | Export types to shared crate | Low (reference impl) |
| Splice | Rename fields, adopt Magellan span format | High |
| llmsearch | Add wrapper, flatten Position, rename fields | Medium |
| llmastsearch | Add wrapper, flatten Position, add match_id | Medium |
| llmtransform | Add wrapper, add Span to edits | Medium |
| llmfilewrite | Implement from scratch using shared types | High |

---

## Field Name Reference

### Standardized Field Names

| Concept | Standard | Aliases (for transition) |
|---------|----------|--------------------------|
| Operation tracking | `execution_id` | `operation_id` (Splice) |
| Schema version | `schema_version` | `version` (Splice) |
| File path | `file_path` | `file` (llmsearch) |
| Line start | `start_line` | `line_start` (Splice), `line_number` (llmsearch), `start.row` (llmastsearch) |
| Line end | `end_line` | `line_end` (Splice), `end.row` (llmastsearch) |
| Column start | `start_col` | `col_start` (Splice), `column_number` (llmsearch), `start.column` (llmastsearch) |
| Column end | `end_col` | `col_end` (Splice), `end.column` (llmastsearch) |

---

## Examples

### Complete Workflow Example

**1. Query symbol (Magellan):**

```bash
magellan find --name "function_name" --db code.db
```

```json
{
  "schema_version": "1.0.0",
  "execution_id": "aaa11111-2222-3333-4444-555555555555",
  "data": {
    "matches": [{
      "match_id": "bbb22222-3333-4444-5555-666666666666",
      "span": {
        "span_id": "a1b2c3d4e5f6g7h8",
        "file_path": "src/lib.rs",
        "byte_start": 100,
        "byte_end": 200,
        "start_line": 10,
        "start_col": 4,
        "end_line": 15,
        "end_col": 2
      },
      "name": "function_name",
      "kind": "function"
    }],
    "count": 1
  }
}
```

**2. Search text (llmsearch):**

```bash
llmsearch --pattern "TODO" --glob "**/*.rs"
```

```json
{
  "schema_version": "1.0.0",
  "execution_id": "ccc33333-4444-5555-6666-777777777777",
  "data": {
    "pattern": "TODO",
    "matches": [{
      "match_id": "ddd44444-5555-6666-7777-888888888888",
      "span": {
        "span_id": "i1j2k3l4m5n6o7p8",
        "file_path": "src/lib.rs",
        "byte_start": 250,
        "byte_end": 254,
        "start_line": 20,
        "start_col": 8,
        "end_line": 20,
        "end_col": 12
      },
      "matched_text": "TODO",
      "context_before": ["// Fix this later"],
      "context_after": ["let x = 1;"]
    }],
    "match_count": 1
  }
}
```

**3. Patch function (Splice):**

```bash
splice patch --file src/lib.rs --symbol function_name --with new_impl.rs
```

```json
{
  "schema_version": "2.0.0",
  "execution_id": "eee55555-6666-7777-8888-999999999999",
  "operation_type": "patch",
  "status": "ok",
  "message": "Successfully patched 1 symbol in 1 file",
  "timestamp": "2026-01-22T10:00:00Z",
  "result": {
    "type": "patch",
    "file": "src/lib.rs",
    "symbol": "function_name",
    "kind": "function",
    "spans": [{
      "span_id": "a1b2c3d4e5f6g7h8",
      "file_path": "src/lib.rs",
      "byte_start": 100,
      "byte_end": 200,
      "start_line": 10,
      "start_col": 4,
      "end_line": 15,
      "end_col": 2,
      "before_hash": "abc123...",
      "after_hash": "def456..."
    }],
    "lines_added": 5,
    "lines_removed": 3
  }
}
```

**4. Query AST (llmastsearch):**

```bash
llmastsearch --query "(function_item name: (identifier) @name)" --glob "**/*.rs"
```

```json
{
  "schema_version": "1.0.0",
  "execution_id": "fff66666-7777-8888-9999-000000000000",
  "data": {
    "file_path": "src/main.rs",
    "language": "Rust",
    "query": "(function_item name: (identifier) @name)",
    "matches": [{
      "match_id": "00077777-8888-9999-0000-111111111111",
      "span": {
        "span_id": "q1r2s3t4u5v6w7x8",
        "file_path": "src/main.rs",
        "byte_start": 100,
        "byte_end": 200,
        "start_line": 10,
        "start_col": 0,
        "end_line": 15,
        "end_col": 1
      },
      "pattern_index": 0,
      "captures": [{
        "name": "name",
        "byte_start": 103,
        "byte_end": 116,
        "content": "function_name"
      }]
    }],
    "match_count": 1
  }
}
```

---

## CLI Flags for Optional Features

All rich span extensions are **opt-in** via CLI flags. This keeps the default output minimal while allowing tools to request additional data when needed.

### Context Flags

| Flag | Short | Purpose | Default |
|------|-------|---------|---------|
| `--with-context` | `-C` | Include context (before/after/selected) | Off |
| `--context-lines <n>` | | Number of context lines (default: 3) | 3 |
| `--no-context` | | Disable context even if requested | - |

**Examples:**

```bash
# Magellan: Get symbol with 5 lines of context
magellan find --name "process" --with-context --context-lines 5

# Splice: Get symbol with context for safer patching
splice get --symbol "process" --with-context

# llmsearch: Search with context
llmsearch --pattern "TODO" --with-context --context-lines 2
```

### Relationship Flags

| Flag | Short | Purpose | Default |
|------|-------|---------|---------|
| `--with-callers` | | Include callers in relationships | Off |
| `--with-callees` | | Include callees in relationships | Off |
| `--with-imports` | | Include imports in relationships | Off |
| `with-exports` | | Include exports in relationships | Off |
| `--with-all-relationships` | `-R` | Include all relationship types | Off |

**Examples:**

```bash
# Get symbol with full call graph
magellan find --name "process" --with-all-relationships

# Get impact analysis before patching
splice delete --symbol "helper" --with-callers --with-callees
```

### Semantic Flags

| Flag | Short | Purpose | Default |
|------|-------|---------|---------|
| `--with-semantics` | `-S` | Include semantic_kind and language | Off |
| `--no-semantics` | | Explicitly exclude semantics | - |

**Examples:**

```bash
# Get symbol with semantic info
llmastsearch --query "(function_item) @func" --with-semantics
```

### Checksum Flags

| Flag | Short | Purpose | Default |
|------|-------|---------|---------|
| `--with-checksums` | | Include checksums for validation | Off |
| `verify-checksums` | | Verify checksums before applying (Splice only) | Off |

**Examples:**

```bash
# Get spans with checksums for safe patching
magellan find --name "foo" --with-checksums

# Apply patch with checksum verification
splice patch --file src/lib.rs --span "...span_id..." --verify-checksums
```

### Tool Hint Flags

| Flag | Purpose | Default |
|------|---------|---------|
| `--atomic` | Require atomic operation | false |
| `--full-context` | Require full file context for patches | false |

**Examples:**

```bash
# Atomic operation (all or nothing)
splice patch --plan plan.json --atomic

# Full context for macros
splice patch --file src/lib.rs --full-context
```

### Error Code Flags

| Flag | Short | Purpose | Default |
|------|-------|---------|---------|
| `--error-codes` | `-E` | Include machine-readable error codes | Off |
| `explain <code>` | | Show detailed explanation for error code | - |

**Examples:**

```bash
# Query with error codes
magellan query --labels rust fn --error-codes

# Explain an error
splice explain SPL-E001
```

### Combined Flag Examples

```bash
# Rich span for LLM consumption (everything enabled)
magellan find --name "process" \
  --with-context --context-lines 5 \
  --with-all-relationships \
  --with-semantics \
  --with-checksums \
  --error-codes

# Safe patching workflow
splice patch --file src/lib.rs --symbol "process" \
  --with-context \
  --with-callers \
  --with-checksums \
  --verify-checksums \
  --atomic

# Impact analysis before deletion
splice delete --symbol "helper" \
  --with-callers \
  --with-callees \
  --error-codes
```

---

## Extension Summary

| Extension | Flag(s) | Adds to Output | Use Case |
|-----------|---------|---------------|----------|
| **Context** | `--with-context`, `-C` | `context.before`, `context.selected`, `context.after` | LLM context, safer patches |
| **Relationships** | `--with-callers`, `--with-callees`, `-R` | `relationships.*` | Impact analysis, blast radius |
| **Semantics** | `--with-semantics`, `-S` | `semantic_kind`, `language` | Smart transforms, multi-language |
| **Checksums** | `--with-checksums`, `--verify-checksums` | `checksums.*` | Race protection, validation |
| **Tool Hints** | `--atomic`, `--full-context` | `tool_hints.*` | Coordination, safety |
| **Error Codes** | `--error-codes`, `-E` | `diagnostics[].code` | Automated repair, debugging |

**All extensions are:**
- **Optional** - Default output remains minimal
- **Opt-in** - Only included when explicitly requested
- **Backward compatible** - Old parsers continue to work
- **Composable** - Flags can be combined

---

*Document created: 2026-01-22*
*Last updated: 2026-01-22*
*Status: Ready for review and implementation*

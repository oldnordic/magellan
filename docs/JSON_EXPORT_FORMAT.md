# JSON Export Format Reference

This document describes the unified JSON output format used by Magellan and to be followed by other tools in the integrated toolset.

## Table of Contents

1. [Response Wrapper](#response-wrapper)
2. [Common Field Types](#common-field-types)
3. [Span Format](#span-format)
4. [Response Types](#response-types)
5. [Export Formats](#export-formats)
6. [Schema Versioning](#schema-versioning)

---

## Response Wrapper

All JSON responses from Magellan commands are wrapped in a `JsonResponse` structure:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "67abc123d4567",
  "tool": "magellan",
  "timestamp": "2026-01-24T12:34:56Z",
  "partial": false,
  "data": { /* command-specific data */ }
}
```

### Wrapper Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema_version` | string | Yes | Schema version for parsing stability |
| `execution_id` | string | Yes | Unique execution ID (hex timestamp + pid) |
| `tool` | string | No | Tool name (e.g., "magellan") |
| `timestamp` | string | No | ISO 8601 timestamp in RFC 3339 format |
| `partial` | boolean | No | Whether response is truncated/incomplete |
| `data` | varies | Yes | Command-specific response data |

### Schema Version Format

- Format: `MAJOR.MINOR.PATCH` (semver)
- Current: `1.0.0`
- Incremented on breaking changes to response structure

### Execution ID Format

- Format: `{timestamp_hex}-{pid_hex}`
- Example: `67abc123d4567-1234`
- Used for tracing and log correlation

---

## Common Field Types

### Span Location

All locations use the `Span` type for consistent position representation:

```json
{
  "span_id": "a1b2c3d4e5f6g7h8",
  "file_path": "src/main.rs",
  "byte_start": 42,
  "byte_end": 100,
  "start_line": 3,
  "start_col": 5,
  "end_line": 7,
  "end_col": 10
}
```

#### Span Fields

| Field | Type | Description |
|-------|------|-------------|
| `span_id` | string | Stable SHA-256 based ID (16 hex chars) |
| `file_path` | string | Path to source file |
| `byte_start` | number | Start byte offset (inclusive) |
| `byte_end` | number | End byte offset (exclusive) |
| `start_line` | number | Start line (1-indexed) |
| `start_col` | number | Start column (0-indexed, byte-based) |
| `end_line` | number | End line (1-indexed) |
| `end_col` | number | End column (0-indexed, byte-based) |

#### Span ID Generation

```
span_id = SHA256(file_path + ":" + byte_start + ":" + byte_end)[0:16]
```

16 hex characters (64 bits) of the SHA-256 hash.

### Symbol Match

Represents a found symbol in query results:

```json
{
  "match_id": "abc123",
  "span": { /* Span object */ },
  "name": "my_function",
  "kind": "Function",
  "parent": "MyModule",
  "symbol_id": "a1b2c3d4e5f6g7h8"
}
```

#### SymbolMatch Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `match_id` | string | Yes | Hash-based match ID |
| `span` | Span | Yes | Symbol location |
| `name` | string | Yes | Symbol name |
| `kind` | string | Yes | Symbol kind (normalized) |
| `parent` | string | No | Parent symbol name |
| `symbol_id` | string | No | Stable symbol ID (16 hex chars) |

### Reference Match

Represents a reference to a symbol:

```json
{
  "match_id": "ref_abc123",
  "span": { /* Span object */ },
  "referenced_symbol": "println",
  "reference_kind": "call",
  "target_symbol_id": "a1b2c3d4e5f6g7h8"
}
```

#### ReferenceMatch Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `match_id` | string | Yes | Hash-based match ID (prefixed with "ref_") |
| `span` | Span | Yes | Reference location |
| `referenced_symbol` | string | Yes | Name of referenced symbol |
| `reference_kind` | string | No | Kind of reference (call, read, write, etc.) |
| `target_symbol_id` | string | No | Stable ID of referenced symbol |

---

## Span Format

### Half-Open Range Convention

Spans use half-open ranges `[start, end)`:
- `byte_start` is **inclusive** (first byte included)
- `byte_end` is **exclusive** (first byte NOT included)

Example: For `"fn main()"`, span covering `"main"`:
- `byte_start = 3` (points to 'm')
- `byte_end = 7` (points to '(')
- Length = `7 - 3 = 4`
- Slice: `source[3..7] == "main"`

### UTF-8 Byte Offsets

- All offsets are UTF-8 byte positions
- Columns are byte offsets within the line
- Multi-byte UTF-8 characters occupy multiple column positions

### Line Numbering

- Lines are **1-indexed** (matches editor line numbers)
- Tree-sitter uses 0-indexed, converted to 1-indexed for output

---

## Response Types

### StatusResponse

Database statistics:

```json
{
  "files": 42,
  "symbols": 1337,
  "references": 256,
  "calls": 128,
  "code_chunks": 512
}
```

### QueryResponse

Symbols found in a file:

```json
{
  "symbols": [ /* SymbolMatch array */ ],
  "file_path": "src/main.rs",
  "kind_filter": "Function"
}
```

### FindResponse

Search results:

```json
{
  "matches": [ /* SymbolMatch array */ ],
  "query_name": "main",
  "file_filter": "src/"
}
```

### RefsResponse

Call graph results:

```json
{
  "references": [ /* ReferenceMatch array */ ],
  "symbol_name": "main",
  "file_path": "src/main.rs",
  "direction": "in"
}
```

### FilesResponse

Indexed file list:

```json
{
  "files": ["src/main.rs", "src/lib.rs", ...],
  "symbol_counts": {
    "src/main.rs": 10,
    "src/lib.rs": 25
  }
}
```

### ValidationResponse

Validation results:

```json
{
  "passed": false,
  "error_count": 2,
  "errors": [
    {
      "code": "FILE_MISSING",
      "message": "Indexed file not found on filesystem",
      "entity_id": "a1b2c3d4",
      "details": { "path": "src/removed.rs" }
    }
  ],
  "warning_count": 1,
  "warnings": [
    {
      "code": "STALE_INDEX",
      "message": "File modified since last index",
      "entity_id": "e5f6g7h8",
      "details": { "path": "src/main.rs" }
    }
  ]
}
```

### ErrorResponse

Error information:

```json
{
  "code": "MAG-REF-001",
  "error": "symbol_not_found",
  "message": "Symbol 'nonexistent' not found in 'src/main.rs'",
  "span": null,
  "remediation": "Check the symbol name and file path"
}
```

---

## Export Formats

### GraphExport (export command)

Complete graph export:

```json
{
  "version": "2.0.0",
  "files": [ /* FileExport array */ ],
  "symbols": [ /* SymbolExport array */ ],
  "references": [ /* ReferenceExport array */ ],
  "calls": [ /* CallExport array */ ],
  "collisions": [ /* CollisionExport array */ ]
}
```

#### FileExport

```json
{
  "path": "src/main.rs",
  "hash": "sha256:abc123..."
}
```

#### SymbolExport

```json
{
  "symbol_id": "a1b2c3d4e5f6g7h8",
  "canonical_fqn": "my_crate::src/main.rs::Function main",
  "display_fqn": "my_crate::main",
  "name": "main",
  "kind": "Function",
  "kind_normalized": "fn",
  "file": "src/main.rs",
  "byte_start": 42,
  "byte_end": 100,
  "start_line": 3,
  "start_col": 5,
  "end_line": 7,
  "end_col": 10
}
```

#### ReferenceExport

```json
{
  "file": "src/main.rs",
  "referenced_symbol": "println",
  "target_symbol_id": "xyz789abc123",
  "byte_start": 150,
  "byte_end": 160,
  "start_line": 10,
  "start_col": 8,
  "end_line": 10,
  "end_col": 18
}
```

#### CallExport

```json
{
  "file": "src/main.rs",
  "caller": "main",
  "callee": "println",
  "caller_symbol_id": "a1b2c3d4",
  "callee_symbol_id": "xyz789abc",
  "byte_start": 150,
  "byte_end": 160,
  "start_line": 10,
  "start_col": 8,
  "end_line": 10,
  "end_col": 18
}
```

#### CollisionExport

```json
{
  "field": "fqn",
  "value": "main",
  "count": 3,
  "candidates": [
    {
      "entity_id": 123,
      "symbol_id": "a1b2c3d4",
      "canonical_fqn": "crate1::src/main.rs::Function main",
      "display_fqn": "crate1::main",
      "name": "main",
      "file_path": "crate1/src/main.rs"
    }
  ]
}
```

### JSONL Format

JSON Lines (one JSON object per line):

```
{"type":"Version","version":"2.0.0"}
{"type":"File","path":"src/main.rs","hash":"sha256:..."}
{"type":"Symbol","symbol_id":"...",...}
{"type":"Reference","file":"src/main.rs",...}
{"type":"Call","file":"src/main.rs",...}
```

---

## Rich Span Extensions

Optional fields that can be added to spans:

### SpanContext

Source code context lines:

```json
{
  "before": ["line 1", "line 2"],
  "selected": ["line 3", "line 4"],
  "after": ["line 5", "line 6"]
}
```

### SpanSemantics

Semantic information:

```json
{
  "kind": "function",
  "language": "rust"
}
```

### SpanRelationships

Call graph relationships:

```json
{
  "callers": [
    {"file": "src/caller.rs", "symbol": "caller_fn", "byte_start": 10, "byte_end": 20, "line": 5}
  ],
  "callees": [
    {"file": "src/callee.rs", "symbol": "callee_fn", "byte_start": 30, "byte_end": 40, "line": 10}
  ],
  "imports": [],
  "exports": []
}
```

### SpanChecksums

Content verification:

```json
{
  "checksum_before": "sha256:abc123...",
  "file_checksum_before": "sha256:def456..."
}
```

---

## Schema Versioning

### Version History

| Version | Changes |
|---------|---------|
| 1.0.0 | Initial schema with JsonResponse wrapper |
| 2.0.0 | Added symbol_id, canonical_fqn, display_fqn to exports |

### Backward Compatibility

- New fields are added with `serde(default)` or `skip_serializing_if`
- Required fields are never removed in minor versions
- Parse with `#[serde(default)]` for forward compatibility

### Format-Specific Versions

Export formats use separate versioning:
- JSON export: Top-level `version` field (currently "2.0.0")
- JSONL: Version record as first line
- CSV: Header comment `# Magellan Export Version: 2.0.0`

---

## Conventions for Other Tools

When implementing JSON output for other tools:

1. **Always use JsonResponse wrapper**: Include schema_version and execution_id
2. **Use Span for locations**: Consistent position representation
3. **Follow naming conventions**: snake_case for all field names
4. **Include tool name**: Set the `tool` field
5. **Add timestamps**: ISO 8601 format in UTC
6. **Use appropriate response type**: StatusResponse, QueryResponse, etc.
7. **Support partial responses**: Set `partial` flag when data is truncated
8. **Document schema version**: Update on breaking changes

### Example Response Structure

```json
{
  "schema_version": "1.0.0",
  "execution_id": "67abc123d4567",
  "tool": "llmsearch",
  "timestamp": "2026-01-24T12:34:56Z",
  "data": {
    "results": [
      {
        "span": { /* standard Span */ },
        "score": 0.95,
        "snippet": "..."
      }
    ],
    "query": "function definition",
    "total_count": 42
  }
}
```

# Context API JSON Contract

**Version:** 1.0.0  
**Stability:** Stable (v3.0.0+)

This document specifies the deterministic JSON contract for Magellan's LLM Context API. All responses follow this schema for predictable parsing by downstream tools.

---

## Table of Contents

1. [Project Summary](#1-project-summary)
2. [Symbol List](#2-symbol-list)
3. [Symbol Detail](#3-symbol-detail)
4. [File Context](#4-file-context)
5. [Pagination](#5-pagination)
6. [Error Responses](#6-error-responses)

---

## 1. Project Summary

**Endpoint:** `magellan context summary --db <path> --json`

**Purpose:** High-level project overview (~50 tokens)

### Schema

```json
{
  "schema_version": "1.0.0",
  "execution_id": "<uuid>",
  "data": {
    "name": "<string>",
    "version": "<string>",
    "language": "<string>",
    "total_files": "<integer>",
    "total_symbols": "<integer>",
    "symbol_counts": {
      "functions": "<integer>",
      "methods": "<integer>",
      "structs": "<integer>",
      "traits": "<integer>",
      "enums": "<integer>",
      "modules": "<integer>",
      "other": "<integer>"
    },
    "entry_points": ["<string>"],
    "description": "<string>"
  },
  "tool": "magellan",
  "timestamp": "<ISO8601>"
}
```

### Example

```json
{
  "schema_version": "1.0.0",
  "execution_id": "69a5379e-44d43",
  "data": {
    "name": "magellan",
    "version": "3.0.0",
    "language": "Rust",
    "total_files": 143,
    "total_symbols": 2271,
    "symbol_counts": {
      "functions": 1942,
      "methods": 0,
      "structs": 150,
      "traits": 2,
      "enums": 28,
      "modules": 149,
      "other": 0
    },
    "entry_points": [],
    "description": "magellan 3.0.0 written in Rust, 143 files, 2271 symbols (1942 functions, 150 structs)"
  },
  "tool": "magellan",
  "timestamp": "2026-03-02T07:13:54Z"
}
```

### Field Descriptions

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Project name (from Cargo.toml or directory) |
| `version` | string | Project version |
| `language` | string | Primary language (Rust, Python, Java, etc.) |
| `total_files` | integer | Number of indexed source files |
| `total_symbols` | integer | Total symbol count |
| `symbol_counts` | object | Breakdown by symbol kind |
| `entry_points` | array | Known entry points (main functions, etc.) |
| `description` | string | Human-readable summary |

---

## 2. Symbol List

**Endpoint:** `magellan context list --db <path> --kind <kind> --page <n> --page-size <n> --json`

**Purpose:** Paginated symbol listing

### Schema

```json
{
  "schema_version": "1.0.0",
  "execution_id": "<uuid>",
  "data": {
    "page": "<integer>",
    "total_pages": "<integer>",
    "page_size": "<integer>",
    "total_items": "<integer>",
    "next_cursor": "<string | null>",
    "prev_cursor": "<string | null>",
    "items": [
      {
        "name": "<string>",
        "kind": "<string>",
        "file": "<string>",
        "line": "<integer>"
      }
    ]
  },
  "tool": "magellan",
  "timestamp": "<ISO8601>"
}
```

### Example

```json
{
  "schema_version": "1.0.0",
  "execution_id": "69a5379e-44d50",
  "data": {
    "page": 1,
    "total_pages": 389,
    "page_size": 5,
    "total_items": 1942,
    "next_cursor": "cGFnZT0y",
    "prev_cursor": null,
    "items": [
      {
        "name": "make_test_file",
        "kind": "fn",
        "file": "/home/user/project/benches/harness.rs",
        "line": 0
      },
      {
        "name": "setup_test_graph",
        "kind": "fn",
        "file": "/home/user/project/benches/harness.rs",
        "line": 0
      }
    ]
  },
  "tool": "magellan",
  "timestamp": "2026-03-02T07:15:00Z"
}
```

### Pagination

| Field | Type | Description |
|-------|------|-------------|
| `page` | integer | Current page number (1-indexed) |
| `total_pages` | integer | Total pages available |
| `page_size` | integer | Items per page |
| `total_items` | integer | Total items across all pages |
| `next_cursor` | string\|null | Cursor for next page (base64 encoded) |
| `prev_cursor` | string\|null | Cursor for previous page |

### Cursor Format

Cursors are base64-encoded JSON:
```
cGFnZT0y  →  {"page":2}
```

---

## 3. Symbol Detail

**Endpoint:** `magellan context symbol --db <path> --name <name> --callers --callees --json`

**Purpose:** Detailed symbol information with call graph

### Schema

```json
{
  "schema_version": "1.0.0",
  "execution_id": "<uuid>",
  "data": {
    "name": "<string>",
    "kind": "<string>",
    "file": "<string>",
    "line": "<integer>",
    "signature": "<string | null>",
    "documentation": "<string | null>",
    "callers": ["<string>"],
    "callees": ["<string>"],
    "related": ["<string>"]
  },
  "tool": "magellan",
  "timestamp": "<ISO8601>"
}
```

### Example

```json
{
  "schema_version": "1.0.0",
  "execution_id": "69a5379e-44d60",
  "data": {
    "name": "run_indexer",
    "kind": "fn",
    "file": "/home/user/project/src/indexer.rs",
    "line": 92,
    "signature": "pub fn run_indexer(root_path: PathBuf, db_path: PathBuf) -> Result<usize>",
    "documentation": null,
    "callers": ["main", "run_watch_pipeline"],
    "callees": ["handle_event", "reconcile_deleted_files"],
    "related": ["run_indexer_n", "run_watch_pipeline", "handle_event"]
  },
  "tool": "magellan",
  "timestamp": "2026-03-02T07:16:00Z"
}
```

### Field Descriptions

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Symbol name |
| `kind` | string | Symbol kind (fn, struct, enum, etc.) |
| `file` | string | Absolute file path |
| `line` | integer | Line number (1-indexed) |
| `signature` | string\|null | Type signature (from LSP enrichment) |
| `documentation` | string\|null | Documentation (from LSP enrichment) |
| `callers` | array | Functions that call this symbol |
| `callees` | array | Functions this symbol calls |
| `related` | array | Symbols in same file/module |

---

## 4. File Context

**Endpoint:** `magellan context file --db <path> --path <path> --json`

**Purpose:** File-level context with symbol breakdown

### Schema

```json
{
  "schema_version": "1.0.0",
  "execution_id": "<uuid>",
  "data": {
    "path": "<string>",
    "language": "<string>",
    "symbol_count": "<integer>",
    "symbol_counts": {
      "functions": "<integer>",
      "methods": "<integer>",
      "structs": "<integer>",
      "traits": "<integer>",
      "enums": "<integer>",
      "modules": "<integer>",
      "other": "<integer>"
    },
    "public_symbols": ["<string>"],
    "imports": ["<string>"]
  },
  "tool": "magellan",
  "timestamp": "<ISO8601>"
}
```

### Example

```json
{
  "schema_version": "1.0.0",
  "execution_id": "69a5379e-44d70",
  "data": {
    "path": "/home/user/project/src/main.rs",
    "language": "rust",
    "symbol_count": 24,
    "symbol_counts": {
      "functions": 3,
      "methods": 0,
      "structs": 0,
      "traits": 0,
      "enums": 0,
      "modules": 21,
      "other": 0
    },
    "public_symbols": [
      "mod:ast_cmd",
      "mod:collisions_cmd",
      "fn:main"
    ],
    "imports": []
  },
  "tool": "magellan",
  "timestamp": "2026-03-02T07:17:00Z"
}
```

---

## 5. Pagination

All list endpoints support pagination via query parameters:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `page` | integer | 1 | Page number (1-indexed) |
| `page_size` | integer | 50 | Items per page (max: 100) |
| `cursor` | string | null | Cursor from previous response |

### Cursor-Based Pagination

For efficient pagination, use cursors instead of page numbers:

```bash
# First page
magellan context list --db code.db --page 1 --page-size 50 --json

# Next page (use cursor from response)
magellan context list --db code.db --cursor cGFnZT0y --json
```

---

## 6. Error Responses

### Schema

```json
{
  "schema_version": "1.0.0",
  "error": {
    "code": "<string>",
    "message": "<string>",
    "details": "<object | null>"
  }
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `NOT_FOUND` | 404 | Symbol or file not found |
| `INVALID_CURSOR` | 400 | Malformed pagination cursor |
| `DATABASE_ERROR` | 500 | Database access error |
| `RATE_LIMITED` | 429 | Too many requests |

### Example

```json
{
  "schema_version": "1.0.0",
  "error": {
    "code": "NOT_FOUND",
    "message": "Symbol 'nonexistent' not found",
    "details": {
      "suggestions": ["main", "main_loop", "main_thread"]
    }
  }
}
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-03-02 | Initial release (Magellan v3.0.0) |

---

## Implementation Notes

### Determinism

All responses are deterministic:
- Same query → Same output
- Symbol ordering is stable (sorted by file, then line)
- Pagination cursors are reproducible

### Token Efficiency

Responses are optimized for LLM context windows:
- Project summary: ~50 tokens
- Symbol list (50 items): ~200 tokens
- Symbol detail: ~100-150 tokens
- File context: ~100 tokens

### Backward Compatibility

The `schema_version` field ensures forward compatibility. Clients should:
1. Check `schema_version` before parsing
2. Handle unknown fields gracefully (ignore, don't error)
3. Not rely on field ordering (use keys)

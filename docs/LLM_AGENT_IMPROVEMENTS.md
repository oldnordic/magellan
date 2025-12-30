# Magellan Improvements for LLM Agents

**Date**: 2025-12-27
**Purpose**: Suggestions for making Magellan more useful for AI agents that need to understand and navigate codebases

---

## Context

This document outlines improvements to Magellan from the perspective of an **LLM agent** that uses the tool to:
- Understand codebase structure
- Navigate symbol relationships
- Answer questions about code organization
- Find what functions call what
- Analyze dependencies

---

## Priority Matrix

| Priority | Feature | Impact | Effort |
|----------|---------|--------|--------|
| HIGH | Initial full scan | Can query immediately on new project | Low |
| HIGH | Call graph edges | Bidirectional navigation (who calls X, what X calls) | Medium |
| MEDIUM | Line/column spans | Precise locations for references | Low |
| MEDIUM | Symbol kind filtering | Cleaner queries | Low |
| LOW | JSON export | Snapshots, diffs, non-SQLite consumers | Low |

---

## 1. Initial Full Scan (HIGH Priority)

### Current Behavior
Magellan only indexes files when they change via filesystem events.

**Problem**: Empty database until files are modified. When an LLM agent first joins a project, it sees nothing and must wait for file edits to trigger indexing.

### Proposed Addition

```bash
magellan watch --root <DIR> --db <FILE> --scan-initial
```

On startup:
1. Scan all `.rs` files in the directory tree
2. Extract symbols and references
3. Populate database
4. Then enter watch mode for incremental updates

### Alternative: Separate Scan Command

```bash
magellan scan --root <DIR> --db <FILE>
```

One-time scan, exits after completion. Useful for:
- CI/CD pipelines
- Pre-indexing before starting watch mode
- Snapshots without long-running process

### Implementation Notes

- Walk directory tree using `walkdir` crate
- Use existing `extract_symbols()` and `extract_references()` functions
- Batch inserts for performance
- Show progress: `Scanning... 142/300 files`

---

## 2. Call Graph Edges (HIGH Priority)

### Current Behavior
Magellan tracks `REFERENCES` edges (who references this symbol), but not the direction of calls.

**Problem**: Can ask "who calls `foo()`" but not "what does `foo()` call?" — the forward call graph.

### Proposed Schema Addition

Add a second edge type for outbound calls:

```rust
// Existing
EdgeType::Reference  // "X references Y" (bidirectional)

// New
EdgeType::Calls      // "X calls Y" (directional, from caller to callee)
```

### Edge Semantics

| Edge Type | Direction | Meaning |
|-----------|-----------|---------|
| `REFERENCES` | Y → X | "Y contains a reference to symbol X" |
| `CALLS` | X → Y | "Function X contains a call to function Y" |

### Query Examples

```rust
// Who calls this function? (existing, via REFERENCES)
magellan_db.references_to_symbol("process_request")

// What does this function call? (new, via CALLS)
magellan_db.calls_from_symbol("process_request")

// Get full call chain
magellan_db.call_chain("main")  // main → parse → execute → process → respond
```

### Use Cases

- **Downstream impact**: "If I change `Database::execute`, what breaks?"
- **Upstream analysis**: "What code path led to this error?"
- **Dead code detection**: Functions with zero inbound calls
- **Complexity analysis**: "Which functions have too many responsibilities?"

### Implementation Notes

- Extracted during reference extraction
- Distinguish function calls from type references
- `identifier` within function scope → `CALLS` edge
- `scoped_identifier` → `REFERENCES` edge (type usage)

---

## 3. Line/Column Spans (MEDIUM Priority)

### Current Behavior
Symbols have byte spans, but references don't have precise line/column locations.

**Problem**: Can't point to exact locations when navigating. Byte spans require file content to resolve.

### Proposed Addition

Store `(start_line, start_col, end_line, end_col)` for both definitions and references.

```rust
pub struct Span {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}
```

### Schema Change

```sql
-- Current
CREATE TABLE graph_entities (
    id INTEGER PRIMARY KEY,
    kind TEXT,
    name TEXT,
    file_path TEXT,
    data TEXT  -- JSON with byte_start, byte_end
);

-- Proposed: Add line/col to data JSON
{
    "byte_start": 142,
    "byte_end": 256,
    "start_line": 12,
    "start_col": 4,
    "end_line": 18,
    "end_col": 2
}
```

### Use Cases

- "Go to definition" without loading file
- Preview snippets from database
- Line-based diffs

---

## 4. Symbol Kind Filtering (MEDIUM Priority)

### Current Behavior
Query APIs return all symbols; filtering must happen client-side.

### Proposed Addition

Add filter parameter to query functions:

```rust
pub fn symbols_in_file(
    &self,
    file_pattern: &str,
    kind: Option<SymbolKind>,  // NEW: filter by kind
) -> Result<Vec<Symbol>>
```

### Example

```rust
// Get only functions
let functions = db.symbols_in_file("src/lib.rs", Some(SymbolKind::Function))?;

// Get all symbols
let all = db.symbols_in_file("src/lib.rs", None)?;
```

### SymbolKind Values

- Function
- Struct
- Enum
- Trait
- Module
- Impl

---

## 5. JSON Export (LOW Priority)

### Current Behavior
Direct SQLite access required. No export format.

### Proposed Addition

```bash
magellan export --db <FILE> --format json > codegraph.json
magellan export --db <FILE> --format jsonl > codegraph.jsonl
```

### Use Cases

- Snapshots for code review diffs
- Non-SQLite consumers
- Archive and restore
- Debugging and inspection

### Output Format

```json
{
  "version": "0.1.0",
  "exported_at": "2025-12-27T10:00:00Z",
  "files": 142,
  "symbols": 387,
  "references": 1241,
  "entities": [
    {
      "id": 1,
      "kind": "Function",
      "name": "process_request",
      "file_path": "src/main.rs",
      "span": {"start_line": 12, "end_line": 45}
    },
    ...
  ],
  "edges": [
    {
      "from": 2,
      "to": 1,
      "edge_type": "CALLS"
    },
    ...
  ]
}
```

---

## Non-Goals (Explicitly Excluded)

These are **out of scope** for Magellan:

- ❌ Semantic analysis (type checking, trait resolution)
- ❌ LSP server functionality
- ❌ Async runtimes or background threads
- ❌ Config files
- ❌ Multi-language support (Rust only)
- ❌ Web APIs or network services

Magellan is a **dumb, deterministic codebase mapping tool**. Keep it focused.

---

## Implementation Order

1. **Phase 1**: Initial full scan (`--scan-initial` flag)
2. **Phase 2**: Line/Column spans (extend data JSON)
3. **Phase 3**: Symbol kind filtering (add parameter)
4. **Phase 4**: Call graph edges (new edge type)
5. **Phase 5**: JSON export (new command)

Each phase:
1. Update CHANGELOG.md with feature description
2. Write tests (TDD first)
3. Implement feature
4. Verify all tests pass
5. Rebuild binary

---

*Last Updated: 2025-12-27*
*Status: Proposal — Awaiting Implementation*

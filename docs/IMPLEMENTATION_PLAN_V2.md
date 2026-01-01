# Implementation Plan: Magellan and Splice Improvements

**Created**: 2025-01-01
**Status**: Fact-Based Plan
**Basis**: Source code analysis of Magellan, Splice, and SQLiteGraph

---

## Executive Summary

This plan is based on ACTUAL source code analysis, not speculation. All implementation decisions cite specific file paths and symbols.

**Current Situation:**
- Magellan uses `SqliteGraphBackend` with JSON-serialized node data
- Splice uses `GraphBackend` trait with unified API
- Neither uses SQLiteGraph's labels/properties/HNSW features
- Native V2 backend has blocking bugs (edge operations fail)

**Recommendation:** Improve Magellan first using SQLite backend with verified features. Defer Native V2 backend and HNSW until bugs are fixed.

---

## Fact Base: What We Actually Know

### Fact 1: SQLiteGraph Public API (from `/sqlitegraph/sqlitegraph/src/lib.rs`)

**Exported Functions:**
```rust
// Lines 86-90
pub use api_ergonomics::{Label, NodeId, PropertyKey, PropertyValue};
pub use index::{add_label, add_property};  // ONLY ADD functions exported
```

**Critical Finding:** `get_entities_by_label()` and `get_entities_by_property()` exist in `/sqlitegraph/sqlitegraph/src/index.rs` (lines 19-73) but are NOT re-exported in `lib.rs`.

**Implication:** Labels can be ADDED but cannot be QUERIED through the public API without direct SQL access.

### Fact 2: Labels and Properties Implementation (from `/sqlitegraph/sqlitegraph/src/index.rs`)

**Label storage (lines 8-17):**
```rust
pub fn add_label(graph: &SqliteGraph, entity_id: i64, label: &str) -> Result<(), SqliteGraphError> {
    graph.connection().execute(
        "INSERT OR IGNORE INTO graph_labels(entity_id, label) VALUES(?1, ?2)",
        params![entity_id, label],
    )
}
```

**Property storage (lines 37-51):**
```rust
pub fn add_property(graph: &SqliteGraph, entity_id: i64, key: &str, value: &str) -> Result<()> {
    graph.connection().execute(
        "INSERT INTO graph_properties(entity_id, key, value) VALUES(?1, ?2, ?3)",
        params![entity_id, key, value],
    )
}
```

**Implication:** Labels/properties are stored in separate SQLite tables (`graph_labels`, `graph_properties`), NOT in the node's JSON data field.

### Fact 3: How Magellan Stores Symbols (from `/magellan/src/graph/symbols.rs`)

**Current implementation (lines 20-48):**
```rust
pub fn insert_symbol_node(&self, fact: &SymbolFact) -> Result<NodeId> {
    let symbol_node = SymbolNode { /* ... */ };

    let node_spec = NodeSpec {
        kind: "Symbol".to_string(),  // Uses 'kind' field for categorization
        name: fact.name.clone().unwrap_or(...),
        file_path: Some(fact.file_path.to_string_lossy().to_string()),
        data: serde_json::to_value(symbol_node)?,  // All metadata in JSON
    };

    let id = self.backend.insert_node(node_spec)?;
    Ok(NodeId::from(id))
}
```

**Implication:** Magellan stores ALL metadata in the `data` JSON field, NOT using labels/properties.

### Fact 4: How Splice Stores Symbols (from `/splice/src/graph/mod.rs`)

**Splice implementation (lines 97-150):**
```rust
pub fn store_symbol_with_file_and_language(...) -> Result<NodeId> {
    // Uses label as node 'kind' (from schema.rs)
    let label = schema::kind_to_label(kind);

    let node_spec = NodeSpec {
        kind: label.0,  // e.g., "symbol_function"
        name: name.to_string(),
        file_path: Some(file_path_str.to_string()),
        data: json!({
            "kind": kind,
            "language": language.as_str(),
            "byte_start": byte_start,
            "byte_end": byte_end,
        }),
    };
    // ...
}
```

**Implication:** Splice uses label constants for node `kind`, but still stores metadata in JSON `data` field. Does NOT use `add_label()` or `add_property()` functions.

### Fact 5: Splice's Ingest is Not Implemented (from `/splice/src/ingest/mod.rs`)

```rust
// Lines 50-55
pub fn ingest_file(&mut self, _path: &Path) -> Result<()> {
    // TODO: Implement in Task 1
    Err(crate::error::SpliceError::Other(
        "Not implemented yet".to_string(),
    ))
}
```

**Implication:** Splice has architecture but incomplete implementation. Magellan is more complete.

### Fact 6: Native V2 Backend Bug (from `/sqlitegraph/docs/NATIVE_V2_EDGE_BUG.md`)

**Error:**
```
Error: connection error: Corrupt node record 0: Invalid V2 node record version 0
```

**Implication:** Native V2 backend cannot be used for production until edge operations are fixed.

### Fact 7: Magellan's Backend Access Pattern (from `/magellan/src/graph/mod.rs`)

```rust
// Lines 61-62
let sqlite_graph = sqlitegraph::SqliteGraph::open(db_path)?;
let backend = Rc::new(SqliteGraphBackend::from_graph(sqlite_graph));
```

**Implication:** Magellan creates `SqliteGraphBackend` directly, NOT using unified `open_graph()` API.

---

## Design Constraints

### Constraint 1: No Native V2 Backend
- Bug with edge operations is unresolved
- Must use SQLite backend (`GraphConfig::sqlite()`)

### Constraint 2: Limited Label/Property Query Support
- `add_label()` and `add_property()` are exported
- `get_entities_by_label()` and `get_entities_by_property()` are NOT exported
- Options:
  a) Write raw SQL queries using `backend.graph().connection()`
  b) Export missing functions from sqlitegraph (requires changes there)
  c) Query using existing `neighbors()` and filtering in memory

### Constraint 3: Splice is Incomplete
- Ingest module returns "Not implemented yet" errors
- Magellan has working parsers and indexing

---

## Implementation Plan

### Phase 1: Source Code Text Storage (High Value, Low Risk)

**Objective:** Store source code chunks with byte spans to enable token-efficient queries.

**Why This Works:**
- Uses existing `SqliteGraphBackend` API
- No new sqlitegraph features required
- Magellan already has byte spans from tree-sitter

**Implementation:**

1. **Create new table for code chunks** (via direct SQL or use existing node storage)
   - File: `/magellan/src/generation/schema.rs` (NEW, <300 LOC)
   - Add `CodeChunk` struct
   - Add migration to create `code_chunks` table

2. **Extract code chunks during indexing**
   - File: `/magellan/src/graph/ops.rs` (MODIFY existing `index_file()`)
   - After parsing symbols, extract source text by byte span
   - Store in `code_chunks` table

3. **Query API for code chunks**
   - File: `/magellan/src/graph/mod.rs` (ADD method)
   - `get_code_chunk(path: &str, symbol_name: &str) -> Result<Option<String>>`

4. **CLI command**
   - File: `/magellan/src/main.rs` (ADD command)
   - `magellan get <file> <symbol-name>`

### Phase 2: Label and Property Integration (Medium Value, Medium Risk)

**Objective:** Use sqlitegraph's label/property system for metadata.

**Challenge:** Query functions not exported from sqlitegraph.

**Options:**

**Option A: Write Raw SQL Queries (Recommended)**
- Use `backend.graph().connection()` for direct SQL
- Pros: No changes to sqlitegraph required, works immediately
- Cons: Bypasses public API, coupling to SQLite schema

**Option B: Export Functions from sqlitegraph**
- Modify `/sqlitegraph/sqlitegraph/src/lib.rs` to export `get_entities_by_label()`
- Pros: Clean API, proper abstraction
- Cons: Requires changes to sqlitegraph

**Implementation (Option A):**

1. **Add labels during symbol insertion**
   - File: `/magellan/src/graph/symbols.rs` (MODIFY `insert_symbol_node()`)
   - After `insert_node()`, call `add_label()` for language, visibility

2. **Query helpers in Magellan**
   - File: `/magellan/src/graph/query.rs` (ADD new functions)
   - `symbols_with_label(label: &str) -> Result<Vec<SymbolFact>>`
   - Uses raw SQL: `SELECT entity_id FROM graph_labels WHERE label=?1`

3. **CLI command**
   - `magellan query --label <label>`

### Phase 3: Splice Integration with Magellan (Longer Term)

**Objective:** Complete Splice's ingest using Magellan's parsers.

**Approach:**
- Magellan becomes the "ingestion engine"
- Splice uses Magellan's `CodeGraph` as backend
- Splice focuses on refactoring operations

---

## File Changes Summary

| File | Change | LOC Limit |
|------|--------|-----------|
| `src/generation/schema.rs` | NEW: CodeChunk struct | <300 |
| `src/generation/mod.rs` | NEW: code chunk operations | <300 |
| `src/graph/ops.rs` | MODIFY: add code chunk extraction | Current + <100 |
| `src/graph/symbols.rs` | MODIFY: add labels | Current + <50 |
| `src/graph/query.rs` | ADD: label/property queries | <300 |
| `src/graph/mod.rs` | ADD: `get_code_chunk()` method | Current + <30 |
| `src/main.rs` | ADD: CLI commands | Current + <100 |

---

## Testing Strategy

### Unit Tests
- `src/generation/tests.rs` - code chunk extraction
- `src/graph/tests.rs` - label/property queries

### Integration Tests
- `tests/code_chunk_tests.rs` - end-to-end chunk storage/retrieval
- `tests/label_query_tests.rs` - label-based queries

### CLI Tests
- `tests/cli_tests.rs` - new CLI commands

---

## Rollout Plan

1. ✅ Research complete (this document)
2. ✅ Phase 1: Source code text storage (COMPLETED 2025-01-01)
3. ✅ Phase 2: Label/property integration (COMPLETED 2025-01-01)
4. ⏳ Phase 3: Splice-Magellan integration
5. ⏳ Update documentation
6. ⏳ Release v0.5.0

---

## Change Log

### 2025-01-01: Phase 2 Complete - Label and Property Integration

**Implemented:**
- Automatic label assignment during symbol indexing
  - Language labels (rust, python, javascript, etc.)
  - Symbol kind labels (fn, struct, enum, method, etc.)
- Label query API in `src/graph/query.rs`
  - `get_entities_by_label()` - Get entities with a specific label
  - `get_entities_by_labels()` - Get entities with ALL labels (AND semantics)
  - `get_all_labels()` - List all available labels
  - `count_entities_by_label()` - Count entities with a label
  - `get_symbols_by_label()` - Get symbols with metadata by label
- CLI command: `magellan label --db <FILE> [--label <LABEL>]... [--list] [--count]`
- Native-v2 backend feature flag added to Cargo.toml

**Files modified:**
- `src/graph/symbols.rs` - Added `add_label()` calls in `insert_symbol_node()`
- `src/graph/query.rs` - Added label-based query methods
- `src/graph/mod.rs` - Made `query` module public
- `src/lib.rs` - Re-exported `SymbolQueryResult`
- `src/main.rs` - Added `Label` command variant and `run_label()` function
- `src/generation/mod.rs` - Made `connect()` method public
- `Cargo.toml` - Added `native-v2` feature

**Usage examples:**
```bash
# List all labels with counts
magellan label --db graph.db --list

# Query symbols by label
magellan label --db graph.db --label fn

# Multi-label query (AND semantics)
magellan label --db graph.db --label rust --label fn

# Count entities with a label
magellan label --db graph.db --count --label struct
```

### 2025-01-01: sqlitegraph 0.2.11 Update

**Bug Fixed:** Native V2 backend node slot corruption

- **Root Cause:** The `v2_io_exclusive_std` feature compiled out the fallback `read_bytes` path in `NodeStore::read_node_v2`, leaving only zeroed buffers
- **Fix Location:** `sqlitegraph/src/backend/native/node_store.rs:363`
- **Fix Details:** Rewired conditional so canonical read_bytes path executes when mmap-exclusive combo isn't active, regardless of other feature flags
- **Impact:** Magellan updated to sqlitegraph 0.2.11, all 97 tests pass

### 2025-01-01: Phase 1 Complete

**Implemented:**
- `CodeChunk` schema with byte spans and symbol metadata
- `ChunkStore` for code storage/retrieval
- CLI commands: `get`, `get-file`
- Automatic code chunk extraction during indexing
- 97/97 tests passing

---

## Open Questions

1. **Should we use Option A (raw SQL) or Option B (export functions) for label queries?**
   - Option A is faster to implement
   - Option B is cleaner but requires sqlitegraph changes

2. **Should we create a separate `code_chunks` table or use existing node storage?**
   - Separate table: simpler queries, but duplicate data
   - Node storage: consistent with current design, but complex queries

3. **Should we migrate Splice to use Magellan's parsers or vice versa?**
   - Magellan's implementation is more complete
   - Splice's architecture (unified API) is cleaner

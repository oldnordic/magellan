# Magellan Database Invariants

**Version:** 3.1.0
**Purpose:** Document enforced invariants across all backends

---

## Overview

Magellan maintains several invariants to ensure data consistency and correctness.
These invariants are enforced through database operations, re-index logic, and
query semantics.

---

## Symbol ID Invariants

### Stable Symbol IDs

**Invariant:** Symbol IDs are stable across re-index operations when symbol
definition has not changed.

**Mechanism:**
```rust
// SHA-256 hash of language:fqn:span_id
let symbol_id = sha256(format!("{}:{}:{}", language, fqn, span_id));
```

**Guarantees:**
- Same symbol in same location → same ID
- Different symbols → different IDs (collision-resistant via SHA-256)
- ID is deterministic and reproducible

**Where enforced:** `src/graph/symbols.rs:generate_symbol_id()`

### Symbol ID Monotonicity

**SQLite:** Node IDs (graph_entities.id) are monotonic within a database but
NOT stable across re-index. Use `symbol_id` field for stable identifiers.

**Geometric:** Symbol IDs are assigned sequentially but symbol_id (SHA-256)
remains stable across operations.

---

## Name Resolution Invariants

### No Silent Ambiguity Resolution

**Invariant:** When multiple symbols have the same name, Magellan does NOT
silently pick one. It returns all matches.

**Behavior:**
```bash
magellan find --db code.db --name "process"
# Returns ALL symbols named "process" across all files
```

**Where enforced:** `src/graph/query.rs:symbols_by_name()` returns `Vec<Symbol>`

### FQN Uniqueness

**Invariant:** Fully-qualified names (FQNs) are unique within a language
namespace.

**Mechanism:**
- Symbols are indexed by FQN
- FQN includes module/crate path for disambiguation
- Example: `crate::foo::bar::process` vs `crate::baz::process`

**Exception:** Different languages may have different FQN formats. Cross-language
queries return both results.

---

## Path Normalization Invariants

### Path Normalization Rules

**Invariant:** File paths are normalized before storage and comparison.

**Rules:**
1. Remove `./` prefix
2. Collapse redundant `./` segments
3. Resolve `..` segments relative to project root
4. Use forward slashes (even on Windows)

**Examples:**
```
./src/main.rs       → src/main.rs
src/../src/main.rs  → src/main.rs
src/./lib.rs        → src/lib.rs
```

**Where enforced:** `src/indexer.rs` and `src/graph/files.rs`

### Path Comparison Semantics

**Invariant:** Path comparisons use normalized paths, not raw strings.

**Impact:**
- `src/main.rs` and `./src/main.rs` refer to same file
- Queries work regardless of `./` prefix
- Re-index detection uses hash, not path comparison

---

## Re-Index Correctness Invariants

### Stable Symbol Counts

**Invariant:** Re-indexing an unchanged file does NOT change symbol count.

**Verified by:** `tests/churn_harness_test.rs`

**Expected behavior:**
```
Cycle 1: 6 symbols
Cycle 2: 6 symbols (unchanged)
Cycle 3: 6 symbols (unchanged)
...
```

**Failure mode:** Symbol count increases on each re-index → indicates bug
in delete-or-insert logic.

### Stable File Counts

**Invariant:** Re-indexing does NOT create duplicate file entries.

**Mechanism:**
- File nodes are looked up by path
- Existing file node is updated, not duplicated
- DEFINES edges from file to symbols are recreated on each re-index

### Hash-Based Freshness

**Invariant:** File content hash is the source of truth for freshness.

**Mechanism:**
1. Compute SHA-256 of file contents
2. Compare with stored hash in FileNode
3. If hashes match → file is unchanged, skip re-index
4. If hashes differ → file has changed, delete and re-index

**Where enforced:** `src/graph/ops.rs:reconcile_file_path()`

### Atomic Re-Index

**Invariant:** Re-index is all-or-nothing. Partial updates are not persisted.

**Mechanism:**
- Delete all existing data for file (symbols, references, calls, CFG, AST, chunks)
- Insert all new data for file
- Transaction commits only if all operations succeed

**Failure mode:** Database crash during re-index → may leave file in
inconsistent state. Next re-index will correct it.

---

## Logical Truth vs Physical Storage Truth

### Symbol ID is Logical Identity

**Invariant:** `symbol_id` (SHA-256) represents the logical identity of a symbol,
regardless of storage backend.

**Implication:**
- Same symbol has same `symbol_id` in SQLite and Geometric backends
- Queries can use `symbol_id` for cross-backend references
- Internal node IDs are backend-specific and not portable

### File Hash is Logical Version

**Invariant:** File content hash represents the logical version of a file.

**Implication:**
- Files with same hash are logically identical
- Hash comparison determines re-index necessity
- Hash is independent of filesystem metadata (mtime, etc.)

---

## CFG Vacuum Invariants

### Live Block Tracking

**Invariant (Geometric):** `cfg_function_ids` set is the source of truth for
which function_ids have live CFG blocks.

**Mechanism:**
- On index: Add function_id to tracking set
- On reconcile (file change): Remove function_id from tracking set
- On vacuum: Only persist blocks for tracked function_ids

**Where enforced:** `src/graph/geometric_backend.rs`

### Stale Data Isolation

**Invariant (Geometric):** Stale CFG blocks are excluded from queries before
vacuum.

**Mechanism:**
1. Blocks are loaded from storage
2. Filtered by `cfg_function_ids` tracking set
3. Only live blocks are returned to queries

**Benefit:** No need for immediate vacuum - stale data is invisible.

### Vacuum Does Not Affect Symbols

**Invariant:** CFG vacuum operation does NOT modify symbols or call graph.

**Reason:** Symbols and CFG are stored in separate sections. Vacuum only
touches CFG section.

---

## Chunk/Callgraph Cleanup Truth

### Chunk Deletion Cascades

**Invariant (SQLite):** Deleting a symbol deletes associated code chunks.

**Mechanism:**
- Re-index deletes all symbols for a file
- Chunk deletion is cascaded via file_path or symbol association
- Orphan chunks are not left behind

### Call Graph Consistency

**Invariant:** Call graph edges are removed when caller or callee is deleted.

**Mechanism:**
- SQLite: Edges touching deleted entities are removed via
  `delete_edges_touching_entities()`
- Geometric: Call graph is rebuilt from live symbols on each save

**Where enforced:**
- SQLite: `src/graph/ops.rs:delete_file_facts()`
- Geometric: `src/graph/geometric_backend.rs:save_symbol_index()`

---

## Snapshot Truth

### Current Snapshot Queries

**Invariant:** Queries use `SnapshotId::current()` unless time-travel is
explicitly requested.

**Mechanism:**
```rust
let snapshot = SnapshotId::current();
let node = backend.get_node(snapshot, node_id)?;
```

**Implication:** Queries always see latest data, not historical snapshots.

### Historical Snapshots (Incomplete)

**Status:** MVCC timestamp fields exist in Geometric backend but time-travel
queries are NOT implemented.

**Fields (Geometric):**
- `begin_ts: u64` - Placeholder
- `end_ts: u64` - Placeholder
- `tx_id: u64` - Placeholder
- `visibility: u8` - Not used

**Do NOT rely on:** Temporal queries, version comparison, time-travel pathfinding.

---

## Edge Consistency Invariants

### No Orphan Edges

**Invariant (SQLite):** Edges with non-existent endpoints are cleaned up after
deletion.

**Mechanism:**
```rust
delete_edges_touching_entities(conn, &deleted_entity_ids)?;
```

**Where enforced:** `src/graph/ops.rs:delete_file_facts()`

### Bidirectional Call Graph

**Invariant (Geometric):** Call graph maintains both caller→callees and
callee→callers indexes.

**Mechanism:**
```rust
call_graph.insert_call(caller_id, callee_id);
// Automatically updates both directions
```

**Where enforced:** `src/graph/geometric_calls.rs`

---

## Position Convention Invariants

### Line Numbering

**Invariant:** Line numbers are 1-indexed (line 1 is first line).

**Source:** tree-sitter convention

**Examples:**
- `start_line: 1` means first line of file
- `end_line: 5` means fifth line of file

### Column Numbering

**Invariant:** Column numbers are 0-indexed (column 0 is first character).

**Source:** tree-sitter convention

**Examples:**
- `start_col: 0` means first character in line
- `start_col: 4` means fifth character in line

### Byte Offsets

**Invariant:** Byte offsets are 0-indexed from file start (byte 0 is first byte).

**Source:** tree-sitter convention

**Consistency:** Byte offsets, line numbers, and column numbers all refer to
the same position in the source file.

---

## Schema Version Invariants

### Schema Version Check

**Invariant:** Database schema version is checked on open.

**Mechanism:**
```rust
if current_schema != MAGELLAN_SCHEMA_VERSION {
    return Err("Schema version mismatch");
}
```

**Where enforced:** `src/graph/db_compat.rs:ensure_schema()`

### Migration Path

**Invariant:** Schema changes include migration logic.

**Mechanism:**
- Each schema version has migration function
- Migrations run in version order
- Migration failure is an error

**Current version:** `MAGELLAN_SCHEMA_VERSION = 6`

---

## Backend-Specific Invariants

### SQLite

- Auto-increment IDs are monotonic but not stable
- Foreign key relationships are implicit (not enforced by SQLite)
- Transaction rollback on failure

### Geometric

- Single-file format (no sidecar files)
- CFG blocks are lazily deleted (marked stale, vacuumed later)
- Symbol index is in-memory with lazy persistence
- 3D spatial coordinates enable O(log n) queries

### Native V3

- High-performance KV store
- B+Tree clustered adjacency
- Same invariants as SQLite for data consistency

---

## Testing Invariants

### Churn Test

**Test:** `tests/churn_harness_test.rs`

**Validates:**
- Symbol count stable across 5 re-index cycles
- File count stable
- Database size stabilizes after initial WAL creation
- VACUUM reclaims space

### Backend Parity Tests

**Tests:** `tests/backend_parity_*.rs`

**Validates:**
- Same symbols extracted by both backends
- Same call graph structure
- Same query results
- Same export format

---

## Violation Detection

### Doctor Command

```bash
magellan doctor --db code.db
```

**Checks:**
- Schema version compatibility
- Orphaned edges
- Inconsistent symbol counts
- Missing required tables/sections

### Self-Test Mode

```bash
magellan doctor --db code.db --fix
```

**Attempts repair:**
- Removes orphaned edges
- Rebuilds indexes
- Updates schema version

---

## See Also

- [ARCHITECTURE.md](ARCHITECTURE.md) - Backend architecture
- [SCHEMA_SQLITE.md](SCHEMA_SQLITE.md) - SQLite schema
- [SCHEMA_GEOMETRIC.md](SCHEMA_GEOMETRIC.md) - Geometric schema

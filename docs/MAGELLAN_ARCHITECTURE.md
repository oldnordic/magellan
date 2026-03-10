# Magellan Architecture & Integration Guide

**Status:** Reference Document
**Purpose:** Design and integration guide for Magellan
**Date:** 2026-03-10
**Magellan Version:** 3.1.0

---

## Overview

Magellan is a **deterministic, idempotent codebase mapping tool** that:

1. **Observes files** - Detects source code changes
2. **Extracts symbols** - Uses tree-sitter parsers for multi-language support
3. **Persists facts** - Stores nodes and edges in a graph database
4. **Enables queries** - Provides symbol/reference/call graph lookups

**Key Principle:** Magellan is "dumb" - it records facts, doesn't interpret behavior.

---

## Three Backend Architecture

Magellan supports **three distinct storage backends**:

### Backend Comparison

| Backend | Feature | File Ext | Status | Performance |
|---------|---------|----------|--------|-------------|
| **SQLite** (default) | SQL-based storage | `.db` | ✅ Complete | Baseline |
| **Native V3** | Binary KV store | `.v3` | ✅ Complete | 10-20x faster |
| **Geometric** | 3D spatial indexing | `.geo` | ⚠️ 3D Complete, 4D Incomplete | Experimental |

**Important:** Database files are **not** compatible between backends.

### SQLite Backend (Default)

**Status:** ✅ Feature-complete, stable, production-ready

Uses SQL queries on relational tables via sqlitegraph:

```
graph_entities ──┐
graph_edges ─────┼──► SQLite database (.db)
graph_labels ────┤
code_chunks ─────┤
ast_nodes ───────┘
```

### Native V3 Backend

**Status:** ✅ Complete, recommended for production

- High-performance native binary format
- KV-based storage (no SQLite dependency)
- Unlimited capacity
- B+Tree clustered adjacency storage

Build with:
```bash
cargo build --release --features native-v3
```

### Geometric Backend ⚠️

**Status:** 3D spatial features complete, 4D temporal features incomplete

The geometric backend uses **GeoGraphDB** for 3D spatial indexing of CFG blocks:

#### 3D Spatial Mapping (COMPLETE)

| Coordinate | CFG Property | Meaning |
|------------|--------------|---------|
| X | Dominator Depth | How deep in dominator tree |
| Y | Loop Nesting | How many loops deep |
| Z | Branch Count | Number of branches |

**Performance:** O(log n) path queries instead of O(2^n) enumeration

#### 4D Temporal Features (INCOMPLETE)

The `NodeRec` structure has MVCC fields for time-travel queries:

```rust
pub struct NodeRec {
    // ... 3D spatial fields ...
    pub begin_ts: u64,    // MVCC timestamp (PLACEHOLDER)
    pub end_ts: u64,      // MVCC timestamp (PLACEHOLDER)
    pub tx_id: u64,       // Transaction ID (PLACEHOLDER)
    pub visibility: u8,   // Visibility flag
    // ...
}
```

**What's NOT implemented:**
- Temporal queries: "what did CFG look like at time T?"
- Version comparison between timestamps
- CFG evolution tracking
- Time-travel pathfinding

Build with:
```bash
# 3D spatial features work
cargo build --release --features geometric-backend

# Standalone CLI
magellan-geometric create --db code.geo
magellan-geometric index --root . --db code.geo
```

---

## Backend Capability Model (v3.1.0)

### Capability Detection

Magellan 3.1.0 introduces a **runtime capability model** that enables:

1. **Backend-aware help/usage messaging** - Commands show what they support
2. **Command validation** - Early error when backend lacks required capability
3. **Build feature detection** - `--backends` flag shows compiled-in backends
4. **Operational status reporting** - `status` shows backend type

### Capability Structure

Located in `src/capabilities.rs`:

```rust
pub enum BackendType {
    SQLite,      // .db files (default)
    Geometric,   // .geo files (requires geometric-backend feature)
    NativeV3,    // .v3 files (requires native-v3 feature)
}

pub struct BackendCapabilities {
    // Core capabilities
    pub supports_symbol_queries: bool,
    pub supports_call_graph: bool,
    pub supports_cfg_analysis: bool,
    pub supports_chunks: bool,
    pub supports_cycles: bool,
    pub supports_paths: bool,
    pub supports_slice: bool,
    pub supports_vacuum_maintenance: bool,
    pub supports_dead_code: bool,
    pub supports_reachability: bool,
    pub supports_export: bool,
    pub supports_ast: bool,
    pub supports_labels: bool,
    // ... metadata fields
}
```

### Backend Detection by File Extension

```rust
BackendType::from_extension(Some("db"))   // => Some(SQLite)
BackendType::from_extension(Some("geo"))  // => Some(Geometric) if built, None otherwise
BackendType::from_extension(Some("v3"))   // => Some(NativeV3)
BackendType::from_extension(Some("xyz"))  // => Some(SQLite) - default fallback
BackendType::from_extension(None)         // => Some(SQLite) - default fallback
```

**Important:** Unknown extensions default to SQLite. This ensures tool compatibility
even when users specify arbitrary filenames.

### Capability Matrix

| Capability | SQLite | Geometric | Native V3 |
|------------|--------|-----------|-----------|
| Symbol queries | ✅ | ✅ | ✅ |
| Call graph | ✅ | ✅ | ✅ |
| CFG analysis | ✅ | ✅ | ✅ |
| Chunks | ✅ | ✅ | ✅ |
| Cycles | ✅ | ✅ | ✅ |
| Paths (enumeration) | ❌ | ✅ | ❌ |
| Slice | ✅ | ✅ | ✅ |
| AST queries | ✅ | ❌ | ✅ |
| Labels | ✅ | ❌ | ✅ |
| Vacuum/maintenance | ✅ | ✅ | ✅ |
| Dead code | ✅ | ✅ | ✅ |
| Reachability | ✅ | ✅ | ✅ |
| Export | ✅ | ✅ | ✅ |

### Command Routing

Commands are validated against backend capabilities before execution:

```rust
// src/capabilities.rs
pub fn validate_command(
    command: &str,
    backend_caps: &BackendCapabilities,
) -> Result<(), CommandValidationError> {
    // Check if command is supported by this backend
    // Return error if capability missing
}
```

**Example validation:**
- `ast` command → requires `supports_ast` → fails on Geometric
- `paths` command → requires `supports_paths` → fails on SQLite/Native V3
- `label` command → requires `supports_labels` → fails on Geometric
- `find` command → requires `supports_symbol_queries` → works on all backends

### Checking Compiled Backends

```bash
# Show available backends
magellan --backends

# Output example:
# Backend      | Ext | Built | Feature         | Capabilities
# -------------|-----|-------|-----------------|------------------
# SQLite       | db  | Yes   | sqlite-backend  | symbol queries, call graph, CFG analysis...
# Geometric    | geo | Yes   | geometric-backend| symbol queries, call graph, CFG analysis, path enumeration...
# Native V3    | v3  | No    | native-v3       | Not built (requires --features native-v3)

# Version shows compiled backends
magellan --version
# magellan 3.1.0 (abc123 2026-03-10) rustc 1.75.0 backends: sqlite,geometric
```

---

## Re-Index Semantics (v3.1.0)

### Reconcile Behavior

The `reconcile_file_path()` method in both `CodeGraph` (SQLite) and `GeometricBackend`
provides **deterministic, idempotent re-indexing**:

### SQLite Reconcile (`src/graph/ops.rs`)

```rust
pub fn reconcile_file_path(
    graph: &mut CodeGraph,
    path: &Path,
    path_key: &str,
) -> Result<ReconcileOutcome>
```

**Algorithm:**

1. **Check file existence:**
   - If file does NOT exist on filesystem → delete all facts, return `Deleted`

2. **Compute content hash:**
   - Read file contents
   - Compute SHA-256 hash

3. **Check for changes:**
   - Compare with stored hash in `graph_entities` (FileNode)
   - If hash matches → return `Unchanged` (no-op)

4. **Delete old data:**
   - Delete all symbols via DEFINES edges from file
   - Delete all references for those symbols
   - Delete all calls for those symbols
   - Delete all AST nodes for the file
   - Delete all CFG blocks for the file
   - Delete all code chunks for the file

5. **Re-index:**
   - Run `index_file()` to extract symbols
   - Run `index_references()` to extract references
   - Return `Reindexed { symbols, references, calls }`

**Key Invariants:**

- **Symbol counts are stable** across re-index cycles (no inflation)
- **File hash is the source of truth** for freshness
- **Delete-then-insert ensures** no stale data accumulates
- **No partial updates** - transaction either fully commits or fully rolls back

### Geometric Reconcile (`src/graph/geo_index.rs`)

```rust
pub fn reconcile_file_path(
    backend: &mut GeometricBackend,
    path: &Path,
) -> Result<GeoReconcileOutcome>
```

**Algorithm:**

1. **Check file existence:**
   - If file does NOT exist → delete symbols, remove from tracking

2. **Compute content hash:**
   - Read file, compute hash

3. **Check for changes:**
   - Compare with stored hash
   - If unchanged → return `Unchanged`

4. **Delete old data:**
   - Remove symbols from in-memory index
   - Remove function_ids from CFG tracking (marks CFG blocks as stale)
   - Stale CFG blocks are excluded from next save (garbage collection)

5. **Re-index:**
   - Extract symbols via tree-sitter
   - Extract CFG blocks if applicable
   - Insert into in-memory structures
   - Return `Reindexed`

**Key Difference from SQLite:**

- CFG blocks are **not immediately deleted** from storage
- They become **stale** and excluded from `cfg_function_ids` tracking
- Vacuum operation physically removes stale blocks

### Re-Index Idempotence

Both backends guarantee idempotence:

```rust
// Running reconcile multiple times on unchanged file produces same result
assert!(matches!(reconcile_file_path(path), Ok(ReconcileOutcome::Unchanged)));
assert!(matches!(reconcile_file_path(path), Ok(ReconcileOutcome::Unchanged)));
assert!(matches!(reconcile_file_path(path), Ok(ReconcileOutcome::Unchanged)));
```

### Churn Test Validation

See `tests/churn_harness_test.rs` for validation that:
- Symbol counts remain constant across 5 re-index cycles
- File counts remain constant
- Database size stabilizes after initial WAL creation
- VACUUM reclaims space after deletion

---

## Vacuum and Maintenance (v3.1.0)

### SQLite VACUUM

SQLite backend supports standard SQLite VACUUM via `rusqlite`:

```bash
# Not directly exposed via CLI (future enhancement)
# Use sqlite3 directly:
sqlite3 code.db "VACUUM;"
```

**Effects:**
- Rebuilds database file, reclaiming free space
- Resets auto-increment sequences
- Defragments indexes
- Requires ~2x temporary disk space during operation

### Geometric CFG Vacuum

Geometric backend provides `vacuum_cfg()` method in `GeometricBackend`:

```rust
pub fn vacuum_cfg(&self) -> Result<VacuumResult>
```

**Algorithm:**

1. **Get tracked function IDs** - Source of truth for live CFG
2. **Count before state:**
   - Live blocks (blocks for tracked functions)
   - Total blocks (includes stale)
   - Live edges (edges between live blocks)
   - Total edges (includes stale)
3. **Build fresh CFG section:**
   - Iterate tracked function IDs
   - Copy only live blocks to new data structure
   - Copy only live edges
4. **Write to storage:**
   - Open existing `.geo` file
   - Replace CFG section with new data
   - Flush changes
5. **Calculate reclaimed:**
   - `blocks_reclaimed = total_blocks_before - live_blocks_before`
   - `edges_reclaimed = total_edges_before - live_edges_before`
   - `bytes_reclaimed = file_size_before - file_size_after`

**VacuumResult:**

```rust
pub struct VacuumResult {
    pub live_blocks_before: usize,
    pub total_blocks_before: usize,
    pub blocks_reclaimed: usize,
    pub live_edges_before: usize,
    pub total_edges_before: usize,
    pub edges_reclaimed: usize,
    pub bytes_reclaimed: u64,
}
```

**When to Vacuum:**

After multiple re-index cycles on the same files, stale CFG data
accumulates in the in-memory CfgStore. Calling `vacuum_cfg()` rebuilds
the persisted CFG section with only live data.

**Important:** Vacuum does NOT affect symbols or call graph - only CFG
blocks and edges. Stale symbols are handled differently (see Re-Index
Semantics).

---

## Database Schema

### Core Tables (SQLite Backend)

#### `graph_entities` - Symbol Nodes

```sql
CREATE TABLE graph_entities (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    kind      TEXT NOT NULL,
    name      TEXT NOT NULL,
    file_path TEXT,
    data      TEXT NOT NULL
);
```

**data payload (SymbolNode JSON):**
```json
{
    "symbol_id": "28e17e99cb937643",
    "fqn": "tests::test_default_config",
    "canonical_fqn": "codemcp::/path/to/file.rs::Function test_default_config",
    "display_fqn": "codemcp::tests::test_default_config",
    "name": "test_default_config",
    "kind": "Function",
    "kind_normalized": "fn",
    "byte_start": 19452,
    "byte_end": 20580,
    "start_line": 627,
    "end_line": 659,
    "start_col": 4,
    "end_col": 5
}
```

#### `graph_edges` - Relationships

```sql
CREATE TABLE graph_edges (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id   INTEGER NOT NULL,
    to_id     INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    data      TEXT NOT NULL
);
```

**Edge Types:**
| edge_type | Meaning |
|-----------|---------|
| REFERENCES | Symbol references another symbol |
| CALLS | Function calls another function |
| DEFINES | File defines a symbol |
| CALLER | Reverse of CALLS |

#### `code_chunks` - Source Code Storage

```sql
CREATE TABLE code_chunks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path     TEXT NOT NULL,
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL,
    content       TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    symbol_name   TEXT,
    symbol_kind   TEXT,
    created_at    INTEGER NOT NULL
);
```

#### `ast_nodes` - AST Node Storage

```sql
CREATE TABLE ast_nodes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id  INTEGER,
    kind       TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end   INTEGER NOT NULL
);
```

---

## CLI Interface

### Command Structure

```bash
magellan <command> [arguments]
```

### Commands

| Command | Purpose | Key Arguments |
|---------|---------|---------------|
| `watch` | Index + watch for changes | `--root`, `--db`, `--debounce-ms` |
| `export` | Export graph data | `--db`, `--format`, `--output` |
| `status` | Database statistics | `--db` |
| `query` | List symbols in file | `--db`, `--file`, `--kind` |
| `find` | Find symbol by name | `--db`, `--name` |
| `refs` | Show callers/callees | `--db`, `--name`, `--direction` |
| `context` | LLM Context API | `--db`, `summary`/`list`/`symbol`/`file` |
| `enrich` | LSP type enrichment | `--db` |
| `files` | List all files | `--db` |
| `doctor` | Self-diagnostics | `--db`, `--fix` |

### Context API (v3.0.0+)

```bash
# Project overview (~50 tokens)
magellan context summary --db code.db --json

# Paginated symbol list
magellan context list --db code.db --kind fn --page 1 --page-size 50 --json

# Symbol detail with call graph
magellan context symbol --db code.db --name main --callers --callees --json

# File-level context
magellan context file --db code.db --path src/main.rs --json
```

### Export Formats

```bash
# JSON export
magellan export --db code.db --format json > graph.json

# LSIF export (for cross-repo navigation)
magellan export --db code.db --format lsif --output project.lsif

# Import external LSIF
magellan import-lsif --db code.db --input dependency.lsif
```

---

## Code Structure

```
magellan/src/
├── main.rs              # CLI entry point
├── lib.rs               # Public API exports
├── common.rs            # Language detection
├── indexer.rs           # Indexing orchestrator
├── cli.rs               # Argument parsing
├── geometric_cmd.rs     # ⚠️ Geometric backend CLI
├── ingest/              # Language parsers
│   ├── mod.rs
│   ├── detect.rs
│   ├── rust.rs
│   ├── c.rs
│   ├── cpp.rs
│   ├── java.rs
│   ├── python.rs
│   ├── javascript.rs
│   └── typescript.rs
├── graph/               # Database operations
│   ├── mod.rs           # CodeGraph main type
│   ├── schema.rs        # Node payload types
│   ├── ops.rs           # CRUD operations
│   ├── query.rs         # Symbol queries
│   ├── algorithms.rs    # Graph algorithms
│   ├── geometric_backend.rs  # ⚠️ 3D complete, 4D incomplete
│   ├── cfg_*.rs         # CFG modules
│   ├── ast_*.rs         # AST modules
│   └── metrics/         # Code metrics
├── context/             # LLM Context API
├── lsp/                 # LSP enrichment
├── lsif/                # LSIF export/import
├── generation/          # Code chunk storage
├── output/              # Output formatting
├── watcher/             # Async file watcher
└── diagnostics/         # Watch event tracking
```

---

## Architecture Flow

```
┌──────────────────────────────────────────────────────────────┐
│                    Indexing Pipeline                        │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐ │
│  │  File   │───▶│ Ingest  │───▶│ Extract │───▶│ Persist │ │
│  │ System  │    │ Parser │    │ Symbols │    │   DB    │ │
│  └─────────┘    └─────────┘    └─────────┘    └─────────┘ │
│                     │                              │        │
│                     ▼                              ▼        │
│              ┌─────────┐                    ┌─────────┐    │
│              │ Detect  │                    │ CodeGraph│    │
│              │Language│                    │ Wrapper │    │
│              └─────────┘                    └─────────┘    │
│                                                     │       │
└─────────────────────────────────────────────────────────────┘
```

---

## Position Conventions

Magellan uses **tree-sitter position conventions**:

| Type | Convention | Example |
|------|------------|---------|
| Lines | 1-indexed | Line 1 is first line |
| Columns | 0-indexed | Column 0 is first character |
| Bytes | 0-indexed | Byte 0 is first byte |

---

## Integration Points

### Same Database Principle

**Extend `codegraph.db`** with new tables (NOT separate database):

**Why same DB?**
- Single source of truth
- JOIN queries work natively
- Atomic updates (symbol + path in ONE transaction)
- Better performance

### Linking via symbol_id

Use the **stable symbol_id** from Magellan's SymbolNode:

```rust
pub struct SymbolNode {
    pub symbol_id: Option<String>,  // SHA-256 of language:fqn:span_id
    // ...
}
```

---

## Design Recommendations

### Schema Extension

Add to Magellan's migration system in `src/graph/db_compat.rs`:

```rust
pub const CUSTOM_SCHEMA_VERSION: i32 = 1;

pub fn ensure_custom_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS custom_table (...)",
        [],
    )?;
    Ok(())
}
```

### Incremental Computation

When Magellan indexes a file:

```rust
pub fn on_file_indexed(codegraph: &CodeGraph, path: &str) -> Result<()> {
    // 1. Get all functions in the file
    let functions = codegraph.symbols_in_file_with_kind(path, Some(SymbolKind::Function))?;

    // 2. For each function, build CFG
    for func in functions {
        let cfg = build_cfg(&func)?;
        codegraph.store_cfg(func.id, cfg)?;
    }

    Ok(())
}
```

---

## Backend Selection Guide

### When to use SQLite Backend

- Compatibility with existing tools
- Debugging (human-readable SQL)
- Small to medium codebases
- Development/testing

### When to use Native V3 Backend

- Production deployments
- Large codebases (10M+ LOC)
- High query throughput
- When performance matters

### When to use Geometric Backend ⚠️

- CFG analysis experiments
- When O(log n) path queries are needed
- **NOT for production** (incomplete features)
- **NOT for temporal queries** (not implemented)

---

## Summary

### Database Decision

**Three backends, choose based on use case:**

| Use Case | Backend | File Extension |
|----------|---------|----------------|
| Default/Compatibility | SQLite | `.db` |
| Production Performance | Native V3 | `.v3` |
| CFG Experiments Only | Geometric | `.geo` |

### Building with Different Backends

```bash
# SQLite (default)
cargo build --release

# Native V3 (recommended for production)
cargo build --release --features native-v3

# Geometric (⚠️ experimental)
cargo build --release --features geometric-backend
```

### Integration Pattern

```
Magellan (structure) + Analysis Tool (behavior) = Complete Code Intelligence
         ↓                        ↓
    WHERE things are        HOW code behaves
```

---

## References

- Magellan Source: `/home/feanor/Projects/magellan/`
- Magellan CLI: `src/main.rs`
- Graph Schema: `src/graph/schema.rs`
- CodeGraph API: `src/graph/mod.rs`
- Geometric Backend: `src/graph/geometric_backend.rs` ⚠️
- Geometric Docs: `docs/GEOGRAPHDB_*.md`

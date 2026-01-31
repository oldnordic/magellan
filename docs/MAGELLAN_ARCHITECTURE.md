# Magellan Architecture & Integration Guide

**Status:** Reference Document
**Purpose:** Design LogicGraph/PathGraph integration with Magellan
**Date:** 2026-01-30
**Magellan Version:** 1.4.0

---

## Overview

Magellan is a **deterministic, idempotent codebase mapping tool** that:

1. **Observes files** - Detects source code changes
2. **Extracts symbols** - Uses tree-sitter parsers for multi-language support
3. **Persists facts** - Stores nodes and edges in SQLiteGraph database
4. **Enables queries** - Provides symbol/reference/call graph lookups

**Key Principle:** Magellan is "dumb" - it records facts, doesn't interpret behavior.

---

## Database Schema

### Core Tables

#### `graph_entities` - Symbol Nodes

```sql
CREATE TABLE graph_entities (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,  -- Internal node ID
    kind      TEXT NOT NULL,                       -- Symbol kind (Function, Struct, etc.)
    name      TEXT NOT NULL,                       -- Symbol display name
    file_path TEXT,                               -- Source file path
    data      TEXT NOT NULL                        -- JSON: SymbolNode payload
);
```

**Indexes:**
- `idx_entities_kind_id` on `(kind, id)` - Fast kind filtering

**data payload (SymbolNode JSON):**
```json
{
    "symbol_id": "28e17e99cb937643",           -- Stable SHA-256 derived ID
    "fqn": "tests::test_default_config",       -- Fully-qualified name
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
    from_id   INTEGER NOT NULL,    -- Source node ID (graph_entities.id)
    to_id     INTEGER NOT NULL,    -- Target node ID (graph_entities.id)
    edge_type TEXT NOT NULL,       -- Relationship type
    data      TEXT NOT NULL        -- JSON: edge metadata (usually {})
);
```

**Indexes:**
- `idx_edges_from` on `(from_id)` - Forward traversal
- `idx_edges_to` on `(to_id)` - Reverse traversal
- `idx_edges_type` on `(edge_type)` - Edge type filtering

**Edge Types & Distribution:**
| edge_type | Count | Meaning |
|-----------|-------|---------|
| REFERENCES | 102,104 | Symbol references another symbol |
| CALLS | 26,325 | Function calls another function |
| DEFINES | 17,017 | File defines a symbol |
| CALLER | 6,776 | Reverse of CALLS |

#### `graph_labels` - Tagging System

```sql
CREATE TABLE graph_labels (
    entity_id INTEGER NOT NULL,  -- References graph_entities.id
    label     TEXT NOT NULL       -- Tag: "Rust", "pub", "Function", etc.
);
```

**Indexes:**
- `idx_labels_label` on `(label)`
- `idx_labels_label_entity_id` on `(label, entity_id)`

**Usage:** Tag symbols with language, visibility, custom categories.

#### `graph_properties` - Key-Value Metadata

```sql
CREATE TABLE graph_properties (
    entity_id INTEGER NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL
);
```

**Indexes:**
- `idx_props_key_value` on `(key, value)`
- `idx_props_key_value_entity_id` on `(key, value, entity_id)`

---

### Extended Tables

#### `code_chunks` - Source Code Storage

```sql
CREATE TABLE code_chunks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path     TEXT NOT NULL,
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL,
    content       TEXT NOT NULL,         -- Source code snippet
    content_hash  TEXT NOT NULL,         -- SHA-256 of content
    symbol_name   TEXT,
    symbol_kind   TEXT,
    created_at    INTEGER NOT NULL,
    UNIQUE(file_path, byte_start, byte_end)
);
```

**Indexes:**
- `idx_chunks_file_path` on `(file_path)`
- `idx_chunks_symbol_name` on `(symbol_name)`
- `idx_chunks_content_hash` on `(content_hash)` - For duplicate detection

#### `execution_log` - Operation Tracking

```sql
CREATE TABLE execution_log (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_id      TEXT NOT NULL UNIQUE,
    tool_version      TEXT NOT NULL,
    args              TEXT NOT NULL,
    root              TEXT,
    db_path           TEXT NOT NULL,
    started_at        INTEGER NOT NULL,
    finished_at       INTEGER,
    duration_ms       INTEGER,
    outcome           TEXT NOT NULL,        -- 'success' | 'error'
    error_message     TEXT,
    files_indexed     INTEGER DEFAULT 0,
    symbols_indexed   INTEGER DEFAULT 0,
    references_indexed INTEGER DEFAULT 0
);
```

**Indexes:**
- `idx_execution_log_started_at` on `(started_at DESC)`
- `idx_execution_log_execution_id` on `(execution_id)`
- `idx_execution_log_outcome` on `(outcome)`

---

### Semantic Search Tables (HNSW)

#### `hnsw_indexes` - Vector Index Definitions

```sql
CREATE TABLE hnsw_indexes (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    name             TEXT NOT NULL UNIQUE,
    dimension        INTEGER NOT NULL,
    m                INTEGER NOT NULL,        -- HNSW M parameter
    ef_construction  INTEGER NOT NULL,        -- HNSW ef-construction
    distance_metric  TEXT NOT NULL,           -- 'cosine', 'l2', etc.
    vector_count     INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);
```

#### `hnsw_vectors` - Embedding Storage

```sql
CREATE TABLE hnsw_vectors (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    index_id   INTEGER NOT NULL,
    vector_data BLOB NOT NULL,                -- Serialized vector
    metadata   TEXT,                         -- JSON metadata
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (index_id) REFERENCES hnsw_indexes(id) ON DELETE CASCADE
);
```

#### `hnsw_layers` - HNSW Graph Structure

```sql
CREATE TABLE hnsw_layers (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    index_id    INTEGER NOT NULL,
    layer_level INTEGER NOT NULL,
    node_id     INTEGER NOT NULL,
    connections BLOB NOT NULL,                -- Serialized neighbor list
    FOREIGN KEY (index_id) REFERENCES hnsw_indexes(id) ON DELETE CASCADE,
    UNIQUE(index_id, layer_level, node_id)
);
```

---

### Metadata Tables

#### `graph_meta` - Schema Versioning

```sql
CREATE TABLE graph_meta (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL
);
```

#### `magellan_meta` - Cross-Tool Version Tracking

```sql
CREATE TABLE magellan_meta (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    magellan_schema_version   INTEGER NOT NULL,
    sqlitegraph_schema_version INTEGER NOT NULL,
    created_at                INTEGER NOT NULL
);
```

#### `graph_meta_history` - Migration Log

```sql
CREATE TABLE graph_meta_history (
    version   INTEGER NOT NULL,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

---

## Entity Kinds (Symbol Types)

| kind | Count | Description |
|------|-------|-------------|
| Reference | 333,570 | Symbol reference (usage site) |
| Call | 113,464 | Function/method call |
| Symbol | 1,517 | Definition site |

**Symbol kinds include:** `Function`, `Struct`, `Enum`, `Trait`, `Impl`, `Const`, `Static`, `Type`, `Module`, `Variable`, `Field`, `Method`, `Class`, `Interface`, etc.

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
| `find` | Find symbol by name | `--db`, `--name`, `--symbol-id` |
| `refs` | Show callers/callees | `--db`, `--name`, `--direction` |
| `get` | Get source code | `--db`, `--file`, `--symbol` |
| `files` | List all files | `--db` |
| `label` | Query by labels | `--db`, `--label`, `--list` |
| `migrate` | Upgrade schema | `--db`, `--dry-run` |
| `verify` | Validate vs filesystem | `--root`, `--db` |

### Global Arguments

```bash
--output <FORMAT>    # human (default), json, pretty
```

### Watch Command

```bash
magellan watch \
    --root <DIR> \
    --db <FILE> \
    [--debounce-ms <N>] \
    [--watch-only] \
    [--validate] \
    [--output <FORMAT>]
```

### Status Command

```bash
magellan status --db <FILE> [--output <FORMAT>]
```

**Returns:**
```
files: 104
symbols: 7562
references: 102104
calls: 26325
code_chunks: 456
```

---

## Code Structure

```
magellan/src/
├── main.rs              # CLI entry point, command parsing
├── lib.rs               # Public API exports
├── common.rs            # Language detection, path utilities
├── indexer.rs           # Main indexing orchestrator
├── ingest/              # Language parsers (tree-sitter)
│   ├── mod.rs
│   ├── detect.rs        # Language detection from extension
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
│   ├── scan.rs          # Directory scanning
│   ├── symbols.rs       # Symbol operations
│   ├── references.rs    # Reference operations
│   ├── calls.rs         # Call graph operations
│   ├── filter.rs        # File filtering
│   ├── freshness.rs     # staleness detection
│   ├── export.rs        # JSON/SCIP export
│   ├── validation.rs    # Graph validation
│   └── cache.rs         # File node cache
├── generation/          # Code chunk storage
│   ├── mod.rs
│   └── schema.rs        # ChunkStore types
├── watcher.rs           # File system watcher
├── diagnostics/         # Watch event tracking
├── output/              # Output formatting
│   ├── mod.rs
│   ├── command.rs       # Response types
│   └── rich.rs          # Human-readable output
├── references.rs        # Reference fact types
└── verify.rs            # Graph validation logic
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
│              ┌─────────┐                    ┌─────────┐  │
│              │ Detect  │                    │ CodeGraph│  │
│              │Language│                    │ Wrapper │  │
│              └─────────┘                    └─────────┘  │
│                                                     │       │
└─────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│                    Query Pipeline                             │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐ │
│  │  CLI    │───▶│ Command │───▶│  Graph │───▶│ Results │ │
│  │ Request│    │ Handler │    │  Query  │    │ Output  │ │
│  └─────────┘    └─────────┘    └─────────┘    └─────────┘ │
│                                                     │        │
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

## Integration Points for LogicGraph/PathGraph

### Design Decision: SAME Database

**Extend `codegraph.db`** with new tables (NOT separate database):

**Why same DB?**
- Single source of truth
- JOIN queries work natively
- Atomic updates (when file changes → delete symbol + delete paths + recompute in ONE transaction)
- Magellan already extends the DB (graph_edges, graph_labels, code_chunks, hnsw_*)
- Better performance

### Proposed Schema Extension

```sql
-- CFG Nodes (per function)
CREATE TABLE cfg_nodes (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id  INTEGER NOT NULL,     -- References graph_entities.id
    node_type    TEXT NOT NULL,        -- 'entry', 'exit', 'branch', 'merge', 'call', 'return'
    byte_start   INTEGER,
    byte_end     INTEGER
);

-- CFG Edges (control flow)
CREATE TABLE cfg_edges (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    from_node_id INTEGER NOT NULL,     -- References cfg_nodes.id
    to_node_id   INTEGER NOT NULL,     -- References cfg_nodes.id
    edge_type    TEXT NOT NULL,        -- 'branch_true', 'branch_false', 'fallthrough', 'exception'
    condition_id INTEGER,             -- Optional: reference to condition expression
    data         TEXT NOT NULL         -- JSON metadata
);

-- Paths (enumerated execution paths)
CREATE TABLE control_flow_paths (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id   INTEGER NOT NULL,     -- References graph_entities.id
    path_hash     TEXT NOT NULL,        -- SHA-256 of path for deduplication
    entry_node    INTEGER NOT NULL,     -- References cfg_nodes.id
    exit_node     INTEGER NOT NULL,     -- References cfg_nodes.id
    path_type     TEXT NOT NULL,        -- 'normal', 'error', 'degenerate'
    is_reachable  BOOLEAN NOT NULL,
    proof_hash    TEXT,                 -- Reachability proof
    created_at    INTEGER NOT NULL
);

-- Path Steps (ordered nodes in a path)
CREATE TABLE path_steps (
    path_id      INTEGER NOT NULL,
    step_order   INTEGER NOT NULL,
    node_id      INTEGER NOT NULL,     -- References cfg_nodes.id
    edge_id      INTEGER,              -- References cfg_edges.id
    PRIMARY KEY (path_id, step_order)
);

-- Call Chain Analysis (interprocedural)
CREATE TABLE call_chain_blast_zone (
    changed_function   TEXT NOT NULL,
    affected_function TEXT NOT NULL,
    depth             INTEGER NOT NULL,
    is_error_path     BOOLEAN NOT NULL,
    proof_id          INTEGER            -- References control_flow_paths.id
);
```

### Linking via symbol_id

Use the **stable symbol_id** from Magellan's SymbolNode:

```rust
// In Magellan
pub struct SymbolNode {
    pub symbol_id: Option<String>,  // SHA-256 of language:fqn:span_id
    // ...
}
```

LogicGraph queries:
```sql
-- Get paths for a function
SELECT * FROM control_flow_paths
WHERE function_id = (SELECT id FROM graph_entities WHERE data->>'symbol_id' = '28e17e99cb937643');

-- Get call chain blast zone
WITH RECURSIVE call_chain AS (
    SELECT id, 0 as depth
    FROM graph_entities WHERE data->>'name' = 'run_init'
    UNION ALL
    SELECT ge.id, cc.depth + 1
    FROM call_chain cc
    JOIN graph_edges e ON cc.id = e.from_id
    JOIN graph_entities ge ON e.to_id = ge.id
    WHERE e.edge_type = 'CALLS' AND cc.depth < 10
)
SELECT * FROM call_chain;
```

---

## CLI Compatibility

### Match Magellan's CLI Pattern

```bash
# Magellan
magellan watch --root . --db .codemcp/codegraph.db

# LogicGraph (proposal) - extends Magellan
magellan paths --db .codemcp/codegraph.db --function run_init
magellan unreachable --db .codemcp/codegraph.db --entry main
magellan blast-zone --db .codemcp/codegraph.db --symbol run_init
magellan cfg --db .codemcp/codegraph.db --function run_init
```

### Shared Arguments

| Argument | Meaning | Used By |
|----------|---------|---------|
| `--root <DIR>` | Workspace root | Magellan, LogicGraph |
| `--db <FILE>` | Database path | Magellan, LogicGraph |
| `--output <FORMAT>` | Output format | All tools |
| `--debounce-ms <N>` | Watch debounce | Magellan only |

---

## Design Recommendations

### 1. Schema Extension

Add to Magellan's migration system in `src/graph/db_compat.rs`:

```rust
pub const LOGICGRAPH_SCHEMA_VERSION: i32 = 1;

pub fn ensure_logicgraph_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_nodes (...)",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_edges (...)",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS control_flow_paths (...)",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS path_steps (...)",
        [],
    )?;
    Ok(())
}
```

### 2. MCP Tool Interface

```typescript
// Path Query Tools
get_cfg(function_name: string): CFG
get_paths(function_name: string, filter: PathFilter): Path[]
prove_reachability(from: string, to: string): Proof
find_unreachable(entry_point: string): Node[]

// Call Graph Tools
get_call_chain(from: string, direction: 'forward'|'backward'): Symbol[]
get_blast_zone(symbol: string): BlastZoneResult

// Analysis Tools
find_cfg_duplicates(threshold: number): DuplicatePair[]
validate_path(path_id: string): ValidationResult
```

### 3. Incremental Path Computation

When Magellan indexes a file:

```rust
// After Magellan indexes file.rs
pub fn on_file_indexed(codegraph: &CodeGraph, path: &str) -> Result<()> {
    // 1. Get all functions in the file
    let functions = codegraph.symbols_in_file_with_kind(path, Some(SymbolKind::Function))?;

    // 2. For each function, build CFG
    for func in functions {
        let cfg = build_cfg(&func)?;
        codegraph.store_cfg(func.id, cfg)?;
    }

    // 3. Enumerate paths
    codegraph.enumerate_paths()?;

    Ok(())
}
```

---

## Summary

### Database Decision

**Use `codegraph.db`** - extend existing schema, not separate database.

**Reasons:**
1. Atomic updates (symbol + path in one transaction)
2. JOIN queries between structure and behavior
3. Single source of truth
4. Magellan already extends the DB (edges, labels, chunks, HNSW)
5. Better performance

### Building LogicGraph

1. Extend Magellan schema with `cfg_*` and `control_flow_paths` tables
2. Hook into `reconcile_file_path` to update paths when symbols change
3. Add CLI commands: `paths`, `unreachable`, `blast-zone`, `cfg`
4. Provide MCP tools for LLM consumption

### Integration Pattern

```
Magellan (structure) + LogicGraph (paths) = Complete Code Intelligence
         ↓                        ↓
    WHERE things are        HOW code behaves
```

---

## References

- Magellan Source: `/home/feanor/Projects/magellan/`
- Magellan CLI: `src/main.rs`
- Graph Schema: `src/graph/schema.rs`
- CodeGraph API: `src/graph/mod.rs`
- Indexer: `src/indexer.rs`
- Path-Aware Design: `docs/PATH_AWARE_CODE_INTELLIGENCE_DESIGN.md`

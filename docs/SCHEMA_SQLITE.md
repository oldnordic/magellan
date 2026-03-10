# SQLite Backend Schema Reference

**Version:** 3.1.0
**Backend:** SQLite (sqlitegraph)
**File Extension:** `.db`

---

## Overview

The SQLite backend uses `sqlitegraph` as a graph database layer on top of SQLite3.
Data is stored in a relational schema with nodes in `graph_entities` and edges in `graph_edges`.

---

## Core Tables

### `graph_entities` - Node Storage

Primary table for all node types. Uses sqlitegraph's entity schema.

```sql
CREATE TABLE graph_entities (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    kind      TEXT NOT NULL,
    name      TEXT NOT NULL,
    file_path TEXT,
    data      TEXT NOT NULL
);
```

**Fields:**
- `id` - Unique node identifier (auto-increment)
- `kind` - Node type: `File`, `Symbol`, `Reference`, `Call`, `CfgBlock`, `Import`
- `name` - Display name for the node
- `file_path` - Optional file path (for file-local nodes)
- `data` - JSON payload with node-specific data

**Important:** Node IDs are **monotonic** within a database but **not stable** across
re-index operations. Use `symbol_id` field for stable identifiers.

### `graph_edges` - Edge Storage

Primary table for all relationships between nodes.

```sql
CREATE TABLE graph_edges (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id   INTEGER NOT NULL,
    to_id     INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    data      TEXT NOT NULL
);
```

**Fields:**
- `id` - Unique edge identifier (auto-increment)
- `from_id` - Source node ID (references `graph_entities.id`)
- `to_id` - Target node ID (references `graph_entities.id`)
- `edge_type` - Relationship type (see below)
- `data` - JSON payload with edge-specific data

**Edge Types:**
| edge_type | Meaning | Direction |
|-----------|---------|-----------|
| `DEFINES` | File defines a symbol | File → Symbol |
| `REFERENCES` | Symbol references another symbol | Symbol → Symbol |
| `CALLS` | Function calls another function | Symbol → Symbol |
| `CALLER` | Reverse of CALLS (derived) | Symbol → Symbol |
| `CFG_BLOCK` | CFG edge between blocks | CfgBlock → CfgBlock |

### `code_chunks` - Source Code Storage

Stores source code excerpts for LLM context and code retrieval.

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

**Fields:**
- `id` - Unique chunk identifier
- `file_path` - Source file path
- `byte_start`, `byte_end` - Span in source file
- `content` - Actual source code text
- `content_hash` - SHA-256 hash for deduplication
- `symbol_name`, `symbol_kind` - Optional symbol association
- `created_at` - Unix timestamp

### `ast_nodes` - AST Node Storage

Stores tree-sitter AST nodes for structural queries.

```sql
CREATE TABLE ast_nodes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id  INTEGER,
    kind       TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end   INTEGER NOT NULL
);
```

**Fields:**
- `id` - Unique AST node identifier
- `parent_id` - Parent AST node (NULL for root)
- `kind` - tree-sitter node kind (e.g., `function_definition`, `if_expression`)
- `byte_start`, `byte_end` - Node span in source file

---

## Node Payloads (JSON in `data` field)

### FileNode

```json
{
    "path": "src/main.rs",
    "hash": "a1b2c3d4e5f6...",
    "last_indexed_at": 1699200000,
    "last_modified": 1699200000
}
```

**Fields:**
- `path` - File path (as stored during indexing)
- `hash` - SHA-256 of file contents (for change detection)
- `last_indexed_at` - Unix timestamp of last successful index
- `last_modified` - Unix mtime of file when indexed

**Purpose:** Authoritative source for file existence and freshness.
Hash comparison determines if re-index is needed.

### SymbolNode

```json
{
    "symbol_id": "28e17e99cb937643...",
    "fqn": "crate::module::function_name",
    "canonical_fqn": "my_crate::src/lib.rs::Function function_name",
    "display_fqn": "my_crate::module::function_name",
    "name": "function_name",
    "kind": "Function",
    "kind_normalized": "fn",
    "byte_start": 1234,
    "byte_end": 5678,
    "start_line": 42,
    "start_col": 4,
    "end_line": 50,
    "end_col": 5,
    "cfg_conditions": ["feature=gpu", "target_arch=x86_64"]
}
```

**Fields:**
- `symbol_id` - Stable SHA-256 hash of `language:fqn:span_id` (cross-run stable)
- `fqn` - Fully-qualified name (language-specific format)
- `canonical_fqn` - Unambiguous name with file path
- `display_fqn` - Human-readable name
- `name` - Simple identifier
- `kind` - Symbol kind: `Function`, `Struct`, `Enum`, `Trait`, `Const`, `Static`, `TypeAlias`, `Module`, `Impl`, ...
- `kind_normalized` - Normalized: `fn`, `struct`, `enum`, `trait`, `const`, `static`, `type`, `module`, `impl`
- `byte_start`, `byte_end` - Span in source file (0-indexed)
- `start_line`, `end_line` - Line span (1-indexed)
- `start_col`, `end_col` - Column span (0-indexed)
- `cfg_conditions` - Conditional compilation requirements

**Purpose:** Authoritative symbol metadata. All symbol queries use this data.

### ReferenceNode

```json
{
    "file": "src/main.rs",
    "byte_start": 200,
    "byte_end": 208,
    "start_line": 15,
    "start_col": 8,
    "end_line": 15,
    "end_col": 16
}
```

**Purpose:** Tracks where symbols are referenced. Not persisted for all references
(only explicitly stored references).

### CallNode

```json
{
    "file": "src/main.rs",
    "caller": "crate::caller_name",
    "callee": "crate::callee_name",
    "caller_symbol_id": "abc123...",
    "callee_symbol_id": "def456...",
    "byte_start": 300,
    "byte_end": 308,
    "start_line": 20,
    "start_col": 4,
    "end_line": 20,
    "end_col": 12
}
```

**Purpose:** Tracks call relationships. Stored in `graph_entities` with `CALLS` edges.
Call graph queries use this data.

### CfgBlock

```json
{
    "function_id": 42,
    "kind": "conditional",
    "terminator": "if",
    "byte_start": 500,
    "byte_end": 600,
    "start_line": 30,
    "start_col": 8,
    "end_line": 35,
    "end_col": 9
}
```

**Fields:**
- `function_id` - Symbol ID of containing function
- `kind` - Block kind: `entry`, `exit`, `conditional`, `loop`, `match`, `return`, ...
- `terminator` - How control leaves: `if`, `loop`, `match`, `return`, `unreachable`

**Purpose:** Control flow graph basic blocks. Edges stored in `graph_edges`
with `edge_type = "CFG_BLOCK"`.

### CfgEdge (in `graph_edges.data`)

```json
{
    "from_block_id": 100,
    "to_block_id": 101,
    "kind": "conditional_true"
}
```

**Edge Kinds:**
- `unconditional` - Always taken
- `conditional_true` - True branch of if
- `conditional_false` - False branch of if
- `loop_entry` - Enter loop body
- `loop_exit` - Exit loop
- `match_arm` - Match case arm

### ImportNode

```json
{
    "file": "src/main.rs",
    "import_kind": "use_crate",
    "import_path": ["std", "collections"],
    "imported_names": ["HashMap", "HashSet"],
    "is_glob": false,
    "byte_start": 10,
    "byte_end": 50,
    "start_line": 3,
    "start_col": 0,
    "end_line": 3,
    "end_col": 40
}
```

**Purpose:** Tracks import statements for module resolution.

---

## Authoritative vs Derived Data

### Authoritative (Persisted)

- **FileNode** - File hash, path, timestamps
- **SymbolNode** - All symbol metadata
- **ReferenceNode** - Explicitly stored references
- **CallNode** - Call relationships
- **CfgBlock** - CFG basic blocks
- **code_chunks** - Source code excerpts
- **ast_nodes** - AST node structure

### Derived (Computed on Query)

- **CALLER edges** - Reverse of CALLS (computed by traversing CALLS edges)
- **Reachability** - Transitive closure of call graph
- **Cycles (SCCs)** - Computed via Tarjan's algorithm
- **Dominators** - Computed via Cooper-Harvey-Kennedy algorithm
- **Program slices** - Computed via backward/forward reachability

---

## Re-Index Deletion Semantics

When `reconcile_file_path()` determines a file needs re-indexing:

1. **Delete all DEFINES edges** from the file node to symbols
2. **Delete all symbol entities** whose incoming DEFINES edge was removed
3. **Delete all Reference entities** for deleted symbols
4. **Delete all Call entities** for deleted symbols
5. **Delete all CfgBlock entities** with `function_id` in deleted set
6. **Delete all CFG_BLOCK edges** from deleted blocks
7. **Delete all orphaned edges** (edges referencing deleted entities)

**Key Invariant:** Deletion is cascading via edge relationships. Orphan cleanup
ensures no dangling references remain.

---

## Indexes

Implicit indexes created by sqlitegraph:

- Primary key on `graph_entities.id`
- Primary key on `graph_edges.id`
- Foreign key relationship: `graph_edges.from_id → graph_entities.id`
- Foreign key relationship: `graph_edges.to_id → graph_entities.id`

Application-level queries typically create transient indexes via:
- `WHERE kind = ?` for node type queries
- `WHERE edge_type = ?` for edge type queries

---

## Maintenance

### VACUUM

SQLite VACUUM reclaims space from deleted nodes/edges:

```bash
sqlite3 code.db "VACUUM;"
```

**Effects:**
- Rebuilds database file
- Resets auto-increment sequences
- Defragments indexes
- Requires ~2x temporary disk space

### Schema Version

Current schema version stored in `src/graph/db_compat.rs`:

```rust
pub const MAGELLAN_SCHEMA_VERSION: i32 = 6;
```

Schema migrations are applied on database open via `ensure_schema()`.

---

## See Also

- [ARCHITECTURE.md](ARCHITECTURE.md) - Backend architecture overview
- [SCHEMA_GEOMETRIC.md](SCHEMA_GEOMETRIC.md) - Geometric backend schema
- [INVARIANTS.md](INVARIANTS.md) - Database invariants

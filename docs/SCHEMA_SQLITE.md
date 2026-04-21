# SQLite Backend Schema Reference

**Version:** 3.1.6+
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
- `kind` - Node type: `File`, `Symbol`, `Reference`, `Call`, `Import`, `DisplayName`
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
| `CFG_BLOCK` | CFG edge between blocks (legacy, see `cfg_edges` table) | CfgBlock → CfgBlock |

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
    created_at    INTEGER NOT NULL,
    UNIQUE(file_path, byte_start, byte_end)
);
```

**Fields:**
- `id` - Unique chunk identifier
- `file_path` - Source file path
- `byte_start`, `byte_end` - Span in source file
- `content` - Actual source code text
- `content_hash` - SHA-256 hash (64-character hex) for deduplication
- `symbol_name`, `symbol_kind` - Optional symbol association
- `created_at` - Unix timestamp

**Indexes:**
- `idx_chunks_file_path` on `file_path`
- `idx_chunks_symbol_name` on `symbol_name`
- `idx_chunks_content_hash` on `content_hash`

### `ast_nodes` - AST Node Storage

Stores tree-sitter AST nodes for structural queries.

```sql
CREATE TABLE ast_nodes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id  INTEGER,
    kind       TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end   INTEGER NOT NULL,
    file_id    INTEGER
);
```

**Fields:**
- `id` - Unique AST node identifier
- `parent_id` - Parent AST node (NULL for root)
- `kind` - tree-sitter node kind (e.g., `function_definition`, `if_expression`)
- `byte_start`, `byte_end` - Node span in source file
- `file_id` - Optional file ID for per-file tracking

**Indexes:**
- `idx_ast_nodes_parent` on `parent_id`
- `idx_ast_nodes_span` on `byte_start, byte_end`
- `idx_ast_nodes_file_id` on `file_id`

---

## Node Payloads (JSON in `data` field)

### FileNode

```json
{
    "path": "src/main.rs",
    "hash": "fe378df1f333834e...",
    "last_indexed_at": 1699200000,
    "last_modified": 1699200000
}
```

**Fields:**
- `path` - File path (as stored during indexing)
- `hash` - SHA-256 of file contents (64-character hex, for change detection)
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
    "end_col": 9,
    "cfg_hash": "abc123...",
    "statements": ["let x = 1;", "x + 2"],
    "coord_x": 2,
    "coord_y": 1,
    "coord_z": 3,
    "coord_t": null
}
```

**Fields:**
- `function_id` - Symbol ID of containing function (references `graph_entities.id`)
- `kind` - Block kind (see below)
- `terminator` - How control leaves: `if`, `loop`, `match`, `return`, `unreachable`
- `byte_start`, `byte_end` - Byte span in source file
- `start_line`, `end_line` - Line span (1-indexed)
- `start_col`, `end_col` - Column span (0-indexed)
- `cfg_hash` - Optional hash of MIR/AST for cache invalidation
- `statements` - Optional array of high-fidelity statements (MIR instructions or AST snippets)
- `coord_x` - Dominator depth (structural hierarchy). 0 = entry block, increases with nesting depth
- `coord_y` - Loop nesting level (iterative complexity). 0 = no loops, increases with nested loop depth
- `coord_z` - Branch count (decision density). Number of branch decisions from entry to this block
- `coord_t` - Time/version (git commit hash or trace timestamp). `null` for current version

**Block Kinds:**
- `entry` - Function entry point
- `exit` - Function exit point
- `conditional` - Branch condition block
- `loop` - Loop header block
- `match` - Match expression block
- `match_arm` - Individual match arm
- `match_guard` - Match arm guard condition
- `return` - Return statement block
- `call` - Function call block
- `and` - Short-circuit `&&` / `and` block
- `or` - Short-circuit `||` / `or` block
- `try` - Try/catch or `?` expression block
- `stmt` - Generic statement block
- `merge` - Control flow merge point
- `if` - If expression block
- `for` / `while` / `loop` - Loop body blocks
- `let` - Let binding block
- `break` / `continue` - Loop control blocks
- `attribute_item` - Attribute/annotation block
- `use_declaration` - Import statement block
- `const_item` - Constant declaration block
- `function_item` - Nested function block
- `struct_expression` / `tuple_expression` / `field_expression` - Expression blocks
- `macro` - Macro invocation block
- `identifier` / `boolean_literal` / `float_literal` / `scoped_identifier` / `self` - Terminal blocks
- `line_comment` - Comment block (preserved in CFG)

**Purpose:** Control flow graph basic blocks stored in the `cfg_blocks` table.
Edges are stored in the separate `cfg_edges` table (not `graph_edges`).

**4D Coordinates (Schema v10+):**
The `coord_x`, `coord_y`, `coord_z` columns provide spatial coordinates for each block:
- **X (Dominator Depth):** How deeply nested this block is in the dominator tree. Entry block = 0.
- **Y (Loop Nesting):** How many loop nestings enclose this block. No loops = 0.
- **Z (Branch Distance):** Cumulative branch decisions from entry. Linear code = 0.
- **T (Temporal):** Git commit or trace timestamp for historical queries. `null` = current.

These coordinates enable efficient spatial queries (e.g., "find blocks at loop depth > 2")
and are used by Mirage for complexity analysis and hotspot detection.

### CfgEdge (in `cfg_edges` table)

CFG edges are stored in the dedicated `cfg_edges` table (not in `graph_edges`):

```sql
CREATE TABLE cfg_edges (
    from_id   INTEGER NOT NULL,
    to_id     INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    PRIMARY KEY (from_id, to_id, edge_type),
    FOREIGN KEY (from_id) REFERENCES cfg_blocks(id),
    FOREIGN KEY (to_id) REFERENCES cfg_blocks(id)
);
```

**Fields:**
- `from_id` - Source block ID (references `cfg_blocks.id`)
- `to_id` - Target block ID (references `cfg_blocks.id`)
- `edge_type` - Edge kind (see below)

**Edge Kinds:**
| Edge Type | Description |
|-----------|-------------|
| `fallthrough` | Unconditional fall-through to next block |
| `conditional_true` | True/then branch of conditional |
| `conditional_false` | False/else branch of conditional |
| `jump` | Unconditional jump (goto, break, continue) |
| `back_edge` | Loop back-edge |
| `call` | Function call edge |
| `return` | Return from function |

**Legacy:** Older databases may also store CFG edges in `graph_edges` with
`edge_type = "CFG_BLOCK"` and edge details in the `data` JSON field.

**Indexes:**
- `idx_cfg_edges_from` on `from_id`
- `idx_cfg_edges_to` on `to_id`

---

## CFG Analysis Tables

### `cfg_paths` - Execution Path Storage

Stores pre-computed execution paths through a function's CFG.

```sql
CREATE TABLE cfg_paths (
    path_id     TEXT PRIMARY KEY,
    function_id INTEGER NOT NULL,
    path_kind   TEXT NOT NULL,
    entry_block INTEGER NOT NULL,
    exit_block  INTEGER NOT NULL,
    length      INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (function_id) REFERENCES graph_entities(id)
);
```

**Fields:**
- `path_id` - Unique path identifier (UUID or deterministic key)
- `function_id` - Function this path belongs to
- `path_kind` - Path classification (e.g., `happy_path`, `error_path`, `complete`)
- `entry_block` - Starting block ID
- `exit_block` - Ending block ID
- `length` - Number of blocks in the path
- `created_at` - Unix timestamp

**Indexes:**
- `idx_cfg_paths_function` on `function_id`
- `idx_cfg_paths_kind` on `path_kind`

### `cfg_path_elements` - Path Block Sequence

Stores the ordered sequence of blocks within each path.

```sql
CREATE TABLE cfg_path_elements (
    path_id        TEXT NOT NULL,
    sequence_order INTEGER NOT NULL,
    block_id       INTEGER NOT NULL,
    PRIMARY KEY (path_id, sequence_order),
    FOREIGN KEY (path_id) REFERENCES cfg_paths(path_id)
);
```

**Fields:**
- `path_id` - References `cfg_paths.path_id`
- `sequence_order` - Zero-based position in the path
- `block_id` - Block ID at this position

### `cfg_dominators` - Dominator Relationships

Stores immediate and transitive dominator relationships.

```sql
CREATE TABLE cfg_dominators (
    block_id      INTEGER NOT NULL,
    dominator_id  INTEGER NOT NULL,
    is_strict     BOOLEAN NOT NULL,
    PRIMARY KEY (block_id, dominator_id, is_strict),
    FOREIGN KEY (block_id) REFERENCES cfg_blocks(id),
    FOREIGN KEY (dominator_id) REFERENCES cfg_blocks(id)
);
```

**Fields:**
- `block_id` - The dominated block
- `dominator_id` - The dominating block
- `is_strict` - `true` if strict dominator (block != dominator), `false` if self-dominance

### `cfg_post_dominators` - Post-Dominator Relationships

Stores post-dominator relationships for backward analysis.

```sql
CREATE TABLE cfg_post_dominators (
    block_id          INTEGER NOT NULL,
    post_dominator_id INTEGER NOT NULL,
    is_strict         BOOLEAN NOT NULL,
    PRIMARY KEY (block_id, post_dominator_id, is_strict),
    FOREIGN KEY (block_id) REFERENCES cfg_blocks(id),
    FOREIGN KEY (post_dominator_id) REFERENCES cfg_blocks(id)
);
```

**Fields:**
- `block_id` - The post-dominated block
- `post_dominator_id` - The post-dominating block
- `is_strict` - `true` if strict post-dominator, `false` if self-post-dominance

---

## Metadata and Metrics Tables

### `magellan_meta` - Schema Version Tracking

Tracks the Magellan and sqlitegraph schema versions.

```sql
CREATE TABLE magellan_meta (
    id                         INTEGER PRIMARY KEY CHECK (id = 1),
    magellan_schema_version    INTEGER NOT NULL,
    sqlitegraph_schema_version INTEGER NOT NULL,
    created_at                 INTEGER NOT NULL
);
```

**Fields:**
- `id` - Always 1 (singleton row)
- `magellan_schema_version` - Current Magellan schema version (see below)
- `sqlitegraph_schema_version` - Current sqlitegraph schema version
- `created_at` - Unix timestamp of database creation

### `file_metrics` - Per-File Metrics

```sql
CREATE TABLE file_metrics (
    file_path       TEXT PRIMARY KEY,
    symbol_count    INTEGER DEFAULT 0,
    loc             INTEGER DEFAULT 0,
    estimated_loc   REAL DEFAULT 0,
    fan_in          INTEGER DEFAULT 0,
    fan_out         INTEGER DEFAULT 0,
    complexity_score REAL DEFAULT 0,
    last_updated    INTEGER NOT NULL
);
```

### `symbol_metrics` - Per-Symbol Metrics

```sql
CREATE TABLE symbol_metrics (
    symbol_id           INTEGER PRIMARY KEY,
    symbol_name         TEXT NOT NULL,
    kind                TEXT NOT NULL,
    file_path           TEXT NOT NULL,
    loc                 INTEGER DEFAULT 0,
    estimated_loc       REAL DEFAULT 0,
    fan_in              INTEGER DEFAULT 0,
    fan_out             INTEGER DEFAULT 0,
    cyclomatic_complexity INTEGER DEFAULT 0,
    last_updated        INTEGER NOT NULL
);
```

### `cross_file_refs` - Cross-File Reference Index

Efficient lookup table for cross-file references without graph traversal.

```sql
CREATE TABLE cross_file_refs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    from_symbol_id TEXT NOT NULL,
    to_symbol_id   TEXT NOT NULL,
    file_path     TEXT NOT NULL,
    line_number   INTEGER NOT NULL,
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL
);
```

**Indexes:**
- `idx_cross_file_refs_to` on `to_symbol_id`
- `idx_cross_file_refs_from` on `from_symbol_id`
- `idx_cross_file_refs_file` on `file_path`

### `execution_log` - Indexing Operation Log

Tracks Magellan indexing operations for auditing.

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
    outcome           TEXT NOT NULL,
    error_message     TEXT,
    files_indexed     INTEGER DEFAULT 0,
    symbols_indexed   INTEGER DEFAULT 0,
    references_indexed INTEGER DEFAULT 0
);
```

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
- **CfgBlock** - CFG basic blocks (in `cfg_blocks` table)
- **CfgEdge** - CFG edges (in `cfg_edges` table)
- **code_chunks** - Source code excerpts
- **ast_nodes** - AST node structure
- **cfg_paths** / **cfg_path_elements** - Execution paths
- **cfg_dominators** / **cfg_post_dominators** - Dominance analysis

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
5. **Delete all cfg_blocks rows** with `function_id` in deleted set
6. **Delete all cfg_edges rows** referencing deleted blocks
7. **Delete all orphaned edges** (edges referencing deleted entities)

**Key Invariant:** Deletion is cascading via edge relationships. Orphan cleanup
ensures no dangling references remain.

---

## Indexes

### sqlitegraph Core Indexes

Implicit indexes created by sqlitegraph:

- Primary key on `graph_entities.id`
- Primary key on `graph_edges.id`
- `idx_edges_from` on `graph_edges(from_id)`
- `idx_edges_to` on `graph_edges(to_id)`
- `idx_edges_type` on `graph_edges(edge_type)`
- `idx_entities_kind_id` on `graph_entities(kind, id)`
- `idx_labels_label` on `graph_labels(label)`
- `idx_labels_label_entity_id` on `graph_labels(label, entity_id)`
- `idx_props_key_value` on `graph_properties(key, value)`
- `idx_props_key_value_entity_id` on `graph_properties(key, value, entity_id)`

### CFG Indexes

- `idx_cfg_blocks_function` on `cfg_blocks(function_id)`
- `idx_cfg_blocks_hash` on `cfg_blocks(cfg_hash)`
- `idx_cfg_edges_from` on `cfg_edges(from_id)`
- `idx_cfg_edges_to` on `cfg_edges(to_id)`
- `idx_cfg_paths_function` on `cfg_paths(function_id)`
- `idx_cfg_paths_kind` on `cfg_paths(path_kind)`

### Other Indexes

- `idx_chunks_file_path` on `code_chunks(file_path)`
- `idx_chunks_symbol_name` on `code_chunks(symbol_name)`
- `idx_chunks_content_hash` on `code_chunks(content_hash)`
- `idx_ast_nodes_parent` on `ast_nodes(parent_id)`
- `idx_ast_nodes_span` on `ast_nodes(byte_start, byte_end)`
- `idx_ast_nodes_file_id` on `ast_nodes(file_id)`
- `idx_symbol_metrics_file_path` on `symbol_metrics(file_path)`
- `idx_cross_file_refs_to` on `cross_file_refs(to_symbol_id)`
- `idx_cross_file_refs_from` on `cross_file_refs(from_symbol_id)`
- `idx_cross_file_refs_file` on `cross_file_refs(file_path)`

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

Current schema version stored in `src/migrate_cmd.rs`:

```rust
pub const MAGELLAN_SCHEMA_VERSION: i64 = 11;
```

**Version History:**
| Version | Changes |
|---------|---------|
| v4 | BLAKE3-based SymbolId, canonical_fqn, display_fqn |
| v5 | AST nodes table for hierarchy storage |
| v6 | AST nodes `file_id` column for per-file tracking |
| v7 | CFG blocks table for control flow graph storage |
| v8 | `cfg_blocks.cfg_hash` column for cache invalidation |
| v9 | `cfg_blocks.statements` column for AST snippets |
| v10 | `cfg_blocks` 4D spatial-temporal coordinate columns (`coord_x`, `coord_y`, `coord_z`, `coord_t`) |
| v11 | `geo_index_meta` table for lazy geometric index tracking |

Schema migrations are applied on database open via `ensure_magellan_meta()` in `src/graph/db_compat.rs`.

---

## See Also

- [ARCHITECTURE.md](ARCHITECTURE.md) - Backend architecture overview
- [SCHEMA_GEOMETRIC.md](SCHEMA_GEOMETRIC.md) - Geometric backend schema
- [INVARIANTS.md](INVARIANTS.md) - Database invariants

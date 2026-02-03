# Mirage CFG Schema Extension

This document describes the Control Flow Graph (CFG) tables that Magellan should create during indexing to support Mirage's path-aware code intelligence.

## Overview

Mirage extends Magellan's symbol graph with control flow analysis capabilities:
- **Control Flow Graphs (CFG)**: Function-level execution paths
- **Path Enumeration**: All possible execution paths through functions
- **Dominance Analysis**: Must-execute relationships between code blocks
- **Loop Detection**: Natural loop identification
- **Blast Zone Analysis**: Impact scope from code changes

## Table of Contents

1. [Dependencies](#dependencies)
2. [Table Schema](#table-schema)
3. [Integration with Magellan](#integration-with-magellan)
4. [Indexing Flow](#indexing-flow)
5. [API Extensions](#api-extensions)

---

## Dependencies

### Required Crates (add to Magellan's Cargo.toml)

```toml
[dependencies]
# CFG construction and analysis
petgraph = "0.6"
# Path hashing
blake3 = "1.5"
# Date/time for schema versioning
chrono = "0.4"
```

### Optional Dependencies (for MIR-based CFG)

```toml
[dependencies]
# Charon for Rust MIR extraction (optional, feature-gated)
charon = { git = "https://github.com/AeneasVerif/charon", optional = true }
serde_json = "1.0"
```

Feature flag:
```toml
[features]
default = ["mirage-cfg"]
mirage-cfg = []
mirage-mir = ["charon", "serde_json"]
```

---

## Table Schema

### mirage_meta

Mirage schema version tracking (separate from Magellan's versioning).

```sql
CREATE TABLE IF NOT EXISTS mirage_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    mirage_schema_version INTEGER NOT NULL,
    magellan_schema_version INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | INTEGER | Always 1 (singleton row) |
| `mirage_schema_version` | INTEGER | Mirage schema version (bump on breaking changes) |
| `magellan_schema_version` | INTEGER | Magellan version this was built with |
| `created_at` | INTEGER | Unix timestamp of schema creation |

### cfg_blocks

Basic blocks in control flow graphs. Each row represents one block in a function's CFG.

```sql
CREATE TABLE IF NOT EXISTS cfg_blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id INTEGER NOT NULL,
    block_kind TEXT NOT NULL,
    byte_start INTEGER,
    byte_end INTEGER,
    terminator TEXT,
    function_hash TEXT,
    FOREIGN KEY (function_id) REFERENCES graph_entities(id)
);
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | INTEGER | Internal block ID (auto-increment) |
| `function_id` | INTEGER | Foreign key to `graph_entities.id` |
| `block_kind` | TEXT | Block kind: "Entry", "Exit", or "Normal" |
| `byte_start` | INTEGER | Start byte offset (for source location) |
| `byte_end` | INTEGER | End byte offset (for source location) |
| `terminator` | TEXT | Serialized terminator (JSON: `Terminator` enum) |
| `function_hash` | TEXT | BLAKE3 hash of function body (for incremental updates) |

**Terminator JSON Format**:
```json
{
  "type": "Return",
  // or "Goto" { "target": 3 },
  // or "SwitchInt" { "targets": [1, 2], "otherwise": 3 },
  // or "Call" { "target": 5, "unwind": null },
  // or "Unreachable"
}
```

### cfg_edges

Control flow edges between blocks.

```sql
CREATE TABLE IF NOT EXISTS cfg_edges (
    from_id INTEGER NOT NULL,
    to_id INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    FOREIGN KEY (from_id) REFERENCES cfg_blocks(id),
    FOREIGN KEY (to_id) REFERENCES cfg_blocks(id)
);
```

| Field | Type | Description |
|-------|------|-------------|
| `from_id` | INTEGER | Source block ID (FK to `cfg_blocks.id`) |
| `to_id` | INTEGER | Target block ID (FK to `cfg_blocks.id`) |
| `edge_type` | TEXT | Edge type classification |

**Edge Types**:
| Type | Description |
|------|-------------|
| `Fallthrough` | Normal sequential flow |
| `TrueBranch` | True branch of conditional |
| `FalseBranch` | False branch of conditional |
| `Switch` | Switch case branch |
| `SwitchDefault` | Switch default branch |
| `Call` | Function call (may not return) |
| `CallReturn` | Return from function call |
| `Unwind` | Exception unwinding path |

### cfg_paths

Enumerated execution paths through functions.

```sql
CREATE TABLE IF NOT EXISTS cfg_paths (
    path_id TEXT PRIMARY KEY,
    function_id INTEGER NOT NULL,
    path_kind TEXT NOT NULL,
    entry_block INTEGER NOT NULL,
    exit_block INTEGER NOT NULL,
    length INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (function_id) REFERENCES graph_entities(id)
);
```

| Field | Type | Description |
|-------|------|-------------|
| `path_id` | TEXT | BLAKE3 hash of block sequence (unique identifier) |
| `function_id` | INTEGER | Foreign key to `graph_entities.id` |
| `path_kind` | TEXT | Path classification: "Normal", "Error", "Degenerate", "Unreachable" |
| `entry_block` | INTEGER | First block ID in path |
| `exit_block` | INTEGER | Last block ID in path |
| `length` | INTEGER | Number of blocks in path |
| `created_at` | INTEGER | Unix timestamp when path was enumerated |

### cfg_path_elements

Ordered block IDs for each path.

```sql
CREATE TABLE IF NOT EXISTS cfg_path_elements (
    path_id TEXT NOT NULL,
    sequence_order INTEGER NOT NULL,
    block_id INTEGER NOT NULL,
    PRIMARY KEY (path_id, sequence_order),
    FOREIGN KEY (path_id) REFERENCES cfg_paths(path_id),
    FOREIGN KEY (block_id) REFERENCES cfg_blocks(id)
);
```

| Field | Type | Description |
|-------|------|-------------|
| `path_id` | TEXT | Reference to `cfg_paths.path_id` |
| `sequence_order` | INTEGER | Position in path (0-indexed) |
| `block_id` | INTEGER | Block ID at this position |

### cfg_dominators

Dominance relationships: block A dominates block B if every path from entry to B goes through A.

```sql
CREATE TABLE IF NOT EXISTS cfg_dominators (
    block_id INTEGER NOT NULL,
    dominator_id INTEGER NOT NULL,
    is_strict BOOLEAN NOT NULL,
    PRIMARY KEY (block_id, dominator_id, is_strict),
    FOREIGN KEY (block_id) REFERENCES cfg_blocks(id),
    FOREIGN KEY (dominator_id) REFERENCES cfg_blocks(id)
);
```

| Field | Type | Description |
|-------|------|-------------|
| `block_id` | INTEGER | Block being dominated |
| `dominator_id` | INTEGER | Block that dominates |
| `is_strict` | BOOLEAN | True for strict dominance (A != B), false for A dominates B |

### cfg_post_dominators

Post-dominance relationships: block A post-dominates block B if every path from B to exit goes through A.

```sql
CREATE TABLE IF NOT EXISTS cfg_post_dominators (
    block_id INTEGER NOT NULL,
    post_dominator_id INTEGER NOT NULL,
    is_strict BOOLEAN NOT NULL,
    PRIMARY KEY (block_id, post_dominator_id, is_strict),
    FOREIGN KEY (block_id) REFERENCES cfg_blocks(id),
    FOREIGN KEY (post_dominator_id) REFERENCES cfg_blocks(id)
);
```

### Indexes

```sql
-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function ON cfg_blocks(function_id);
CREATE INDEX IF NOT EXISTS idx_cfg_blocks_kind ON cfg_blocks(block_kind);
CREATE INDEX IF NOT EXISTS idx_cfg_edges_from ON cfg_edges(from_id);
CREATE INDEX IF NOT EXISTS idx_cfg_edges_to ON cfg_edges(to_id);
CREATE INDEX IF NOT EXISTS idx_cfg_paths_function ON cfg_paths(function_id);
CREATE INDEX IF NOT EXISTS idx_cfg_paths_kind ON cfg_paths(path_kind);
CREATE INDEX IF NOT EXISTS cfg_path_elements_block ON cfg_path_elements(block_id);
CREATE INDEX IF NOT EXISTS cfg_dominators_block ON cfg_dominators(block_id);
CREATE INDEX IF NOT EXISTS cfg_dominators_dominator ON cfg_dominators(dominator_id);
```

---

## Integration with Magellan

### Relationship to Existing Tables

Mirage CFG tables reference existing Magellan tables:

```sql
-- cfg_blocks.function_id → graph_entities.id
-- Only functions (kind = 'Function' or kind_normalized = 'fn')
```

### Function Identification

When creating CFG blocks, use `graph_entities` to resolve functions:

```sql
-- Get function ID from name
SELECT id FROM graph_entities
WHERE kind_normalized = 'fn'
AND name = 'my_function';

-- Or from symbol_id
SELECT id FROM graph_entities
WHERE kind_normalized = 'fn'
AND symbol_id = 'abc123...';
```

### File Path Resolution

Source location for blocks comes from `graph_entities.file_path`:

```sql
SELECT ge.file_path, cb.byte_start, cb.byte_end
FROM cfg_blocks cb
JOIN graph_entities ge ON cb.function_id = ge.id
WHERE cb.id = ?;
```

---

## Indexing Flow

### Phase 1: During Magellan Indexing

When Magellan indexes a file with functions:

1. **Extract function symbols** (already done)
2. **Build CFG** for each function:
   - Entry block (function entry)
   - Exit blocks (return, panic, etc.)
   - Normal blocks (basic blocks)
   - Edges (control flow)
3. **Store in `cfg_blocks` and `cfg_edges`**
4. **Compute dominance** and store in `cfg_dominators`
5. **Skip path enumeration** (expensive, done on-demand by Mirage)

### Phase 2: On-Demand by Mirage

Mirage computes expensive operations when requested:

1. **Path enumeration**: User runs `mirage paths`
   - Enumerate all paths through function
   - Store in `cfg_paths` and `cfg_path_elements`
   - Cached for subsequent queries

2. **Blast zone analysis**: User runs `mirage blast-zone`
   - Compute reachability from given block/path
   - Uses CFG structure + pre-computed dominators

### Incremental Updates

Use `function_hash` to detect changed functions:

```sql
-- Before re-indexing a function
SELECT function_hash FROM cfg_blocks
WHERE function_id = ?
LIMIT 1;

-- If hash differs:
DELETE FROM cfg_blocks WHERE function_id = ?;
DELETE FROM cfg_edges WHERE from_id IN (SELECT id FROM cfg_blocks WHERE function_id = ?);
-- Insert new CFG data
```

---

## API Extensions

### Magellan CLI Additions

```bash
# Enable CFG indexing
magellan index --project . --with-cfg

# Show CFG statistics
magellan status --cfg
```

### Rust API Extensions

```rust
// In magellan/lib.rs or similar

pub mod cfg {
    use petgraph::graph::DiGraph;

    /// Basic block in CFG
    pub struct CfgBlock {
        pub kind: CfgBlockKind,
        pub byte_start: Option<usize>,
        pub byte_end: Option<usize>,
        pub terminator: Terminator,
    }

    pub enum CfgBlockKind {
        Entry,
        Exit,
        Normal,
    }

    pub enum Terminator {
        Return,
        Goto { target: usize },
        SwitchInt { targets: Vec<usize>, otherwise: usize },
        Call { target: Option<usize>, unwind: Option<usize> },
        Unreachable,
    }

    /// Control flow graph type
    pub type Cfg = DiGraph<CfgBlock, CfgEdge>;

    /// Edge type in CFG
    #[derive(Clone, Copy)]
    pub enum CfgEdge {
        Fallthrough,
        TrueBranch,
        FalseBranch,
        Switch,
        SwitchDefault,
        Call,
        CallReturn,
        Unwind,
    }

    /// Build CFG from AST (for any language)
    pub fn build_cfg_from_ast(ast_nodes: &[AstNode]) -> Result<Cfg>;

    /// Store CFG in database
    pub fn store_cfg(
        conn: &mut Connection,
        function_id: i64,
        cfg: &Cfg,
    ) -> Result<()>;
}
```

---

## Schema Versioning

### Version 1 (Current)

- Initial CFG schema
- Basic blocks, edges, paths, dominators
- BLAKE3-based path IDs
- Function hash for incremental updates

### Future Versions

When making breaking changes:

1. Bump `mirage_schema_version` in `mirage_meta`
2. Add migration function in Mirage
3. Document changes in this file

---

## Example Usage

### Creating a CFG During Indexing

```rust
use petgraph::graph::DiGraph;

// After extracting a function from AST
let mut cfg = DiGraph::new();

// Add entry block
let entry = cfg.add_node(CfgBlock {
    kind: CfgBlockKind::Entry,
    byte_start: Some(func.byte_start),
    byte_end: Some(func.byte_start + 10),
    terminator: Terminator::Goto { target: 1 },
});

// Add first basic block
let block1 = cfg.add_node(CfgBlock {
    kind: CfgBlockKind::Normal,
    byte_start: Some(50),
    byte_end: Some(100),
    terminator: Terminator::SwitchInt {
        targets: vec![2],
        otherwise: 3,
    },
});

// Add edges
cfg.add_edge(entry, block1, CfgEdge::Fallthrough);

// Store in database
store_cfg(conn, function_id, &cfg)?;
```

### Querying CFG Data

```sql
-- Get all blocks for a function
SELECT id, block_kind, byte_start, byte_end, terminator
FROM cfg_blocks
WHERE function_id = ?
ORDER BY id;

-- Get all edges
SELECT e.from_id, e.to_id, e.edge_type
FROM cfg_edges e
JOIN cfg_blocks b1 ON e.from_id = b1.id
JOIN cfg_blocks b2 ON e.to_id = b2.id
WHERE b1.function_id = ?;
```

---

## Performance Considerations

### Path Enumeration Cost

Path enumeration can be exponential in the worst case. Strategies:

1. **Lazy enumeration**: Only enumerate when user requests
2. **Depth limiting**: Max path length config (default 1000)
3. **Count limiting**: Max paths to enumerate (default 10,000)
4. **Caching**: Store results with function_hash invalidation

### Storage Size Estimates

Per function (approximate):
- `cfg_blocks`: 10-100 rows (function size)
- `cfg_edges`: 10-100 rows
- `cfg_paths`: 10-10,000 rows (depends on complexity)
- `cfg_path_elements`: Sum of all path lengths
- `cfg_dominators`: O(n²) in worst case

Typical Rust function: ~20 blocks, ~30 edges, ~50 paths

---

## Appendix: Terminator Serialization

### JSON Format for Storage

```json
// Return
{"Return": null}

// Goto
{"Goto": {"target": 5}}

// SwitchInt
{"SwitchInt": {"targets": [1, 2, 3], "otherwise": 4}}

// Call
{"Call": {"target": 10, "unwind": null}}

// Unreachable
{"Unreachable": null}
```

### Rust Enum

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Terminator {
    Return,
    Goto { target: usize },
    SwitchInt { targets: Vec<usize>, otherwise: usize },
    Call { target: Option<usize>, unwind: Option<usize> },
    Unreachable,
}
```

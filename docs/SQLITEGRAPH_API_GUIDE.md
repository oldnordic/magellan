# SQLiteGraph API Guide for Magellan

**Purpose**: Document the SQLiteGraph API used in Magellan implementation.
**Last Updated**: 2025-01-01
**sqlitegraph version**: 0.2.10

---

## Table of Contents

1. [Core Types](#core-types)
2. [Database Schema](#database-schema)
3. [Backend API](#backend-api)
4. [Labels and Properties](#labels-and-properties)
5. [Raw SQL Workarounds](#raw-sql-workarounds)
6. [Usage Examples](#usage-examples)

---

## Core Types

### Source: `/sqlitegraph/sqlitegraph/src/api_ergonomics.rs`

```rust
/// Wrapper for entity/node IDs
pub struct NodeId(pub i64);

impl NodeId {
    pub fn as_i64(self) -> i64 { self.0 }
}

impl From<i64> for NodeId {
    fn from(value: i64) -> Self { NodeId(value) }
}
```

**Usage in Magellan:**
```rust
use sqlitegraph::NodeId;

// Create from i64
let node_id = NodeId::from(42);

// Extract i64
let id: i64 = node_id.as_i64();
```

### Source: `/sqlitegraph/sqlitegraph/src/graph/types.rs`

```rust
/// Entity/node stored in the graph
pub struct GraphEntity {
    pub id: i64,
    pub kind: String,      // e.g., "Symbol", "File"
    pub name: String,      // e.g., "main", "src/main.rs"
    pub file_path: Option<String>,
    pub data: serde_json::Value,  // Arbitrary metadata
}

/// Edge stored in the graph
pub struct GraphEdge {
    pub id: i64,
    pub from_id: i64,
    pub to_id: i64,
    pub edge_type: String,  // e.g., "DEFINES", "CALLS"
    pub data: serde_json::Value,
}
```

### Source: `/sqlitegraph/sqlitegraph/src/backend/sqlite/types.rs`

```rust
/// Specification for inserting a node
pub struct NodeSpec {
    pub kind: String,           // Node type/category
    pub name: String,           // Node name
    pub file_path: Option<String>,
    pub data: serde_json::Value, // Metadata as JSON
}

/// Specification for inserting an edge
pub struct EdgeSpec {
    pub from: i64,
    pub to: i64,
    pub edge_type: String,
    pub data: serde_json::Value,
}

/// Query configuration for neighbor lookups
pub struct NeighborQuery {
    pub direction: BackendDirection,
    pub edge_type: Option<String>,
}

/// Direction for graph traversal
pub enum BackendDirection {
    Outgoing,  // From this node to others
    Incoming,  // From others to this node
}
```

### Source: `/sqlitegraph/sqlitegraph/src/api_ergonomics.rs`

```rust
/// Label for categorizing entities (stored in graph_labels table)
pub struct Label(pub String);

/// Property key (stored in graph_properties table)
pub struct PropertyKey(pub String);

/// Property value (stored in graph_properties table)
pub struct PropertyValue(pub String);
```

---

## Database Schema

### Source: `/sqlitegraph/sqlitegraph/src/schema.rs` (lines 94-128)

```sql
-- Core entity storage
CREATE TABLE IF NOT EXISTS graph_entities (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    kind      TEXT NOT NULL,
    name      TEXT NOT NULL,
    file_path TEXT,
    data      TEXT NOT NULL  -- JSON serialized
);

-- Edge storage
CREATE TABLE IF NOT EXISTS graph_edges (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id   INTEGER NOT NULL,
    to_id     INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    data      TEXT NOT NULL  -- JSON serialized
);

-- Label storage (many-to-many)
CREATE TABLE IF NOT EXISTS graph_labels (
    entity_id INTEGER NOT NULL,
    label     TEXT NOT NULL
);

-- Property storage (key-value pairs)
CREATE TABLE IF NOT EXISTS graph_properties (
    entity_id INTEGER NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL
);

-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_edges_from ON graph_edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON graph_edges(to_id);
CREATE INDEX IF NOT EXISTS idx_edges_type ON graph_edges(edge_type);
CREATE INDEX IF NOT EXISTS idx_labels_label ON graph_labels(label);
CREATE INDEX IF NOT EXISTS idx_labels_label_entity_id ON graph_labels(label, entity_id);
CREATE INDEX IF NOT EXISTS idx_props_key_value ON graph_properties(key, value);
CREATE INDEX IF NOT EXISTS idx_props_key_value_entity_id ON graph_properties(key, value, entity_id);
CREATE INDEX IF NOT EXISTS idx_entities_kind_id ON graph_entities(kind, id);
```

---

## Backend API

### Source: `/sqlitegraph/sqlitegraph/src/backend.rs` (lines 33-97)

```rust
pub trait GraphBackend {
    // Basic operations
    fn insert_node(&self, node: NodeSpec) -> Result<i64, SqliteGraphError>;
    fn get_node(&self, id: i64) -> Result<GraphEntity, SqliteGraphError>;
    fn insert_edge(&self, edge: EdgeSpec) -> Result<i64, SqliteGraphError>;

    // Traversal
    fn neighbors(&self, node: i64, query: NeighborQuery) -> Result<Vec<i64>, SqliteGraphError>;
    fn bfs(&self, start: i64, depth: u32) -> Result<Vec<i64>, SqliteGraphError>;
    fn shortest_path(&self, start: i64, end: i64) -> Result<Option<Vec<i64>>, SqliteGraphError>;

    // Advanced queries
    fn node_degree(&self, node: i64) -> Result<(usize, usize), SqliteGraphError>;
    fn k_hop(&self, start: i64, depth: u32, direction: BackendDirection) -> Result<Vec<i64>, SqliteGraphError>;
    fn k_hop_filtered(&self, start: i64, depth: u32, direction: BackendDirection,
                      allowed_edge_types: &[&str]) -> Result<Vec<i64>, SqliteGraphError>;
    fn chain_query(&self, start: i64, chain: &[ChainStep]) -> Result<Vec<i64>, SqliteGraphError>;
    fn pattern_search(&self, start: i64, pattern: &PatternQuery) -> Result<Vec<PatternMatch>, SqliteGraphError>;

    // System operations
    fn checkpoint(&self) -> Result<(), SqliteGraphError>;
    fn snapshot_export(&self, export_dir: &Path) -> Result<SnapshotMetadata, SqliteGraphError>;
    fn snapshot_import(&self, import_dir: &Path) -> Result<ImportMetadata, SqliteGraphError>;
}
```

### Source: `/sqlitegraph/sqlitegraph/src/backend/sqlite/impl_.rs` (lines 21-46)

```rust
pub struct SqliteGraphBackend {
    graph: SqliteGraph,
}

impl SqliteGraphBackend {
    /// Create backend with in-memory database
    pub fn in_memory() -> Result<Self, SqliteGraphError>;

    /// Create backend from existing SqliteGraph
    pub fn from_graph(graph: SqliteGraph) -> Self;

    /// Access underlying SqliteGraph
    pub fn graph(&self) -> &SqliteGraph;

    /// Get all entity IDs
    pub fn entity_ids(&self) -> Result<Vec<i64>, SqliteGraphError>;
}
```

### Source: `/sqlitegraph/sqlitegraph/src/graph/core.rs` (lines 45-72)

```rust
impl SqliteGraph {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, SqliteGraphError>;
    pub fn open_in_memory() -> Result<Self, SqliteGraphError>;
    pub fn open_without_migrations<P: AsRef<Path>>(path: P) -> Result<Self, SqliteGraphError>;
}
```

---

## Labels and Properties

### Source: `/sqlitegraph/sqlitegraph/src/lib.rs` (lines 86-90)

**Exported Functions:**
```rust
pub use index::{add_label, add_property};
```

### Source: `/sqlitegraph/sqlitegraph/src/index.rs`

```rust
/// Add a label to an entity
/// Uses INSERT OR IGNORE - duplicate labels are silently ignored
pub fn add_label(graph: &SqliteGraph, entity_id: i64, label: &str)
    -> Result<(), SqliteGraphError>;

/// Add a property to an entity
/// Does NOT use INSERT OR IGNORE - duplicate properties cause errors
pub fn add_property(graph: &SqliteGraph, entity_id: i64, key: &str, value: &str)
    -> Result<(), SqliteGraphError>;
```

### IMPORTANT: NOT Exported

The following functions exist in `/sqlitegraph/sqlitegraph/src/index.rs` but are **NOT** re-exported in `lib.rs`:

```rust
// NOT exported - must use raw SQL workaround
pub fn get_entities_by_label(graph: &SqliteGraph, label: &str)
    -> Result<Vec<GraphEntity>, SqliteGraphError>;

// NOT exported - must use raw SQL workaround
pub fn get_entities_by_property(graph: &SqliteGraph, key: &str, value: &str)
    -> Result<Vec<GraphEntity>, SqliteGraphError>;
```

---

## Raw SQL Workarounds

Since `get_entities_by_label` and `get_entities_by_property` are not exported, we need raw SQL access.

### Access Pattern

**Source:** `/sqlitegraph/sqlitegraph/src/backend/sqlite/impl_.rs` (lines 38-41)

```rust
pub fn graph(&self) -> &SqliteGraph { &self.graph }
```

**Source:** `/sqlitegraph/sqlitegraph/src/graph/core.rs` (line 22)

```rust
pub struct SqliteGraph {
    pub(crate) conn: Connection,
    // ...
}
```

### Raw SQL Helper Function

```rust
use rusqlite::params;

/// Query entities by label using raw SQL
pub fn query_by_label(
    backend: &SqliteGraphBackend,
    label: &str,
) -> Result<Vec<GraphEntity>, SqliteGraphError> {
    let conn = backend.graph().connection();

    // Get entity IDs with the label
    let mut stmt = conn.prepare_cached(
        "SELECT entity_id FROM graph_labels WHERE label=?1 ORDER BY entity_id"
    ).map_err(|e| SqliteGraphError::query(e.to_string()))?;

    let rows = stmt.query_map(params![label], |row| row.get::<_, i64>(0))
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;

    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }

    // Fetch full entities
    let mut entities = Vec::new();
    for id in ids {
        entities.push(backend.get_node(id)?);
    }

    Ok(entities)
}

/// Query entities by property using raw SQL
pub fn query_by_property(
    backend: &SqliteGraphBackend,
    key: &str,
    value: &str,
) -> Result<Vec<GraphEntity>, SqliteGraphError> {
    let conn = backend.graph().connection();

    let mut stmt = conn.prepare_cached(
        "SELECT entity_id FROM graph_properties WHERE key=?1 AND value=?2 ORDER BY entity_id"
    ).map_err(|e| SqliteGraphError::query(e.to_string()))?;

    let rows = stmt.query_map(params![key, value], |row| row.get::<_, i64>(0))
        .map_err(|e| SqliteGraphError::query(e.to_string()))?;

    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(|e| SqliteGraphError::query(e.to_string()))?);
    }

    let mut entities = Vec::new();
    for id in ids {
        entities.push(backend.get_node(id)?);
    }

    Ok(entities)
}
```

---

## Usage Examples

### Example 1: Insert a Symbol Node

```rust
use sqlitegraph::{
    SqliteGraph, SqliteGraphBackend,
    backend::NodeSpec, index::add_label,
};
use serde_json::json;

// Create backend
let sqlite_graph = SqliteGraph::open("my_graph.db")?;
let backend = SqliteGraphBackend::from_graph(sqlite_graph);

// Insert symbol node
let node_spec = NodeSpec {
    kind: "Symbol".to_string(),
    name: "my_function".to_string(),
    file_path: Some("src/main.rs".to_string()),
    data: json!({
        "kind": "Function",
        "byte_start": 100,
        "byte_end": 200,
        "start_line": 10,
        "end_line": 15,
    }),
};

let node_id = backend.insert_node(node_spec)?;

// Add label for categorization
add_label(backend.graph(), node_id, "rust")?;
add_label(backend.graph(), node_id, "public")?;

// Add property for metrics
add_property(backend.graph(), node_id, "complexity", "5")?;
add_property(backend.graph(), node_id, "lines_of_code", "42")?;
```

### Example 2: Query with DEFINES Edge

```rust
use sqlitegraph::{
    backend::{EdgeSpec, NeighborQuery, BackendDirection},
};

// Create File node
let file_spec = NodeSpec {
    kind: "File".to_string(),
    name: "src/main.rs".to_string(),
    file_path: Some("src/main.rs".to_string()),
    data: json!({"path": "src/main.rs"}),
};
let file_id = backend.insert_node(file_spec)?;

// Create DEFINES edge: File ─[DEFINES]→ Symbol
let edge_spec = EdgeSpec {
    from: file_id,
    to: node_id,
    edge_type: "DEFINES".to_string(),
    data: json!({}),
};
backend.insert_edge(edge_spec)?;

// Query all symbols defined by this file
let neighbors = backend.neighbors(file_id, NeighborQuery {
    direction: BackendDirection::Outgoing,
    edge_type: Some("DEFINES".to_string()),
})?;
```

### Example 3: Query by Label (Raw SQL Workaround)

```rust
// Using the helper function defined above
let rust_symbols = query_by_label(&backend, "rust")?;

for entity in rust_symbols {
    println!("Found Rust symbol: {}", entity.name);
}
```

---

## Summary Table

| Feature | Exported | How to Use |
|---------|----------|------------|
| `insert_node()` | ✅ Yes | `backend.insert_node(spec)` |
| `get_node()` | ✅ Yes | `backend.get_node(id)` |
| `insert_edge()` | ✅ Yes | `backend.insert_edge(spec)` |
| `neighbors()` | ✅ Yes | `backend.neighbors(id, query)` |
| `add_label()` | ✅ Yes | `add_label(backend.graph(), id, "label")` |
| `add_property()` | ✅ Yes | `add_property(backend.graph(), id, "key", "value")` |
| `get_entities_by_label()` | ❌ No | Use raw SQL helper |
| `get_entities_by_property()` | ❌ No | Use raw SQL helper |
| `connection()` | ✅ Yes | `backend.graph().connection()` |
| `entity_ids()` | ✅ Yes | `backend.entity_ids()` |

---

## File Reference

| File | Purpose |
|------|---------|
| `/sqlitegraph/sqlitegraph/src/lib.rs` | Public API exports |
| `/sqlitegraph/sqlitegraph/src/api_ergonomics.rs` | Core types (NodeId, Label, etc.) |
| `/sqlitegraph/sqlitegraph/src/backend.rs` | GraphBackend trait |
| `/sqlitegraph/sqlitegraph/src/backend/sqlite/impl_.rs` | SqliteGraphBackend implementation |
| `/sqlitegraph/sqlitegraph/src/backend/sqlite/types.rs` | NodeSpec, EdgeSpec, NeighborQuery |
| `/sqlitegraph/sqlitegraph/src/graph/core.rs` | SqliteGraph construction |
| `/sqlitegraph/sqlitegraph/src/graph/types.rs` | GraphEntity, GraphEdge |
| `/sqlitegraph/sqlitegraph/src/schema.rs` | Database schema |
| `/sqlitegraph/sqlitegraph/src/index.rs` | add_label, add_property |

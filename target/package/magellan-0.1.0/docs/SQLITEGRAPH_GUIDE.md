# Sqlitegraph Integration Guide

**Purpose:** Document critical sqlitegraph API patterns and constraints discovered during Magellan Phase 3 implementation.

**Date:** 2025-12-24
**Sqlitegraph Version:** 0.2.10 (local path dependency)

---

## Overview

sqlitegraph is a graph database built on SQLite with a minimal API surface. This document captures **hard-earned lessons** from integrating sqlitegraph into Magellan.

**CRITICAL RULE:** Do NOT assume sqlitegraph APIs work like other graph databases. Read the source code before using any API.

---

## Architecture

### Backend Trait Pattern

sqlitegraph uses a trait-based backend system:

```rust
pub trait GraphBackend {
    fn insert_node(&self, node: NodeSpec) -> Result<i64, SqliteGraphError>;
    fn get_node(&self, id: i64) -> Result<GraphEntity, SqliteGraphError>;
    fn insert_edge(&self, edge: EdgeSpec) -> Result<i64, SqliteGraphError>;
    fn neighbors(&self, node: i64, query: NeighborQuery) -> Result<Vec<i64>, SqliteGraphError>;
    // ... other methods
}
```

**Key Implementation:** `SqliteGraphBackend` is the concrete SQLite implementation.

---

## Lesson 1: Opaque JSON Payloads (NO Per-Property Access)

### ❌ WRONG ASSUMPTION
```rust
// WRONG: Assuming per-property access like Neo4j
let node = graph.get_node(id)?;
let name: String = node.get_property("name")?;
let kind: String = node.get_property("kind")?;
```

**This does NOT exist in sqlitegraph.**

### ✅ CORRECT APPROACH
```rust
// CORRECT: All data stored as opaque JSON payload
#[derive(Serialize, Deserialize)]
struct FileNode {
    path: String,
    hash: String,
}

// Store entire struct as JSON
let node_spec = NodeSpec {
    kind: "File".to_string(),
    name: path.clone(),
    file_path: Some(path.clone()),
    data: serde_json::to_value(FileNode {
        path: path.to_string(),
        hash: hash.to_string(),
    })?,
};

let id = backend.insert_node(node_spec)?;

// Retrieve: Deserialize entire payload
let entity = backend.get_node(id)?;
let file_node: FileNode = serde_json::from_value(entity.data)?;
```

### Why This Design?
- **Simplicity:** Single column stores all properties
- **Flexibility:** Each consumer owns their schema
- **Performance:** No property table joins
- **Schemaless:** Add fields without migrations

---

## Lesson 2: Use Concrete Type, Not Trait Object

### ❌ WRONG APPROACH
```rust
// WRONG: Using trait object and downcasting
use sqlitegraph::{open_graph, GraphBackend, GraphConfig};

let cfg = GraphConfig::sqlite();
let graph: Box<dyn GraphBackend> = open_graph(db_path, &cfg)?;

// Try to downcast to access delete operations
let backend = *graph
    .as_any()
    .downcast::<SqliteGraphBackend>()
    .map_err(|e| anyhow::anyhow!("Failed to downcast: {:?}", e))?;

// ERROR: as_any() method doesn't exist on Box<dyn GraphBackend>
```

**Problem:** `GraphBackend` trait doesn't expose delete operations needed for cleanup.

### ✅ CORRECT APPROACH
```rust
// CORRECT: Directly create concrete type
use sqlitegraph::{SqliteGraph, SqliteGraphBackend};

let sqlite_graph = SqliteGraph::open(db_path)?;
let backend = SqliteGraphBackend::from_graph(sqlite_graph);

// Now backend has full API access
backend.graph().delete_entity(node_id)?;
backend.graph().delete_edge(edge_id)?;
```

### When to Use Each
- **Use `GraphBackend` trait:** When you only need read/query operations
- **Use `SqliteGraphBackend`:** When you need delete operations or direct SQLite access

---

## Lesson 3: NeighborQuery Field Names

### ❌ WRONG ASSUMPTION
```rust
// WRONG: Assuming neo4j-style field names
let neighbor_ids = backend.neighbors(node_id, NeighborQuery {
    direction: BackendDirection::Outgoing,
    edge_filter: Some("DEFINES".to_string()),  // ❌ Wrong field name
    node_filter: None,                          // ❌ Field doesn't exist
})?;
```

### ✅ CORRECT APPROACH
```rust
// CORRECT: Use actual field names
use sqlitegraph::{BackendDirection, NeighborQuery};

let neighbor_ids = backend.neighbors(node_id, NeighborQuery {
    direction: BackendDirection::Outgoing,
    edge_type: Some("DEFINES".to_string()),  // ✅ Correct
})?;
```

### NeighborQuery Definition
```rust
pub struct NeighborQuery {
    pub direction: BackendDirection,  // Outgoing | Incoming
    pub edge_type: Option<String>,    // Filter by edge type
    // NO node_filter field
}
```

---

## Lesson 4: Must Import Trait to Use Methods

### ❌ COMMON MISTAKE
```rust
use sqlitegraph::{SqliteGraphBackend, NeighborQuery, BackendDirection};
// Missing: GraphBackend trait import

let backend = SqliteGraphBackend::in_memory()?;

let neighbors = backend.neighbors(node_id, NeighborQuery::default())?;
// ERROR: no method named `neighbors` found for `SqliteGraphBackend`
```

**Problem:** Rust requires trait to be in scope to call its methods.

### ✅ CORRECT APPROACH
```rust
use sqlitegraph::{
    SqliteGraphBackend,
    GraphBackend,  // ✅ Import trait
    NeighborQuery,
    BackendDirection,
};

let backend = SqliteGraphBackend::in_memory()?;

let neighbors = backend.neighbors(node_id, NeighborQuery::default())?;
// ✅ Works! GraphBackend trait is in scope
```

### Rule of Thumb
If you see error "method not found for `SqliteGraphBackend`", check if you need to import `GraphBackend`.

---

## Lesson 5: Public vs Private Entity ID Methods

### ❌ WRONG ASSUMPTION
```rust
// WRONG: Assuming all_entity_ids() is public
let graph = backend.graph();
let ids = graph.all_entity_ids()?;
// ERROR: method `all_entity_ids` is private
```

### ✅ CORRECT APPROACH
```rust
// CORRECT: Use the public wrapper method
let ids = backend.entity_ids()?;
```

### API Surface
```rust
impl SqliteGraphBackend {
    /// Get all entity IDs (PUBLIC)
    pub fn entity_ids(&self) -> Result<Vec<i64>, SqliteGraphError> {
        self.graph.all_entity_ids()  // Calls private method
    }
}

impl SqliteGraph {
    /// Get all entity IDs (PRIVATE)
    pub(crate) fn all_entity_ids(&self) -> Result<Vec<i64>, SqliteGraphError> {
        // ...
    }
}
```

### Pattern
- **Private methods:** Internal implementation details
- **Public wrapper:** Safe access through backend type

---

## Complete Example: Magellan's CodeGraph

Here's the full pattern used in Magellan:

```rust
use sqlitegraph::{
    BackendDirection,
    GraphBackend,              // ✅ Import trait
    NeighborQuery,
    NodeId,
    NodeSpec,
    EdgeSpec,
    SqliteGraph,              // ✅ Use concrete types
    SqliteGraphBackend,
};

pub struct CodeGraph {
    backend: SqliteGraphBackend,  // ✅ Concrete type
    file_index: HashMap<String, NodeId>,
}

impl CodeGraph {
    pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        // Directly create SqliteGraph
        let sqlite_graph = SqliteGraph::open(db_path)?;

        Ok(Self {
            backend: SqliteGraphBackend::from_graph(sqlite_graph),
            file_index: HashMap::new(),
        })
    }

    pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        // 1. Compute hash
        let hash = self.compute_hash(source);

        // 2. Find or create file node
        let file_id = self.find_or_create_file_node(path, &hash)?;

        // 3. Delete old symbols (cascade)
        self.delete_file_symbols(file_id)?;

        // 4. Parse symbols
        let mut parser = Parser::new()?;
        let symbol_facts = parser.extract_symbols(PathBuf::from(path), source);

        // 5. Insert new symbols
        for fact in &symbol_facts {
            let symbol_id = self.insert_symbol_node(fact)?;
            self.insert_defines_edge(file_id, symbol_id)?;
        }

        Ok(symbol_facts.len())
    }

    fn insert_symbol_node(&self, fact: &SymbolFact) -> Result<NodeId> {
        // ✅ Opaque JSON payload
        let symbol_node = SymbolNode {
            name: fact.name.clone(),
            kind: format!("{:?}", fact.kind),
            byte_start: fact.byte_start,
            byte_end: fact.byte_end,
        };

        let node_spec = NodeSpec {
            kind: "Symbol".to_string(),
            name: fact.name.clone().unwrap_or_else(|| {
                format!("<{:?} at {}>", fact.kind, fact.byte_start)
            }),
            file_path: Some(fact.file_path.to_string_lossy().to_string()),
            data: serde_json::to_value(symbol_node)?,  // ✅ JSON payload
        };

        let id = self.backend.insert_node(node_spec)?;
        Ok(NodeId::from(id))
    }

    fn insert_defines_edge(&self, file_id: NodeId, symbol_id: NodeId) -> Result<()> {
        let edge_spec = EdgeSpec {
            from: file_id.as_i64(),
            to: symbol_id.as_i64(),
            edge_type: "DEFINES".to_string(),
            data: serde_json::json!({}),  // Empty payload
        };

        self.backend.insert_edge(edge_spec)?;
        Ok(())
    }

    pub fn symbols_in_file(&mut self, path: &str) -> Result<Vec<SymbolFact>> {
        let file_id = match self.find_file_node(path)? {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        // ✅ Correct NeighborQuery fields
        let neighbor_ids = self.backend.neighbors(
            file_id.as_i64(),
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("DEFINES".to_string()),  // ✅ edge_type, not edge_filter
            },
        )?;

        let mut symbols = Vec::new();
        for symbol_node_id in neighbor_ids {
            if let Ok(Some(fact)) = self.symbol_fact_from_node(symbol_node_id, PathBuf::from(path)) {
                symbols.push(fact);
            }
        }

        Ok(symbols)
    }
}
```

---

## Common Pitfalls

### Pitfall 1: Forgetting serde_json::Value
```rust
// ❌ WRONG
let data = "some string";

// ✅ CORRECT
let data = serde_json::to_value(some_struct)?;
```

### Pitfall 2: Using wrong NodeId type
```rust
// ❌ WRONG
let node_id: i64 = backend.insert_node(node_spec)?;

// ✅ CORRECT
let id: i64 = backend.insert_node(node_spec)?;
let node_id = NodeId::from(id);
```

### Pitfall 3: Assuming cascade delete
```rust
// ❌ WRONG: Deleting file node doesn't auto-delete symbols
backend.graph().delete_entity(file_id)?;
// Symbols still exist, edges are broken

// ✅ CORRECT: Manually delete symbols first
self.delete_file_symbols(file_id)?;
backend.graph().delete_entity(file_id)?;
```

### Pitfall 4: Not checking Option results
```rust
// ❌ WRONG: Will panic on None
let node: FileNode = serde_json::from_value(entity.data).unwrap();

// ✅ CORRECT: Handle deserialization failure
let node: Option<FileNode> = serde_json::from_value(entity.data).ok();
let node = match node {
    Some(n) => n,
    None => return Ok(None),
};
```

---

## Performance Considerations

### 1. In-Memory Indexing
Graph traversals are expensive. Maintain in-memory indexes:

```rust
pub struct CodeGraph {
    backend: SqliteGraphBackend,
    file_index: HashMap<String, NodeId>,  // Fast lookups
}

fn find_file_node(&mut self, path: &str) -> Result<Option<NodeId>> {
    // Check index first (O(1))
    if let Some(&id) = self.file_index.get(path) {
        return Ok(Some(id));
    }

    // Fallback: expensive graph scan
    self.rebuild_file_index()?;
    Ok(self.file_index.get(path).copied())
}
```

### 2. Batch Operations
Minimize round trips:

```rust
// ❌ WRONG: N queries
for fact in &symbol_facts {
    backend.insert_edge(...)?;
}

// ✅ BETTER: Single transaction (if needed)
// Note: sqlitegraph auto-handles transactions per operation
```

### 3. Hash-Based Change Detection
Avoid unnecessary re-parsing:

```rust
let hash = self.compute_hash(source);

// Only re-index if hash changed
if let Some(existing) = self.get_file_hash(path)? {
    if existing == hash {
        return Ok(0);  // No changes
    }
}
```

---

## Type Reference

### Core Types
```rust
/// Node specification for insertion
pub struct NodeSpec {
    pub kind: String,              // Node label (e.g., "File", "Symbol")
    pub name: String,              // Human-readable name
    pub file_path: Option<String>, // Optional file path
    pub data: serde_json::Value,   // Opaque payload
}

/// Edge specification for insertion
pub struct EdgeSpec {
    pub from: i64,                 // Source node ID
    pub to: i64,                   // Target node ID
    pub edge_type: String,         // Edge label (e.g., "DEFINES")
    pub data: serde_json::Value,   // Opaque payload
}

/// Neighbor query configuration
pub struct NeighborQuery {
    pub direction: BackendDirection,  // Outgoing | Incoming
    pub edge_type: Option<String>,    // Filter by edge type
}

/// Graph entity (node)
pub struct GraphEntity {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub file_path: Option<String>,
    pub data: serde_json::Value,
}

/// Direction for traversals
pub enum BackendDirection {
    Outgoing,
    Incoming,
}

/// Node ID wrapper
pub struct NodeId(i64);

impl NodeId {
    pub fn from(id: i64) -> Self;
    pub fn as_i64(&self) -> i64;
}
```

---

## Testing Patterns

### Unit Test Pattern
```rust
#[test]
fn test_graph_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Test insertion
    let count = graph.index_file("test.rs", b"fn test() {}").unwrap();
    assert_eq!(count, 1);

    // Test retrieval
    let symbols = graph.symbols_in_file("test.rs").unwrap();
    assert_eq!(symbols.len(), 1);
}
```

### In-Memory Database Pattern
```rust
let backend = SqliteGraphBackend::in_memory().unwrap();
// Useful for fast tests, no file cleanup needed
```

---

## Summary of Critical Rules

1. **Use opaque JSON payloads** - No per-property access
2. **Use concrete `SqliteGraphBackend`** - Not trait object, for delete operations
3. **Import `GraphBackend` trait** - Required to call trait methods
4. **Use correct field names** - `edge_type` not `edge_filter`, no `node_filter`
5. **Use public API methods** - `entity_ids()` not `all_entity_ids()`
6. **Handle `Option` results** - Deserialization can fail
7. **Manual cascade deletes** - Delete dependents before parent
8. **Maintain in-memory indexes** - Graph scans are expensive

---

## Further Reading

- sqlitegraph source: `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/`
- Magellan implementation: `/home/feanor/Projects/magellan/src/graph.rs`
- Test examples: `/home/feanor/Projects/magellan/tests/graph_persist.rs`

**Remember:** Always read the source code before assuming sqlitegraph APIs work a certain way!

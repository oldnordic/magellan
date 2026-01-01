# SQLiteGraph Research Findings

**Date**: 2025-01-01
**Status**: Research Complete
**Test File**: `tests/sqlitegraph_exploration.rs`

---

## Test Results Summary

All three sqlitegraph features tested are **WORKING** and ready for integration:

### 1. HNSW Vector Embeddings ✅

```rust
use sqlitegraph::hnsw::{hnsw_config, DistanceMetric, HnswIndex};

// Production-ready config for OpenAI text-embedding-ada-002 (1536 dim)
let config = hnsw_config()
    .dimension(1536)
    .m_connections(20)
    .ef_construction(400)
    .ef_search(100)
    .distance_metric(DistanceMetric::Cosine)
    .build()?;

let mut hnsw = HnswIndex::new(config)?;

// Insert with metadata
hnsw.insert_vector(&embedding, Some(metadata))?;

// Search similar
let results = hnsw.search(&query, k)?;
```

**Performance** (synthetic data):
- Insert 4 vectors (1536 dim): ~92µs
- Search for top-3: ~227µs

**Schema created**:
- `hnsw_indexes` - Index definitions
- `hnsw_vectors` - Vector data + metadata
- `hnsw_layers` - HNSW graph structure
- `hnsw_entry_points` - Search entry points

**Key finding**: HNSW is FULLY FUNCTIONAL for semantic code search with 1536-dim OpenAI embeddings.

---

### 2. Graph Labels ✅

```rust
use sqlitegraph::{SqliteGraph, add_label};

let graph = SqliteGraph::open(path)?;
let entity_id = graph.insert_entity(&entity)?;

// Add labels
add_label(&graph, entity_id, "rust")?;
add_label(&graph, entity_id, "public")?;
add_label(&graph, entity_id, "async")?;
```

**Schema**: `graph_labels(entity_id, label)`
- Query via: `SELECT label FROM graph_labels WHERE entity_id=?1`

**Key finding**: Labels work for categorizing symbols by language, visibility, domain, etc.

---

### 3. Graph Properties ✅

```rust
use sqlitegraph::{SqliteGraph, add_property};

let graph = SqliteGraph::open(path)?;
let entity_id = graph.insert_entity(&entity)?;

// Add properties
add_property(&graph, entity_id, "complexity", "12")?;
add_property(&graph, entity_id, "lines_of_code", "87")?;
add_property(&graph, entity_id, "test_coverage", "65%")?;
```

**Schema**: `graph_properties(entity_id, key, value)`
- Query via: `SELECT key, value FROM graph_properties WHERE entity_id=?1`

**Key finding**: Properties work for storing metrics (complexity, coverage, size).

---

## What Magellan Currently Uses

| Feature | Current Usage | Potential |
|---------|---------------|-----------|
| **Backend** | SQLite only | Native V2 available but requires `native-v2` feature |
| **Tables** | `graph_entities`, `graph_edges` | Also has `graph_labels`, `graph_properties` |
| **HNSW** | Not used | Fully functional, 1536-dim ready |
| **Graph queries** | Custom SQL | `GraphBackend` trait available |

---

## API Limitations Found

Some helper functions are NOT exported in sqlitegraph 0.2.10:
- `get_entities_by_label()` - must use raw SQL instead
- `get_entities_by_property()` - must use raw SQL instead
- `labels_for_entity()` - must use raw SQL instead
- `properties_for_entity()` - must use raw SQL instead

**Workaround**: Direct SQL queries work fine.

---

## Integration Recommendations

### 1. Source Code Text Storage

Store code chunks with byte spans:

```sql
CREATE TABLE code_blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    content TEXT NOT NULL,
    hash TEXT NOT NULL,
    last_modified INTEGER NOT NULL,
    symbol_name TEXT,
    symbol_kind TEXT
);
```

### 2. HNSW for Semantic Search

```rust
// During indexing, generate embeddings for function bodies
let embedding = openai_client.embed(function_body)?;
hnsw.insert_vector(&embedding, Some(json!({
    "graph_node_id": node_id,
    "symbol_name": function_name,
    "file_path": file_path,
    "byte_start": byte_start,
    "byte_end": byte_end,
})))?;

// Query: "find functions similar to X"
let results = hnsw.search(&query_embedding, 10)?;
```

### 3. Labels for Quick Filtering

```rust
// During indexing
add_label(&graph, symbol_id, "rust")?;
add_label(&graph, symbol_id, "public")?;
add_label(&graph, symbol_id, "test")?;

// Query: "all public test functions"
// SELECT entity_id FROM graph_labels WHERE label IN ('public', 'test')
// GROUP BY entity_id HAVING COUNT(*) = 2
```

### 4. Properties for Metrics

```rust
// During analysis
add_property(&graph, symbol_id, "complexity", &complexity.to_string())?;
add_property(&graph, symbol_id, "test_coverage", &coverage.to_string())?;

// Query: "high complexity functions"
// SELECT entity_id FROM graph_properties WHERE key='complexity' AND value > '10'
```

---

## Next Steps

1. ✅ Research complete - all features work
2. ⏳ Design integration schema
3. ⏳ Implement source text storage
4. ⏳ Implement HNSW integration (with OpenAI API)
5. ⏳ Add label/property queries to Magellan

---

## References

- Test: `/home/feanor/Projects/magellan/tests/sqlitegraph_exploration.rs`
- Plan: `/home/feanor/Projects/magellan/docs/SOURCE_TEXT_INTEGRATION_PLAN.md`
- Original idea: `/home/feanor/Projects/odincode/docs/SOURCE_CODE_TEXT_IN_DB.md`

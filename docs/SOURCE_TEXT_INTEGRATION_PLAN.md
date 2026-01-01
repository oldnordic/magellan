# Source Code Text Integration Plan

**Status**: Research Complete ✅
**Created**: 2025-01-01
**Updated**: 2025-01-01
**Goal**: Integrate source code text storage with Magellan using full sqlitegraph capabilities

**Research Findings**: See `docs/SQLITEGRAPH_RESEARCH_FINDINGS.md`

---

## Problem Statement

Currently Magellan:
1. Reads entire files from disk for every operation (token-inefficient)
2. Stores only metadata (byte spans) in the database
3. Uses SQLite backend only (~5K-10K ops/sec)
4. No semantic search capability
5. Uses only ~20% of sqlitegraph's features

**Desired State**:
- Query code blocks directly from DB (no file I/O)
- Optional Native V2 backend for 10x performance
- HNSW embeddings for semantic code search
- Rich metadata via graph_properties

---

## Research Tasks

### Phase 1: Feature Exploration (Current)

- [ ] Test Native V2 backend vs SQLite backend performance
- [ ] Test HNSW vector embeddings for code
- [ ] Test graph_labels for symbol categorization
- [ ] Test graph_properties for rich metadata
- [ ] Measure actual performance differences

### Phase 2: Design (After Research)

- [ ] Design source text storage schema
- [ ] Design embedding generation strategy
- [ ] Design migration path from SQLite to Native V2
- [ ] Design semantic search queries

### Phase 3: Implementation

- [ ] Implement source text storage
- [ ] Implement optional Native V2 backend
- [ ] Implement HNSW integration
- [ ] Implement semantic search queries

---

## SQLiteGraph Features to Test

### 1. Native V2 Backend

**Documentation Reference**: `/home/feanor/Projects/sqlitegraph/manual.md` Section 2

```rust
// SQLite Backend (current)
let graph = SqliteGraph::open("data.db")?;

// Native V2 Backend (10x faster)
let config = GraphConfig::native();
let graph = open_graph("data.db", &config)?;
```

**Expected Performance**:
- Node Insert: ~5K ops/sec (SQLite) → ~50K ops/sec (V2)
- Edge Insert: ~10K ops/sec (SQLite) → ~100K ops/sec (V2)

### 2. HNSW Vector Search

**Documentation Reference**: `/home/feanor/Projects/sqlitegraph/manual.md` Section 9

```rust
use sqlitegraph::hnsw::{hnsw_config, DistanceMetric, HnswIndex};

let config = hnsw_config()
    .dimension(1536)  // OpenAI embedding size
    .distance_metric(DistanceMetric::Cosine)
    .build()?;

let hnsw = HnswIndex::new(config)?;

// Store code embedding with metadata
let metadata = json!({
    "symbol_name": "function_name",
    "file_path": "src/file.rs",
    "code_snippet": "fn function_name() { ... }"
});
hnsw.insert_vector(&embedding, Some(metadata))?;

// Search similar code
let results = hnsw.search(&query_embedding, 10)?;
```

**Schema**: Creates `hnsw_indexes`, `hnsw_vectors`, `hnsw_layers`, `hnsw_entry_points` tables

### 3. Graph Labels

```rust
// Label entities for categorization
graph.insert_label(entity_id, "rust")?;
graph.insert_label(entity_id, "function")?;
graph.insert_label(entity_id, "public")?;
```

### 4. Graph Properties

```rust
// Key-value metadata
graph.insert_property(entity_id, "complexity", "5")?;
graph.insert_property(entity_id, "test_coverage", "80%")?;
```

---

## Test Plan

### Test 1: Native V2 Backend Performance

Create `tests/native_v2_test.rs`:
- Benchmark file indexing with SQLite backend
- Benchmark file indexing with Native V2 backend
- Compare results

### Test 2: HNSW Code Embeddings

Create `tests/hnsw_test.rs`:
- Generate embeddings for function code
- Store in HNSW index
- Test similarity search

### Test 3: Labels and Properties

Create `tests/metadata_test.rs`:
- Test label-based queries
- Test property-based queries
- Test combined graph+metadata queries

---

## Notes from Manual Review

### Backend Comparison

| Characteristic | SQLite Backend | Native V2 Backend |
|----------------|----------------|-------------------|
| Performance | Standard | 10x faster |
| Transactions | Full ACID | Atomic, optimized |
| Maturity | Battle-tested | Production ready |
| Use Case | General purpose | High performance |

### HNSW Configuration

| Use Case | Dimension | M | ef_construction | ef_search |
|----------|-----------|---|-----------------|-----------|
| OpenAI embeddings | 1536 | 20 | 400 | 100 |
| Custom embeddings | 768 | 16 | 200 | 50 |
| Lightweight | 256 | 12 | 150 | 40 |

---

## Next Steps

1. Run feature tests
2. Document actual vs expected performance
3. Design integration based on test results
4. Create implementation plan

---

## References

- SQLiteGraph Manual: `/home/feanor/Projects/sqlitegraph/manual.md`
- Original Idea: `/home/feanor/Projects/odincode/docs/SOURCE_CODE_TEXT_IN_DB.md`
- Current Schema: `/home/feanor/Projects/magellan/src/graph/schema.rs`

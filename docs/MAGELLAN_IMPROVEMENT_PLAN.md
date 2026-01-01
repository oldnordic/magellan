# Magellan Improvement Plan

**Created**: 2025-01-01
**Status**: Ready for Implementation
**Based on**: SQLiteGraph Research Findings

---

## Features to Implement (All Confirmed Working)

| Priority | Feature | Benefit | Complexity |
|----------|---------|---------|------------|
| 1 | Source Code Text Storage | Token-efficient queries | Medium |
| 2 | Graph Labels | Categorize symbols (language, visibility) | Low |
| 3 | Graph Properties | Store metrics (complexity, coverage) | Low |
| 4 | HNSW Embeddings | Semantic code search | High* |

\* HNSW requires external embedding API (OpenAI, etc.)

---

## Phase 1: Source Code Text Storage (High Value)

### Problem
Currently, to modify code, you must read the entire file (thousands of tokens) just to find the byte span.

### Solution
Store code chunks in the database with their byte spans.

### Schema
```sql
CREATE TABLE code_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    content TEXT NOT NULL,
    hash TEXT NOT NULL,
    last_modified INTEGER NOT NULL,
    symbol_name TEXT,
    symbol_kind TEXT,
    UNIQUE(file_path, byte_start, byte_end)
);
```

### Integration Points
1. **Indexing** (`src/graph/ops.rs:index_file`):
   - After parsing symbols, extract code chunks by byte span
   - Store chunks in `code_chunks` table

2. **Querying** (New CLI command):
   ```bash
   magellan get-chunk <file> <symbol-name>
   ```

3. **Modification** (Future):
   - Read chunk from DB instead of entire file
   - Modify and write back

---

## Phase 2: Graph Labels (Quick Win)

### Use Cases
- Categorize by language: `rust`, `python`, `javascript`
- Categorize by visibility: `public`, `private`
- Categorize by domain: `api`, `database`, `ui`

### Implementation

In `src/graph/ops.rs` during `index_file`:

```rust
use sqlitegraph::add_label;

// After inserting symbol node
let entity_id = graph.insert_entity(&entity)?;

// Add labels based on symbol metadata
add_label(&graph.backend, entity_id, language)?;  // "rust", "python", etc.
if symbol.is_public {
    add_label(&graph.backend, entity_id, "public")?;
}
```

### Query Enhancement
```bash
magellan query --label public,rust
magellan query --label api --kind Function
```

---

## Phase 3: Graph Properties (Metrics)

### Use Cases
- Cyclomatic complexity
- Lines of code
- Test coverage percentage
- Last modified date

### Implementation

```rust
use sqlitegraph::add_property;

// During or after indexing
add_property(&graph.backend, entity_id, "complexity", &complexity.to_string())?;
add_property(&graph.backend, entity_id, "lines_of_code", &loc.to_string())?;
add_property(&graph.backend, entity_id, "test_coverage", &coverage.to_string())?;
```

### Query Enhancement
```bash
magellan query --property complexity>10
magellan query --property test_coverage<50
```

---

## Phase 4: CLI Enhancements

### New Commands

```bash
# Get source code for a symbol (token-efficient)
magellan get <file> <symbol-name>
magellan get <file> <symbol-name> --line-range

# Query by labels/properties
magellan query --label public
magellan query --property complexity>5

# Show symbol metadata
magellan info <file> <symbol-name>
# Returns: byte span, labels, properties, source code snippet
```

---

## Implementation Order

### Step 1: Database Schema Update
Create `src/graph/schema.rs` additions:
- `CodeChunk` struct
- Migration to add `code_chunks` table

### Step 2: Indexing Integration
Modify `src/graph/ops.rs:index_file`:
- Extract code chunks during parsing
- Store chunks in database
- Add labels for language, visibility

### Step 3: Query API
Add to `src/graph/mod.rs` (CodeGraph):
- `get_code_chunk(path, symbol_name)` -> Option<String>
- `symbols_by_label(label)` -> Vec<SymbolFact>
- `symbols_by_property(key, value)` -> Vec<SymbolFact>

### Step 4: CLI Commands
Add to `src/main.rs`:
- `magellan get <file> <symbol>`
- `magellan query --label <label>`
- `magellan query --property <key><op><value>`

---

## File Changes Summary

| File | Change |
|------|--------|
| `src/graph/schema.rs` | Add `CodeChunk` struct |
| `src/graph/ops.rs` | Extract and store code chunks; add labels |
| `src/graph/mod.rs` | Add query methods for chunks/labels/properties |
| `src/main.rs` | Add new CLI commands |
| `docs/` | Update manual with new features |

---

## Testing Strategy

1. **Unit tests** for code chunk extraction
2. **Integration tests** for label/property queries
3. **CLI tests** for new commands
4. **Performance tests** comparing file I/O vs DB queries

---

## Rollout Plan

1. ✅ Research complete (features confirmed working)
2. ⏳ Implement schema and indexing
3. ⏳ Implement query API
4. ⏳ Add CLI commands
5. ⏳ Update documentation
6. ⏳ Release v0.5.0

---

## Future: HNSW Semantic Search

Once source code is stored, we can add:

```rust
// During indexing, generate embeddings
let embedding = openai_client.embed(&code_chunk)?;
hnsw.insert_vector(&embedding, Some(metadata)?)?;

// Query similar code
let results = hnsw.search(&query_embedding, 10)?;
```

This requires:
- OpenAI API key or local embedding model
- Embedding generation during indexing
- `--semantic-search` CLI command

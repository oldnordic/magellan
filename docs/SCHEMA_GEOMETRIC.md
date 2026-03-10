# Geometric Backend Schema Reference

**Version:** 3.1.0
**Backend:** Geometric (GeoGraphDB)
**File Extension:** `.geo`
**Required Feature:** `geometric-backend`

---

## Overview

The Geometric backend uses **GeoGraphDB** for 3D spatial indexing of CFG blocks
combined with a **single-file container format** that stores all data in one `.geo`
file with multiple sections.

**Key Differences from SQLite:**
- Single-file format (no separate sidecar files)
- 3D spatial indexing for O(log n) CFG path queries
- In-memory symbol index with persistence on save
- CFG vacuum operation instead of automatic deletion

---

## Single-File Format

### File Structure

```
code.geo (single file container)
├── GRAPH section     - Required: 3D spatial index (GeoGraphDB format)
├── CFG section       - Required: CFG blocks and edges
├── SYMBOLS section   - Optional: Symbol index and call graph
├── COMPLEXITY section- Optional: Cyclomatic complexity history
├── AST section       - Optional: AST nodes (not fully implemented)
├── CHUNKS section    - Optional: Code chunks
├── LABELS section    - Optional: Symbol labels
└── LOGS section      - Optional: Execution logs
```

**One-File Rule:** All persistence goes to internal sections inside the `.geo`
file. NO sidecar files are created (this is non-negotiable).

### Section Access Pattern

```rust
// Open/create
let mut storage = SectionedStorage::create(path)?;
let mut storage = SectionedStorage::open(path)?;

// Check section existence
GraphSectionAdapter::exists(&storage);
CfgSectionAdapter::exists(&storage);

// Load section data
let graph_data = GraphSectionAdapter::load(&mut storage)?;
let cfg_data = CfgSectionAdapter::load(&mut storage)?;

// Save section data
GraphSectionAdapter::save(&mut storage, &graph_data)?;
CfgSectionAdapter::save(&mut storage, &cfg_data)?;

// Flush changes to disk
storage.flush()?;
```

---

## GRAPH Section (Required)

### Format

GeoGraphDB native binary format. Contains 3D spatial index of CFG blocks.

### Coordinate Mapping

| Coordinate | CFG Property | Meaning |
|------------|--------------|---------|
| X | Dominator Depth | How deep in dominator tree |
| Y | Loop Nesting | How many loops deep |
| Z | Branch Count | Number of branches |

**Purpose:** Enables O(log n) spatial queries for:
- Path finding (A* algorithm)
- Range queries (blocks in spatial region)
- Nearest neighbor queries

### Access

```rust
// Load
let graph_data = GraphSectionAdapter::load(&mut storage)?;
// graph_data contains spatial index structures

// Save
GraphSectionAdapter::save(&mut storage, &graph_data)?;
```

---

## CFG Section (Required)

### Format

Binary format containing:

```rust
pub struct CfgData {
    /// All CFG blocks across all functions
    pub blocks: Vec<CfgBlock>,
    /// All CFG edges between blocks
    pub edges: Vec<CfgEdge>,
}
```

### CfgBlock Structure

```rust
pub struct CfgBlock {
    /// Unique block ID
    pub id: u64,
    /// Function this block belongs to
    pub function_id: u64,
    /// Block kind (entry, conditional, loop, match, return, etc.)
    pub kind: String,
    /// Terminator (how control exits)
    pub terminator: String,
    /// Byte span in source file
    pub byte_start: u64,
    pub byte_end: u64,
    /// Line span (1-indexed)
    pub start_line: u64,
    pub end_line: u64,
}
```

### CfgEdge Structure

```rust
pub struct CfgEdge {
    /// Source block ID
    pub src: u64,
    /// Target block ID
    pub dst: u64,
    /// Edge kind (unconditional, conditional_true, conditional_false, etc.)
    pub flags: String,
}
```

**Edge Kinds:**
- `unconditional` - Always taken (fallthrough, jump)
- `conditional_true` - True branch of if
- `conditional_false` - False branch of if
- `loop_entry` - Enter loop body
- `loop_exit` - Exit loop
- `match_arm` - Match case arm

### Stale Block Handling

**Important:** CFG blocks are NOT immediately deleted when files are re-indexed.
Instead, they become **stale**:

1. On re-index, function_id is removed from `cfg_function_ids` tracking set
2. Blocks with untracked function_ids are considered stale
3. Stale blocks are excluded from queries
4. `vacuum_cfg()` physically removes stale blocks from storage

**Why this approach:**
- Avoids seeking in file during re-index
- Batch deletion is more efficient
- Enables copy-on-write semantics

### Access

```rust
// Load
let cfg_data = CfgSectionAdapter::load(&mut storage)?;
let blocks = cfg_data.blocks;
let edges = cfg_data.edges;

// Save
CfgSectionAdapter::save(&mut storage, &cfg_data)?;
```

---

## SYMBOLS Section (Optional)

### Format

JSON-serialized symbol index and call graph.

```rust
pub struct SymbolsData {
    /// FQN → ID mapping
    pub fqn_to_id: HashMap<String, u64>,
    /// Name → IDs mapping (for duplicate name handling)
    pub name_to_ids: HashMap<String, Vec<u64>>,
    /// ID → metadata mapping
    pub id_to_metadata: HashMap<u64, SymbolData>,
    /// Call graph: caller → callees
    pub call_references: HashMap<u64, Vec<u64>>,
}
```

### SymbolData Structure

```rust
pub struct SymbolData {
    /// Fully-qualified name
    pub fqn: String,
    /// Simple name
    pub name: String,
    /// Symbol kind
    pub kind: String,
    /// File path
    pub file_path: String,
    /// Byte span
    pub byte_start: u64,
    pub byte_end: u64,
    /// Line span (1-indexed)
    pub start_line: u64,
    pub end_line: u64,
}
```

### Persistence Behavior

**On save:** If SYMBOLS section exists, it's overwritten. If not, it's created.

**On load:** If SYMBOLS section exists, load it. If not, start with empty index.

**Dirty flag:** GeometricBackend tracks `dirty` flag. Only saves when:
- Explicit `save_to_disk()` call
- Backend is dropped (if dirty)

### Access

```rust
// Load
let symbols_data = SymbolsSectionAdapter::load(&mut storage)?;
let symbol_index = SymbolIndex::from_data(
    symbols_data.fqn_to_id,
    symbols_data.name_to_ids,
    symbols_data.id_to_metadata,
);

// Save
let data = from_symbol_index_and_calls(&symbol_index, &call_refs)?;
SymbolsSectionAdapter::save(&mut storage, &data)?;
```

---

## COMPLEXITY Section (Optional)

### Format

Binary format storing cyclomatic complexity history per function.

### Purpose

Tracks complexity changes over time for:
- Trend analysis
- Complexity growth detection
- Refactoring impact assessment

**Note:** Temporal features are incomplete (4D coordinates are placeholders).

---

## AST, CHUNKS, LABELS, LOGS Sections (Optional)

These sections are for parity with SQLite backend but not fully implemented:

- **AST section** - AST nodes (not fully implemented)
- **CHUNKS section** - Code chunks (not fully implemented)
- **LABELS section** - Symbol labels (not fully implemented)
- **LOGS section** - Execution logs (not fully implemented)

**Status:** Placeholders for future parity with SQLite backend.

---

## Authoritative vs Derived Data

### Authoritative (Persisted)

- **GRAPH section** - 3D spatial index of CFG blocks
- **CFG section** - All CFG blocks and edges
- **SYMBOLS section** - Symbol index and call graph (when persisted)
- **COMPLEXITY section** - Complexity history (when persisted)

### In-Memory (Derived)

- **cfg_function_ids** - Tracked function IDs (source of truth for live CFG)
- **cfg block counts** - Per-function block counts (for stats)
- **symbol index** - In-memory symbol lookup tables

### Derived (Computed on Query)

- **Reachability** - Transitive closure of call graph
- **Cycles (SCCs)** - Computed via Tarjan's algorithm
- **Dominators** - Computed via iterative algorithm
- **Program slices** - Computed via backward/forward reachability
- **Paths** - Computed via A* on 3D spatial index

---

## Re-Index Behavior

### Reconcile Algorithm

```rust
pub fn reconcile_file_path(
    backend: &mut GeometricBackend,
    path: &Path,
) -> Result<GeoReconcileOutcome>
```

1. **Check file existence:**
   - If file does NOT exist → delete symbols from index, return `Deleted`

2. **Compute content hash:**
   - Read file, compute SHA-256

3. **Check for changes:**
   - Compare with stored hash
   - If unchanged → return `Unchanged`

4. **Delete old data:**
   - Remove symbols from in-memory index
   - Remove function_id from `cfg_function_ids` tracking set
   - **Stale CFG blocks remain in storage** (garbage collected on vacuum)

5. **Re-index:**
   - Extract symbols via tree-sitter
   - Extract CFG blocks if applicable
   - Insert into in-memory structures
   - Return `Reindexed`

### Garbage Collection

**CFG vacuum** physically removes stale blocks:

```rust
pub fn vacuum_cfg(&self) -> Result<VacuumResult>
```

1. Get tracked `cfg_function_ids` (source of truth for live CFG)
2. Filter blocks to only include tracked function_ids
3. Filter edges to only include live blocks
4. Rebuild CFG section with filtered data
5. Write to file (in-place update)

---

## Vacuum Behavior

### CFG Vacuum

See `src/graph/geometric_backend.rs:vacuum_cfg()`.

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

**When to call:**
- After multiple re-index cycles on same files
- When database size has grown significantly
- Before committing to version control

**Effects:**
- Removes stale CFG blocks and edges
- Reclaims disk space
- Does NOT affect symbols or call graph

### No Full-File Vacuum

Unlike SQLite VACUUM, the geometric backend does NOT have a full-file vacuum.
Each section is updated independently via section-specific save operations.

---

## Persistence Model

### Lazy Persistence

Symbol index and call graph are persisted **only when dirty**:

```rust
impl Drop for GeometricBackend {
    fn drop(&mut self) {
        if self.dirty.get() {
            let _ = self.save_to_disk();
        }
    }
}
```

### Explicit Save

Users can call `save_to_disk()` explicitly:

```rust
backend.save_to_disk()?;
```

### Save-to-Disk Algorithm

1. Check if dirty (early return if not)
2. Get stats before save (for logging)
3. Save SYMBOLS section (if exists)
4. Flush changes to disk
5. Clear dirty flag

---

## In-Memory Structures

### SymbolIndex

```rust
pub struct SymbolIndex {
    fqn_to_id: HashMap<String, u64>,
    name_to_ids: HashMap<String, Vec<u64>>,
    id_to_metadata: HashMap<u64, SymbolData>,
}
```

**Purpose:** Fast symbol lookups without reading storage.

### CallGraph

```rust
pub struct CallGraph {
    call_refs: HashMap<u64, Vec<u64>>,      // caller → callees
    caller_refs: HashMap<u64, Vec<u64>>,    // callee → callers
}
```

**Purpose:** Bidirectional call graph traversal.

### CfgFunctionIds

```rust
cfg_function_ids: Arc<Mutex<HashSet<u64>>>
```

**Purpose:** Source of truth for which function_ids have live CFG.
Used to filter stale blocks during queries and vacuum.

---

## Differences from SQLite

| Aspect | SQLite | Geometric |
|--------|--------|-----------|
| File format | Relational tables | Single-file sections |
| CFG queries | O(V+E) graph traversal | O(log n) spatial queries |
| Symbol storage | In database (graph_entities) | In-memory + persisted |
| Re-index deletion | Immediate cascade | Lazy (marked stale) |
| Vacuum | Full-file VACUUM | CFG-only vacuum |
| Path enumeration | Not supported | O(log n) via A* |
| AST queries | Supported | Not fully implemented |

---

## See Also

- [ARCHITECTURE.md](ARCHITECTURE.md) - Backend architecture overview
- [SCHEMA_SQLITE.md](SCHEMA_SQLITE.md) - SQLite backend schema
- [INVARIANTS.md](INVARIANTS.md) - Database invariants

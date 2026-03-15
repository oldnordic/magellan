# GeoDB Contract Audit Report

**Date:** 2026-03-11
**Scope:** Audit Magellan's geometric backend integration against actual geographdb-core API
**Purpose:** Identify API mismatches, invariants violations, and recovery path for CFG-heavy workflows

---

## Executive Summary

**Finding:** Magellan's geometric backend is partially compatible with geographdb-core but has critical gaps that prevent CFG-heavy workflows (Mirage/Splice) from functioning correctly.

**Key Issues:**
1. Symbol metadata not persisted (names, file_paths, FQNs stored only in memory)
2. SccResult struct missing `cycle_count` field from upstream
3. CfgEdge type inconsistency (`src_id/dst_id` in one API, `source_idx/target_idx` in another)
4. No symbolic lookups (find_by_name, find_by_fqn) actually work
5. CFG blocks indexed but not queryable

---

## Phase 1: Actual GeoDB API Summary

### Source: `/home/feanor/Projects/geographdb-core/src/lib.rs`

#### Core Types (Public API)

```rust
// Storage primitives
pub use storage::{
    // Sectioned file container
    SectionedStorage,
    // File header constants
    GeoFileHeader, FILE_MAGIC, FORMAT_VERSION, HEADER_SIZE,
    // Section management
    Section, SectionEntry,
    // Sidecar path helpers
    all_sidecar_paths, geo_cfg_path, geo_complexity_path, geo_idx_path,
};

// Graph data structures
pub use storage::{
    GraphData, GraphSectionAdapter,
    CfgData, CfgSectionAdapter, SerializableCfgBlock, CfgEdge,
    NodeRec, EdgeRec, MetadataRec,
};

// Algorithms
pub use algorithms::astar::{CfgGraphNode, CfgPath, PathComplexity};
pub use algorithms::scc::{
    tarjan_scc, condense_graph, find_cycles, has_cycles,
    SccResult  // Contains: components, node_to_component, cycle_count
};
```

#### SectionedStorage API

```rust
impl SectionedStorage {
    // Creation
    pub fn create(path: &Path) -> Result<Self>;
    pub fn open(path: &Path) -> Result<Self>;
    pub fn is_sectioned_file(path: &Path) -> bool;

    // Section management
    pub fn create_section(&mut self, name: &str, capacity: u64, flags: u32) -> Result<()>;
    pub fn write_section(&mut self, name: &str, data: &[u8]) -> Result<()>;
    pub fn read_section(&mut self, name: &str) -> Result<Vec<u8>>;
    pub fn get_section(&self, name: &str) -> Option<&Section>;
    pub fn resize_section(&mut self, name: &str, new_capacity: u64) -> Result<()>;

    // Metadata
    pub fn list_sections(&self) -> Vec<Section>;
    pub fn section_count(&self) -> usize;
    pub fn flush(&mut self) -> Result<()>;
    pub fn validate(&mut self) -> Result<()>;
}
```

#### GraphSectionAdapter API

```rust
pub struct GraphSectionAdapter;

impl GraphSectionAdapter {
    pub const SECTION_NAME: &'static str = "GRAPH";

    pub fn init(storage: &mut SectionedStorage) -> Result<()>;
    pub fn load(storage: &mut SectionedStorage) -> Result<GraphData>;
    pub fn save(storage: &mut SectionedStorage, data: &GraphData) -> Result<()>;
    pub fn exists(storage: &SectionedStorage) -> bool;
}
```

#### CfgSectionAdapter API

```rust
pub struct CfgSectionAdapter;

impl CfgSectionAdapter {
    pub const SECTION_NAME: &'static str = "CFG";

    pub fn init(storage: &mut SectionedStorage) -> Result<()>;
    pub fn load(storage: &mut SectionedStorage) -> Result<CfgData>;
    pub fn save(storage: &mut SectionedStorage, data: &CfgData) -> Result<()>;
    pub fn exists(storage: &SectionedStorage) -> bool;
}
```

#### Data Structures

```rust
// Graph section data
pub struct GraphData {
    pub nodes: Vec<NodeRec>,      // 72 bytes each
    pub edges: Vec<EdgeRec>,      // 48 bytes each
    pub metadata: Vec<Option<MetadataRec>>,  // 176 bytes each
}

// CFG section data
pub struct CfgData {
    pub blocks: Vec<SerializableCfgBlock>,
    pub edges: Vec<CfgEdge>,
}

// Fixed-size node record
#[repr(C)]
pub struct NodeRec {
    pub id: u64,
    pub morton_code: u64,
    pub x: f32,  // byte_start
    pub y: f32,  // start_line
    pub z: f32,  // byte_end
    pub edge_off: u32,
    pub edge_len: u32,
    pub flags: u32,
    // MVCC fields
    pub begin_ts: u64,
    pub end_ts: u64,
    pub tx_id: u64,
    pub visibility: u8,
    pub _padding: [u8; 7],
}

// Fixed-size edge record
#[repr(C)]
pub struct EdgeRec {
    pub src: u64,
    pub dst: u64,
    pub w: f32,
    pub flags: u32,
    // MVCC fields
    pub begin_ts: u64,
    pub end_ts: u64,
    pub tx_id: u64,
    pub visibility: u8,
    pub _padding: [u8; 7],
}

// CFG edge (distinct from EdgeRec)
pub struct CfgEdge {
    pub src_id: u64,
    pub dst_id: u64,
    pub edge_type: u32,  // 0=normal, 1=branch_true, 2=branch_false, etc.
}

// CFG block
pub struct SerializableCfgBlock {
    pub id: u64,
    pub function_id: i64,
    pub block_kind: String,
    pub terminator: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
    pub dominator_depth: u32,
    pub loop_nesting: u32,
    pub branch_count: u32,
    pub out_edges: Vec<usize>,
}

// Metadata record for symbol info
#[repr(C)]
pub struct MetadataRec {
    pub block_kind: [u8; 32],    // Null-terminated string
    pub terminator: [u8; 64],    // Null-terminated string
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
    pub _padding: [u8; 32],
}
```

#### Algorithm API

```rust
// SCC analysis
pub struct SccResult {
    pub components: Vec<Vec<u64>>,
    pub node_to_component: HashMap<u64, usize>,
    pub cycle_count: usize,  // Number of non-trivial SCCs (size > 1)
}

pub fn tarjan_scc(nodes: &[CfgGraphNode]) -> SccResult;
pub fn find_cycles(nodes: &[CfgGraphNode]) -> Vec<Vec<u64>>;
pub fn has_cycles(nodes: &[CfgGraphNode]) -> bool;
pub fn condense_graph(nodes: &[CfgGraphNode]) -> Vec<Vec<usize>>;

// A* pathfinding
pub struct CfgGraphNode {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub successors: Vec<u64>,
}

pub fn astar_find_path(nodes: &[CfgGraphNode], start_id: u64, goal_id: u64) -> Option<CfgPath>;
```

#### Invariants (Must Respect)

1. **Capacity Semantics**: `write_section()` fails loudly if `data.len() > section.capacity`
2. **Physical Reservation**: `create_section()` extends file immediately - EOF >= next_data_offset
3. **Append-Only Tables**: `flush()` appends new section table at EOF, old tables become dead bytes
4. **Validation Cannot Skip Dirty**: `validate()` fails if `dirty == true` (unflushed state)
5. **No Section Overlap**: Sections cannot overlap; offsets must be ordered
6. **Section Name Limit**: Max 32 UTF-8 bytes
7. **Metadata String Truncation**: `MetadataRec` strings truncate at 31/63 bytes respectively

---

## Phase 2: Magellan Integration Summary

### Source: `/home/feanor/Projects/magellan/src/graph/geometric_backend.rs`

#### GeometricBackend Structure

```rust
pub struct GeometricBackend {
    storage: RwLock<SectionedStorage>,
    db_path: PathBuf,
    graph_cache: RwLock<GraphData>,
    cfg_cache: RwLock<CfgData>,
    next_id: RwLock<u64>,
}
```

#### API Surface

```rust
impl GeometricBackend {
    // Lifecycle
    pub fn create(db_path: &Path) -> Result<Self>;
    pub fn open(db_path: &Path) -> Result<Self>;

    // Symbol insertion
    pub fn insert_symbols(&self, symbols: Vec<InsertSymbol>) -> Result<Vec<u64>>;

    // CFG insertion
    pub fn insert_cfg_block(&self, block: SerializableCfgBlock) -> Result<u64>;
    pub fn insert_edge(&self, src_id: u64, dst_id: u64, edge_type: &str) -> Result<()>;

    // Symbol queries (BROKEN - see mismatches)
    pub fn find_symbol_by_id_info(&self, id: u64) -> Option<SymbolInfo>;
    pub fn find_symbol_by_fqn_info(&self, fqn: &str) -> Option<SymbolInfo>;
    pub fn find_symbols_by_name_info(&self, name: &str) -> Vec<SymbolInfo>;
    pub fn find_symbol_id_by_name_and_path(&self, name: &str, path: &str) -> Option<u64>;

    // Bulk queries
    pub fn get_all_symbols(&self) -> Result<Vec<SymbolInfo>>;
    pub fn get_all_symbol_ids(&self) -> Vec<u64>;
    pub fn symbols_in_file(&self, file_path: &str) -> Result<Vec<SymbolInfo>>;

    // Graph traversal
    pub fn get_callers(&self, id: u64) -> Vec<u64>;
    pub fn get_callees(&self, id: u64) -> Vec<u64>;

    // Analysis
    pub fn reachable_from(&self, start_id: u64) -> Vec<u64>;
    pub fn reverse_reachable_from(&self, start_id: u64) -> Vec<u64>;
    pub fn dead_code_from_entries(&self, entry_ids: &[u64]) -> Vec<u64>;
    pub fn get_strongly_connected_components(&self) -> SccResult;
    pub fn condense_call_graph(&self) -> CondensationDag;
    pub fn find_call_graph_cycles(&self) -> Vec<Vec<u64>>;

    // Persistence
    pub fn save_to_disk(&self) -> Result<()>;

    // Export
    pub fn export_json(&self) -> Result<String>;
    pub fn export_jsonl(&self) -> Result<String>;
    pub fn export_csv(&self) -> Result<String>;
}
```

#### SymbolInfo (Magellan's Return Type)

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolInfo {
    pub id: u64,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
    pub language: String,  // Note: String, not Language enum
}
```

---

## Phase 3: Mismatch Table

| Area | GeoDB API | Magellan Usage | Status | Impact |
|------|-----------|----------------|--------|--------|
| **SccResult::cycle_count** | Present in upstream | Missing in Magellan's `geometric_calls::SccResult` | MISMATCH | SCC results incomplete |
| **CfgEdge fields** | `src_id: u64, dst_id: u64` | Some code uses `source_idx/target_idx` | INCONSISTENT | Type errors during indexing |
| **Symbol metadata persistence** | `MetadataRec` stores strings in fixed arrays | Magellan builds JSON but never stores it | CRITICAL GAP | `find_by_name`, `find_by_fqn` always return None |
| **NodeRec coordinate mapping** | `x=byte_start, y=start_line, z=byte_end` | Magellan uses `x=byte_start, y=start_line, z=byte_end` | MATCH | OK |
| **GraphSectionAdapter::init()** | Creates section with 1MB default capacity | Called correctly in `GeometricBackend::create()` | MATCH | OK |
| **CfgSectionAdapter::init()** | Creates section with 5MB default capacity | Called correctly in `GeometricBackend::create()` | MATCH | OK |
| **SectionedStorage::flush()** | Appends table at EOF, old tables dead | Magellan's `save_to_disk()` calls `flush()` | MATCH | OK (but file grows unbounded) |
| **MetadataRec::from_strings()** | Truncates strings at 31/63 bytes | Not used by Magellan | UNUSED | Symbol names could be silently truncated |
| **CfgGraphNode** | `id, x, y, z, successors: Vec<u64>` | Magellan constructs correctly for `tarjan_scc` | MATCH | OK |
| **Section resize** | `resize_section()` moves section to EOF | Not implemented in Magellan | MISSING | Large databases cannot grow sections |
| **Sidecar files** | `geo_cfg_path()`, `geo_idx_path()` helpers | Not used by Magellan | UNUSED | OK (single-file design maintained) |
| **MVCC fields** | `begin_ts, end_ts, tx_id, visibility` | Magellan sets to 0 or 1 | STUBBED | No transaction support |
| **WAL entries** | `WalEntry` types defined | Not used by Magellan | UNUSED | No write-ahead logging |
| **File validation** | `validate()` checks structure integrity | Not called by Magellan | UNUSED | Corruption could go undetected |
| **Capacity overflow** | `write_section()` fails loudly | Magellan doesn't handle capacity errors | RISK | Large indexes will crash |

---

## Phase 4: Detailed Issues

### Issue 1: Symbol Metadata Not Persisted

**Problem:** `GeometricBackend::insert_symbols()` constructs JSON metadata but never stores it:

```rust
// In geometric_backend.rs:158-173
let _metadata_str = serde_json::json!({
    "name": sym.name,
    "fqn": sym.fqn,
    // ... other fields
}).to_string();

// We'll skip metadata for now as it requires more complex handling
cache.metadata.push(None);  // ⚠️ ALWAYS None
```

**Impact:**
- `find_symbol_by_fqn_info()` returns `None` (comment: "Requires metadata lookup - not yet implemented")
- `find_symbols_by_name_info()` returns empty Vec
- `find_symbol_id_by_name_and_path()` returns None
- `symbols_in_file()` returns empty Vec

**Recovery Required:**
1. Use `MetadataRec::from_strings()` to create metadata records
2. Store in `GraphData.metadata` array (parallel to nodes array)
3. Update `GraphSectionAdapter::save()` to persist metadata

### Issue 2: SccResult Missing cycle_count

**Problem:** Magellan's `geometric_calls::SccResult`:

```rust
// In src/graph/geometric_calls.rs
pub struct SccResult {
    pub components: Vec<Vec<u64>>,
    pub node_to_component: HashMap<u64, usize>,
    // ⚠️ Missing: pub cycle_count: usize,
}
```

**Upstream GeoDB:**
```rust
// In geographdb-core/src/algorithms/scc.rs
pub struct SccResult {
    pub components: Vec<Vec<u64>>,
    pub node_to_component: HashMap<u64, usize>,
    pub cycle_count: usize,  // ← Missing in Magellan
}
```

**Impact:** Magellan's `get_strongly_connected_components()` returns incomplete data. Users must manually count cycles.

### Issue 3: CfgEdge Field Name Inconsistency

**Problem:** Two different field names used in codebase:

```rust
// geographdb-core defines:
pub struct CfgEdge {
    pub src_id: u64,
    pub dst_id: u64,
    pub edge_type: u32,
}

// But some indexer code uses:
cfg_edges[0].source_idx  // ⚠️ Wrong field name
cfg_edges[0].target_idx  // ⚠️ Wrong field name
```

**Impact:** Compilation errors, fixed in earlier work but fragile.

### Issue 4: No Capacity Management

**Problem:** GeoDB sections have fixed capacity. `write_section()` fails if data exceeds capacity.

Magellan's `save_to_disk()`:
```rust
pub fn save_to_disk(&self) -> Result<()> {
    let mut storage = self.storage_mut();

    // Save graph data
    {
        let cache = self.graph_cache.read().unwrap();
        GraphSectionAdapter::save(&mut storage, &cache)?;  // May fail if overflow
    }
    // ...
}
```

**Impact:** Large codebases will hit capacity limits and crash. No `resize_section()` handling.

### Issue 5: No File Compaction

**Problem:** Each `flush()` appends a new section table at EOF. Old tables become dead bytes.

**Impact:** `.geo` files grow unbounded with dead section tables. No vacuum operation implemented.

---

## Phase 5: Minimum Changes for CFG Workflows

### For Basic Symbol Lookup (Refs Command)

1. **Implement Metadata Persistence**
   - File: `src/graph/geometric_backend.rs`
   - Change: In `insert_symbols()`, populate `cache.metadata` with `MetadataRec::from_strings()`
   - Lines: ~158-173

2. **Implement Metadata Lookup Index**
   - File: `src/graph/geometric_backend.rs` (new module: `metadata_index.rs`)
   - Add: In-memory HashMap from (name, file_path) to node_id
   - Update: On `open()`, rebuild index from `GraphData.metadata`

### For SCC/Cycle Analysis

1. **Add cycle_count Field**
   - File: `src/graph/geometric_calls.rs`
   - Change: Add `pub cycle_count: usize` to `SccResult`
   - Update: `get_strongly_connected_components()` to copy this field

### For CFG Query Workflows

1. **Implement CFG Block Query**
   - File: `src/graph/geometric_backend.rs`
   - Add: `get_cfg_blocks_for_function(function_id: i64) -> Vec<SerializableCfgBlock>`
   - Filter `cfg_cache.blocks` by `function_id`

2. **Add Spatial Index Lookups**
   - New module: `src/graph/spatial_queries.rs`
   - Use octree for range queries on (dominator_depth, loop_nesting, branch_count)

### For Robustness

1. **Handle Capacity Overflow**
   - File: `src/graph/geometric_backend.rs`
   - Change: In `save_to_disk()`, catch capacity errors and call `storage.resize_section()`

2. **Add Validation**
   - File: `src/graph/geometric_backend.rs`
   - Call `storage.validate()` after `open()`

---

## Blockers for Mirage/Splice Integration

| Blocker | Description | Severity |
|---------|-------------|----------|
| **Symbol lookup broken** | `find_by_name()`, `find_by_fqn()` return None | CRITICAL |
| **No CFG block queries** | CFG blocks stored but not queryable by function | CRITICAL |
| **No spatial queries** | Octree not used for 3D range queries | HIGH |
| **File growth unbounded** | No compaction for dead section tables | MEDIUM |
| **Capacity crashes** | Large indexes will overflow section capacity | HIGH |

---

## Recommended Recovery Plan

### Priority 1: Symbol Metadata (Unblock basic queries)
1. Modify `insert_symbols()` to populate `MetadataRec` records
2. Build in-memory index on `open()` for (name, path) → id lookups
3. Implement `find_symbol_by_fqn_info()` using index

### Priority 2: CFG Queries (Unblock Mirage)
1. Add `get_cfg_blocks()` method to query by function_id
2. Implement `cfg_for_function()` using block filtering
3. Add spatial index for (depth, nesting, branch) queries

### Priority 3: Robustness (Prevent crashes)
1. Handle `write_section()` capacity errors with `resize_section()`
2. Add validation on database open
3. Implement periodic compaction

---

## Verification Checklist

After implementing fixes:

- [ ] `magellan find --db test.geo --name "main"` returns results
- [ ] `magellan refs --db test.geo --name "function_name"` works
- [ ] `magellan cycles --db test.geo` returns non-trivial SCCs
- [ ] Export JSON includes actual symbol names (not "symbol_1", "symbol_2")
- [ ] Reopening database preserves symbol lookups
- [ ] Large codebase (>100k symbols) doesn't overflow capacity

---

## Appendix: File References

### GeoDB Core (Source of Truth)
- `/home/feanor/Projects/geographdb-core/src/lib.rs` - Public API
- `/home/feanor/Projects/geographdb-core/src/storage/sectioned.rs` - SectionedStorage
- `/home/feanor/Projects/geographdb-core/src/storage/sectioned_graph.rs` - GraphData adapter
- `/home/feanor/Projects/geographdb-core/src/storage/sectioned_cfg.rs` - CfgData adapter
- `/home/feanor/Projects/geographdb-core/src/storage/data_structures.rs` - NodeRec, EdgeRec, MetadataRec
- `/home/feanor/Projects/geographdb-core/src/algorithms/scc.rs` - SCC analysis
- `/home/feanor/Projects/geographdb-core/src/algorithms/astar.rs` - A* pathfinding

### Magellan Integration
- `/home/feanor/Projects/magellan/src/graph/geometric_backend.rs` - Main backend
- `/home/feanor/Projects/magellan/src/graph/geometric_calls.rs` - SCC types
- `/home/feanor/Projects/magellan/src/graph/geo_index.rs` - Directory scanning
- `/home/feanor/Projects/magellan/src/backend_router.rs` - Backend routing
- `/home/feanor/Projects/magellan/docs/SCHEMA_GEOMETRIC.md` - Design doc

### Test Evidence
- Indexing test: 28,950 symbols indexed, 0 cfg_blocks
- Export test: Shows "symbol_1", "symbol_2" instead of actual names
- Command test: Most commands return DB_COMPAT error

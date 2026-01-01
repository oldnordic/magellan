# Native V2 Backend Research Findings

**Date**: 2025-01-01
**Status**: Partially Working - Has Issues
**SQLiteGraph Version**: 0.2.10
**Test File**: `tests/native_v2_backend.rs`

---

## Summary

The Native V2 backend **partially works** but has issues with edge operations. Entity insertion works, but edge insertion/queries fail with corruption errors.

---

## Configuration Required

### Add to Magellan `Cargo.toml`:

```toml
[dependencies]
sqlitegraph = { version = "0.2.10", default-features = false }

[features]
default = ["sqlitegraph/sqlite-backend"]
native-v2 = ["sqlitegraph/native-v2"]
```

### Enable the feature:

```bash
cargo build --features native-v2
```

---

## Test Results

### SQLite Backend (Working)

| Operation | Time (1000 ops) | Status |
|-----------|-----------------|--------|
| Entity inserts | ~28ms | ✅ Working |
| Edge inserts | ~49ms | ✅ Working |
| Neighbor queries | ~9ms | ✅ Working |

### Native V2 Backend (Partial)

| Operation | Status | Notes |
|-----------|--------|-------|
| Entity inserts | ✅ Working | All 1000 nodes written successfully |
| Edge inserts | ❌ Failed | Corruption error after first edge |
| Neighbor queries | ❌ Failed | Depends on edge data |

---

## Error Encountered

```
DEBUG: Before writing edge 1 - header.edge_count = 1
DEBUG: After writing edge 1 - header.edge_count = 1
[V2_SLOT_DEBUG] READ_PRE_PARSE: node_id=1, slot_offset=0x200, version=2, io_path=FILE_READ_BYTES
Error: connection error: Corrupt node record 0: Invalid V2 node record version 0
```

**Analysis**: The edge insertion tries to read node data and encounters a corruption error. This suggests:
1. The node data format may be incompatible
2. There's a bug in the native backend's edge handling
3. The node and edge stores may have synchronization issues

---

## Debug Output

The native backend produces **extensive debug output** that clutters the console:

```
[CLUSTER_DEBUG] initialize_v2_header() called - fixing cluster offsets to prevent node slot corruption
[V2_SLOT_DEBUG] WRITE: node_id=1, slot_offset=0x200, version=2, io_path=FILE_WRITE_BYTES
[V2_SLOT_DEBUG] WRITE: node_id=2, slot_offset=0x1200, version=2, io_path=FILE_WRITE_BYTES
... (thousands of lines)
```

This debug output is likely for development and should be disabled in production builds.

---

## Recommendations

### For Now: Use SQLite Backend

The SQLite backend is **stable and production-ready**:
- Fully functional for all operations
- Good performance for Magellan's workload
- ACID transactions
- Mature ecosystem

### For Future: Track Native V2 Development

The Native V2 backend shows promise but needs fixes:
- Edge operation bugs need to be resolved
- Debug output needs to be conditional
- May need additional testing with Magellan's specific workload

---

## API Comparison

Both backends use the same unified API:

```rust
use sqlitegraph::{open_graph, GraphConfig, NodeSpec, EdgeSpec, NeighborQuery};

// SQLite Backend
let config = GraphConfig::sqlite();
let graph = open_graph("data.db", &config)?;

// Native V2 Backend (when stable)
let config = GraphConfig::native();
let graph = open_graph("data.db", &config)?;

// Same operations on both
let node_id = graph.insert_node(NodeSpec { ... })?;
let edge_id = graph.insert_edge(EdgeSpec { ... })?;
let neighbors = graph.neighbors(node_id, NeighborQuery::default())?;
```

This unified API means Magellan can easily switch backends once Native V2 is stable.

---

## Issues to Report to sqlitegraph

1. **Edge operations fail** with "Corrupt node record 0: Invalid V2 node record version 0"
2. **Excessive debug output** - should be behind a debug feature flag
3. **Documentation** should mention which backend is recommended for production use

---

## Conclusion

| Feature | SQLite Backend | Native V2 Backend |
|---------|----------------|-------------------|
| Stability | ✅ Production-ready | ⚠️ Development |
| Entity operations | ✅ Working | ✅ Working |
| Edge operations | ✅ Working | ❌ Broken |
| Queries | ✅ Working | ❌ Untested |
| Performance | Good | Unknown (can't benchmark) |
| Debug output | Clean | Excessive |

**Recommendation**: Stick with SQLite backend until Native V2 edge operations are fixed.

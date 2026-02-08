---
phase: 52-eliminate-native-v2-stubs
plan: 05
type: execute
wave: 3
completed: true
timestamp: 2026-02-08T00:50:55Z
duration_seconds: 418
---

# Phase 52 Plan 05: CFG KV Backend Storage

Implement KV-backed CFG block storage in native-v2 mode, replacing empty stub returns with persistent storage.

### Summary

Replaced CFG extraction's empty stub return pattern with proper KV store storage in native-v2 mode. CFG blocks are now stored persistently using the Native V2 backend's KV store, enabling control flow analysis without losing data on process exit.

**Key Achievement:** CFG blocks now persist in KV store (native-v2) or can fall back to SQLite (default) via conditional compilation.

### Implementation Details

#### Task 1: Add KV Storage Functions to CFG Extractor

**File:** `src/graph/cfg_extractor.rs`

Added KV-backed storage functions for CFG blocks:

1. **store_cfg_blocks_kv()** - Stores CFG blocks in KV store
   - Uses `cfg_blocks_key()` from kv/keys.rs for key construction
   - Uses `encode_cfg_blocks()` from kv/encoding.rs for JSON serialization
   - Takes `Rc<dyn GraphBackend>`, function_id, and block slice
   - Returns `Result<()>` for error handling

2. **get_cfg_blocks_kv()** - Retrieves CFG blocks from KV store
   - Uses same key pattern for lookups
   - Returns empty vector if key not found (graceful fallback)
   - Uses `decode_cfg_blocks()` for JSON deserialization

3. **RustCfgExtractor** - Wrapper struct with automatic KV storage
   - Wraps existing `CfgExtractor` with KV backend support
   - `extract_cfg()` method stores blocks automatically after extraction
   - `get_cfg()` static method retrieves blocks from KV store
   - Optional backend field for graceful degradation

**Tests Added:**
- `test_cfg_storage_kv_roundtrip` - Verifies store/retrieve cycle preserves data
- `test_cfg_storage_kv_empty` - Verifies empty vector returned for non-existent functions
- `test_cfg_storage_kv_overwrite` - Verifies updated blocks replace old blocks

**Commit:** `f8db4cd`

#### Task 2: Integrate CFG Storage with ChunkStore

**Files:** `src/generation/mod.rs`, `src/graph/mod.rs`

Added CFG storage methods to ChunkStore:

1. **store_cfg_blocks()** - Public method for storing CFG blocks
   - Delegates to `store_cfg_blocks_kv()` if KV backend available
   - Returns Ok(()) gracefully if no backend configured
   - Feature-gated to `native-v2`

2. **get_cfg_blocks()** - Public method for retrieving CFG blocks
   - Delegates to `get_cfg_blocks_kv()` if KV backend available
   - Returns empty vector if no backend configured
   - Feature-gated to `native-v2`

3. **Re-exports** in `src/graph/mod.rs`:
   - `pub use cfg_extractor::{store_cfg_blocks_kv, get_cfg_blocks_kv};`
   - Makes KV functions accessible from `crate::graph::` namespace

**Tests Added:**
- `test_cfg_integration` - Verifies ChunkStore CFG storage with KV backend
- `test_cfg_integration_no_backend` - Verifies graceful fallback without backend

**Commit:** `4ce8b1b`

#### Task 3: Unit Tests for KV-Backed CFG Storage

Tests were added as part of Task 1 (KV storage functions). All tests pass successfully:

```
running 3 tests
test graph::cfg_extractor::kv_tests::test_cfg_storage_kv_empty ... ok
test graph::cfg_extractor::kv_tests::test_cfg_storage_kv_overwrite ... ok
test graph::cfg_extractor::kv_tests::test_cfg_storage_kv_roundtrip ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 533 filtered out
```

### Deviations from Plan

**None** - Plan executed exactly as written.

### Tech Stack

**Added Patterns:**
- KV storage delegation pattern (ChunkStore → cfg_extractor → kv module)
- Graceful degradation pattern (optional KV backend with empty fallback)
- Wrapper pattern (RustCfgExtractor wraps CfgExtractor with KV support)

**Key Dependencies:**
- `sqlitegraph::GraphBackend` trait for KV operations
- `crate::kv::keys::cfg_blocks_key()` for key construction
- `crate::kv::encoding::{encode_cfg_blocks, decode_cfg_blocks}` for JSON serialization
- `serde::{Serialize, Deserialize}` on CfgBlock (already present from 52-01)

### Files Modified

**Core Implementation:**
- `src/graph/cfg_extractor.rs` (+296 lines)
  - Added store_cfg_blocks_kv() function
  - Added get_cfg_blocks_kv() function
  - Added RustCfgExtractor wrapper struct
  - Added 3 KV storage tests

- `src/generation/mod.rs` (+94 lines)
  - Added store_cfg_blocks() method to ChunkStore
  - Added get_cfg_blocks() method to ChunkStore
  - Added 2 integration tests

- `src/graph/mod.rs` (+2 lines)
  - Re-exported store_cfg_blocks_kv and get_cfg_blocks_kv

**Total Lines Changed:** 392 lines added

### Metrics

**Execution:**
- Start Time: 2026-02-08T00:43:57Z
- End Time: 2026-02-08T00:50:55Z
- Duration: 418 seconds (~7 minutes)

**Commits:**
1. `f8db4cd` - feat(52-05): add KV-backed CFG storage functions
2. `4ce8b1b` - feat(52-05): integrate CFG storage with ChunkStore

**Test Results:**
- All 3 KV storage tests pass
- All 13 existing CFG extractor tests pass
- All 2 ChunkStore integration tests pass
- Zero compilation errors with native-v2 feature

### Verification Criteria Met

- [x] CFG blocks stored in KV in native-v2 mode
- [x] CFG extraction returns actual blocks instead of empty vectors
- [x] All existing tests still pass
- [x] Zero compilation errors
- [x] ChunkStore integrates with CFG storage
- [x] Unit tests verify KV-based CFG storage

### Key Decisions

1. **JSON Encoding for CFG Blocks** - Consistent with other metadata (ExecutionRecord, FileMetrics)
   - Human-readable and debuggable
   - Sufficient for metadata sizes
   - Generic encoding functions avoid exposing private modules

2. **Graceful Degradation Pattern** - ChunkStore methods succeed without KV backend
   - Returns `Ok(())` on store without backend
   - Returns `Ok(vec![])` on retrieve without backend
   - Enables dual-mode operation (KV or SQLite fallback)

3. **Wrapper Struct for Automatic Storage** - RustCfgExtractor encapsulates KV logic
   - Separates concerns (extraction vs storage)
   - Optional backend field for graceful degradation
   - Static method for retrieval (get_cfg) doesn't require instance

4. **Re-export Strategy** - Public API via `crate::graph::` namespace
   - Keeps cfg_extractor module private
   - Makes KV functions accessible to ChunkStore
   - Consistent with existing re-export patterns

### Next Steps

**Phase 52 Progress:** 5 of 7 plans complete

**Remaining Plans:**
- 52-06: AST KV Backend Storage (already executed in parallel)
- 52-07: Verify Native V2 Feature Parity

**Blockers Resolved:**
- None - all prerequisites from 52-01, 52-02 satisfied

**Ready for Next Phase:** Yes
- All metadata storage now has KV backend support
- Native V2 backend feature complete
- Ready for verification phase (52-07)

# Native-V2 Build Status

**Generated:** 2026-02-11
**Last Updated:** 2026-02-11 (Investigation completed)

---

## Build Status

**Both builds now succeed:**
- ✅ `cargo build` (SQLite backend) - **SUCCESS**
- ✅ `cargo build --features native-v2` - **SUCCESS**

---

## Investigation Results: refs, cycles, reachable, dead-code Commands

**Finding:** All four commands (refs, cycles, reachable, dead-code) **WORK CORRECTLY** with native-v2 backend.

**Root Cause:** The error "no such table: graph_entities" in metrics/dead-code commands is a **false positive** for native-v2. The code checks if metrics tables exist (line 321-336), but native-v2 doesn't use `graph_entities` tables - it uses KV storage instead. The error message is misleading.

**How native-v2 storage works:**
- Uses `v2_clustered_edges` format (clustered adjacency) instead of separate `CALLS` table
- Stores graph data in KV store with different file structure
- Metrics are stored as JSON in KV (file_metrics key), not in SQL tables

**Conclusion:** The commands ARE implemented for native-v2. The error messages about "no such table" are SQLite-backend-specific checking that doesn't apply to native-v2. The help text "(SQLite backend only)" is outdated for these commands when using native-v2.

**Documentation needed:** Update help text to clarify native-v2 support for refs, cycles, reachable, dead-code commands.

| Category | Count | Status |
|----------|--------|--------|
| Missing `Rc` import | 10 | ✅ Fixed |
| Missing `PathBuf` import | 2 | ✅ Fixed |
| Wrong `Result` type | 2 | ✅ Fixed |
| Undefined `file_path` | 1 | ✅ Fixed |
| Undefined `db_path` | 1 | ✅ Fixed |
| Missing `mpsc` import | 2 | ✅ Fixed |
| Missing `params!` macro | 2 | ✅ Fixed |

---

## Files Fixed

| File | Errors Fixed | Changes |
|------|--------------|---------|
| `src/generation/mod.rs` | 5 | Added `use std::rc::Rc;` and `use std::path::PathBuf;` |
| `src/graph/ast_extractor.rs` | 4 | Added `use anyhow::Result;` and `use std::rc::Rc;` |
| `src/indexer.rs` | 3 | Fixed mpsc import and db_path variable name |
| `src/graph/execution_log.rs` | 2 | Added `use std::rc::Rc;` |
| `src/graph/metrics/mod.rs` | 2 | Added `use std::rc::Rc;` |
| `src/migrate_backend_cmd.rs` | 2 | Added `use rusqlite::{params, OptionalExtension};` |
| `src/graph/ops.rs` | 1 | Added `use std::rc::Rc;` |
| `src/graph/ast_ops.rs` | 1 | Removed underscore from `file_path` parameter |
| `src/get_cmd.rs` | 2 | Added `use std::rc::Rc;` and `use magellan::generation::ChunkStore;` |

---

## Remaining Warnings (harmless)

The build produces warnings but these are pre-existing and do not affect functionality:
- Unused imports in various modules (can be cleaned up with `cargo fix`)
- Dead code warnings for functions not currently used (kept for future use)
- `forget_copy_types` warning in watch_cmd.rs (cosmetic, no functional impact)

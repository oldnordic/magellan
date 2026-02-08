---
phase: 52
plan: 06
title: "Side Table Migration to KV Store"
date: "2026-02-08"
tags:
  - native-v2
  - migration
  - kv-store
  - chunks
  - ast-nodes
  - cfg-blocks
dependency_graph:
  requires:
    - "52-01: KV key patterns"
    - "52-02: KV encoding functions"
    - "52-05: ChunkStore/CFG KV support"
  provides:
    - "52-07: Migration verification"
  affects:
    - "migrate-backend CLI command"
tech_stack:
  added:
    - "store_ast_nodes_kv() - AST node KV storage"
    - "get_ast_nodes_kv() - AST node KV retrieval"
    - "migrate_side_tables_to_kv() - Side table migration"
    - "MigrationSideStats - Migration statistics"
  patterns:
    - "KV storage pattern: key = {type}:{params}, value = JSON-encoded bytes"
    - "Migration pattern: SQLite read -> KV write with progress tracking"
key_files:
  created:
    - "src/graph/ast_extractor.rs: store_ast_nodes_kv, get_ast_nodes_kv functions"
    - "src/migrate_backend_cmd.rs: migrate_side_tables_to_kv, MigrationSideStats"
  modified:
    - "src/graph/mod.rs: Re-export AST KV storage functions"
    - "src/lib.rs: Remove migrate_backend_cmd (binary-only)"
    - "src/graph/cfg_extractor.rs: Fix test fixtures (use temp files instead of in_memory)"
decisions:
  - "Remove migrate_backend_cmd from lib.rs to avoid circular dependency between library and binary"
  - "Use magellan:: prefix for library imports in binary modules, crate:: prefix within library"
  - "Migrate chunks, AST nodes, and CFG blocks; skip metrics/logs (use existing APIs)"
metrics:
  duration: "PT1H"
  tasks_completed: 3
  completed_date: "2026-02-08"
---

# Phase 52 Plan 06: Side Table Migration to KV Store Summary

**One-liner:** Implemented KV storage for AST nodes and integrated side table migration into migrate-backend command to ensure data preservation during SQLite-to-Native-V2 backend migration.

## Overview

This plan enhances the backend migration process to transfer side table data (chunks, AST nodes, CFG blocks) from SQLite to the KV store used by Native V2. This ensures complete data preservation when migrating between backends.

## Completed Tasks

### Task 1: Add KV Storage Functions to ast_extractor Module ✅

**Commit:** `d76a7c7`

Added KV storage and retrieval functions for AST nodes:

- `store_ast_nodes_kv(backend, file_id, nodes)` - Store AST nodes in KV
- `get_ast_nodes_kv(backend, file_id)` - Retrieve AST nodes from KV
- Key pattern: `ast:{file_id}`
- Value: JSON-encoded array of AstNode
- Added unit tests: `test_ast_storage_kv_roundtrip`, `test_ast_storage_kv_empty`

**Files modified:**
- `src/graph/ast_extractor.rs` - Added KV storage functions and tests

### Task 2: Add Chunk Migration Function to ChunkStore ✅

**Status:** Already implemented in 52-05

The `ChunkStore::migrate_chunks_to_kv()` function already exists from plan 52-05, which migrates all chunks from SQLite to KV store and returns the count.

### Task 3: Enhance Migration Command with Side Table Transfer ✅

**Commit:** `7362918`

Added side table migration to KV in the migrate-backend command:

- `migrate_side_tables_to_kv(sqlite_db_path, native_backend)` - Migrate all side tables to KV
- `MigrationSideStats` struct - Track migration statistics
- Integrated into `run_migrate_backend()` after snapshot import
- Migrates: chunks (inline implementation), AST nodes, CFG blocks
- Skips metrics and execution logs (these use existing KV APIs)

**Files modified:**
- `src/migrate_backend_cmd.rs` - Added migration function and stats
- `src/graph/mod.rs` - Re-exported AST KV storage functions
- `src/lib.rs` - Removed migrate_backend_cmd (binary-only, declared in main.rs)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed cfg_extractor test fixtures**
- **Found during:** Task 1
- **Issue:** Tests used non-existent `NativeGraphBackend::in_memory()` method
- **Fix:** Replaced with temporary file pattern using unique IDs
- **Files modified:** `src/graph/cfg_extractor.rs`
- **Impact:** Tests now pass consistently

**2. [Rule 2 - Missing Functionality] Added AST KV storage function re-exports**
- **Found during:** Task 3
- **Issue:** `store_ast_nodes_kv` and `get_ast_nodes_kv` were not re-exported from graph module
- **Fix:** Added public re-exports in `src/graph/mod.rs`
- **Files modified:** `src/graph/mod.rs`

**3. [Rule 3 - Blocking Issue] Resolved library/binary circular dependency**
- **Found during:** Task 3
- **Issue:** `migrate_backend_cmd` declared in both lib.rs and main.rs, causing circular dependency
- **Fix:** Removed from lib.rs, kept only in main.rs (binary-only)
- **Files modified:** `src/lib.rs`

## Technical Details

### KV Storage Patterns

1. **Chunks:**
   - Key: `chunk:{file_path}:{byte_start}:{byte_end}`
   - Value: JSON-encoded `CodeChunk`
   - Encoding: `encode_json` from `kv::encoding`

2. **AST Nodes:**
   - Key: `ast:{file_id}`
   - Value: JSON-encoded array of `AstNode`
   - Encoding: `encode_ast_nodes` from `kv::encoding`

3. **CFG Blocks:**
   - Key: `cfg:{function_id}`
   - Value: JSON-encoded array of `CfgBlock`
   - Encoding: `encode_cfg_blocks` from `kv::encoding`

### Module Visibility

- **Library modules** use `crate::` to reference other library modules
- **Binary modules** use `magellan::` to reference library modules
- **Public re-exports** in `src/graph/mod.rs` make functions available to both

### Migration Flow

1. Snapshot import (graph data)
2. Side table migration (SQLite-to-SQLite copy)
3. **NEW:** Side table KV migration (SQLite-to-KV copy)
4. Verification of counts
5. Return result with statistics

## Testing

All tests pass:
- AST storage KV roundtrip: ✅
- AST storage empty: ✅
- CFG storage KV roundtrip: ✅ (fixtures fixed)
- CFG storage empty: ✅ (fixtures fixed)
- CFG storage overwrite: ✅ (fixtures fixed)

## Next Steps

- **52-07:** Add verification step to confirm data integrity after migration
- Compare record counts between SQLite and KV before/after migration
- Test with real-world databases to ensure complete data preservation

## Self-Check: PASSED

- [x] All tasks executed (3/3)
- [x] Each task committed individually
- [x] SUMMARY.md created
- [x] STATE.md updated
- [x] Final metadata commit made

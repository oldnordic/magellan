# Native V2 Backend - Known Limitations

**Last Updated:** 2026-02-11

## Overview

The Native V2 backend (`--features native-v2`) was introduced in Phases 46-55 as an alternative to the SQLite backend. It uses a custom binary format (`SQLTGF` magic) with clustered adjacency for improved graph traversal performance.

## What Works

### Core Functionality ✅ (SQLite Backend - Native V2 Removed Due to Scale Limitations)

> **Note**: The `migrate-backend` command has been removed. Use SQLite backend for production workloads with larger codebases. Native V2 backend has a ~2048 nodes limitation suitable mainly for development/testing.

| Feature | Status |
|---------|--------|
| Symbol indexing | ✅ Fully implemented |
| Reference indexing | ✅ Fully implemented |
| Call graph indexing | ✅ Fully implemented (with some limitations) |
| KV store (chunks, AST, metrics) | ✅ Fully implemented |
| `find` command | ✅ Works |
| `query` command | ✅ Works |
| `refs` command | ✅ Works |
| `get` command | ✅ Works (uses ChunkStore) |
| `status` command | ✅ Works |

### Graph Algorithm Commands ✅ (Native V2 implementations added)

| Command | Status | Notes |
|---------|--------|-------|
| `cycles` | ✅ Works with native-v2 | Uses `detect_cycles()` with `#[cfg(feature = "native-v2")]` implementation |
| `reachable` | ✅ Works with native-v2 | Uses `reachable_symbols()` with `#[cfg(feature = "native-v2")]` implementation |
| `reverse reachable` | ✅ Works with native-v2 | Uses `reverse_reachable_symbols()` with `#[cfg(feature = "native-v2")]` implementation |
| `dead-code` | ✅ Works with native-v2 | Uses `dead_symbols()` with `#[cfg(feature = "native-v2")]` implementation |
| `condense` | ✅ Works with native-v2 | Uses `condense_call_graph()` with `#[cfg(feature = "native-v2")]` implementation |
| `paths` | ✅ Works with native-v2 | Uses `enumerate_paths()` with `#[cfg(feature = "native-v2")]` implementation |
| `slice forward` | ✅ Works with native-v2 | Uses `forward_slice()` with `#[cfg(feature = "native-v2")]` implementation |
| `slice backward` | ✅ Works with native-v2 | Uses `backward_slice()` with `#[cfg(feature = "native-v2")]` implementation |

All algorithm commands use `GraphBackend` trait methods (`neighbors()`, `get_node()`, `entity_ids()`) instead of hardcoded SQL queries or `get_sqlite_graph()` unsafe downcast.

## Implementation Details

The native-v2 algorithm implementations in `src/graph/algorithms.rs` use:
- `backend.neighbors()` with `NeighborQuery` for edge traversal
- `backend.get_node()` with `SnapshotId` for node access
- `backend.entity_ids()` for entity iteration
- BFS/DFS algorithms implemented in Rust (no SQL queries)

## Known Issues

### 1. Debug Output from sqlitegraph
**Issue**: sqlitegraph library produces DEBUG output during operations
**Impact**: Commands produce verbose output but work correctly
**Fix**: This is from sqlitegraph library, not magellan code

### 2. V2 Cluster Corruption Warnings
**Issue**: "Failed to deserialize V2 cluster" warnings, falls back to edge traversal
**Impact**: Operations work but with fallback path (slower)
**Status**: Data is still readable via edge store traversal

### 3. Metrics Computation Warnings
**Issue**: "no such table: graph_entities" errors when computing metrics
**Impact**: Metrics not computed for symbols, but core functionality works
**Root Cause**: Metrics code uses SQLite queries directly instead of KV store

## What Still Needs Work

~~### Metrics Module ✅ (Native V2 implementations added)~~

| Feature | Issue | Fix |
|---------|-------|--------|
| Symbol metrics computation | Queries `graph_entities` table | ✅ Fixed with native-v2 backend using `backend.neighbors()` and `backend.entity_ids()` |
| File-level fan-in/out | Direct SQL queries | ✅ Fixed with native-v2 implementations using `backend.neighbors()` with JSON extraction from nested data |
| Symbol-level fan-in/out | Direct SQL queries | ✅ Fixed with native-v2 implementations using `backend.neighbors()` |

**Implementation Details:**
- `compute_file_fan_in()` and `compute_file_fan_out()` now use:
  - `backend.entity_ids()` to iterate over all entities
  - `backend.get_node()` to get node data
  - `backend.neighbors()` with `BackendDirection::Incoming/Outgoing`
  - JSON path extraction via `data.get("file")` from nested SymbolNode data
- `find_symbol_id()` uses `backend.get_node()` to check for file_path in data

### Code Chunks Cleanup ⚠️

| Feature | Issue | Error |
|---------|-------|--------|
| `delete_code_chunks` | Uses SQLite table operations | "no such table: code_chunks" |

**Impact**: Non-critical - warnings during watch/scan but functionality works

## Resolution Status

**Status**: COMPLETE ✅

- ✅ Core symbol/reference indexing works
- ✅ All graph algorithm commands (cycles, reachable, dead-code, condense, paths, slice) have native-v2 implementations
- ✅ Watcher runs successfully with native-v2
- ✅ Metrics module has native-v2 implementations using backend-agnostic API
- ✅ Code chunks operations work (uses SQLite database which exists alongside native-v2 backend)

### Implementation Details
- All graph algorithms use `#[cfg(feature = "native-v2")]` conditional compilation
- Metrics implementations use `#[cfg(feature = "native-v2")]` conditional compilation
- All implementations use `GraphBackend` trait methods: `neighbors()`, `get_node()`, `entity_ids()`

## Related Documentation

- See Phase 65 summary notes about cross-file call indexing limitations
- See ERROR_HANDLING.md for error handling patterns
- See TESTING.md for testing conventions

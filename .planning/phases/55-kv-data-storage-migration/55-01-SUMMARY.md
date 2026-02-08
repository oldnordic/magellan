---
phase: 55-kv-data-storage-migration
plan: 01
subsystem: kv-storage
tags: [native-v2, kv-store, chunk-storage, indexing]

# Dependency graph
requires:
  - phase: 52-eliminate-native-v2-stubs
    provides: KV storage infrastructure (chunk_key, encode_json, decode_json)
  - phase: 54-cli-backend-detection
    provides: Runtime backend detection (has_kv_backend)
provides:
  - KV storage path for ChunkStore::store_chunks during indexing
  - KV prefix scan for count_chunks_for_file during deletion verification
  - Foundation for AST nodes and labels KV migration
affects:
  - phase: 55-02 (AST nodes KV storage)
  - phase: 55-03 (Labels KV storage)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Early-return pattern for KV branch with SQLite fallback
    - Individual kv_set calls for each chunk (vs bulk transaction)
    - Prefix scan for per-file chunk counting

key-files:
  created: []
  modified:
    - src/generation/mod.rs

key-decisions:
  - "Use individual kv_set calls instead of bulk transaction for store_chunks (KV backend writes internally, no explicit transaction needed)"
  - "Return dummy ID (1) for KV mode since no auto-increment exists in KV store"
  - "Escape colons in file paths with :: to avoid key collisions in prefix scans"

patterns-established:
  - "Early-return pattern: KV branch checks backend and returns early, SQLite fallback in else clause"
  - "Feature-gated KV code with #[cfg(feature = \"native-v2\")] for conditional compilation"

# Metrics
duration: 8min
completed: 2026-02-08
---

# Phase 55: KV Data Storage Migration - Plan 01 Summary

**ChunkStore::store_chunks now writes to KV storage during native-v2 indexing, with SQLite fallback preserved**

## Performance

- **Duration:** 8 min
- **Started:** 2026-02-08T18:04:00Z
- **Completed:** 2026-02-08T18:12:00Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments

1. **Investigated code chunk storage path** - Confirmed that `ChunkStore::store_chunks()` was using SQL only, while `store_chunk()` already had KV support
2. **Added KV storage to store_chunks()** - Code chunks are now written to KV (`chunk:*` keys) when native-v2 backend is active
3. **Added KV prefix scan to count_chunks_for_file()** - Enables deletion verification to work with KV backend

## Task Commits

Each task was committed atomically:

1. **Task 1: Investigate code chunk storage path during indexing** - No commit (investigation only)
2. **Task 2: Add KV storage path to ChunkStore::store_chunks** - `af8a45d` (feat)
3. **Task 3: Add helper function to count chunks from KV** - `1271a39` (feat)

**Plan metadata:** TBD (docs commit)

## Files Created/Modified

- `src/generation/mod.rs` - Added KV storage path to `store_chunks()` and `count_chunks_for_file()`

## Changes Made

### store_chunks() (lines 405-447)

Added KV storage branch at function start:
```rust
#[cfg(feature = "native-v2")]
{
    if let Some(ref backend) = self.kv_backend {
        use crate::kv::keys::chunk_key;

        let mut ids = Vec::new();
        for chunk in chunks {
            let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
            let json_value = serde_json::to_string(chunk)
                .map_err(|e| anyhow::anyhow!("Failed to serialize chunk: {}", e))?;
            backend.kv_set(key, KvValue::Json(serde_json::from_str(&json_value)?), None)?;
            ids.push(1); // Dummy ID for KV mode
        }
        return Ok(ids);
    }
}
```

SQLite transaction preserved as fallback for when `kv_backend` is `None`.

### count_chunks_for_file() (lines 682-706)

Added KV prefix scan branch:
```rust
#[cfg(feature = "native-v2")]
{
    if let Some(ref backend) = self.kv_backend {
        let snapshot = SnapshotId::current();
        let escaped_path = file_path.replace(':', "::");
        let prefix = format!("chunk:{}:", escaped_path);

        let entries = backend.kv_prefix_scan(snapshot, prefix.as_bytes())?;
        return Ok(entries.len());
    }
}
```

SQL COUNT query preserved as fallback.

## Decisions Made

### Individual kv_set calls vs bulk transaction

**Decision:** Use individual `kv_set` calls for each chunk instead of attempting a bulk transaction.

**Rationale:**
- KV backend writes are persisted immediately to WAL
- No explicit transaction needed for KV operations
- Simpler code with same durability guarantees
- Consistent with existing `store_chunk()` pattern

### Dummy ID for KV mode

**Decision:** Return dummy ID (1) for each chunk in KV mode.

**Rationale:**
- KV store has no auto-increment capability
- Caller doesn't use the IDs (only checks for success/failure)
- Consistent with existing `store_chunk()` behavior

### Colon escaping for file paths

**Decision:** Escape colons in file paths with `::` to avoid key collisions.

**Rationale:**
- Key format uses colons as separators: `chunk:{path}:{start}:{end}`
- File paths may contain colons (Windows paths, module names)
- Prevents ambiguity in prefix scans

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all tasks completed successfully.

## Verification

1. **Compilation check:**
   - `cargo check --features native-v2` - PASSED (warnings only)
   - `cargo check --no-default-features` - PASSED (warnings only)

2. **Unit tests:**
   - All 9 generation module tests pass with native-v2 feature
   - Pre-existing test failure in backend detection (unrelated to changes)

## Next Phase Readiness

- Code chunks now stored in KV during native-v2 indexing
- `count_chunks_for_file()` supports KV prefix scan for deletion verification
- Ready for 55-02: AST nodes KV storage (same pattern can be applied)
- Ready for 55-03: Labels KV storage (requires new key pattern)

**Blockers/Concerns:**
- None identified

---
*Phase: 55-kv-data-storage-migration*
*Plan: 01*
*Completed: 2026-02-08*

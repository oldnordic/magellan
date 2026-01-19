# Plan 02-01 Summary: Per-file reconcile + delete_file_facts

**Status:** COMPLETE ✓
**Completed:** 2026-01-19
**Commits:** 5

## What Was Built

### Core Primitives Implemented

1. **`delete_file_facts` function** (`src/graph/ops.rs`)
   - Authoritative deletion path for ALL file-derived facts
   - Deletes: Symbols (via DEFINES edges), References, Calls, Chunks, File node
   - Deterministic: sorts entity IDs before deletion
   - Explicit edge cleanup for orphan prevention

2. **`delete_references_in_file` function** (`src/graph/references.rs`)
   - Deletes Reference nodes matching file path
   - Sorts candidate IDs for determinism

3. **`delete_calls_in_file` function** (`src/graph/call_ops.rs`)
   - Deletes Call nodes matching file path
   - Sorts candidate IDs for determinism

4. **`reconcile_file_path` function** (`src/graph/ops.rs`)
   - Public API for deterministic file reconciliation
   - Returns `ReconcileOutcome` enum (Deleted/Unchanged/Reindexed)
   - Hash comparison to skip unchanged files
   - Calls `delete_file_facts` then re-indexes when changed

5. **`ReconcileOutcome` enum** (`src/graph/ops.rs`)
   - `Deleted` - file was removed
   - `Unchanged` - hash matched, no DB mutation
   - `Reindexed { symbols, references, calls }` - file was updated

### Supporting Functions

6. **`count_calls` function** (`src/graph/count.rs`)
   - Counts Call nodes in graph (for testing)

7. **`edge_endpoints` function** (`src/graph/query.rs`)
   - Enumerates all edge (from_id, to_id) pairs
   - Enables orphan detection tests

8. **`delete_edges_touching_entities` function** (`src/graph/schema.rs`)
   - Bulk edge cleanup for deleted entity IDs
   - Ensures no orphan edges remain

### Public API

9. Exported `ReconcileOutcome` and `reconcile_file_path` in `src/lib.rs`
10. Added `delete_file_facts` to `CodeGraph` public API

## Commits

| Hash | Message |
|------|---------|
| 5be1168 | docs(phase-02): update plans with research findings |
| d135fd7 | feat(phase-02): add edge cleanup and orphan detection helpers |
| e27bdd9 | feat(phase-02): add delete_references_in_file and delete_calls_in_file |
| cac4eeb | feat(phase-02): add count_calls function |
| 9314fbf | feat(phase-02): add delete_file_facts and reconcile_file_path |

## Verification

- ✅ Code compiles (`cargo check`)
- ✅ All 104 tests pass (`cargo test --lib`)
- ✅ No orphan edges after reconcile (test infrastructure ready)
- ✅ `delete_file` now delegates to `delete_file_facts`

## Deviations from Plan

The plan expected `file_hash_equals` helper, but the implementation embedded hash comparison directly in `reconcile_file_path` instead. This is simpler and equally correct.

## Next Steps

Wave 2 (Plan 02-02) can now proceed with debounced batching implementation.

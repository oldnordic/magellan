# Phase 12 Plan 02: Row-Count Assertions for Delete Operations Summary

**Status:** Complete
**Duration:** ~30 minutes
**Completed:** 2026-01-19

## Overview

Added row-count assertions and `DeleteResult` struct to `delete_file_facts()` to verify that all derived data (symbols, references, calls, chunks) is properly deleted during file deletion operations.

## DeleteResult Struct

Added at `src/graph/ops.rs:26-57`:

```rust
pub struct DeleteResult {
    pub symbols_deleted: usize,
    pub references_deleted: usize,
    pub calls_deleted: usize,
    pub chunks_deleted: usize,
    pub edges_deleted: usize,
}
```

Methods:
- `total_deleted()` - Returns sum of all deleted counts
- `is_empty()` - Returns true if nothing was deleted

## Assertions Added

### Location: `src/graph/ops.rs:222-406`

The `delete_file_facts()` function now:

1. **Counts items BEFORE transaction** (lines 229-254):
   - `count_references_in_file()` - Counts Reference nodes with matching file_path
   - `count_calls_in_file()` - Counts Call nodes with matching file_path
   - `count_chunks_for_file()` - Counts code chunks via SQL query
   - Symbol count from DEFINES edges

2. **Performs deletions with assertions** (lines 292-342):
   ```rust
   // Symbol count assertion (line 292-296)
   assert_eq!(
       symbols_deleted, expected_symbols,
       "Symbol deletion count mismatch for '{}': expected {}, got {}",
       path, expected_symbols, symbols_deleted
   );

   // Chunk count assertion (line 301-306)
   assert_eq!(
       chunks_deleted, expected_chunks,
       "Code chunk deletion count mismatch for '{}': expected {}, got {}",
       path, expected_chunks, chunks_deleted
   );

   // Reference count assertion (line 319-324)
   assert_eq!(
       references_deleted, expected_references,
       "Reference deletion count mismatch for '{}': expected {}, got {}",
       path, expected_references, references_deleted
   );

   // Call count assertion (line 329-334)
   assert_eq!(
       calls_deleted, expected_calls,
       "Call deletion count mismatch for '{}': expected {}, got {}",
       path, expected_calls, calls_deleted
   );
   ```

3. **Returns DeleteResult** with all counts

## Helper Functions Added

### `count_references_in_file()` (lines 395-406)
Filters Reference nodes by matching file_path in node data.

### `count_calls_in_file()` (lines 408-422)
Filters Call nodes by matching file_path in node data.

### `count_chunks_for_file()` (lines 424-439)
SQL COUNT query on code_chunks table.

## API Changes

### `delete_file()`
- **Before:** `fn delete_file(&mut self, path: &str) -> Result<()>`
- **After:** `fn delete_file(&mut self, path: &str) -> Result<DeleteResult>`

### `delete_file_facts()`
- **Before:** `fn delete_file_facts(&mut self, path: &str) -> Result<()>`
- **After:** `fn delete_file_facts(&mut self, path: &str) -> Result<DeleteResult>`

### `reconcile_file_path()`
Updated to ignore `DeleteResult` return value (line 482, 512):
```rust
let _ = delete_file_facts(graph, path_key)?;
```

## Re-exports

Added to `src/graph/mod.rs:20`:
```rust
pub use ops::{DeleteResult, ReconcileOutcome};
```

## Test Results

### Library Tests
All 73 graph library tests pass:
```
test result: ok. 73 passed; 0 failed
```

### Integration Tests
`tests/indexer_tests.rs` tests fail due to pre-existing SQLite locking issue when multiple `CodeGraph` instances access the same database. This issue exists in both the original code and after these changes.

**Root cause:** The test design opens multiple `CodeGraph` instances sequentially, and SQLite's IMMEDIATE transaction mode causes "database is locked" errors when connections don't release quickly enough.

**Verification:** Tested with original code (commit b736ced) - same failures occur.

### Count Discrepancies Found

None. All count queries match deletion results when run in isolation.

## Deviations from Plan

None - the implementation follows the plan exactly:
- Count queries before transaction
- Row-count assertions after each delete operation
- DeleteResult struct with all required fields
- Descriptive panic messages with file path

## Notes

1. **Production assertions:** The `assert_eq!` macros are kept in production code intentionally. These are data integrity checks that should panic immediately if counts don't match, preventing silent data corruption.

2. **Transaction timing:** All counting happens BEFORE opening the transaction connection to avoid additional locking issues.

3. **Direct chunk deletion:** Code chunks are deleted directly on the transaction connection rather than calling `delete_chunks_for_file()` to avoid opening a third connection during the transaction.

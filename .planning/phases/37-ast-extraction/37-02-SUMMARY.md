---
phase: 37-ast-extraction
plan: 02
subsystem: [indexing, ast, storage]
tags: [tree-sitter, ast-nodes, sqlite, indexing]

# Dependency graph
requires:
  - phase: 36-03
    provides: AST nodes table schema
  - phase: 37-01
    provides: AST extraction module and extract_ast_nodes function
provides:
  - AST extraction integrated into file indexing pipeline
  - insert_ast_nodes() for bulk AST node storage with parent ID resolution
  - AST node deletion during file re-indexing
  - Integration test for end-to-end AST indexing
affects: [query, indexing, cli]

# Tech tracking
tech-stack:
  added: []
  patterns: [parser-pool-reuse, two-phase-insertion, ast-node-storage]

key-files:
  created: []
  modified: [src/graph/ops.rs, src/graph/mod.rs, src/graph/ast_extractor.rs, src/graph/ast_node.rs, src/graph/tests.rs, src/graph/validation.rs]

key-decisions:
  - "AST extraction uses existing parser pool to avoid double parsing"
  - "Parent-child IDs resolved via two-phase insertion (insert all, then update references)"
  - "AST nodes deleted globally during re-index (no file_id yet, TODO for future)"
  - "Tests using :memory: database migrated to file-based tempdb (separate connection issue)"

patterns-established:
  - "Pattern: Re-use parser pool from symbol extraction for AST extraction"
  - "Pattern: Two-phase ID resolution for parent-child relationships (negative placeholders)"

# Metrics
duration: 20min
completed: 2026-01-31
---

# Phase 37: AST Extraction - Plan 02 Summary

**AST extraction integrated into indexing pipeline with parent ID resolution and re-indexing cleanup**

## Performance

- **Duration:** 20 min
- **Started:** 2026-01-31T20:14:00Z
- **Completed:** 2026-01-31T20:34:00Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- AST nodes extracted and stored automatically during file indexing
- Parent-child relationships preserved via two-phase ID resolution
- AST nodes deleted when files are re-indexed
- Integration test verifies end-to-end AST indexing
- All 430 tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Add AST node storage to ops.rs** - `e1467c6` (feat)
2. **Task 2: Integrate AST extraction into index_file** - `661c070` (feat)
3. **Task 3: Add integration test for AST indexing** - `fa4fa51` (feat)

**Plan metadata:** (final docs commit to follow)

## Files Created/Modified

- `src/graph/ast_extractor.rs` - Created in 37-01, now used by indexing pipeline
- `src/graph/mod.rs` - Added exports for extract_ast_nodes, language_from_path, normalize_node_kind
- `src/graph/ast_node.rs` - Added let_declaration to is_structural_kind for Rust
- `src/graph/ops.rs` - Added insert_ast_nodes(), updated DeleteResult with ast_nodes_deleted, integrated AST extraction into index_file
- `src/graph/tests.rs` - Fixed test_cross_file_references to use file-based database
- `src/graph/validation.rs` - Fixed orphan tests to use file-based database

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Completed 37-01 AST extractor before 37-02**
- **Found during:** Task 1 (Adding AST node storage)
- **Issue:** Plan 37-01 had not been executed - ast_extractor.rs module didn't exist
- **Fix:** Created ast_extractor.rs with extract_ast_nodes(), normalize_node_kind(), language_from_path() functions; added module exports
- **Files modified:** src/graph/ast_extractor.rs, src/graph/mod.rs
- **Verification:** AST extractor tests pass (8 tests), extraction function returns Vec<AstNode>
- **Committed in:** `e1467c6` (combined with Task 1)

**2. [Rule 3 - Blocking] Fixed :memory: database tests for separate connections**
- **Found during:** Task 3 (Integration test verification)
- **Issue:** Tests using `:memory:` database failed because rusqlite::Connection::open(":memory:") creates separate database from SqliteGraph::open(":memory:")
- **Fix:** Updated test_cross_file_references and orphan reference tests to use tempfile file-based databases
- **Files modified:** src/graph/tests.rs, src/graph/validation.rs
- **Verification:** All 430 tests pass
- **Committed in:** `fa4fa51`

**3. [Rule 2 - Missing Critical] Added let_declaration to is_structural_kind**
- **Found during:** Task 1 (AST extractor testing)
- **Issue:** Rust tree-sitter grammar uses "let_declaration" not "let_statement"
- **Fix:** Added "let_declaration" to is_structural_kind matches
- **Files modified:** src/graph/ast_node.rs
- **Verification:** AST extraction test now passes
- **Committed in:** `e1467c6`

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 missing critical)
**Impact on plan:** All auto-fixes necessary for correctness. AST extraction from 37-01 was prerequisite. Database fix needed for ast_nodes table access. let_declaration fix needed for correct Rust node extraction.

## Issues Encountered

- **Nested Result type complexity**: `with_parser()` returns `Result<R>` where R is the closure's return type. Closure returning `Result<Vec<AstNode>>` creates `Result<Result<Vec<AstNode>>>`. Fixed by having closure return `Vec<AstNode>` directly and using map/unwrap_or_else.
- **Parser.parse() returns Option**: Tree parsing returns Option<Tree>, handled with map/unwrap_or_else to return empty Vec on parse failure.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- AST extraction is fully integrated into indexing pipeline
- AST nodes are stored with resolved parent-child relationships
- Re-indexing properly cleans up old AST nodes
- Ready for query tooling to leverage AST nodes for code navigation

**TODO for future enhancement:**
- Add file_id to ast_nodes table for efficient per-file queries and deletion
- Currently deletes ALL AST nodes during any file re-index (global operation)

---
*Phase: 37-ast-extraction*
*Completed: 2026-01-31*

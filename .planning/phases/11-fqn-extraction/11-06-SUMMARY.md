---
phase: 11-fqn-extraction
plan: 06
subsystem: graph-symbols, ingest
tags: [fqn, symbol-id, database-version, integration-tests]

# Dependency graph
requires:
  - phase: 11-fqn-extraction
    plan: 02
    provides: Rust parser FQN extraction with ScopeStack
  - phase: 11-fqn-extraction
    plan: 03
    provides: Dot-separated language FQN extraction (Java, Python, JS, TS)
  - phase: 11-fqn-extraction
    plan: 04
    provides: C/C++ parser FQN extraction with namespace tracking
  - phase: 11-fqn-extraction
    plan: 05
    provides: FQN-based symbol lookup in query.rs, calls.rs, references.rs
provides:
  - Complete FQN implementation with symbol_id using FQN in hash
  - Integration tests verifying FQN extraction across all languages
  - Database version bump (v2 -> v3) for breaking symbol_id change
  - Helpful migration error message for old databases
affects:
  - Phase 12 (transactional deletes - uses FQN for symbol identification)
  - Phase 13 (SCIP tests + docs - FQN required for SCIP export)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - symbol_id = hash(language, FQN, span_id) for stable identification
    - FQN-first fallback: fqn.or(name).unwrap_or_default()
    - Database version rejection with helpful error message

key-files:
  created:
    - tests/fqn_integration_tests.rs - Comprehensive FQN extraction tests
  modified:
    - src/graph/symbols.rs - Clarified FQN-first behavior in comments
    - src/graph/files.rs - Fixed symbol_fact_from_node to use symbol_node.fqn
    - src/ingest/mod.rs - Added test_fqn_always_populated test
    - src/ingest/cpp.rs - Fixed namespace extraction to create namespace symbols
    - src/graph/db_compat.rs - Bumped MAGELLAN_SCHEMA_VERSION to 3

key-decisions:
  - "Bug fix: symbol_fact_from_node was using symbol_node.name instead of symbol_node.fqn"
  - "C++ namespaces now create symbols with proper FQNs (previously skipped)"
  - "Database version bumped from 2 to 3 for FQN-based symbol_id change"
  - "Migration strategy: reject old databases with helpful error message"

patterns-established:
  - Integration tests verify end-to-end FQN extraction
  - All parsers set fqn: Some(fqn) - never None for named symbols
  - Version mismatch error includes migration instructions

# Metrics
duration: 8min
completed: 2026-01-19
---

# Phase 11: FQN Implementation Complete Summary

**FQN-based symbol_id generation, integration tests, and database version bump complete**

## Performance

- **Duration:** 8 min
- **Started:** 2026-01-19T20:45:20Z
- **Completed:** 2026-01-19T20:53:42Z
- **Tasks:** 4
- **Files modified:** 6
- **Tests added:** 6

## Accomplishments

- Verified and documented symbol_id generation uses FQN as primary key
- Created comprehensive FQN integration tests covering Rust, Java, Python, C++
- Fixed bug in `symbol_fact_from_node` where FQN was being lost on database query
- Fixed C++ parser to create namespace symbols (previously skipped)
- Added test verifying FQN is always populated for named symbols
- Bumped database version from 2 to 3 with helpful migration message

## Task Commits

Each task was committed atomically:

1. **Task 1: Verify symbol_id generation uses FQN** - `0788801` (docs)
2. **Task 2: Create FQN integration tests and fix bug** - `9824462` (feat)
3. **Task 3: Ensure FQN always populated** - `68e5587` (feat)
4. **Task 4: Bump database version** - `cb183c4` (feat)

**Plan metadata:** TBD (docs: complete plan)

## Files Created/Modified

- `src/graph/symbols.rs` - Clarified FQN-first behavior in comments
- `src/graph/files.rs` - Fixed bug: changed `fqn: symbol_node.name` to `fqn: symbol_node.fqn`
- `src/ingest/mod.rs` - Added `test_fqn_always_populated` test
- `src/ingest/cpp.rs` - Fixed namespace handling to create namespace symbols
- `src/graph/db_compat.rs` - Bumped `MAGELLAN_SCHEMA_VERSION` to 3 with migration message
- `tests/fqn_integration_tests.rs` - New file with 5 integration tests

## Decisions Made

- **Bug fix:** `symbol_fact_from_node` in `files.rs` was using `symbol_node.name` instead of `symbol_node.fqn` for the fqn field, causing FQNs to be lost when querying symbols from the database
- **C++ namespaces:** Updated to create namespace symbols (not just track scope) - namespace_definition now creates symbols before recursing into children
- **Database version:** Bumped from 2 to 3 with error message explaining the breaking change and providing migration instructions

## Deviations from Plan

### Rule 1 - Bug Fix: symbol_fact_from_node using wrong field

- **Found during:** Task 2 (integration tests)
- **Issue:** `src/graph/files.rs:190` was setting `fqn: symbol_node.name` instead of `fqn: symbol_node.fqn`
- **Fix:** Changed to `fqn: symbol_node.fqn` to properly retrieve the FQN from the database
- **Files modified:** `src/graph/files.rs`
- **Commit:** `9824462`

### Rule 1 - Bug Fix: C++ namespace symbols not created

- **Found during:** Task 3 (test verification)
- **Issue:** C++ parser was tracking namespace scope but not creating namespace symbols
- **Fix:** Modified `walk_tree_with_scope` to call `extract_symbol_with_fqn` for namespace_definition before recursing
- **Files modified:** `src/ingest/cpp.rs`
- **Commit:** `68e5587`

### Rule 2 - Missing Critical: Integration test fix

- **Found during:** Task 2 (integration tests)
- **Issue:** C++ test used function declaration instead of definition
- **Fix:** Changed `void draw(const Point& p);` to `void draw(const Point& p) {}`
- **Files modified:** `tests/fqn_integration_tests.rs`
- **Commit:** `9824462`

## Authentication Gates

None - no external services required for this plan.

## Next Phase Readiness

- symbol_id generation explicitly uses FQN in hash computation
- SymbolFact.fqn always populated (never None for named symbols)
- SymbolNode.fqn stored and retrieved correctly from database
- Integration tests pass for Rust, Java, Python, C++
- Database version 3 rejects old databases with helpful re-index instruction
- Ready for Phase 12: Transactional Deletes

---
*Phase: 11-fqn-extraction*
*Completed: 2026-01-19*

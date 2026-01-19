---
phase: 06-query-ux
plan: 03
subsystem: call-graph
tags: [symbol_id, call-graph, serde, stable-identifiers]

# Dependency graph
requires:
  - phase: 05-stable-symbol-ids
    provides: SymbolNode.symbol_id field and generation function
  - phase: 06-query-ux/06-02
    provides: ReferenceMatch with target_symbol_id pattern
provides:
  - CallNode with caller_symbol_id and callee_symbol_id fields
  - CallFact with caller_symbol_id and callee_symbol_id fields
  - Call indexing populates stable symbol IDs from Symbol nodes
  - Refs command JSON output includes symbol IDs for both directions
affects: [future-call-graph-analysis, api-clients]

# Tech tracking
tech-stack:
  added: []
  patterns: [stable-symbol-id-propagation, backward-compatible-serde-fields]

key-files:
  created: []
  modified:
    - src/graph/schema.rs - CallNode with symbol_id fields
    - src/references.rs - CallFact with symbol_id fields
    - src/graph/call_ops.rs - Call indexing with symbol_id lookup
    - src/refs_cmd.rs - JSON output using symbol_id from CallFact
    - src/ingest/*.rs - All language parsers updated for CallFact fields
    - tests/cli_query_tests.rs - Tests for symbol_id in refs output

key-decisions:
  - "Use caller_symbol_id/callee_symbol_id in CallNode for stable correlation"
  - "Populate symbol_ids during indexing by looking up SymbolNode data"
  - "Use #[serde(default)] for backward compatibility with old Call nodes"
  - "Refs JSON uses symbol_id directly from CallFact instead of database lookup"

patterns-established:
  - "Pattern: Optional stable ID fields with #[serde(default)] for backward compatibility"
  - "Pattern: Symbol ID lookup via (file_path, symbol_name) key map"
  - "Pattern: target_symbol_id field direction-aware (caller for 'in', callee for 'out')"

# Metrics
duration: 9min
completed: 2026-01-19
---

# Phase 6: Query UX - Call Graph Symbol IDs Summary

**Call graph results now include stable symbol IDs for both caller and callee, enabling correlation across indexing runs**

## Performance

- **Duration:** 9 min
- **Started:** 2026-01-19T13:12:01Z
- **Completed:** 2026-01-19T13:21:09Z
- **Tasks:** 6
- **Files modified:** 11

## Accomplishments

- Extended CallNode schema with optional caller_symbol_id and callee_symbol_id fields
- Extended CallFact with optional caller_symbol_id and callee_symbol_id fields
- Call indexing now populates stable symbol IDs by looking up SymbolNode data
- Refs command JSON output includes target_symbol_id for both incoming and outgoing calls
- All language parsers updated to handle new CallFact fields
- Tests verify symbol_id presence in JSON output for both directions

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend CallNode schema with symbol_id fields** - `455aa95` (feat)
2. **Task 2: Extend CallFact with symbol_id fields** - `3b1cd34` (feat)
3. **Task 3: Update call indexing to store symbol_ids** - `90a7798` (feat)
4. **Task 4: Update call_fact_from_node to deserialize symbol_ids** - `d9a8d53` (fix)
5. **Task 5: Update refs command to include symbol_ids in JSON output** - `51da0de` (feat)
6. **Task 6: Add tests for call graph symbol_id propagation** - `52d8c65` (test)

**Plan metadata:** None (inline fixes only)

## Files Created/Modified

- `src/graph/schema.rs` - Added caller_symbol_id and callee_symbol_id Option<String> fields to CallNode
- `src/references.rs` - Added caller_symbol_id and callee_symbol_id Option<String> fields to CallFact
- `src/graph/call_ops.rs` - Updated index_calls to build stable_symbol_ids map and populate CallFact fields
- `src/refs_cmd.rs` - Simplified JSON output to use symbol_id directly from CallFact
- `src/ingest/c.rs` - Updated CallFact constructor with symbol_id fields (set to None)
- `src/ingest/cpp.rs` - Updated CallFact constructor with symbol_id fields (set to None)
- `src/ingest/java.rs` - Updated CallFact constructor with symbol_id fields (set to None)
- `src/ingest/javascript.rs` - Updated CallFact constructor with symbol_id fields (set to None)
- `src/ingest/python.rs` - Updated CallFact constructor with symbol_id fields (set to None)
- `src/ingest/typescript.rs` - Updated CallFact constructor with symbol_id fields (set to None)
- `src/output/command.rs` - Fixed test for FilesResponse
- `tests/cli_query_tests.rs` - Added test_refs_callees_includes_symbol_id test

## Decisions Made

- Used `#[serde(default)]` on optional symbol_id fields for backward compatibility with existing Call nodes in databases
- Build stable_symbol_ids lookup map as (file_path, symbol_name) -> Option<String> during indexing
- For "in" direction (callers), target_symbol_id = caller_symbol_id
- For "out" direction (callees), target_symbol_id = callee_symbol_id
- Removed complex database lookup in refs output in favor of using symbol_id directly from CallFact

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Updated all CallFact constructors across language parsers**
- **Found during:** Task 2 (CallFact schema extension)
- **Issue:** Adding fields to CallFact broke compilation in all language parser extract_calls implementations
- **Fix:** Added caller_symbol_id and callee_symbol_id fields (set to None) to all 7 language parser CallFact constructors
- **Files modified:** src/ingest/c.rs, src/ingest/cpp.rs, src/ingest/java.rs, src/ingest/javascript.rs, src/ingest/python.rs, src/ingest/typescript.rs, src/graph/call_ops.rs
- **Verification:** cargo check passes, all parsers compile
- **Committed in:** d9a8d53 (part of Task 3 commit)

**2. [Rule 2 - Missing Critical] Fixed test_json_response_serialization**
- **Found during:** Task 6 (running tests)
- **Issue:** FilesResponse test failed because symbol_counts field was missing
- **Fix:** Added symbol_counts: None to test FilesResponse construction
- **Files modified:** src/output/command.rs
- **Verification:** Tests pass
- **Committed in:** 52d8c65 (part of Task 6 commit)

---

**Total deviations:** 2 auto-fixed (2 missing critical)
**Impact on plan:** Both auto-fixes necessary for compilation and test correctness. No scope creep.

## Issues Encountered

- Uncommitted changes from previous session (06-04 work) caused some confusion - stashed and continued with 06-03 work only
- Fixed borrowed value error in index_calls by saving call_count before moving calls vector

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Call graph now includes stable symbol IDs for correlation
- Ready for enhanced call graph analysis and API clients
- Backward compatibility maintained with #[serde(default)]
- Tests verify symbol_id presence in both directions

---
*Phase: 06-query-ux*
*Plan: 03*
*Completed: 2026-01-19*

---
phase: 06-query-ux
plan: 01
subsystem: query-cli
tags: [symbol-id, json-output, find-command, cli-ux]

# Dependency graph
requires:
  - phase: 05-stable-identity
    provides: symbol_id field on SymbolNode and SymbolMatch types
provides:
  - Consistent symbol_id propagation in find command JSON output
  - --output flag support for find command
  - Test coverage for symbol_id in JSON responses
affects:
  - Phase 6 query UX plans (06-02, 06-03, 06-04)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Command-specific --output flag parsing for JSON output
    - SymbolMatch::new requires symbol_id parameter for stable IDs

key-files:
  created:
    - tests/cli_query_tests.rs (test_find_includes_symbol_id_in_json)
  modified:
    - src/find_cmd.rs (symbol_id propagation fix)
    - src/main.rs (Find command --output flag support)
    - src/refs_cmd.rs (ReferenceMatch::new call fix)

key-decisions:
  - "Find command must support --output flag like status/query commands"
  - "symbol_id propagation must be consistent across all query paths"

patterns-established:
  - "Pattern: All JSON query output must include stable symbol_id when available"
  - "Pattern: Per-command --output flag parsing for explicit format control"

# Metrics
duration: 6min
completed: 2026-01-19
---

# Phase 6: Query UX - Plan 01 Summary

**Consistent symbol_id propagation in find command JSON output with --output flag support**

## Performance

- **Duration:** 6 minutes (368 seconds)
- **Started:** 2026-01-19T13:00:49Z
- **Completed:** 2026-01-19T13:07:05Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Fixed find command to propagate symbol_id in JSON output for exact name searches
- Added --output flag support to find command for explicit format control
- Added comprehensive test for symbol_id presence in find JSON output
- Fixed pre-existing bug in refs_cmd.rs (ReferenceMatch::new call)

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix symbol_id propagation in find_json_mode** - `6ecafe2` (feat)
2. **Task 2: Add test for symbol_id in find JSON output** - `c3eaa09` (test)

**Plan metadata:** N/A (summary-only commit)

## Files Created/Modified

- `src/find_cmd.rs` - Changed SymbolMatch::new to pass s.symbol_id instead of None
- `src/main.rs` - Added output_format field to Find command struct and --output parsing
- `tests/cli_query_tests.rs` - Added test_find_includes_symbol_id_in_json test
- `src/refs_cmd.rs` - Fixed ReferenceMatch::new call with 4th parameter (None for target_symbol_id)

## Decisions Made

- **Find command --output support**: Added per-command --output flag parsing to match status/query commands, enabling explicit JSON output control
- **symbol_id consistency**: Both find-by-name and glob-listing paths now consistently include symbol_id in JSON output

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed ReferenceMatch::new call in refs_cmd.rs**
- **Found during:** Task 2 (test compilation)
- **Issue:** ReferenceMatch::new signature changed to include target_symbol_id parameter, but refs_cmd.rs was calling with old 3-parameter signature
- **Fix:** Added None as 4th parameter to ReferenceMatch::new call in refs_cmd.rs
- **Files modified:** src/refs_cmd.rs
- **Verification:** Compilation succeeds, tests pass
- **Committed in:** c3eaa09 (Task 2 commit)

**2. [Rule 2 - Missing Critical] Added --output flag support to find command**
- **Found during:** Task 2 (test execution)
- **Issue:** Find command didn't support documented --output flag for JSON output, making test impossible
- **Fix:** Added output_format field to Find command struct, added --output parsing in find command handler, updated main match arm
- **Files modified:** src/main.rs
- **Verification:** Test passes, manual verification confirms JSON output works
- **Committed in:** c3eaa09 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing critical)
**Impact on plan:** Both auto-fixes essential for correctness and testability. The --output flag support enables JSON output testing, which was the plan's goal. ReferenceMatch fix resolved pre-existing compilation bug.

## Issues Encountered

- **Test invocation confusion:** Initially tried placing --output before command (global flag pattern), but find command uses per-command flag parsing like status
- **File system reminder:** System noted file was modified externally during edit, required re-adding test code

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Find command now returns consistent symbol_id in JSON output
- --output flag available for format control in find command
- Test coverage validates symbol_id presence
- Ready for Phase 6-02 (target_symbol_id for references)

---
*Phase: 06-query-ux*
*Plan: 01*
*Completed: 2026-01-19*

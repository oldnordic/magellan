---
phase: 02-deterministic-watch--indexing-pipeline
plan: 03
subsystem: filtering
tags: [gitignore, ignore-crate, globset, diagnostics, error-containment]

# Dependency graph
requires:
  - phase: 02-02
    provides: Deterministic watch pipeline with buffering and debouncing
provides:
  - Structured diagnostics types (WatchDiagnostic, SkipReason, DiagnosticStage)
  - FileFilter module for deterministic file filtering
  - Gitignore-style ignore rule support (.gitignore, .ignore)
  - CLI include/exclude glob patterns
  - Per-path error containment in scan/watch pipeline
  - ScanResult type returning both count and diagnostics
affects: [03-query-api, 05-execution-logging]

# Tech tracking
tech-stack:
  added: [ignore 0.4.25 for gitignore-style matching]
  patterns: [Deterministic filtering precedence, sorted diagnostics for output]

key-files:
  created: [src/diagnostics/mod.rs, src/diagnostics/watch_diagnostics.rs, src/graph/filter.rs, tests/ignore_rules_tests.rs]
  modified: [Cargo.toml, src/lib.rs, src/graph/mod.rs, src/graph/scan.rs]

key-decisions:
  - "Use ignore crate instead of hand-rolled gitignore parsing for correctness"
  - "Diagnostics sorted at output time for deterministic ordering regardless of walkdir order"
  - "Internal ignores always win over gitignore for security (db files, .git/, target/, etc.)"
  - "Per-path Result wrapping ensures watch continues on bad files"

patterns-established:
  - "Precedence order: Internal > Gitignore > Include > Exclude"
  - "Diagnostics implement Ord for stable sorting"
  - "ScanResult aggregates both indexed count and skip/error diagnostics"

# Metrics
duration: 33min
completed: 2026-01-19
---

# Phase 2: Plan 03 Summary

**Structured diagnostics with gitignore-style filtering and per-path error containment using ignore crate**

## Performance

- **Duration:** 33 min
- **Started:** 2026-01-19T09:23:33Z
- **Completed:** 2026-01-19T09:56:50Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Added `ignore` crate v0.4.25 for gitignore-style file filtering
- Created structured diagnostic types (WatchDiagnostic, SkipReason, DiagnosticStage) with deterministic ordering
- Implemented FileFilter with precedence: Internal > Gitignore > Include > Exclude
- Updated scan to emit diagnostics for skipped files and continue on per-file errors
- Added 7 integration tests covering gitignore, include/exclude patterns, and error containment

## Task Commits

Each task was committed atomically:

1. **Task 1: Add structured diagnostics types and deterministic ordering** - `257c470` (feat)
2. **Task 2: Implement gitignore filtering + per-path error containment** - `5fc3e1b` (feat)

## Files Created/Modified

### Created
- `src/diagnostics/mod.rs` - Diagnostics module re-exports
- `src/diagnostics/watch_diagnostics.rs` - WatchDiagnostic, SkipReason, DiagnosticStage types
- `src/graph/filter.rs` - FileFilter with gitignore and glob support
- `tests/ignore_rules_tests.rs` - Integration tests for filtering and error containment

### Modified
- `Cargo.toml` - Added `ignore = "0.4.25"` dependency
- `src/lib.rs` - Exported diagnostics types, FileFilter, ScanResult
- `src/graph/mod.rs` - Made filter and scan modules public
- `src/graph/scan.rs` - Added ScanResult, filter integration, error containment, diagnostics sorting

## Decisions Made

1. **Use ignore crate instead of hand-rolled gitignore parsing**
   - Rationale: The ignore crate is battle-tested (used by ripgrep, cargo) and handles edge cases correctly
   - Alternative considered: Hand-rolled glob matching (rejected due to complexity)

2. **Sort diagnostics at output time, not collection time**
   - Rationale: walkdir iteration order is not deterministic; sorting at end ensures consistent output
   - Pattern: All scan results sorted before return

3. **Internal ignores always win over gitignore**
   - Rationale: Security - must never index our own .db files regardless of user's gitignore
   - Pattern: Precedence order in should_skip() function

4. **Per-path Result wrapping in scan loop**
   - Rationale: WATCH-05A requires watch to continue on bad files
   - Pattern: Match each Result, push diagnostic on error, continue to next path

## Deviations from Plan

None - plan executed exactly as written. All tests pass as specified.

## Issues Encountered

1. **Gitignore crate API mismatch**
   - Issue: `GitignoreBuilder::add()` returns `Option<Error>` not `Result`
   - Resolution: Updated code to match actual API

2. **walkdir non-deterministic iteration**
   - Issue: Diagnostics collected in walkdir order varied between runs
   - Resolution: Added `diagnostics.sort()` before returning ScanResult

3. **Database WAL files appearing in scan directory**
   - Issue: test*.db files created WAL files that were being counted as diagnostics
   - Resolution: Updated test to use separate temp dir for databases

4. **Unix-specific permission tests**
   - Issue: `set_mode` requires `PermissionsExt` trait import
   - Resolution: Added cfg-based imports and separate test paths for Unix/non-Unix

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- WATCH-03 satisfied: include/exclude rules applied deterministically with skip diagnostics
- WATCH-05A satisfied: per-file failures captured as structured diagnostics, watch continues
- Diagnostics types ready for JSON serialization in Phase 3
- FileFilter ready for CLI integration (--include/--exclude flags)

**Blockers:** None

**Concerns:**
- Nested .gitignore files not yet supported (only root .gitignore/.ignore)
- Phase 3 will need to add JSON output contract for diagnostics

---
*Phase: 02-deterministic-watch--indexing-pipeline*
*Completed: 2026-01-19*

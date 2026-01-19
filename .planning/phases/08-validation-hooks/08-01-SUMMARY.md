---
phase: 08-validation-hooks
plan: 01
subsystem: graph-validation
tags: [sqlitegraph, neighbor-query, validation, json-output, deterministic-sorting]

# Dependency graph
requires:
  - phase: 07-deterministic-exports
    provides: graph schema with ReferenceNode, CallNode, SymbolNode
  - phase: 02-deterministic-watch-pipeline
    provides: CodeGraph with references and calls backend access
provides:
  - Validation module with ValidationReport type following VerifyReport pattern
  - Orphan detection using sqlitegraph neighbor query API (REFERENCES, CALLER, CALLS edges)
  - Pre-run validation for database and input path checking
  - Deterministic error sorting for consistent JSON output
affects: [08-02, 09-cli-integration]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - ValidationReport follows VerifyReport pattern (passed, errors, warnings fields)
    - SCREAMING_SNAKE_CASE error codes for machine-readability
    - Deterministic sorting by code then message for consistent output
    - sqlitegraph NeighborQuery with BackendDirection for graph traversal

key-files:
  created:
    - src/graph/validation.rs
  modified:
    - src/graph/mod.rs

key-decisions:
  - "Validation module is internal API for now - no re-exports from mod.rs"
  - "Error sorting uses code then message for deterministic JSON output"
  - "GraphBackend trait must be imported for get_node/neighbors methods"

patterns-established:
  - "Validation reports follow VerifyReport pattern: passed bool, errors Vec, helper methods (total_issues, is_clean)"
  - "Error codes use SCREAMING_SNAKE_CASE for machine parsing (ORPHAN_REFERENCE, DB_PARENT_MISSING, etc.)"
  - "Structured details in serde_json::Value for extensible error context"

# Metrics
duration: 4min
completed: 2026-01-19
---

# Phase 8 Plan 1: Validation Module Summary

**Validation module with ValidationReport type, orphan detection via sqlitegraph neighbor queries, and pre-run environment checks**

## Performance

- **Duration:** 4 minutes (219 seconds)
- **Started:** 2026-01-19T14:59:44Z
- **Completed:** 2026-01-19T15:03:23Z
- **Tasks:** 3 (all completed in single atomic commit)
- **Files modified:** 2

## Accomplishments

- Created ValidationReport type following VerifyReport pattern from src/verify.rs
- Implemented orphan detection using sqlitegraph neighbors() with NeighborQuery
- Implemented pre-run validation checking database parent, root path, and input paths
- All validation types implement Serialize/Deserialize for JSON output
- Error vectors sorted deterministically for consistent output

## Task Commits

All tasks completed in single atomic commit:

1. **Task 1: Create validation module with ValidationReport type** - `c043d6f` (feat)
2. **Task 2: Implement orphan detection validation functions** - `c043d6f` (feat)
3. **Task 3: Implement pre-run validation and wire into CodeGraph** - `c043d6f` (feat)

**Commit:** `c043d6f` - feat(08-01): create validation module with ValidationReport type

## Files Created/Modified

- `src/graph/validation.rs` - 512 lines
  - ValidationReport, ValidationError, ValidationWarning types
  - PreValidationReport for pre-run checks
  - validate_graph() orchestrates all post-run checks
  - check_orphan_references() uses neighbors() with BackendDirection::Outgoing
  - check_orphan_calls() checks both CALLER (Incoming) and CALLS (Outgoing)
  - pre_run_validate() checks environment before indexing
  - Unit tests for all types
- `src/graph/mod.rs`
  - Added `mod validation;` declaration

## Decisions Made

- **GraphBackend trait import required**: The get_node() and neighbors() methods are from the GraphBackend trait, which must be explicitly imported with `use sqlitegraph::GraphBackend;`
- **Type annotation for closure sort**: Rust type inference required explicit type annotation for all_warnings.sort_by closure: `|a: &ValidationWarning, b: &ValidationWarning|`
- **Validation module remains internal**: No re-exports from mod.rs as specified in plan - validation is called from CLI handlers, not stored as state

## Deviations from Plan

None - plan executed exactly as written. All three tasks were completed in a single implementation pass since they were tightly related (types, detection functions, and pre-run validation all in the same module).

## Issues Encountered

- **Missing GraphBackend import**: Initial compile failed because get_node() and neighbors() methods require the GraphBackend trait to be in scope. Fixed by adding `use sqlitegraph::GraphBackend;`.
- **Closure type annotation required**: Compiler couldn't infer type for all_warnings.sort_by closure. Fixed by adding explicit type annotation: `|a: &ValidationWarning, b: &ValidationWarning|`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Validation module complete and tested
- Orphan detection uses correct sqlitegraph neighbor query API
- Pre-run validation ready for CLI integration
- Next phase (08-02) will wire validation into CLI commands for JSON output
- No blockers or concerns

---
*Phase: 08-validation-hooks*
*Plan: 01*
*Completed: 2026-01-19*

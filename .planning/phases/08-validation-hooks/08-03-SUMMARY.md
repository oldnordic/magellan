---
phase: 08-validation-hooks
plan: 03
subsystem: cli-validation-integration
tags: [cli-flags, validation-hooks, json-output, pre-post-validation]

# Dependency graph
requires:
  - phase: 08-validation-hooks
    plan: 01
    provides: Validation module with ValidationReport, ValidationError, ValidationWarning
  - phase: 08-validation-hooks
    plan: 02
    provides: ValidationResponse public types for JSON output
provides:
  - CLI --validate flag enabling pre-run and post-run validation
  - CLI --validate-only flag for validation without indexing
  - Pre-run validation hook checking DB parent, root path, and input paths
  - Post-run validation hook checking orphan references and orphan calls
  - JSON validation output with execution_id correlation
  - Exit code 1 on validation failure with structured output
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Pre-run validation before indexing (environment checks)
    - Post-run validation after indexing (graph invariants)
    - Validation modes: validate (pre+post), validate-only (no indexing)
    - JSON validation output wrapped in JsonResponse with execution_id
    - Human validation output with error codes and messages

key-files:
  created: []
  modified:
    - src/main.rs
    - src/watch_cmd.rs
    - src/graph/mod.rs

key-decisions:
  - "Made validation module public (was pub(crate)) for CLI access"
  - "Added --output flag parsing to watch command for JSON validation output"
  - "validate-only implies validate=true (precedence rule)"

patterns-established:
  - "Validation hooks use magellan::output::command::ValidationResponse for JSON output"
  - "Validation failures exit with code 1 in both JSON and human modes"
  - "Pre-validation errors are recorded in execution_log with outcome='error'"
  - "Post-validation failures are recorded with outcome='validation_failed'"

# Metrics
duration: 10min
completed: 2026-01-19
---

# Phase 8 Plan 3: CLI Validation Hooks Summary

**CLI --validate and --validate-only flags with pre/post validation hooks, JSON output, and proper exit codes**

## Performance

- **Duration:** 10 minutes (approximately 580 seconds)
- **Started:** 2026-01-19T15:09:58Z
- **Completed:** 2026-01-19T15:19:36Z
- **Tasks:** 3 (all completed)
- **Files modified:** 3

## Accomplishments

- Added --validate and --validate-only flags to watch command
- Implemented pre-run validation hook (DB parent, root path, input paths)
- Implemented --validate-only mode (pre + post validation, no indexing)
- Implemented post-run validation hook (orphan references, orphan calls)
- Validation failures exit with code 1 and output structured JSON in JSON mode
- Validation results include execution_id via JsonResponse wrapper
- Added --output flag parsing to watch command for JSON output support

## Task Commits

All tasks completed in 2 commits:

1. **Tasks 1-3: Add validation flags and hooks** - `1251187` (feat)
2. **Fix: Add --output flag parsing to watch command** - `29d3e1b` (fix)

## Files Created/Modified

- `src/main.rs` - Added validate/validate_only fields to Command::Watch, flag parsing, help text, and --output flag parsing
- `src/watch_cmd.rs` - Implemented pre-run validation, validate-only mode, and post-run validation hooks with JSON output
- `src/graph/mod.rs` - Changed validation module from pub(crate) to pub for CLI access

## Decisions Made

- **Made validation module public**: Changed from `pub(crate) mod validation` to `pub mod validation` in src/graph/mod.rs to allow the CLI binary to access validation functions. This is appropriate since validation is now a user-facing feature.

- **Added --output flag to watch command**: The plan didn't explicitly mention this, but for JSON validation output to work, the watch command needed to parse the --output flag. This follows the pattern used by other commands (refs, files, status).

- **Validate-only implies validate**: Following the precedence pattern of --watch-only forcing scan_initial=false, --validate-only implies validate=true. This allows validate-only to run both pre and post validation without indexing.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added --output flag parsing to watch command**
- **Found during:** Task 2 (Testing validation with JSON output)
- **Issue:** The global --output flag wasn't being parsed for the watch command. Users couldn't use `--output json` with validation.
- **Fix:** Added output_format field to Command::Watch enum and --output flag parsing in the watch command args section.
- **Files modified:** src/main.rs
- **Verification:** `magellan watch --root ... --validate-only --output json` now outputs JSON correctly.
- **Committed in:** `29d3e1b` (separate commit after initial implementation)

**2. [Rule 1 - Bug] Fixed error_count calculation to avoid moved value error**
- **Found during:** Task 2 (Initial build after implementing validation)
- **Issue:** `report.errors.len()` was used after `report.errors.into_iter()` caused a borrow-after-move error.
- **Fix:** Calculate error_count before consuming the vector in into_iter().
- **Files modified:** src/watch_cmd.rs
- **Committed in:** `1251187` (part of main implementation commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes were necessary for correctness and functionality. No scope creep.

## Issues Encountered

- **Module visibility**: The validation module was `pub(crate)` which wasn't accessible from the binary crate. Fixed by making it `pub`.
- **Borrow-after-move**: Using `report.errors.len()` after consuming it with `into_iter()` caused compilation errors. Fixed by capturing length before the move.
- **Global --output flag**: The watch command didn't parse --output like other commands, preventing JSON validation output. Fixed by adding flag parsing.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CLI validation hooks complete and tested
- Pre-run validation checks environment before indexing
- Post-run validation checks graph invariants after indexing
- --validate-only enables validation-only workflows
- JSON validation output includes execution_id for correlation
- Exit code 1 on validation failure for CI/CD integration
- No blockers or concerns

---
*Phase: 08-validation-hooks*
*Plan: 03*
*Completed: 2026-01-19*

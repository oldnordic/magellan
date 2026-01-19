---
phase: 03-cli-output-contract
plan: 01
subsystem: json-output
tags: [serde, json, cli-output, schema-versioning]

# Dependency graph
requires:
  - phase: 02-deterministic-watch-indexing
    provides: symbol fact types with span fields, deterministic sorting patterns
provides:
  - output module foundation with OutputFormat enum
  - JsonResponse wrapper with schema_version and execution_id
  - generate_execution_id function for run tracking
  - output_json helper for stdout discipline
affects: [03-02, 03-03]

# Tech tracking
tech-stack:
  added: [serde (already present), serde_json (already present)]
  patterns: [schema-versioned JSON responses, hash-based execution IDs, stdout/stderr separation]

key-files:
  created: [src/output/mod.rs, src/output/command.rs]
  modified: [src/lib.rs, src/main.rs, src/find_cmd.rs]

key-decisions:
  - "Hash-based execution_id generation (timestamp + pid) instead of UUID crate for simplicity"
  - "Schema version '1.0.0' for JSON output contract stability"
  - "Stdout = JSON data only, stderr = logs/diagnostics (Pattern 4 from research)"

patterns-established:
  - "Pattern 1: JsonResponse<T> wrapper with schema_version, execution_id, data, partial fields"
  - "Pattern 4: Stdout/stderr discipline - JSON to stdout, logs to stderr via eprintln!"
  - "Deterministic ordering via BTreeMap for JSON output (future use)"
---
# Phase 3 Plan 1: Output Module Foundation Summary

**Output module with schema-versioned JsonResponse wrapper, hash-based execution_id generation, and stdout/stderr discipline for JSON contract**

## Performance

- **Duration:** 10 minutes
- **Started:** 2025-01-19T10:25:24Z
- **Completed:** 2025-01-19T10:35:30Z
- **Tasks:** 6 (verified complete)
- **Files modified:** 5

## Accomplishments

- Created `src/output/` module with `OutputFormat` enum (Human, Json)
- Implemented `JsonResponse<T>` wrapper with `schema_version` and `execution_id` fields
- Implemented `generate_execution_id()` using hash-based approach (timestamp + PID, 16 hex chars)
- Added `output_json()` helper function for stdout discipline
- Verified core fact types (`SymbolFact`, `ReferenceFact`, `CallFact`) have serde derives
- Exported output module from `lib.rs`

## Task Commits

Note: The output module was implemented in prior commits as part of plans 03-02/03-03. This session verified completion and fixed integration issues.

1. **Task 1-6: Output module foundation** - Already existed (from `e73a543`, `7d95a97`, `0b2c109`)
2. **Fix: find_cmd borrow issue** - `1c48e5b` (fix)
3. **Fix: main.rs run_find call** - `201f96e` (fix)

## Files Created/Modified

- `src/output/mod.rs` - Module exports, re-exports JsonResponse, OutputFormat, generate_execution_id, output_json
- `src/output/command.rs` - Core types: JsonResponse, OutputFormat, Span, SymbolMatch, ReferenceMatch, response types
- `src/lib.rs` - Added output module and re-exports (already present in HEAD)
- `src/find_cmd.rs` - Fixed borrow issue in path matching, added JSON output support
- `src/main.rs` - Added OutputFormat::Human to run_find call

## Decisions Made

- **Execution ID strategy**: Hash-based (timestamp + PID) instead of UUID crate to avoid new dependency. Format: 16 hex chars (`{:08x}{:08x}`).
- **Schema version**: Set to "1.0.0" for v1 JSON contract. Enables downstream parsing stability.
- **Stdout/stderr discipline**: stdout = JSON data only, stderr = logs/diagnostics. Uses `println!` for JSON, `eprintln!` for logs.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed borrow issue in find_cmd.rs path matching**
- **Found during:** Plan verification (compilation check)
- **Issue:** `path` variable was being partially moved in match, causing borrow error for later use
- **Fix:** Changed `match path` to `match path.as_ref()` to borrow instead of move
- **Files modified:** src/find_cmd.rs
- **Verification:** `cargo check` passes
- **Committed in:** `1c48e5b`

**2. [Rule 3 - Blocking] Added OutputFormat parameter to run_find call in main.rs**
- **Found during:** Compilation check after fix_cmd fix
- **Issue:** find_cmd::run_find signature includes output_format parameter but main.rs wasn't updated
- **Fix:** Added `OutputFormat::Human` as default argument to run_find call
- **Files modified:** src/main.rs
- **Verification:** `cargo check` passes
- **Committed in:** `201f96e`

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both auto-fixes were necessary for compilation after prior plans added output_format parameter. No scope creep.

## Issues Encountered

- **Partial file state during execution**: main.rs and find_cmd.rs were in partially modified states from previous sessions (03-02/03-03). Resolved by completing the integration.

- **File sync issues**: Files appeared to be modified externally during edits. Resolved by re-reading and re-applying changes.

## Verification Criteria Met

- [x] `src/output/mod.rs` exists with OutputFormat enum
- [x] `JsonResponse` wrapper includes `schema_version` and `execution_id` fields
- [x] `execution_id` generation returns 16+ char hex string
- [x] `SymbolFact`, `ReferenceFact`, `CallFact` all have `Serialize/Deserialize` derives
- [x] `output_json()` and helper functions output to correct streams
- [x] All tests pass (`cargo test --workspace`)

## Next Phase Readiness

Output module foundation is complete and ready for:
- Plan 03-02: CLI --output flag integration for status command
- Plan 03-03: JSON output for query/find/refs/files commands

**Blockers/concerns:** None. The output module is stable and tested.

---
*Phase: 03-cli-output-contract*
*Plan: 01*
*Completed: 2025-01-19*

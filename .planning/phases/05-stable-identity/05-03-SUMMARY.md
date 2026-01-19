---
phase: 05-stable-identity
plan: 03
subsystem: json-output
tags: [symbol-id, serde, json-api, stable-identifiers]

# Dependency graph
requires:
  - phase: 05-stable-identity/05-01
    provides: symbol_id generation in SymbolNode schema
provides:
  - SymbolMatch struct with symbol_id field for JSON output
  - Query function symbol_nodes_in_file_with_ids() that returns symbol_id
  - JSON API consumers can now receive stable symbol identifiers
affects: [json-api, query-command, find-command]

# Tech tracking
tech-stack:
  added: []
  patterns: [stable-symbol-propagation, optional-json-field-with-skip-serializing-if]

key-files:
  created: []
  modified: [src/output/command.rs, src/graph/query.rs, src/query_cmd.rs, src/find_cmd.rs]

key-decisions:
  - "symbol_id is Option<String> with skip_serializing_if for backward compatibility"
  - "New symbol_nodes_in_file_with_ids() function returns (node_id, SymbolFact, Option<String>)"
  - "Command handlers use symbol_nodes_in_file_with_ids() for JSON output paths only"

patterns-established:
  - "Optional stable ID fields: Use Option<T> with serde skip_serializing_if for non-breaking additions"
  - "Query function variants: Create _with_ids variants when internal data needs exposure"

# Metrics
duration: 15min
completed: 2026-01-19
---

# Phase 5 Plan 3: Symbol ID in JSON Output Summary

**SymbolMatch struct with symbol_id field propagated from SymbolNode through query pipeline to JSON API**

## Performance

- **Duration:** 15 min
- **Started:** 2026-01-19T12:23:32Z
- **Completed:** 2026-01-19T12:38:00Z
- **Tasks:** 5
- **Files modified:** 4

## Accomplishments

- Added `symbol_id: Option<String>` field to `SymbolMatch` with `skip_serializing_if` for backward compatibility
- Updated `SymbolMatch::new()` signature to accept `symbol_id` parameter
- Added `symbol_nodes_in_file_with_ids()` query function that returns `(node_id, SymbolFact, Option<String>)`
- Updated `query_cmd.rs` to extract and propagate `symbol_id` in JSON output mode
- Updated `find_cmd.rs` to extract and propagate `symbol_id` via `FoundSymbol` struct
- Added 6 new tests for `SymbolMatch` `symbol_id` field (serialization, deserialization, optional behavior)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add symbol_id field to SymbolMatch struct** - `22a7c27` (feat)
2. **Tasks 2-4: Update query functions and command handlers** - `2628d4c` (feat)
3. **Task 5: Add symbol_id tests** - `6c455ca` (test)

**Plan metadata:** (to be added after final commit)

## Files Created/Modified

- `src/output/command.rs` - Added `symbol_id: Option<String>` field to `SymbolMatch`, updated `new()` signature, added 6 tests
- `src/graph/query.rs` - Added `symbol_nodes_in_file_with_ids()` function returning `(i64, SymbolFact, Option<String>)`
- `src/query_cmd.rs` - Updated `output_json_mode()` to use `symbol_nodes_in_file_with_ids()` and propagate `symbol_id`
- `src/find_cmd.rs` - Added `symbol_id` to `FoundSymbol`, updated `find_in_file()`, `find_all_files()`, and `run_glob_listing()` to extract and propagate `symbol_id`

## Decisions Made

- **symbol_id is Optional with skip_serializing_if**: This ensures backward compatibility - existing JSON consumers won't break, and the field is only present when a symbol has a stable ID
- **New _with_ids query function variant**: Instead of modifying the existing `symbol_nodes_in_file()`, created `symbol_nodes_in_file_with_ids()` to avoid breaking existing code that doesn't need `symbol_id`
- **JSON-only propagation**: The `symbol_id` is only propagated in JSON output mode; human-readable output doesn't change

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed return type mismatch in ExecutionTracker.start()**
- **Found during:** Task 1 (compilation after adding symbol_id field)
- **Issue:** `start_execution()` returns `Result<i64>` but `ExecutionTracker::start()` expected `Result<()>`
- **Fix:** Changed to discard the row ID with `()?;` pattern and return `Ok(())`
- **Files modified:** src/main.rs
- **Verification:** Compilation succeeds, all tests pass
- **Committed in:** Fixed automatically by rustfmt/linter during development

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Bug fix necessary for compilation. No scope creep.

## Issues Encountered

None - all tasks executed as planned.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- JSON API now includes stable `symbol_id` for symbols, enabling correlation across runs
- `symbol_id` propagation is complete through the query pipeline to `SymbolMatch` JSON output
- Tests verify backward compatibility (JSON without `symbol_id` still deserializes correctly)
- Ready for Phase 5 Plan 04 or subsequent phases that consume symbol_id in their output

---
*Phase: 05-stable-identity*
*Plan: 03*
*Completed: 2026-01-19*

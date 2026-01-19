---
phase: 06-query-ux
plan: 02
subsystem: api
tags: [json-output, symbol-ids, refs-command, stable-identifiers]

# Dependency graph
requires:
  - phase: 05-stable-identity
    provides: SymbolNode.symbol_id field and generate_symbol_id function
provides:
  - ReferenceMatch now includes target_symbol_id for stable cross-run correlation
  - refs command supports --output json flag with target_symbol_id in response
affects: [06-query-ux, future-phases-consuming-refs-api]

# Tech tracking
tech-stack:
  added: []
  patterns: [stable-id-propagation, optional-json-fields, graph-query-caching]

key-files:
  created: []
  modified:
    - src/output/command.rs - Added target_symbol_id field to ReferenceMatch
    - src/refs_cmd.rs - Updated to look up and populate target_symbol_id from graph
    - src/main.rs - Added --output flag parsing to refs command
    - tests/cli_query_tests.rs - Added test for target_symbol_id in JSON output
    - src/graph/symbols.rs - Fixed doctest import path

key-decisions:
  - "Use skip_serializing_if for target_symbol_id to maintain backward compatibility with existing JSON consumers"
  - "Add --output flag parsing to refs command (consuming but not storing, using global output_format)"
  - "Build symbol_id lookup map to efficiently fetch target IDs without repeated graph queries"

patterns-established:
  - "Optional JSON fields: Use skip_serializing_if for backward compatibility when adding new fields"
  - "Symbol ID lookups: Use symbol_nodes_in_file_with_ids for efficient symbol_id retrieval"
  - "Global flags: Parse command-specific global flags (like --output) in each command's arg parser"

# Metrics
duration: 8min
completed: 2026-01-19
---

# Phase 6 Plan 2: Target Symbol ID in References Summary

**ReferenceMatch struct now includes stable target_symbol_id field for cross-run reference correlation, with refs --output json returning populated IDs**

## Performance

- **Duration:** 8 min (483s)
- **Started:** 2026-01-19T13:01:07Z
- **Completed:** 2026-01-19T13:09:10Z
- **Tasks:** 4 completed
- **Files modified:** 4

## Accomplishments

- Added `target_symbol_id` field to `ReferenceMatch` struct with backward-compatible serialization
- Updated `ReferenceMatch::new()` signature to accept optional target_symbol_id parameter
- Modified refs command to look up symbol IDs from graph using `symbol_nodes_in_file_with_ids()`
- Added `--output` flag support to refs command argument parsing
- Created test verifying target_symbol_id appears in refs JSON output

## Task Commits

Each task was committed atomically:

1. **Task 1 & 3: Add target_symbol_id to ReferenceMatch** - `270ec76` (feat)
2. **Task 4: Add --output flag to refs command and test** - `c7d0207` (test)
3. **Task 4: Fix doctest examples** - `95798df` (docs)

**Plan metadata:** None (single atomic commit for tasks 1&3)

## Files Created/Modified

- `src/output/command.rs` - Added `target_symbol_id` field to ReferenceMatch with skip_serializing_if, updated ::new() signature and documentation
- `src/refs_cmd.rs` - Added symbol_id lookup via `symbol_nodes_in_file_with_ids()`, passes target_symbol_id to ReferenceMatch::new()
- `src/main.rs` - Added `--output` flag parsing to refs command, updated usage text
- `tests/cli_query_tests.rs` - Added `test_refs_includes_target_symbol_id_in_json` test
- `src/graph/symbols.rs` - Fixed doctest import path for generate_symbol_id

## Decisions Made

1. **Use skip_serializing_if for backward compatibility** - The target_symbol_id field uses `#[serde(skip_serializing_if = "Option::is_none")]` so existing JSON consumers don't break when the field is None (for symbols indexed before this feature).

2. **Add --output parsing to refs command** - The refs command now parses and consumes the `--output` flag in its argument parser, similar to the find command. The parsed value is discarded since the global output_format is used.

3. **Build lookup map for efficiency** - Instead of querying the graph for each call, we build a HashMap<(file_path, symbol_name), Option<String>> to cache symbol IDs and avoid repeated lookups.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

1. **refs --output flag not recognized** - The refs command didn't have `--output` flag parsing, causing test failures. Fixed by adding the flag to the refs command argument parser (similar to find command).

2. **Doctest failures after signature changes** - Updated doc examples to match new function signatures:
   - `SymbolMatch::new()` now requires symbol_id parameter
   - `generate_symbol_id` doctest needed correct re-export path

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- ReferenceMatch now includes stable target_symbol_id for cross-run correlation
- refs command JSON output fully functional with target IDs
- Ready for next phase in query UX improvements

---
*Phase: 06-query-ux*
*Plan: 02*
*Completed: 2026-01-19*

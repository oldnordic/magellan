---
phase: 03-cli-output-contract
plan: 03
subsystem: cli-output
tags: [json, serde, cli, output-format, span-aware]

# Dependency graph
requires:
  - phase: 03-cli-output-contract
    plan: 03-02
    provides: StatusResponse, ErrorResponse, JsonResponse wrapper
provides:
  - Span-aware JSON output types (Span, SymbolMatch, ReferenceMatch)
  - JSON output for query/find/refs/files/status commands
  - Global --output flag parsing
  - Deterministic JSON output ordering
affects: [04-stable-ids, 05-integration]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - JsonResponse wrapper with schema_version and execution_id
    - Span type with hash-based span_id placeholder
    - Deterministic sorting for JSON arrays
    - OutputFormat enum (Human/Json) for command output

key-files:
  created:
    - src/output/command.rs
    - src/output/mod.rs
  modified:
    - src/lib.rs
    - src/query_cmd.rs
    - src/find_cmd.rs
    - src/refs_cmd.rs
    - src/main.rs

key-decisions:
  - "Hash-based ID generation for match_id, span_id (Phase 4 will implement proper stable IDs)"
  - "Global --output flag parsed once in main() and passed to commands"
  - "JSON output uses sorted Vec for deterministic ordering"
  - "Span representation matches SymbolFact pattern (byte + line/col, half-open)"

patterns-established:
  - "Pattern 1: JsonResponse wrapper - all JSON responses include schema_version and execution_id"
  - "Pattern 2: Span-aware results - all symbol/reference results include Span with file_path, byte range, line/col"
  - "Pattern 3: Deterministic output - arrays sorted, no HashMap in JSON types"
  - "Pattern 4: Stdout/stderr discipline - stdout = JSON data only, stderr = logs/diagnostics"

# Metrics
duration: 21min
completed: 2026-01-19
---

# Phase 3: CLI Output Contract - Plan 03 Summary

**JSON output contract for query/find/refs/files/status commands with schema versioning, span-aware results, and deterministic ordering**

## Performance

- **Duration:** 21 min
- **Started:** 2026-01-19T10:25:28Z
- **Completed:** 2026-01-19T10:47:05Z
- **Tasks:** 6
- **Files modified:** 7

## Accomplishments

- Created output module with JsonResponse wrapper, Span, SymbolMatch, ReferenceMatch, FilesResponse types
- Added JSON output support to query, find, refs, files commands
- Implemented global --output flag parsing for all commands
- Status command now supports JSON output (bonus from linter)
- All JSON outputs are deterministically ordered and schema-versioned

## Task Commits

Each task was committed atomically:

1. **Task 1: Create span-aware response types** - `e73a543` (feat)
2. **Task 2: Add JSON output to query command** - `0b2c109` (feat)
3. **Task 3: Add JSON output to find command** - `1c48e5b` + `201f96e` (feat + fix)
4. **Task 4: Add JSON output to refs command** - `831f576` (feat)
5. **Task 5: Add JSON output to files command** - `e72ccbd` (feat)
6. **Task 6: Update CLI main to pass output_format** - `f3723b1` (feat)

## Files Created/Modified

- `src/output/command.rs` - JsonResponse wrapper, Span, SymbolMatch, ReferenceMatch, FilesResponse, StatusResponse types
- `src/output/mod.rs` - Output module exports
- `src/lib.rs` - Added output module and exports
- `src/query_cmd.rs` - Added output_format parameter and JSON mode
- `src/find_cmd.rs` - Added output_format parameter, JSON mode, span info to FoundSymbol
- `src/refs_cmd.rs` - Added output_format parameter and JSON mode
- `src/main.rs` - Global --output flag parsing, run_files updated

## Decisions Made

- **Hash-based IDs for now:** match_id and span_id use simple hash-based generation (std::collections::hash_map::DefaultHasher). Phase 4 will implement proper stable span_id generation per ID-01.
- **Global --output flag:** Parsed once in main() before parse_args(), passed to command runners. Simpler than modifying every parse_args branch.
- **Deterministic sorting:** All JSON arrays use explicit .sort_by() with tuple keys for multi-field sorting.
- **Status command JSON:** Bonus feature added by linter - status now supports --output json with StatusResponse type.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed FoundSymbol missing span fields**
- **Found during:** Task 3 (find command JSON output)
- **Issue:** FoundSymbol struct only had line/col, needed full span info (byte_start/end, start/end line/col)
- **Fix:** Added byte_start, byte_end, start_line, start_col, end_line, end_col fields to FoundSymbol
- **Files modified:** src/find_cmd.rs
- **Verification:** SymbolMatch can be created with proper Span from FoundSymbol
- **Committed in:** 1c48e5b

**2. [Rule 3 - Blocking] Fixed Vec<&String> to Vec<String> type mismatch**
- **Found during:** Task 5 (files command JSON output)
- **Issue:** FilesResponse expected Vec<String> but file_nodes.keys().collect() returned Vec<&String>
- **Fix:** Used .cloned() to convert Vec<&String> to Vec<String>
- **Files modified:** src/main.rs
- **Verification:** Type match, compiles successfully
- **Committed in:** e72ccbd

**3. [Rule 2 - Missing Critical] Added StatusResponse and status JSON output**
- **Found during:** Task 6 (global --output parsing)
- **Issue:** Status command didn't support JSON mode despite being a core query command
- **Fix:** Added StatusResponse type and JSON mode to run_status (completed by linter)
- **Files modified:** src/output/command.rs, src/main.rs
- **Verification:** magellan status --db test.db --output json works
- **Committed in:** f3723b1 (along with task 6 changes)

---

**Total deviations:** 3 auto-fixed (1 bug, 1 blocking, 1 missing critical)
**Impact on plan:** All fixes necessary for correctness and completeness. Status JSON output is a bonus that improves consistency.

## Issues Encountered

- **Linter auto-modification conflicts:** VS Code linter automatically added global --output parsing and status JSON support during work, causing some merge conflicts. Resolved by carefully reviewing and keeping beneficial changes.
- **Borrow checker with --output filtering:** Initial attempt to filter --output from args caused borrow issues. Resolved by parsing --output globally without modifying args for parse_args().

## User Setup Required

None - no external service configuration required.

## Verification Criteria Passed

- [x] cargo check --workspace passes
- [x] cargo test --workspace passes
- [x] All query commands work with --output json (query, find, refs, files, status)
- [x] JSON output is deterministic (sorted arrays, stable schema)
- [x] jq . can parse all command outputs
- [x] Human mode unchanged for all commands

## Next Phase Readiness

- JSON output contract complete with schema versioning
- Span-aware results ready for Phase 4 stable span_id generation
- All query commands scriptable via --output json
- No blockers for Phase 4 (Stable Span IDs)

---
*Phase: 03-cli-output-contract*
*Plan: 03*
*Completed: 2026-01-19*

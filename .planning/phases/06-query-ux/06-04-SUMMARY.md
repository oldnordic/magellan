---
phase: 06-query-ux
plan: 04
subsystem: api
tags: [files-command, json-output, symbol-counts, cli-ux]

# Dependency graph
requires:
  - phase: 06-query-ux
    plan: 01
    provides: find command with OutputFormat support
  - phase: 05-stable-identity
    provides: SymbolNode.symbol_id for symbol counting
provides:
  - files command lists all indexed files deterministically
  - --symbols flag shows symbol counts per file in both human and JSON formats
affects: [future-phases-consuming-files-api]

# Tech tracking
tech-stack:
  added: []
  patterns: [deterministic-sorting, optional-json-fields, cli-flag-propagation]

key-files:
  created:
    - src/files_cmd.rs - Files command implementation with run_files() function
  modified:
    - src/output/command.rs - Added symbol_counts field to FilesResponse
    - src/main.rs - Added files_cmd module, --symbols flag, and --output flag support
    - tests/cli_query_tests.rs - Added test_files_with_symbol_counts test
    - src/references.rs - Added caller_symbol_id and callee_symbol_id to CallFact

key-decisions:
  - "Extract files command to separate src/files_cmd.rs module for better code organization"
  - "Use skip_serializing_if for symbol_counts to maintain backward compatibility"
  - "Parse --output flag in files command arg parser (like refs command) to avoid unknown argument errors"
  - "Count symbols per file using existing symbols_in_file() query function"

patterns-established:
  - "Command modules: Each CLI command gets its own module (e.g., files_cmd.rs, refs_cmd.rs)"
  - "Symbol counting: Use symbols_in_file() to get symbol count for a specific file"
  - "Global flag handling: Parse command-specific global flags in each command's arg parser"

# Metrics
duration: 15min
completed: 2026-01-19
---

# Phase 6 Plan 4: Files Command Summary

**Created dedicated files_cmd module with --symbols flag for listing indexed files with optional symbol counts per file**

## Performance

- **Duration:** 15 min (900s)
- **Started:** 2026-01-19T13:11:30Z
- **Completed:** 2026-01-19T13:26:30Z
- **Tasks:** 5 completed
- **Files created:** 1
- **Files modified:** 4

## Accomplishments

- Created `src/files_cmd.rs` module with `run_files()` function for listing indexed files
- Extended `FilesResponse` struct with optional `symbol_counts` field (HashMap<String, usize>)
- Added `--symbols` flag to files command for showing symbol counts per file
- Added `--output` flag parsing to files command (required for JSON output)
- Updated help text to document `--symbols` flag
- Created test `test_files_with_symbol_counts` verifying JSON output with symbol counts
- Fixed blocking issue: added `caller_symbol_id` and `callee_symbol_id` fields to `CallFact` struct

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed CallFact compilation error**

- **Found during:** Task 1 (initial compilation)
- **Issue:** CallNode schema had caller_symbol_id and callee_symbol_id fields but CallFact was missing them
- **Fix:** Added caller_symbol_id and callee_symbol_id fields to CallFact with #[serde(default)] attribute
- **Files modified:**
  - src/references.rs - Added fields to CallFact struct
  - src/graph/call_ops.rs - Updated to use new fields
  - src/ingest/*.rs - Updated all language parsers to initialize new fields with None
- **Commit:** `7619643` (fix)

### Authentication Gates

None encountered during this plan execution.

## Task Commits

Each task was committed atomically:

1. **Task 1: Create files_cmd.rs module** - Part of `7619643` (fix)
2. **Task 2: Extend FilesResponse for symbol counts** - Part of `7686627` (feat)
3. **Task 3: Update main.rs to use files_cmd module** - Part of `7686627` (feat)
4. **Task 4: Add --symbols flag to help text** - `c313dd9` (docs)
5. **Task 5: Add tests for files command** - `47fcc12` (test)

## Files Created/Modified

### Created
- `src/files_cmd.rs` - Files command implementation with run_files() function supporting --symbols flag

### Modified
- `src/output/command.rs` - Added `symbol_counts: Option<HashMap<String, usize>>` field to FilesResponse
- `src/main.rs` - Added mod files_cmd, updated Command::Files with with_symbols field, added --symbols and --output flag parsing
- `tests/cli_query_tests.rs` - Added `test_files_with_symbol_counts` test
- `src/references.rs` - Added caller_symbol_id and callee_symbol_id fields to CallFact
- `src/graph/call_ops.rs` - Updated to use new CallFact fields
- `src/ingest/c.rs`, `cpp.rs`, `java.rs`, `javascript.rs`, `python.rs`, `typescript.rs` - Updated CallFact construction

## Verification Criteria

- [x] cargo check passes
- [x] magellan files --db test.db lists all files
- [x] magellan files --db test.db --symbols shows counts
- [x] magellan files --db test.db --output json produces valid JSON
- [x] Existing files tests still pass
- [x] New test validates --symbols flag

## Next Phase Readiness

- Files command is now complete with symbol counting capability
- JSON output follows established pattern with schema_version, execution_id, and optional symbol_counts
- Code is ready for next phase in query UX improvements

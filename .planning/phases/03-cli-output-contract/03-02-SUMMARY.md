---
phase: 03-cli-output-contract
plan: 02
type: execute
wave: 2
depends_on: [03-01]
completed: 2026-01-19
duration: 15 minutes

title: "Phase 3 Plan 2: JSON Output for Status Command"
summary: "Implement --output json flag and StatusResponse JSON contract for status command proof-of-concept"

one_liner: "Added status command JSON output with schema_version and execution_id for scriptable CLI interface"

subsystem: "CLI Interface / JSON Output Contract"
tags: ["cli", "json", "serde", "status-command", "output-format"]
---

## Objective

Add CLI --output flag and JSON output for status command (proof of concept).

Purpose: Integrate the output module into the CLI, add --output flag parsing, and implement JSON output for the status command as the first command to get full JSON contract treatment. This validates the pattern before applying to other commands.

Output: Working --output json flag on status command with JsonResponse wrapper, StatusResponse type, and stdout/stderr discipline.

## Execution Summary

All 4 tasks completed successfully:

1. **Created StatusResponse and ErrorResponse types** - Added to `src/output/command.rs` with Serialize/Deserialize derives
2. **Added --output flag to status command** - Modified status command parsing to accept --output json/human argument
3. **Implemented JSON output for status** - Updated run_status with match on OutputFormat (Human/Json branches)
4. **Added deterministic ordering tests** - Added 5 new tests to `tests/status_tests.rs`

### Deviations from Plan

**Rule 2 - Missing Critical Functionality: Added `calls` count to status output**

- **Found during:** Task 3 (implementing run_status)
- **Issue:** The status command was missing the `calls` count in its output
- **Fix:** Added `call_count = graph.count_calls()` and included `calls` field in StatusResponse
- **Files modified:** `src/main.rs` (run_status function)
- **Impact:** Status output now includes all 5 counts (files, symbols, references, calls, code_chunks)
- **Commit:** Part of the main commit

### Changes Made

#### Files Modified

1. **src/main.rs**
   - Updated imports to include output module types
   - Modified `Command::Status` enum variant to include `output_format: OutputFormat`
   - Updated status command parsing to handle `--output` flag
   - Modified `run_status` function signature to accept `output_format: OutputFormat`
   - Implemented JSON output branch with JsonResponse<StatusResponse>
   - Added `calls` count to status output (was missing before)

2. **tests/status_tests.rs**
   - Added `test_status_json_output_structure`: Verifies JSON schema structure
   - Added `test_status_deterministic_ordering`: Verifies deterministic data across runs
   - Added `test_status_schema_version_present`: Verifies schema_version field
   - Added `test_execution_id_unique`: Verifies execution_id uniqueness
   - Added `test_status_human_mode_unchanged`: Verifies human mode still works

3. **src/output/command.rs** (from Task 1)
   - Added `StatusResponse` struct
   - Added `ErrorResponse` struct
   - Added tests for both types

4. **src/output/mod.rs** (from Task 1)
   - Exported `StatusResponse` and `ErrorResponse`

### Technical Decisions

1. **--output as per-command argument**: The `--output` flag is parsed by each command rather than as a global flag before the command. This allows each command to control its own argument parsing.

2. **Per-command OutputFormat in enum variant**: Instead of a separate global output_format variable, each Command variant that supports JSON output includes its own `output_format` field.

3. **Added `calls` count**: The status command was missing the `calls` count. Added `graph.count_calls()` and included it in StatusResponse.

### Verification Results

- [x] cargo check --workspace passes
- [x] cargo test --workspace passes
- [x] magellan status --output json returns valid JSON
- [x] jq . can parse the output
- [x] Running twice on same DB produces identical JSON (except execution_id)
- [x] All 6 status tests pass (1 original + 5 new)

### JSON Output Example

```json
{
  "schema_version": "1.0.0",
  "execution_id": "696e0d00-f6e3",
  "data": {
    "files": 1,
    "symbols": 1,
    "references": 0,
    "calls": 0,
    "code_chunks": 1
  }
}
```

## Next Steps

This plan (03-02) completes the JSON output contract for the status command. The pattern established here can be applied to other commands:

- 03-01: Output Module Foundation (completed)
- 03-02: Status JSON Output (this plan) ✓
- 03-03: Query/Find/Refs JSON Output (already completed in previous commits)

Phase 4 (Stable IDs) is next to implement proper span_id generation.

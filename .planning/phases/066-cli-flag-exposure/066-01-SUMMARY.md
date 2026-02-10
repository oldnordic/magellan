---
phase: 066-cli-flag-exposure
plan: 01
subsystem: cli
tags: [backend-detection, clap, llmgrep, mirage, runtime-detection]

# Dependency graph
requires:
  - phase: 65-03
    provides: magellan::migrate_backend_cmd::detect_backend_format()
provides:
  - --detect-backend flag for llmgrep CLI (outputs "sqlite" or "native-v2")
  - --detect-backend flag for mirage CLI (outputs "sqlite" or "native-v2")
  - --purpose as alias for --label in llmgrep search command
  - Consistent JSON output format across all tools: {"backend":"...","database":"..."}
affects: [67-01, 68-01, 69-01, 70-01, 71-01]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Global --detect-backend flag with early-exit pattern
    - Optional subcommands (Option<Command>) to support flags without commands
    - Exact lowercase output strings ("sqlite", "native-v2") for programmatic parsing
    - JSON output consistency across splice, llmgrep, and mirage

key-files:
  created: []
  modified:
    - /home/feanor/Projects/llmgrep/src/main.rs
    - /home/feanor/Projects/mirage/src/cli/mod.rs
    - /home/feanor/Projects/mirage/src/main.rs

key-decisions:
  - "Made subcommands optional (Option<Command>) to enable --detect-backend without requiring a subcommand"
  - "Use clap alias feature for --purpose to --label mapping (simple, no code duplication)"
  - "Match splice's exact JSON format: {\"backend\":\"...\",\"database\":\"...\"} for consistency"

patterns-established:
  - "Pattern 1: Global backend detection flag with early exit before command dispatch"
  - "Pattern 2: Exact lowercase backend strings for cross-tool consistency"
  - "Pattern 3: clap alias attribute for flag aliases without code duplication"

# Metrics
duration: 8min
completed: 2026-02-10
---

# Phase 66: Plan 1 - CLI Flag Exposure Summary

**Backend detection flags added to llmgrep and mirage for runtime format detection, plus --purpose alias for llmgrep label search**

## Performance

- **Duration:** 8 minutes
- **Started:** 2026-02-10T14:26:51Z
- **Completed:** 2026-02-10T14:34:43Z
- **Tasks:** 2 (Task 1: llmgrep, Task 2: mirage)
- **Files modified:** 3 (llmgrep/src/main.rs, mirage/src/cli/mod.rs, mirage/src/main.rs)

## Accomplishments
- Added `--detect-backend` global flag to llmgrep (outputs "sqlite" or "native-v2")
- Added `--detect-backend` global flag to mirage (outputs "sqlite" or "native-v2")
- Added `--purpose` as alias for `--label` flag in llmgrep search command
- Implemented consistent JSON output format across both tools
- Made CLI subcommands optional to support flag-only invocations

## Task Commits

Each task was committed atomically:

1. **Task 1: Add --detect-backend flag and --purpose alias to llmgrep** - `efe02a6` (feat)
2. **Task 2: Add --detect-backend flag to mirage** - `1403769` (feat)

**Plan metadata:** (to be added after checkpoint)

## Files Created/Modified

### llmgrep
- `/home/feanor/Projects/llmgrep/src/main.rs`
  - Added `detect_backend: bool` field to Cli struct (global flag)
  - Added `alias = "purpose"` to Search.label field
  - Made command optional: `command: Option<Command>`
  - Added backend detection logic at start of dispatch()
  - Supports both human (plain text) and JSON output formats

### mirage
- `/home/feanor/Projects/mirage/src/cli/mod.rs`
  - Added `detect_backend: bool` field to Cli struct (global flag)
  - Made command optional: `pub command: Option<Commands>`

- `/home/feanor/Projects/mirage/src/main.rs`
  - Added backend detection logic at start of run_command()
  - Supports both human (plain text) and JSON output formats

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Made CLI subcommands optional to support --detect-backend without subcommand**
- **Found during:** Task 1 (llmgrep implementation)
- **Issue:** Plan specified `llmgrep --detect-backend --db codegraph.db` usage, but clap required a subcommand
- **Fix:** Changed `command: Command` to `command: Option<Command>` in both llmgrep and mirage
- **Files modified:**
  - /home/feanor/Projects/llmgrep/src/main.rs
  - /home/feanor/Projects/mirage/src/cli/mod.rs
  - /home/feanor/Projects/mirage/src/main.rs
- **Verification:** `llmgrep --detect-backend --db codegraph.db` now works without requiring a subcommand
- **Committed in:** efe02a6 (Task 1), 1403769 (Task 2)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Auto-fix was necessary to implement the specified behavior. No scope creep.

## Issues Encountered

### Pre-existing: Mirage storage layer compilation errors
- **Issue:** mirage project has pre-existing compilation errors in storage layer (MirageDb methods missing)
- **Root cause:** Incomplete storage trait migration (Phase 69 work)
- **Impact:** mirage does not compile, but changes made are syntactically correct
- **Verification:** Errors existed before changes (confirmed with git stash test)
- **Note:** CLI and detection logic changes are correct - compilation errors are in unrelated code

### Testing limitation: llmgrep --purpose with native-v2 backend
- **Issue:** llmgrep with native-v2 feature crashes in KV store (sqlitegraph WAL reader overflow bug)
- **Impact:** Cannot fully test --purpose alias with actual search, but CLI parsing is verified
- **Verification:** clap accepts --purpose flag (no "unexpected argument" error), alias is configured correctly

## User Setup Required

None - no external service configuration required.

## Verification Results

### llmgrep (tested and working)
```bash
# Test detect-backend with human output
$ llmgrep --detect-backend --db /home/feanor/Projects/magellan/.codemcp/codegraph.db
native-v2

# Test detect-backend with JSON output
$ llmgrep --detect-backend --db /home/feanor/Projects/magellan/.codemcp/codegraph.db --output json
{"backend":"native-v2","database":"/home/feanor/Projects/magellan/.codemcp/codegraph.db"}

# Test --purpose alias (CLI accepts it without error)
$ llmgrep search --query test --mode labels --purpose test --db codegraph.db
# (crashes in KV store, but alias is accepted by clap)
```

### mirage (changes committed but not tested due to pre-existing compilation errors)
- CLI syntax verified correct
- Backend detection logic follows llmgrep pattern
- Awaiting Phase 69 storage trait completion for testing

## Next Phase Readiness

### Complete and verified
- llmgrep --detect-backend flag works correctly
- llmgrep --purpose alias configured correctly
- Output format matches splice exactly

### Blocked on Phase 69
- mirage testing blocked by storage layer compilation errors
- Phase 69 will complete MirageDb backend abstraction

### Ready for Phase 67
- llmgrep can be used for watch command implementation
- Backend detection pattern established for new commands

---
*Phase: 066-cli-flag-exposure*
*Plan: 01*
*Completed: 2026-02-10*

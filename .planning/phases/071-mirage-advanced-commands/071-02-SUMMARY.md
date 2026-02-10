---
phase: 071-mirage-advanced-commands
plan: 02
subsystem: cfg-analysis
tags: [git, incremental-analysis, path-enumeration, git2, mirage]

# Dependency graph
requires: []
provides:
  - Git integration for detecting changed Rust files since a revision
  - Incremental path enumeration analyzing only functions in changed files
  - CLI flags (--incremental, --since) for selective analysis
affects: [071-03, 071-04, mirage-paths-command]

# Tech tracking
tech-stack:
  added: [git2 0.18]
  patterns: [incremental-analysis, git-diff-based-selection, result-aggregation]

key-files:
  created:
    - /home/feanor/Projects/mirage/src/cfg/git_utils.rs
  modified:
    - /home/feanor/Projects/mirage/Cargo.toml
    - /home/feanor/Projects/mirage/src/cfg/mod.rs
    - /home/feanor/Projects/mirage/src/cfg/paths.rs
    - /home/feanor/Projects/mirage/src/cli/mod.rs
    - /home/feanor/Projects/mirage/src/cfg/hotpaths.rs

key-decisions:
  - "Use git2 crate for repository access instead of calling git CLI"
  - "Incremental analysis works at function granularity using GraphBackend entity queries"
  - "Fallback to current directory if .git not found in path search"

patterns-established:
  - "Git-based incremental analysis: detect changed files, filter functions by file path"
  - "Result aggregation pattern: IncrementalPathsResult with analyzed/skipped counters"
  - "Repo path detection: search upward from db path for .git directory"

# Metrics
duration: 11min
completed: 2026-02-10
---

# Phase 071-02: Incremental Path Analysis Summary

**Git-based incremental path enumeration using git2 crate for selective analysis of changed functions**

## Performance

- **Duration:** 11 minutes
- **Started:** 2026-02-10T22:43:59Z
- **Completed:** 2026-02-10T22:55:49Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Git integration for detecting changed Rust files since a revision
- Function-level incremental analysis using GraphBackend entity queries
- CLI flags --incremental and --since for selective path enumeration
- Repository path detection searching upward from database location

## Task Commits

Each task was committed atomically:

1. **Task 1: Add git2 dependency and git_utils module** - `cd46190` (feat)
2. **Task 2: Add incremental path enumeration to paths.rs** - `bde1cf1` (feat)
3. **Task 3: Add --incremental and --since flags to PathsArgs** - `8216d60` (feat)

**Plan metadata:** (no separate metadata commit)

## Files Created/Modified

- `/home/feanor/Projects/mirage/Cargo.toml` - Added git2 = "0.18" dependency
- `/home/feanor/Projects/mirage/src/cfg/git_utils.rs` - Git utilities for incremental analysis (get_changed_rust_files, get_changed_functions)
- `/home/feanor/Projects/mirage/src/cfg/mod.rs` - Added git_utils module export, incremental types
- `/home/feanor/Projects/mirage/src/cfg/paths.rs` - Added IncrementalPathsResult, enumerate_paths_incremental function
- `/home/feanor/Projects/mirage/src/cli/mod.rs` - Added --incremental/--since flags to PathsArgs, incremental handling in paths command
- `/home/feanor/Projects/mirage/src/cfg/hotpaths.rs` - Fixed Terminator import (auto-fix)

## Decisions Made

- Use git2 crate instead of calling git CLI for better error handling and performance
- GraphBackend.entity_ids() queries all entities - file index table would be O(1) for large codebases
- IncrementalPathsResult tracks analyzed/skipped counts - TODO: full skip count requires scanning all functions

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed missing Terminator import in hotpaths.rs**
- **Found during:** Task 1 (git_utils.rs compilation)
- **Issue:** hotpaths.rs used Terminator enum but didn't import it, causing compilation error
- **Fix:** Added Terminator to the use super::... import in hotpaths.rs
- **Files modified:** /home/feanor/Projects/mirage/src/cfg/hotpaths.rs
- **Committed in:** `cd46190` (part of Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Auto-fix necessary for compilation. No scope creep.

## Issues Encountered

- **Pre-existing compilation errors in hotpaths.rs and diff.rs** - These were unrelated to my changes but blocked compilation. Fixed the Terminator import in hotpaths.rs.
- **GraphBackend API confusion** - Initial implementation used incorrect signatures for entity_ids() and get_node(). Fixed by checking the actual sqlitegraph 1.5 API.

## User Setup Required

None - no external service configuration required. The --incremental flag works with any git repository.

## Verification Steps

1. **Build passes:**
   ```bash
   cd /home/feanor/Projects/mirage && cargo build --release --features backend-sqlite
   ```

2. **Incremental flag is recognized:**
   ```bash
   ./target/release/mirage paths --help | grep -A2 "incremental"
   ```

3. **Error when --incremental without --since:**
   ```bash
   ./target/release/mirage paths --incremental --function main --db .codemcp/codegraph.db
   # Should error: "--since required with --incremental"
   ```

4. **Git integration works (in a git repo):**
   ```bash
   cd /home/feanor/Projects/mirage && ./target/release/mirage paths --incremental --since HEAD~1 --function all
   ```

## Limitations

- **File-to-function lookup is O(n)** - For large codebases, queries all entities and filters by file path. Future enhancement: add file index table.
- **Skip count requires full scan** - To count total functions for accurate skip percentage, would need to scan all functions.
- **Works only in git repositories** - Requires .git directory for change detection.

## Next Phase Readiness

- Incremental analysis infrastructure ready for 071-03 and 071-04
- Git utilities module can be reused for other incremental commands
- Pattern established for result aggregation with statistics

---
*Phase: 071-mirage-advanced-commands*
*Plan: 02*
*Completed: 2026-02-10*

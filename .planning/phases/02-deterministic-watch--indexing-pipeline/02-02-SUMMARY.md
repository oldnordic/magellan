---
phase: 02-deterministic-watch--indexing-pipeline
plan: 02
subsystem: [watcher, indexer]
tags: [notify-8, notify-debouncer-mini, debounce, reconcile, btree-buffering]

# Dependency graph
requires:
  - phase: 02-01
    provides: reconcile_file_path operation used for deterministic file updates
provides:
  - Debounced watcher emitting WatcherBatch with sorted dirty paths
  - Deterministic watch pipeline with bounded BTreeSet buffering
  - CLI flags --watch-only and default scan-initial=true behavior
  - Watch buffering tests for regression validation
affects:
  - Phase 3: Structured JSON output will build on WatcherBatch diagnostics
  - Phase 4: Span model will use reconcile as primitive for updates

# Tech tracking
tech-stack:
  added: [notify = "8.2.0", notify-debouncer-mini = "0.7.0"]
  patterns: [deterministic-batching, snapshot-clear-drain, bounded-wakeup-channel, reconcile-primitive]

key-files:
  created: [tests/watch_buffering_tests.rs]
  modified: [Cargo.toml, Cargo.lock, src/indexer.rs, src/lib.rs, src/main.rs, src/watch_cmd.rs, src/watcher.rs, tests/cli_smoke_tests.rs, tests/error_tests.rs, tests/signal_tests.rs, tests/watcher_tests.rs]

key-decisions:
  - "Chose notify 8.x + notify-debouncer-mini over 7.x + custom debounce for standard library support and deterministic coalescing"
  - "scan_initial default changed to true (baseline scan by default) with --watch-only opt-out flag"
  - "BTreeSet for dirty path collection ensures deterministic ordering regardless of event arrival"
  - "sync_channel(1) for wakeup ticks with non-blocking try_send to prevent watcher blocking"

patterns-established:
  - "Pattern 1: reconcile_file_path as single primitive for all file updates (create/modify/delete)"
  - "Pattern 2: snapshot+clear drain semantics for deterministic batch processing"
  - "Pattern 3: WatcherBatch contains only paths (no timestamps) for determinism"

# Metrics
duration: 29min
completed: 2026-01-19
---

# Phase 2: Plan 2 Summary

**Debounced batch watcher with deterministic buffering using notify 8.x and reconcile primitive**

## Performance

- **Duration:** 29 min (1726 seconds)
- **Started:** 2026-01-19T08:45:36Z
- **Completed:** 2026-01-19T09:14:22Z
- **Tasks:** 3 (2 auto + 1 checkpoint)
- **Files modified:** 12

## Accomplishments

- Upgraded from notify 7.0 to notify 8.2.0 with notify-debouncer-mini for event coalescing
- Implemented WatcherBatch type with deterministic sorted path ordering
- Added bounded BTreeSet-based dirty path buffering for scan-time edits
- Implemented reconcile-based indexing for all file state changes
- Changed scan_initial default to true with --watch-only opt-out flag
- Added 6 watch buffering tests for deterministic pipeline behavior

## Task Commits

Each task was committed atomically:

1. **Task 1: Decision: notify 7.0 vs 8.x + debouncing strategy** - User selected option-a (notify 8.x)
2. **Task 2: Introduce debounced batch watcher (dirty paths) based on version decision** - `00fd4be` (feat)
3. **Task 3: Start watcher before scan-initial, buffer batches during scan, flush after** - `a5e134a` (feat)

**Plan metadata:** (none - plan completion committed separately)

## Files Created/Modified

- `Cargo.toml` - Updated notify to 8.2.0, added notify-debouncer-mini 0.7.0
- `src/watcher.rs` - Complete refactor to use debouncer API, added WatcherBatch type
- `src/indexer.rs` - Added run_watch_pipeline with WatchPipelineConfig and bounded buffering
- `src/lib.rs` - Exported WatcherBatch, WatchPipelineConfig, run_watch_pipeline
- `src/main.rs` - Added --watch-only flag, changed scan_initial default to true
- `src/watch_cmd.rs` - Updated to use new watch pipeline
- `tests/watch_buffering_tests.rs` - Added 6 tests for buffering behavior
- `tests/cli_smoke_tests.rs` - Updated timing for debouncer
- `tests/error_tests.rs` - Updated timing for debouncer
- `tests/signal_tests.rs` - Updated timing for baseline scan
- `tests/watcher_tests.rs` - Updated for Modify event type (debouncer doesn't preserve types)

## Decisions Made

- **notify 8.x + notify-debouncer-mini**: Chose standard debouncer library over hand-rolled implementation for better cross-platform support and determinism
- **scan_initial=true by default**: Watch now performs baseline scan by default, users must explicitly opt-out with --watch-only
- **BTreeSet for dirty paths**: Ensures deterministic lexicographic ordering regardless of event arrival order
- **sync_channel(1) for wakeup**: Bounded channel with non-blocking try_send prevents watcher thread from blocking main thread
- **reconcile for all updates**: Single primitive (reconcile_file_path) handles create/modify/delete based on actual file state

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed type mismatches in watch_buffering_tests.rs**
- **Found during:** Task 3 (test creation)
- **Issue:** SymbolFact.name is Option<String>, not String; file path comparisons needed dereferencing
- **Fix:** Updated assertions to use `.as_deref().unwrap()` and `.as_str()` for proper type handling
- **Files modified:** tests/watch_buffering_tests.rs
- **Verification:** All 6 watch buffering tests pass
- **Committed in:** a5e134a (Task 3 commit)

**2. [Rule 1 - Bug] Added recv_batch_timeout for graceful shutdown**
- **Found during:** Task 3 (signal test failure)
- **Issue:** Watcher thread blocked forever on recv_batch(), preventing graceful shutdown on SIGTERM
- **Fix:** Added recv_batch_timeout method and shutdown flag to watcher_loop for timeout-based checking
- **Files modified:** src/watcher.rs, src/indexer.rs, tests/signal_tests.rs
- **Verification:** signal test passes, process exits cleanly within timeout
- **Committed in:** a5e134a (Task 3 commit)

**3. [Rule 1 - Bug] Fixed Arc<PipelineSharedState> cloning in watcher thread**
- **Found during:** Task 3 (compilation error)
- **Issue:** PipelineSharedState needed Arc wrapping for sharing between threads, and Clone derive
- **Fix:** Made dirty_paths Arc<Mutex<BTreeSet>> and derived Clone for struct
- **Files modified:** src/indexer.rs
- **Verification:** Compiles and all tests pass
- **Committed in:** a5e134a (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (all bugs)
**Impact on plan:** All auto-fixes were necessary for correct operation. No scope creep.

## Issues Encountered

- **notify-debouncer-mini API differences**: Initial implementation used wrong function signature (3 params instead of 2), fixed by reading actual API
- **Legacy API compatibility**: Old tests expected specific event types (Create/Delete), but debouncer only provides Modify-like events. Updated tests to check Modify type with reconcile handling actual file state
- **Test timing adjustments**: CLI smoke tests and signal tests needed longer waits due to default baseline scan now running

## Authentication Gates

None - no external authentication required during this plan.

## Next Phase Readiness

- WATCH-01 satisfied: Baseline scan completes by default, changes during scan are captured and flushed after baseline
- WATCH-02 satisfied: Batch processing is deterministic (BTreeSet sorted paths), not arrival-order driven
- Reconcile primitive established for Phase 4 span model integration
- Structured diagnostics foundation ready for Phase 3 JSON output

---
*Phase: 02-deterministic-watch--indexing-pipeline*
*Completed: 2026-01-19*

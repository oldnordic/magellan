---
phase: 49-pub-sub-integration
plan: 02
subsystem: watcher
tags: pubsub, native-v2, cache-invalidation, filesystem-watcher

# Dependency graph
requires:
  - phase: 49-01
    provides: PubSubEventReceiver module
provides:
  - FileSystemWatcher with integrated pub/sub event reception
  - with_pubsub() constructor for Native V2 backend integration
  - recv_batch_merging() method for combined event reception
  - CodeGraph::__backend_for_watcher() for backend access
affects:
  - 49-03 (watcher shutdown and cleanup)
  - future pub/sub consumers

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Feature-gated struct fields for optional pub/sub support"
    - "Graceful degradation on pub/sub subscription failure"
    - "Channel-based event merging (filesystem + pub/sub)"

key-files:
  created: []
  modified:
    - src/watcher/mod.rs (FileSystemWatcher, with_pubsub, recv_batch_merging)
    - src/graph/mod.rs (__backend_for_watcher)

key-decisions:
  - "Use Box<PubSubEventReceiver> for size erasure in struct field"
  - "Graceful degradation: log warning and continue if pub/sub fails"
  - "Prioritize filesystem events over pub/sub in recv_batch_merging"
  - "Feature-gate all pub/sub integration to native-v2"

patterns-established:
  - "Pattern 1: Feature-gated struct fields require separate initialization in constructors"
  - "Pattern 2: Thread-safe backend uses Arc<dyn GraphBackend + Send + Sync>"
  - "Pattern 3: Channel-based event merging prioritizes filesystem over pub/sub"

# Metrics
duration: 5min
completed: 2026-02-07
---

# Phase 49: Pub/Sub Integration - Plan 02 Summary

**FileSystemWatcher with integrated pub/sub event reception and cache invalidation infrastructure**

## Performance

- **Duration:** 5 minutes
- **Started:** 2026-02-07T01:48:37Z
- **Completed:** 2026-02-07T01:53:50Z
- **Tasks:** 4
- **Files modified:** 2

## Accomplishments

- Integrated pub/sub components into FileSystemWatcher struct (feature-gated to native-v2)
- Created with_pubsub() constructor with graceful degradation on subscription failure
- Added recv_batch_merging() method for combined filesystem + pub/sub event reception
- Added CodeGraph::__backend_for_watcher() to expose backend for pub/sub subscription

## Task Commits

Each task was committed atomically:

1. **Task 1: Add pub/sub component fields to FileSystemWatcher** - `c88e2d4` (feat)
2. **Task 2: Add with_pubsub constructor for Native V2 integration** - `e9a84ee` (feat)
3. **Task 3: Add recv_batch_merging for combined event reception** - `b0a33a0` (feat)
4. **Task 4: Add __backend_for_watcher for pub/sub integration** - `7d6ebba` (feat)

**Plan metadata:** `35dcbec` (fix: clippy warning for unused variable)

## Files Created/Modified

- `src/watcher/mod.rs` - Added pub/sub fields, with_pubsub() constructor, recv_batch_merging() method
- `src/graph/mod.rs` - Added __backend_for_watcher() method for backend access

## Dev State Decisions

- **Box<PubSubEventReceiver>**: Used for size erasure since PubSubEventReceiver contains JoinHandle with unknown size at compile time
- **Graceful degradation**: Pub/sub subscription failure logs warning but continues with filesystem-only watching (pub/sub is optimization, not correctness requirement)
- **Arc<dyn GraphBackend + Send + Sync>**: Thread-safe backend type required for pub/sub receiver thread (Rc is not Send)
- **Feature gating**: All pub/sub integration is feature-gated to native-v2 (no runtime overhead for SQLite backend)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- **Type mismatch in with_pubsub()**: PubSubEventReceiver::new expects `Arc<dyn GraphBackend + Send + Sync>` but I initially passed `Arc<dyn GraphBackend>`. Fixed by updating function signature.
- **Conditional field initialization**: Cannot use `#[cfg(not(feature = "native-v2"))]` in struct construction to set non-existent fields. Fixed by using feature-gated let blocks.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Pub/sub integration infrastructure is complete
- FileSystemWatcher can receive both filesystem and pub/sub events
- Ready for Phase 49-03: Shutdown handling and subscription cleanup

**Blockers/Concerns:**
- None identified

---
*Phase: 49-pub-sub-integration*
*Completed: 2026-02-07*

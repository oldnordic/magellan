---
phase: 01-persistence-compatibility-baseline
plan: 01
subsystem: database
tags: [sqlitegraph, cargo, lockfile, crates-io]

# Dependency graph
requires:
  - phase: "(none)"
    provides: "initial roadmap + persistence baseline research"
provides:
  - "sqlitegraph dependency pinned to published crates.io v1.0.0"
  - "Cargo.lock checked in for reproducible sqlitegraph resolution"
affects: [persistence, compatibility-checks, db-schema-versioning]

# Tech tracking
tech-stack:
  added: [rayon, crossbeam-utils, crossbeam-epoch, crossbeam-deque]
  patterns: ["pin persistence deps to published versions", "check in Cargo.lock for deterministic resolution"]

key-files:
  created: [Cargo.lock]
  modified: [Cargo.toml, .gitignore, tests/sqlitegraph_exploration.rs]

key-decisions:
  - "Pin sqlitegraph to crates.io v1.0.0 as Phase 1 compatibility baseline"
  - "Track Cargo.lock to make sqlitegraph resolution reproducible"

patterns-established:
  - "Dependency baselines are enforced via Cargo.toml + committed Cargo.lock"

# Metrics
duration: 7 min
completed: 2026-01-18
---

# Phase 1 Plan 01: Persistence Compatibility Baseline Summary

**Pinned Magellan’s persistence layer to sqlitegraph v1.0.0 (crates.io) with a committed lockfile to make DB compatibility baselines reproducible.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-01-18T21:50:03Z
- **Completed:** 2026-01-18T21:57:16Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Upgraded `sqlitegraph` dependency to `1.0.0` while preserving `native-v2` feature wiring.
- Regenerated dependency resolution and ensured it is reproducible by committing `Cargo.lock`.
- Validated build + full workspace tests under both default features and `native-v2`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Upgrade sqlitegraph dependency to v1.0.0** - `c7e8b5c` (chore)
2. **Task 2: Refresh lockfile and run full test suite** - `4b98f23` (fix)

**Plan metadata:** _see docs commit below_

## Files Created/Modified
- `Cargo.toml` - Pins `sqlitegraph = "1.0.0"` (crates.io) and keeps `native-v2 = ["sqlitegraph/native-v2"]`.
- `Cargo.lock` - Captures resolved dependency graph including sqlitegraph v1.0.0.
- `.gitignore` - Stops ignoring Cargo.lock so the lockfile is trackable.
- `tests/sqlitegraph_exploration.rs` - Updates `HnswIndex::new` call to the v1.0.0 API.

## Decisions Made
- Pin `sqlitegraph` to the published crates.io major (`1.0.0`) to make Phase 1 persistence compatibility deterministic.
- Track `Cargo.lock` so sqlitegraph resolution is reproducible across environments and CI.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated sqlitegraph exploration test for v1.0.0 API change**
- **Found during:** Task 2 (Refresh lockfile and run full test suite)
- **Issue:** `tests/sqlitegraph_exploration.rs` used `HnswIndex::new(config)` but sqlitegraph v1.0.0 requires `HnswIndex::new(name, config)`.
- **Fix:** Passed a stable index name ("magellan-exploration") and re-ran the test suite.
- **Files modified:** `tests/sqlitegraph_exploration.rs`
- **Verification:** `cargo test --workspace` + `cargo test --workspace --features native-v2`
- **Committed in:** `4b98f23`

**2. [Rule 2 - Missing Critical] Committed Cargo.lock for reproducible dependency resolution**
- **Found during:** Task 2 (Refresh lockfile and run full test suite)
- **Issue:** `Cargo.lock` was gitignored, preventing deterministic pinning of sqlitegraph v1.0.0 resolution.
- **Fix:** Removed `Cargo.lock` from `.gitignore` and committed the lockfile.
- **Files modified:** `.gitignore`, `Cargo.lock`
- **Verification:** `cargo tree -i sqlitegraph` shows sqlitegraph v1.0.0 from crates.io.
- **Committed in:** `4b98f23`

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing critical)
**Impact on plan:** Both changes were required to complete the planned upgrade with reproducible, testable outcomes. No scope creep.

## Issues Encountered
- Cargo.lock was previously untracked, so the initial lockfile update did not show in `git diff`. Resolved by tracking Cargo.lock.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Ready for `01-02-PLAN.md` (preflight + two-phase CodeGraph open) with sqlitegraph v1.0.0 pinned and validated.

---
*Phase: 01-persistence-compatibility-baseline*
*Completed: 2026-01-18*

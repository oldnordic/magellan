# Known Issues

This document tracks known issues that are not yet resolved.

## Pre-existing Issues (Not Related to Recent Changes)

### 1. stress_database_integrity Test Fails with DEADLOCK

**Status:** Pre-existing (fails both before and after v2.4.7 refactoring)

**Symptom:**
```
test stress_database_integrity ... FAILED
Test should complete without deadlock: "DEADLOCK"
```

**Root Cause:**
SQLite concurrency issue when opening multiple connections simultaneously.
The `side_tables.rs` uses `Mutex<Connection>` but doesn't configure busy timeout,
leading to connection pool exhaustion under high concurrency (500 files).

**Impact:**
- Only affects stress test, not normal usage
- Single-threaded operations work correctly

**Workaround:**
None currently. Test is excluded from normal CI runs.

**Potential Fixes:**
1. Add `conn.busy_timeout(Duration::from_secs(30))` to SQLite connections
2. Use connection pooling (r2d2_sqlite)
3. Use a single shared connection with proper locking

---

## Fixed Issues

### Reference Tracking (Fixed in v2.4.7)
- ~~Internal function calls were not tracked~~ → Fixed
- ~~Cross-crate calls were silently dropped~~ → Fixed

### Metrics (Fixed in v2.4.7)
- ~~Fan-in/fan-out always showed 0~~ → Fixed
- ~~Cyclomatic complexity hardcoded to 1~~ → Fixed

### CLI Complexity (Fixed in v2.4.7)
- ~~parse_args_impl was 1,894 lines with complexity 130~~ → Refactored to 25 parsers

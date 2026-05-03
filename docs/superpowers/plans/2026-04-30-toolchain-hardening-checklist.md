# Toolchain Hardening Checklist

## CLI Contract (all tools must follow)
- `--db <PATH>`: Database file path (required for all read/write operations)
- `--output <FORMAT>`: One of `human`, `json`, `pretty` (default: `human`)
- `--path <FILE>`: Disambiguate symbol by file path
- `--name <SYMBOL>`: Symbol name to search for

## Verified Behaviors
- [x] `magellan doctor --output json` returns valid JSON
- [x] `magellan status --output json` returns valid JSON
- [x] `magellan find --name main --output json` returns valid JSON
- [x] `magellan query --file src/main.rs --output json` returns valid JSON
- [x] Indexing a file via CLI and checking status shows >0 files
- [x] Database remains healthy after indexing (no "malformed" errors)
- [x] Doctor warns on slow query response (>500ms)
- [x] WAL checkpoint retries on DatabaseBusy

## Implementation Summary

### Task 1: Standardize CLI `--output` flag
- `DoctorReport` and `CheckResult` structs collect diagnostics before rendering
- `run_doctor` accepts `OutputFormat` parameter
- Human output includes emoji icons and fix hints

### Task 3: Replace unwrap with poison-safe patterns
- All `side_conn_arc.lock().unwrap()` converted to `.lock().unwrap_or_else(|e| e.into_inner())`

### Task 4: Batch metrics computation
- `compute_for_file` now acquires shared connection once instead of ~8 times per file
- Added `_conn` static variants for all internal helpers

### Task 5: WAL retry and busy timeout
- `checkpoint_conn_with_retry` with exponential backoff (up to 3 retries)
- `PRAGMA busy_timeout = 5000` set on shared connections

### Task 6: Doctor connection health check
- Times a simple `count_files` query
- Warns if response exceeds 500ms

### Task 7: CLI contract tests
- 5 regression tests in `tests/toolchain_regression_tests.rs`

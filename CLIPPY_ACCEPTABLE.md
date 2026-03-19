# Acceptable unwrap() Usage Patterns

This document categorizes and justifies the use of `.unwrap()` in the Magellan codebase.

## Summary

As of 2026-02-09:
- **Total unwrap() calls:** 1061
- **In test code:** ~768 (72%)
- **In production code:** ~293 (28%)

## Acceptable Categories

### 1. Infallible Operations (Production Code)

#### SystemTime::now().duration_since(UNIX_EPOCH).unwrap()

**Locations:**
- `src/generation/mod.rs:131`
- `src/output/command.rs:1060` (timestamp generation)
- `src/verify.rs:169`

**Rationale:** System time always moves forward. `SystemTime::now().duration_since(UNIX_EPOCH)` cannot fail unless the system clock is set before 1970, which would indicate a fundamental system problem that justifies a panic.

**Pattern:**
```rust
let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();
```

### 2. Test Code (All unwrap() acceptable)

**Locations:** All `#[cfg(test)]` modules, `#[test]` functions

**Rationale:** Tests should panic on assertion failures. Using `.unwrap()` in tests provides better stack traces and ensures tests fail explicitly rather than silently.

**Common test patterns:**
- `TempDir::new().unwrap()` - Creating temp directories for test isolation
- `CodeGraph::open(path).unwrap()` - Opening test databases
- `graph.index_file(...).unwrap()` - Indexing in test setup
- `serde_json::to_string(...).unwrap()` - Serialization testing
- `result.expect(...).unwrap()` - Testing Option/Result extraction

### 3. Test-Only Helper Functions

**Locations:**
- `src/generation/mod.rs` - Code generation for tests
- `src/graph/scan.rs` - Test functions (lines 300+)

**Rationale:** Functions only called from test code use unwrap() for simplicity.

## Needs Attention (Future Work)

### 1. Implicit Invariant unwrap() in scan.rs

**Location:** `src/graph/scan.rs:239`
```rust
let source = result.source.as_ref().unwrap();
```

**Context:** This unwrap() is used after checking `result.is_error()`, but the invariant is implicit:
- If `error.is_some()` → continue (skip)
- If `error.is_none()` → expect `source.is_some()`

**Issue:** The struct allows `error.is_none()` AND `source.is_none()` simultaneously, which would cause a panic.

**Recommended fix:** Make the invariant explicit via enum or better API:
```rust
enum FileReadResult {
    Ok(String, String, Vec<u8>),
    Err(String, WatchDiagnostic),
}
```

**Priority:** Low - the current code path is protected by the `is_error()` check, but the invariant is implicit.

## Statistics by File

| File | unwrap() count | Primary usage |
|------|---------------|---------------|
| src/graph/ast_tests.rs | 111 | Test assertions |
| src/graph/scan.rs | 73 | Test setup + 1 production |
| src/graph/execution_log.rs | 62 | Test assertions |
| src/graph/query.rs | 55 | Test assertions |
| src/graph/filter.rs | 50 | Test assertions |
| src/graph/imports.rs | 43 | Test assertions |
| src/output/command.rs | 38 | Test assertions + 1 timestamp |
| src/graph/validation.rs | 38 | Test assertions |
| src/validation.rs | 34 | Test assertions |
| src/migrate_backend_cmd.rs | 17 | Test assertions |

## Clippy Lint Configuration

The following unwrap() patterns are **intentionally allowed** in Magellan:

1. `SystemTime::now().duration_since().unwrap()` - Time always moves forward
2. All test code (`#[cfg(test)]`) - Tests should panic on failures

For clippy baseline, we use:
```bash
cargo clippy -- -W clippy::unwrap_used
```

This will flag all unwrap() calls for review. The acceptable patterns are documented here.

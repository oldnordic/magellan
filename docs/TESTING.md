# Testing Guide

This document describes the test organization and conventions used in the Magellan codebase.

## Test Organization

Tests are organized into three categories:

### 1. Unit Tests

Unit tests are located in the same files as the code they test, using Rust's built-in
`#[cfg(test)]` attribute. Each module with tests contains a nested `mod tests` block
at the bottom of the file.

**Pattern:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Test implementation
    }

    // Test helpers are scoped within this module
    fn create_test_fixture() -> TestFixture {
        // Helper implementation
    }
}
```

**Examples:**
- `src/graph/algorithms.rs` - Graph algorithm tests with `create_test_graph()` helper
- `src/graph/cache.rs` - LRU cache tests
- `src/output/command.rs` - JSON serialization tests

### 2. Integration Tests

Integration tests are located in the `tests/` directory at the project root.
These tests use the public API of the `magellan` crate and exercise multiple
components together.

**Test Categories:**
- `algorithm_tests.rs` - Graph algorithm (SCC, reachability, path enumeration)
- `backend_integration_tests.rs` - Cross-backend compatibility tests
- `call_graph_tests.rs` - Call graph construction and traversal
- `cli_export_tests.rs` - CLI export command integration
- `cli_query_tests.rs` - CLI query command integration
- `indexer_tests.rs` - File indexing tests
- `watcher_tests.rs` - File watcher integration
- `kv_storage_tests.rs` - Key-value storage tests
- `span_tests.rs` - LSP span format tests
- `export_tests.rs` - Graph export tests (SCIP, JSON)

### 3. Test Helpers

Test helper functions are always scoped within test modules, not in production code.
This avoids the need for `#[allow(dead_code)]` suppressions.

**Correct Pattern:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Helper function - only visible to tests
    fn create_test_db() -> tempfile::TempDir {
        // ...
    }

    #[test]
    fn test_feature() {
        let db = create_test_db();
        // ...
    }
}
```

**Incorrect Pattern (do not do this):**
```rust
// In production code
#[cfg(test)]
#[allow(dead_code)]  // <- Avoid this
fn test_helper() {  // <- Don't put helpers at module level
    // ...
}
```

## Running Tests

### Run all tests:
```bash
cargo test --all
```

### Run only library unit tests:
```bash
cargo test --lib
```

### Run only integration tests:
```bash
cargo test --test '*'
```

### Run specific test:
```bash
cargo test test_reachable_symbols
```

### Run tests for specific feature:
```bash
cargo test --features native-v2
```

### Run tests without native-v2 feature:
```bash
cargo test --no-default-features
```

## Test Conventions

### Naming
- Test functions use `test_` prefix
- Test helpers describe what they create (e.g., `create_test_graph()`)
- Integration test files use `_tests.rs` suffix

### Assertions
- Use `assert!()` for boolean conditions
- Use `assert_eq!()` for equality comparisons
- Use `assert_matches!()` from the `assert_matches` crate for pattern matching

### Fixtures
- Use `tempfile::TempDir` for temporary test databases
- Clean up resources in `Drop` implementations or explicit cleanup
- Avoid hardcoded paths that may conflict across tests

### Backend-Specific Tests
- Some tests are gated with `#[cfg(feature = "native-v2")]` for Native V2 backend
- Use `#[cfg(all(test, not(feature = "native-v2")))]` for SQLite-only tests
- Both backends should be tested in CI

## Current Test Coverage

As of 2026-02-11, the project has:
- 503+ passing unit tests
- 40+ integration test files
- Tests covering: indexing, querying, CLI commands, graph algorithms, storage, exports

## Known Test Limitations

1. **Native V2 Backend**: Some algorithm tests are SQLite-only (gated with feature check)
2. **Concurrent Edits**: Stress tests may be flaky on slow CI systems
3. **Performance Tests**: Benchmarks are in separate files to avoid affecting normal test runs

## Adding New Tests

When adding new functionality:

1. **Unit tests**: Add `#[cfg(test)] mod tests` block in the same file
2. **Integration tests**: Create new file in `tests/` directory
3. **Helpers**: Keep helpers inside test modules, scoped with `pub(in super::tests)` if needed
4. **Documentation**: Update this file if adding new test categories

## Test Organization Checklist

Before marking a feature as complete:

- [ ] Unit tests cover public API
- [ ] Integration tests cover cross-module scenarios
- [ ] No `#[allow(dead_code)]` on test helpers
- [ ] Tests are properly gated by feature flags when needed
- [ ] Temporary files are cleaned up
- [ ] Tests are deterministic (no reliance on wall-clock time)

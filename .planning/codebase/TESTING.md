# Testing Patterns

**Analysis Date:** 2026-02-10

## Test Framework

**Runner:**
- Framework: Rust built-in test runner (`cargo test`)
- Config: No dedicated test config file found
- Features: Tests run with `default` features unless specified
- Parallel execution enabled by default

**Assertion Library:**
- Standard library `assert!`, `assert_eq!`, `assert_ne!`
- No external assertion library dependencies
- Custom assertion helpers in test modules when needed

**Run Commands:**
```bash
cargo test                      # Run all tests
cargo test --lib               # Run library tests only
cargo test --test signal_tests # Run specific test file
cargo test --release          # Run tests in release mode
```

## Test File Organization

**Location:**
- Pattern: Separate `tests/` directory (92 source files, 52 test files)
- Integration tests in `tests/` directory
- Unit tests within `#[cfg(test)]` modules in source files
- Test files not co-located with source

**Naming:**
- Pattern: `{feature}_tests.rs` (e.g., `algorithm_tests.rs`, `signal_tests.rs`)
- Descriptive names indicating test scope
- No test prefix/suffix convention beyond `_tests.rs`

**Structure:**
```
tests/
├── algorithm_tests.rs        # Graph algorithm tests
├── cli_smoke_tests.rs        # CLI binary tests
├── signal_tests.rs          # Signal handling tests
├── kv_storage_tests.rs       # KV backend tests
└── [other test files...]
```

## Test Structure

**Suite Organization:**
```rust
// Each test file is self-contained
#[test]
fn test_feature_behavior() {
    // Setup (often using TempDir)
    let temp_dir = TempDir::new().unwrap();

    // Test execution
    let result = compute_something();

    // Verification
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), expected_value);
}
```

**Patterns:**
- Setup: Temporary directories with `TempDir`
- Database: File-based databases for testing (no `:memory:`)
- Isolation: Each test creates its own environment
- Cleanup: Automatic via `Drop` implementations

**Test Data:**
- Inline source code as string literals
- Temporary files created per test
- No persistent test state between runs
- Real code examples in test strings

## Mocking

**Framework:** No external mocking framework used
**Patterns:**
```rust
// Manual mocking for database operations
let mock_db = MockDatabase::new();
let graph = CodeGraph::with_mock(mock_db);

// Stub implementations
#[cfg(test)]
impl MockBackend {
    pub fn fake_lookup(&self, path: &str) -> Option<u64> {
        // Return predefined test data
        Some(TEST_FILE_ID)
    }
}
```

**What to Mock:**
- Database connections for isolation
- File system operations
- Time-based operations
- External API calls (if any)

**What NOT to Mock:**
- Core algorithm logic
- Symbol extraction
- Graph operations (tested against real database)
- Language parsing

## Fixtures and Factories

**Test Data:**
```rust
// Common test patterns
let rust_code = r#"
fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    shared();
}
"#;

// Standard test setup pattern
let temp_dir = TempDir::new().unwrap();
let db_path = temp_dir.path().join("test.db");
let file_path = temp_dir.path().join("test.rs");
```

**Location:**
- Test data defined inline in test functions
- Shared constants at module level when reused
- No dedicated test fixtures directory
- TempDir for file system isolation

## Coverage

**Requirements:** No enforced coverage targets found
**View Coverage:**
```bash
# Generate coverage report
cargo tarpaulin --out html

# Run tests with coverage output
cargo llvm-cov --lcov --output-path coverage.lcov
```

## Test Types

**Unit Tests:**
- Scope: Individual functions and modules
- Location: `#[cfg(test)]` modules in source files
- Pattern: Test isolated logic without external dependencies
- Examples: `ast_ops.rs` internal function tests

**Integration Tests:**
- Scope: Cross-module functionality
- Location: Separate files in `tests/` directory
- Pattern: End-to-end workflows
- Examples: CLI smoke tests, database migration tests

**E2E Tests:**
- Framework: Custom binary spawning
- Pattern: Start magellan process, interact with it, verify output
- Examples: `cli_smoke_tests.rs` with `Command::new()`
- TempDir isolation for file system operations

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn test_async_operation() {
    let result = async_operation().await;
    assert!(result.is_ok());
}
```

**Error Testing:**
```rust
#[test]
fn test_error_conditions() {
    // Test expected errors
    let result = potentially_failing_operation();
    assert!(result.is_err());

    // Test error message content
    match result {
        Err(e) => assert!(e.to_string().contains("expected error")),
        Ok(_) => panic!("Expected error"),
    }
}
```

**Database Testing:**
```rust
// Pattern for database operations
let mut graph = CodeGraph::open(&db_path).unwrap();
graph.index_file(&path_str, source.as_bytes()).unwrap();

// Verify state
let symbols = graph.symbols_in_file(&path_str).unwrap();
assert_eq!(symbols.len(), 2);
```

---

*Testing analysis: 2026-02-10*
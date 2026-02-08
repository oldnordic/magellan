# Testing Patterns

**Analysis Date:** 2026-02-08

## Test Framework

**Runner:**
- Rust built-in test runner (`cargo test`)
- Criterion for benchmark tests (in `benches/`)
- No external test framework dependencies

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ne!`
- Custom assertion helpers for complex checks
- No external assertion library

**Run Commands:**
```bash
cargo test                    # Run all tests
cargo test --lib             # Run library tests only
cargo test --test name       # Run specific integration test
cargo test --release        # Run tests in release mode (for perf regression)
cargo bench                 # Run benchmarks
```

## Test File Organization

**Location:**
- Integration tests in `tests/` directory
- Unit tests within `#[cfg(test)]` modules
- Benchmark tests in `benches/` directory

**Naming:**
- Test files: `snake_case_test.rs` or `feature_tests.rs`
- Test functions: `test_descriptive_behavior()`
- Module tests: `mod tests { ... }`

**Structure:**
- Clear separation of test setup, execution, and verification
- Helper functions when tests share common setup
- Test isolation via temporary directories

## Test Structure

**Suite Organization:**
```rust
#[test]
fn test_feature_behavior() {
    // Arrange: Set up test data and environment
    let temp_dir = TempDir::new().unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Act: Execute the functionality under test
    let result = graph.index_file("test.rs", source.as_bytes());

    // Assert: Verify the expected outcome
    assert!(result.is_ok());
    assert_eq!(graph.symbols_count(), 1);
}
```

**Patterns:**
- Arrange-Act-Assert pattern consistently used
- Descriptive test names that explain the behavior
- Test data creation in setup phase
- Multiple assertions per test when appropriate

## Mocking

**Framework:**
- Minimal mocking - prefer real implementations
- `tempfile` for file system isolation
- In-memory databases (`:memory:`) for speed
- No mock framework dependencies

**Patterns:**
```rust
// Use real implementations when possible
let mut parser = magellan::Parser::new().unwrap();
let symbols = parser.extract_symbols(path, source);

// Isolate file system operations
let temp_dir = TempDir::new().unwrap();
let test_file = temp_dir.path().join("test.rs");
fs::write(&test_file, source).unwrap();
```

**What to Mock:**
- File system operations (using `tempfile`)
- Database operations (using in-memory variants)
- External services (when absolutely necessary)

**What NOT to Mock:**
- Core library functionality
- Business logic under test
- Error handling paths

## Fixtures and Factories

**Test Data:**
- Inline source code strings for AST tests
- Temporary directories with real files
- Database fixtures pre-populated with test data

**Location:**
- Test data created inline or in test setup
- No separate fixture files (to avoid stale data)
- Deterministic generation for reproducible tests

**Patterns:**
```rust
let source = r#"
fn test_function() {
    println!("Hello, world!");
}
"#;
```

## Coverage

**Requirements:**
- Not explicitly enforced
- CI runs all tests on PRs
- Performance regression tests for critical paths
- Thread safety tests (TSAN - currently disabled)

**View Coverage:**
```bash
cargo test -- --test-threads=1  # For debugging
cargo test --release           # For performance tests
```

## Test Types

**Unit Tests:**
- Test individual functions and modules
- Fast execution with minimal setup
- Focus on edge cases and error conditions
- Located in `#[cfg(test)]` modules

**Integration Tests:**
- Test workflows and interactions between components
- Slower due to file system/database operations
- Realistic test scenarios
- Located in `tests/` directory

**E2E Tests:**
- CLI command testing via process spawning
- End-to-end workflows
- Using `Command::new()` to test binary interface

## Common Patterns

**Async Testing:**
- Minimal async test usage (mostly for watcher tests)
- Synchronous testing preferred for simplicity
- Thread operations use `std::thread::sleep` for timing

**Error Testing:**
```rust
#[test]
fn test_error_condition() {
    let result = risky_operation();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Expected error");
}
```

**Temporary Directory Pattern:**
```rust
#[test]
fn test_with_temp_dir() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Test code that creates and cleans up automatically
}
```

**Database Testing:**
- In-memory `:memory:` database for unit tests
- File-based database for integration tests
- Automatic cleanup via `Drop` implementation

**Performance Testing:**
- Specific performance regression tests
- Baseline comparisons on PRs
- Release mode execution for accurate measurements

---

*Testing analysis: 2026-02-08*
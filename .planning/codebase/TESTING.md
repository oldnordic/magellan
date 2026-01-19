# Testing Patterns

**Analysis Date:** 2026-01-19

## Test Framework

**Runner:**
- cargo test (standard Rust test framework)
- Config: Built-in `#[test]` attribute

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ok!` macros
- No external assertion libraries

**Run Commands:**
```bash
cargo test              # Run all tests
cargo test --test graph_persist  # Run specific test file
cargo test -- --nocapture  # Show println! output
cargo test -- --ignored   # Run ignored tests
```

## Test File Organization

**Location:**
- Integration tests: `tests/` directory (separate from src/)
- Unit tests: Inline in source files with `#[cfg(test)]` modules

**Naming:**
- `{topic}_tests.rs`: Integration test files (e.g., `watcher_tests.rs`, `export_tests.rs`)
- `tests` module: Unit tests within source files

**Structure:**
```
tests/
├── graph_persist.rs       # Graph persistence tests
├── watcher_tests.rs       # Filesystem watcher tests
├── cli_export_tests.rs    # Export command tests
├── multi_language_integration_tests.rs
└── ... (25+ test files)
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specific_behavior() {
        let result = function_under_test();
        assert_eq!(result, expected);
    }
}
```

**Patterns:**
- Setup: Create temp directory with `tempfile::TempDir`
- Teardown: Implicit (TempDir auto-cleans on drop)
- Assertion: Standard `assert_eq!`, `assert!` macros

**Example:**
```rust
#[test]
fn test_index_file() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let source = b"fn test() {}";

    let count = graph.index_file("test.rs", source).unwrap();
    assert_eq!(count, 1);
}
```

## Mocking

**Framework:** No mocking framework - use real implementations with temp dirs

**Patterns:**
- Use in-memory database: `CodeGraph::open(":memory:")`
- Use temp directories: `tempfile::TempDir`
- Direct instantiation of types

**Example:**
```rust
let mut graph = CodeGraph::open(":memory:").unwrap();
```

**What to Mock:**
- Generally nothing - use real implementations

**What NOT to Mock:**
- Database, file system (use temp locations)

## Fixtures and Factories

**Test Data:**
- Created inline in each test
- Use helper functions for common setups

**Example:**
```rust
fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}
```

**Location:**
- Inline in test modules or shared test helpers

## Coverage

**Requirements:** No formal coverage threshold enforced

**View Coverage:**
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

## Test Types

**Unit Tests:**
- Scope: Single function or small module
- Approach: Direct function calls with controlled inputs
- Location: Inline `#[cfg(test)]` modules in source files

**Integration Tests:**
- Scope: Full graph operations, multi-step workflows
- Approach: Real database, file system, parsers
- Location: `tests/` directory

**E2E Tests:**
- Framework: Custom CLI smoke tests in `tests/cli_smoke_tests.rs`
- Approach: Spawn `magellan` process, check exit codes and output

## Common Patterns

**Async Testing:**
- Not applicable (synchronous codebase)

**Error Testing:**
```rust
#[test]
fn test_error_handling() {
    let result = function_that_fails();
    assert!(result.is_err());
}
```

**Deterministic Output Testing:**
```rust
#[test]
fn test_sorted_output() {
    let items = vec!["c", "a", "b"];
    let mut sorted = items.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["a", "b", "c"]);
}
```

**Property Testing:**
- Not used (no quickcheck/proptest dependency)

---
*Testing analysis: 2026-01-19*

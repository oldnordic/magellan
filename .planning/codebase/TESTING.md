# Testing Patterns

**Analysis Date:** 2026-01-19

## Test Framework

**Runner:**
- Rust's built-in `cargo test` framework
- No external test runner configuration

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ne!` macros
- Custom assertion messages for clarity

**Run Commands:**
```bash
cargo test              # Run all tests
cargo test --test <name>  # Run specific integration test
cargo test --lib        # Run unit tests in src/
cargo test -- --nocapture  # Show println! output
cargo test -- --ignored  # Run ignored tests
```

**Dev Dependencies for Testing:**
```toml
[dev-dependencies]
tempfile = "3.10"      # Temporary directories for tests
rusqlite = "0.31"      # Direct DB access in tests
```

## Test File Organization

**Location:**
- Integration tests in `tests/` directory at project root (separate from source)
- Unit tests in `src/` modules within `#[cfg(test)]` mod blocks

**Naming:**
- Integration tests: `{feature}_tests.rs` or `{category}_tests.rs`
- Examples: `parser_tests.rs`, `graph_persist.rs`, `call_graph_tests.rs`, `cli_smoke_tests.rs`
- Unit test modules: `mod tests;` within source files

**Structure:**
```
tests/
├── cli_smoke_tests.rs       # CLI binary integration tests
├── graph_persist.rs          # Database persistence tests
├── parser_tests.rs           # Symbol extraction tests
├── indexer_tests.rs          # Watcher/indexer integration tests
├── watcher_tests.rs          # File watching tests
├── call_graph_tests.rs       # Call graph tests
├── multi_language_integration_tests.rs  # Multi-language tests
└── ... (20+ test files total)

src/
├── graph/mod.rs              # Contains `mod tests;` unit tests
├── ingest/mod.rs             # Contains `#[cfg(test)]` module tests
└── ...
```

## Test Structure

**Suite Organization:**
```rust
//! Module-level doc comment describing test purpose

use magellan::{CodeGraph, Parser};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_descriptive_name() {
    // Arrange: Set up test state
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Act: Execute behavior under test
    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", b"fn foo() {}").unwrap();

    // Assert: Verify expected outcome
    let symbols = graph.symbols_in_file("test.rs").unwrap();
    assert_eq!(symbols.len(), 1, "Should have 1 symbol");
}
```

**Patterns:**

**Setup Pattern:**
```rust
// Use TempDir for isolated test environments
let temp_dir = TempDir::new().unwrap();
let db_path = temp_dir.path().join("test.db");

// Open fresh graph for each test
let mut graph = CodeGraph::open(&db_path).unwrap();
```

**Teardown Pattern:**
- Implicit via `TempDir` cleanup (auto-deleted when dropped)
- No explicit teardown in most tests
- Database cleaned up via temp directory removal

**Assertion Pattern:**
```rust
// Standard assertion with context message
assert_eq!(symbols.len(), 1, "Should have 1 symbol");

// Filter and assert patterns
let functions: Vec<_> = symbols
    .iter()
    .filter(|s| s.kind == SymbolKind::Function)
    .collect();
assert_eq!(functions.len(), 1);
```

## Mocking

**Framework:** No formal mocking framework (no `mockall`, `mockito`)

**Patterns:**
- Use real implementations with temp directories
- In-memory databases via `:memory:` path or temp files
- No external service mocking (no network calls)

**Example from `src/graph/tests.rs`:**
```rust
#[test]
fn test_hash_computation() {
    let graph = crate::CodeGraph::open(":memory:").unwrap();
    let source = b"fn test() {}";
    let hash = graph.files.compute_hash(source);

    assert_eq!(hash.len(), 64);
}
```

**What to Mock:**
- Nothing significant - tests use real implementations
- File I/O mocked via `TempDir` (real files, isolated location)
- Database operations use temp SQLite files or `:memory:`

**What NOT to Mock:**
- Parser behavior (test real tree-sitter parsing)
- Graph operations (test real sqlitegraph backend)
- Language detection (test real detection logic)

## Fixtures and Factories

**Test Data:**
```rust
// Inline test code snippets as byte strings
let source = b"
fn foo() {}
struct Bar;
";

// Or raw string literals for multi-line code
let source = r#"
fn main() {
    println!("Hello");
}
"#;
```

**Location:**
- No separate fixture files
- Test data embedded directly in test functions
- Multi-language test samples in `multi_language_integration_tests.rs`

**Example from `tests/parser_tests.rs`:**
```rust
#[test]
fn test_multiple_symbols() {
    let mut parser = Parser::new().unwrap();
    let source = b"
        fn func_a() {}
        struct StructA;
        enum EnumA { X }
        trait TraitA {}
        mod mod_a;
    ";

    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);
    assert!(facts.len() >= 5);
}
```

## Coverage

**Requirements:** None enforced (no `tarpaulin` or coverage CI)

**View Coverage:**
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

**Coverage Notes:**
- High coverage on core operations (graph, parser, indexer)
- CLI command parsing well-tested
- Error handling paths covered in `error_tests.rs`

## Test Types

**Unit Tests:**
- Located in `src/` modules within `#[cfg(test)]` blocks
- Test single functions and methods
- Fast execution, no external dependencies

**Integration Tests:**
- Located in `tests/` directory
- Test full-stack behavior:
  - Database persistence (`graph_persist.rs`)
  - Watcher/indexer coordination (`indexer_tests.rs`)
  - CLI binary execution (`cli_smoke_tests.rs`)
- Use `tempfile` for isolation

**E2E Tests:**
- CLI smoke tests spawn the actual `magellan` binary
- Verify filesystem watcher behavior with real file operations
- Test multi-language scanning end-to-end

## Common Patterns

**Async Testing:**
- Not applicable (synchronous codebase)

**Error Testing:**
```rust
// From tests/error_tests.rs
#[test]
fn test_unreadable_file_prints_error_and_continues() {
    // Arrange: Create file with no permissions
    let mut perms = fs::metadata(&bad_file).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&bad_file, perms).unwrap();

    // Act: Trigger watch
    // ...

    // Assert: Error logged but process continues
    assert!(stdout.contains("ERROR"));
    assert!(stdout.contains("good.rs")); // Other files processed
}
```

**Property-Based Testing:**
- Not used (no `quickcheck` or `proptest`)

**Deterministic Testing:**
- Core project principle: tests must be deterministic
- Use `thread::sleep()` with fixed durations for event coordination
- File writes synchronized with `sync_all()` for guaranteed persistence

**Example from `tests/indexer_tests.rs`:**
```rust
fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;  // Ensure content is flushed
    Ok(())
}
```

**Helper Functions:**
- Common test patterns extracted to helper functions
- File synchronization helpers for watcher tests
- Poll helpers for event-driven tests

**Example helper from `tests/watcher_tests.rs`:**
```rust
/// Helper: poll for event with timeout
fn poll_for_event(watcher: &FileSystemWatcher, timeout_ms: u64) -> Option<FileEvent> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        if let Some(event) = watcher.try_recv_event() {
            return Some(event);
        }

        if start.elapsed() >= timeout {
            return None;
        }

        sleep(Duration::from_millis(50));
    }
}
```

## Test Data Organization

**Multi-Language Support:**
- `multi_language_integration_tests.rs` contains samples for:
  - Rust (`.rs`)
  - Python (`.py`)
  - C (`.c`)
  - C++ (`.cpp`)
  - Java (`.java`)
  - JavaScript (`.js`)
  - TypeScript (`.ts`)

**Sample Structure:**
```rust
let rust_file = temp_dir.path().join("main.rs");
std::fs::write(
    &rust_file,
    r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
}
"#,
).unwrap();
```

## TDD Approach

**Documented Philosophy:**
- Several test files explicitly mention TDD
- Example: `tests/call_graph_tests.rs` header: `//! TDD approach: Write failing test first, then implement feature.`

**Test-First Evidence:**
- Bug-demonstrating tests in `src/graph/tests.rs`:
```rust
#[test]
fn test_cross_file_references() {
    // This test demonstrates the bug: cross-file references are NOT created
    // ...
    // THIS ASSERTION FAILS - demonstrates the bug
    assert!(
        !references.is_empty(),
        "Cross-file references should be created. Found: {} references. \
         This demonstrates the bug: only same-file references are indexed.",
        references.len()
    );
}
```

---

*Testing analysis: 2026-01-19*

# Coding Conventions

**Analysis Date:** 2026-02-10

## Naming Patterns

**Files:**
- snake_case for `.rs` files (e.g., `ast_extractor.rs`, `code_graph.rs`)
- kebab-case for CLI commands and bin (e.g., `magellan watch`)
- PascalCase for test files only when testing specific concepts (e.g., `signal_tests.rs`)
- All files are lowercase with underscores

**Functions:**
- Public: snake_case (e.g., `index_file`, `detect_language`)
- Private: snake_case (e.g., `get_file_id_kv`, `compute_hash`)
- Test functions: snake_case with descriptive names (e.g., `test_watch_command_indexes_file_on_create`)
- Async functions: clear naming indicating async operation (e.g., `run_watch_pipeline`)

**Variables:**
- snake_case throughout
- Descriptive names (e.g., `file_path`, `source_bytes`, `symbol_count`)
- Loop counters: `i`, `j`, `idx` when appropriate for short loops
- Parameters: clear names matching their purpose

**Types:**
- Structs: PascalCase (e.g., `CodeGraph`, `SymbolInfo`, `ChunkStore`)
- Enums: PascalCase (e.g., `SymbolKind`, `ReconcileOutcome`)
- Error types: PascalCase with `Error` suffix (e.g., `ValidationError`)
- Generic parameters: single uppercase letters (e.g., `T`, `K`, `V`)

## Code Style

**Formatting:**
- Tool: rustfmt (standard Rust formatting)
- Line length: 100 characters max (soft limit)
- Indentation: 4 spaces (no tabs)
- Trailing commas: Always in multi-line structures

**Linting:**
- Tool: Clippy with default rules
- Strict error handling: no `unwrap()` in production paths
- Explicit error types over `anywhere` in public APIs
- Appropriate use of `?` for error propagation

**Import Organization:**
1. Standard library imports (e.g., `std::path::Path`)
2. External crate imports (e.g., `anyhow::Result`, `rusqlite::Connection`)
3. Local module imports (e.g., `crate::graph::CodeGraph`)
4. Re-exports at module level with clear grouping

## Error Handling

**Patterns:**
- Result<T> for fallible operations
- anyhow::Result for application-level errors
- Custom error types with context when needed
- Early returns with `?` for error propagation
- Avoidance of `expect()` in production code

**Error Codes:**
- Structured error codes with pattern `MAG-{CATEGORY}-{3-digit}`
- Categories: REF (reference), QRY (query), IO (I/O), V (validation)
- Error codes documented in `src/error_codes.rs`
- Each error code stable and not reused

**Logging:**
- Debug-level logging for operational details
- Info for significant events (file indexing, cycles detected)
- Warning for recoverable issues
- Error for unrecoverable failures
- Structured logging with consistent format

## Comments

**When to Comment:**
- Complex algorithm explanations
- Public API documentation (TSDoc style)
- TODO items with clear action items
- Workarounds for known issues
- Non-obvious side effects

**TSDoc/TSDoc:**
- Comprehensive for public APIs
- Examples where helpful
- Return value descriptions
- Argument documentation

**Function Design:**
- Size: Max 50 lines (exceptions for complex operations)
- Parameters: 3-7 ideal, more use struct parameters
- Return Values: Prefer Result<T, E> over Option<T>
- Single responsibility principle enforced

**Module Design:**
- Exports: Minimal, intentional public API
- Barrel files: Used sparingly for re-exports
- Private implementation details kept hidden
- Clear module boundaries

---

*Convention analysis: 2026-02-10*
# Coding Conventions

**Analysis Date:** 2026-02-08

## Naming Patterns

**Files:**
- Snake_case for modules and commands: `find_cmd.rs`, `call_graph_tests.rs`
- PascalCase for test files: `CliSmokeTests`
- Consistent extension: `.rs` for all Rust source files

**Functions:**
- Public: `snake_case` with descriptive names: `validate_graph`, `detect_language_from_path`
- Private: Leading underscore not used, rely on module privacy
- Test functions: `test_` prefix: `test_extract_calls_detects_function_calls`

**Variables:**
- `snake_case` with descriptive names
- Loop counters: `i`, `j`, `k` only in small, local contexts
- Temp directories: `temp_dir`, `temp_path`
- Result variables: `result`, `outcome`

**Types:**
- Structs: PascalCase: `ValidationReport`, `SymbolInfo`
- Enums: PascalCase: `PathValidationError`
- Error types: PascalCase ending with `Error`: `ValidationError`

**Constants:**
- SCREAMING_SNAKE_CASE: `MAG_REF_001_SYMBOL_NOT_FOUND`
- Module-level constants with prefix: `ERROR_CODE_DOCUMENTATION`

## Code Style

**Formatting:**
- Rust default formatting with `cargo fmt`
- 4-space indentation (tabs not used)
- Maximum line length: 100 characters (soft limit)
- Trailing commas in multi-line contexts

**Linting:**
- Clippy enabled via `cargo clippy --all-targets`
- Error-level lints enforced
- Warning-level lints reported but not blocking

**Comments:**
- Documentation comments (`///`) for all public APIs
- Comments explain "why" not "what"
- Complex algorithms documented with examples
- TODO comments discouraged in favor of issues

## Import Organization

**Order:**
1. Standard library (use std::)
2. External dependencies (use crate::external)
3. Local modules (use crate::internal)

**Grouping:**
- Related imports grouped together
- Blank line between groups
- No alphabetical sorting

**Path Aliases:**
- Direct imports preferred over wildcard
- `use anyhow::Result` (not `use anyhow::*`)
- Common patterns: `use crate::{CodeGraph, SymbolKind}`

## Error Handling

**Patterns:**
- `anyhow::Result<T>` for public APIs
- `thiserror::Error` for custom error types
- Early returns with `?` for error propagation
- Avoid `unwrap()` in production code (1017 instances found - needs attention)

**Error Types:**
- Custom error types with context
- Structured error codes (MAG-{CATEGORY}-{NNN})
- Error messages describe the problem and possible solution

**Success Cases:**
- Return meaningful data, not booleans when possible
- Option types preferred for nullable values
- Documentation for return values

## Logging

**Framework:**
- `log` crate with structured logging
- Debug-level logging for development
- Info-level for significant operations
- Error-level for failures

**Patterns:**
- Log entry/exit for critical operations
- Include context in error logs
- No sensitive data in logs

## Comments

**When to Comment:**
- Complex algorithms or business logic
- Unusual design decisions
- Public API documentation
- Performance-critical sections

**JSDoc/TSDoc:**
- Not used (Rust uses `///` comments)
- Examples provided for complex functions
- Return value documented

## Function Design

**Size:**
- Max 300 LOC per file
- Functions focused on single responsibility
- Helper functions extracted for reuse

**Parameters:**
- Maximum 4 parameters preferred
- Struct parameters for >4 parameters
- Default values for optional parameters

**Return Values:**
- Use `Result` for fallible operations
- Use `Option` for nullable values
- Avoid multiple return types when possible

## Module Design

**Exports:**
- Public API explicitly re-exported in `lib.rs`
- Internal modules not re-exported
- Feature-gated conditional compilation

**Barrel Files:**
- Not used explicitly
- Re-exports in `lib.rs` serve similar purpose
- Module-level organization by feature

---

*Convention analysis: 2026-02-08*
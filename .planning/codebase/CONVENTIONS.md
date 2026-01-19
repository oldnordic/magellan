# Coding Conventions

**Analysis Date:** 2026-01-19

## Naming Patterns

**Files:**
- `snake_case.rs`: All source files (e.g., `watch_cmd.rs`, `call_ops.rs`)
- `mod.rs`: Module definition/exports in directories
- Test files: `{topic}_tests.rs` in `tests/` directory

**Functions:**
- `snake_case`: All functions (e.g., `scan_directory`, `index_file`, `reconcile_file_path`)

**Variables:**
- `snake_case`: Local variables (e.g., `file_path`, `byte_start`, `symbol_id`)

**Types:**
- `PascalCase`: Structs, enums (e.g., `CodeGraph`, `FileNode`, `SymbolFact`)
- `SCREAMING_SNAKE_CASE`: Constants (e.g., `INTERNAL_IGNORE_DIRS`, `STALE_THRESHOLD_SECS`)

## Code Style

**Formatting:**
- Tool: rustfmt (standard Rust formatting)
- Line width: Default (100 chars implied by codebase)
- Indentation: 4 spaces

**Linting:**
- Tool: clippy (standard Rust linter)
- Custom lints: None configured in Cargo.toml

## Import Organization

**Order:**
1. Standard library (`std::*`, `core::*`)
2. External crates (`anyhow`, `serde`, `sqlitegraph`, etc.)
3. Local modules (`crate::*`)

**Path Aliases:**
- No path aliases configured in Cargo.toml
- Use `crate::module::item` for local imports

**Typical import block:**
```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::graph::CodeGraph;
```

## Error Handling

**Patterns:**
- Functions return `Result<T>` from anyhow
- Use `?` operator for propagation
- Context: `.map_err(|e| anyhow::anyhow!("context: {}", e))`
- Avoid `unwrap()` in production code paths

**Example:**
```rust
pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize> {
    let hash = self.files.compute_hash(source);
    let file_id = self.files.find_or_create_file_node(path, &hash)?;
    // ...
    Ok(symbol_facts.len())
}
```

## Logging

**Framework:** stderr eprintln! for errors/diagnostics

**Patterns:**
- Errors: `eprintln!("Error: {}", e)` for user-facing errors
- Progress: `println!()` for status updates (file counts, paths)
- Diagnostics: Structured via `WatchDiagnostic` type, printed as JSON or human-readable

**Example:**
```rust
eprintln!("ERROR {} {}", path_str, e);
println!("MODIFY {} symbols={} refs={} calls={}", path_str, symbols, references, calls);
```

## Comments

**When to Comment:**
- Module-level: `//!` for module documentation
- Function-level: `///` for public API documentation
- Inline: `//` for non-obvious logic

**JSDoc/TSDoc:**
- Use `///` for public functions and structs
- Include `# Arguments`, `# Returns`, `# Behavior` sections
- Document invariants and guarantees

**Example:**
```rust
/// Index a file into the graph (idempotent)
///
/// # Behavior
/// 1. Compute SHA-256 hash of file contents
/// 2. Upsert File node with path and hash
/// 3. DELETE all existing Symbol nodes...
///
/// # Arguments
/// * `path` - File path
/// * `source` - File contents as bytes
///
/// # Returns
/// Number of symbols indexed
pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize>
```

## Function Design

**Size:** Prefer under 50 lines, up to 100 for complex logic

**Parameters:**
- Use `&self` or `&mut self` for methods
- Borrow with `&str` instead of `String` where possible
- Use `&Path` or `PathBuf` for file paths
- Pass slices `&[T]` for collections

**Return Values:**
- Use `Result<T>` for fallible operations
- Use `Option<T>` for optional values
- Return `usize` for counts
- Return `Vec<T>` for collections

## Module Design

**Exports:**
- Re-export public types at module level
- Use `pub use` for convenience
- Example: `pub use schema::{FileNode, SymbolNode, ReferenceNode, CallNode};`

**Barrel Files:**
- `mod.rs` serves as barrel file in each directory
- Re-exports commonly used types
- Example: `src/lib.rs` re-exports entire public API

---
*Convention analysis: 2026-01-19*

# Coding Conventions

**Analysis Date:** 2026-01-19

## Naming Patterns

**Files:**
- `snake_case.rs` for all Rust source files
- Module files match directory names (e.g., `ingest/mod.rs`, `graph/mod.rs`)
- Test files are co-located in `tests/` directory at project root
- Test file naming: `{feature}_tests.rs` or `{category}_tests.rs` (e.g., `parser_tests.rs`, `graph_persist.rs`)

**Functions:**
- `snake_case` for all functions and methods
- Public API functions use descriptive names: `index_file`, `symbols_in_file`, `count_files`
- Internal/private functions prefixed with underscore or clearly scoped within modules
- Getter functions: `get_` prefix for retrieval operations (e.g., `get_file_node`, `get_code_chunks`)

**Variables:**
- `snake_case` for all variables
- Descriptive names over abbreviations (e.g., `symbol_facts` not `syms`, `source_code` not `src`)
- Loop variables: short names acceptable (`id`, `fact`, `node`)

**Types:**
- `PascalCase` for structs, enums, and type aliases
- `PascalCase` for traits (none currently in codebase)
- Enums use `PascalCase` variants

**Constants:**
- `SCREAMING_SNAKE_CASE` for const values (e.g., `STALE_THRESHOLD_SECS`)
- Static values also use `SCREAMING_SNAKE_CASE`

## Code Style

**Formatting:**
- Rust standard `rustfmt` formatting (no custom config detected)
- 4-space indentation (Rust default)
- Line length: appears to follow standard Rust conventions (no `#![warn(clippy::too_long_first_span)]` detected)

**Linting:**
- No explicit `clippy.toml` configuration found
- Standard `cargo clippy` warnings apply
- No custom `eslint` or equivalent (this is a Rust-only project)

**Module Documentation:**
- Every module has a module-level `//!` doc comment explaining its purpose
- Example from `src/lib.rs`: `//! Magellan: A dumb, deterministic codebase mapping tool`

**Function Documentation:**
- Public functions have `///` doc comments with:
  - Brief description
  - `# Arguments` section documenting parameters
  - `# Returns` section describing return values
  - `# Behavior` or `# Guarantees` sections for important semantics
  - `# Note` for caveats

## Import Organization

**Order:**
1. Standard library imports (`std::`)
2. Third-party crates (`anyhow`, `serde`, `tree_sitter`, `sqlitegraph`, etc.)
3. Local crate imports (`crate::`)
4. Module re-exports (`pub use`)

**Path Aliases:**
- No explicit path aliases configured in `Cargo.toml`
- All imports use full `crate::` paths for intra-crate references
- External deps imported directly (e.g., `use anyhow::Result;`)

**Example from `src/graph/mod.rs`:**
```rust
use anyhow::Result;
use sqlitegraph::SqliteGraphBackend;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::generation::{ChunkStore, CodeChunk};
use crate::references::{CallFact, ReferenceFact};
```

## Error Handling

**Primary Error Type:**
- `anyhow::Result<T>` as the main return type for fallible operations
- `anyhow::anyhow!` macro for creating contextual errors
- `thiserror` is listed as a dependency but `anyhow` is used predominantly

**Patterns:**
```rust
// Function returning anyhow::Result
pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self> {
    let sqlite_graph = sqlitegraph::SqliteGraph::open(&db_path_buf)?;
    // ...
}

// Context propagation
.map_err(|e| anyhow::anyhow!("Specific context: {}", e))?

// Option handling with context
let file_id = match graph.files.find_file_node(path)? {
    Some(id) => id,
    None => return Ok(Vec::new()),
};
```

**Error Propagation:**
- `?` operator used consistently for error propagation
- No explicit error type construction in most cases
- Errors bubble up through `anyhow::Result`

**Graceful Degradation:**
- Parse errors in tree-sitter return empty `Vec::new()` instead of errors
```rust
let tree = match self.parser.parse(source, None) {
    Some(t) => t,
    None => return Vec::new(), // Parse error: return empty
};
```

## Logging

**Framework:** `eprintln!` for error output (no structured logging crate)

**Patterns:**
- CLI errors go to stderr via `eprintln!("Error: {}", e)`
- No debug/warn/info levels - just output or error
- No logging crate (no `env_logger`, `tracing`, etc.)

**Example from `src/main.rs`:**
```rust
if let Err(e) = run_status(db_path) {
    eprintln!("Error: {}", e);
    return ExitCode::from(1);
}
```

## Comments

**When to Comment:**
- Module-level documentation with `//!` for every file
- Function documentation with `///` for all public APIs
- Inline comments for non-obvious logic
- "TODO" comments are NOT used (per CLAUDE.md rule: "NO DIRTY FIXES")

**JSDoc/TSDoc:**
- Not applicable (Rust project)
- Uses standard Rust doc comments (`///` and `//!`)

**Comment Style:**
```rust
//! Module-level doc comment describing the module's purpose

/// Brief function description.
///
/// # Arguments
/// * `arg1` - Description
///
/// # Returns
/// Description of return value
///
/// # Guarantees
/// - Behavior guarantee 1
/// - Behavior guarantee 2
pub fn documented_function(arg1: Type) -> Result<ReturnType> {
    // Inline comment for non-obvious logic
    let result = complex_operation();

    Ok(result)
}
```

## Function Design

**Size:**
- Prefer functions under 50 lines
- Larger functions exist (e.g., `parse_args` in `main.rs` at ~500 lines for CLI parsing)
- Complex operations split into helper functions

**Parameters:**
- Prefer fewer parameters (3-4 max typically)
- Many parameters grouped into config structs (e.g., `WatcherConfig`)
- Pass `&mut self` for mutable state

**Return Values:**
- `Result<T>` for fallible operations
- `Option<T>` for optional returns
- `Vec<T>` for collections (not iterators)
- Tuple returns for multiple values: `(i64, SymbolFact)`

**Function Naming Patterns:**
- `get_*` - Retrieve/read operations
- `find_*` - Search operations that may not find anything (return `Option`)
- `is_*` - Boolean predicates
- `compute_*` - Pure computation functions
- `index_*` - Writing/indexing operations
- `delete_*` - Deletion operations
- `count_*` - Return counts

## Module Design

**Exports:**
- Public types re-exported at module level via `pub use`
- Example from `src/lib.rs`:
```rust
pub use generation::{ChunkStore, CodeChunk};
pub use graph::{CodeGraph, ReconcileOutcome, ScanProgress, MAGELLAN_SCHEMA_VERSION};
pub use indexer::{run_indexer, run_indexer_n};
```

**Barrel Files:**
- `mod.rs` files serve as barrel files for each module directory
- Re-exports aggregate public API from submodules
- Private helper modules not re-exported

**Example from `src/ingest/mod.rs`:**
```rust
pub mod c;
pub mod cpp;
pub mod detect;
pub mod java;
pub mod javascript;
pub mod python;
pub mod typescript;

// Re-exports from detect module
pub use detect::{detect_language, Language};
```

**Visibility:**
- Default to private
- Mark `pub` only for library API
- Internal modules use `pub(crate)` when needed

## Async/Await

**Not Used:**
- This is a synchronous codebase
- No `tokio`, `async-std`, or other async runtime
- File I/O is synchronous `std::fs`

## Generic Parameters

**Naming:**
- `T` for single type parameter
- Descriptive names for multiple: `K`, `V` for key/value (rarely used)
- Lifetime parameters: `'a`, `'b` as needed

**Bounds:**
- Minimal trait bounds
- Prefer concrete types over generics unless abstraction is clearly needed

## Constants and Config

**Configuration:**
- Config structs instead of global constants
- Example: `WatcherConfig { debounce_ms: u64 }`
- Environment-based config not used

**Derived Constants:**
- Computed values as functions or associated constants
- Example: `STALE_THRESHOLD_SECS` for timeout values

## Special Patterns

**Result Aliases:**
- `anyhow::Result<T>` used throughout
- Type alias `Result<T>` in modules for brevity

**Idempotency:**
- Many operations documented as "idempotent"
- `index_file` explicitly documented as idempotent (deletes then inserts)

**Determinism:**
- Core design principle: "dumb, deterministic codebase mapping"
- Operations maintain deterministic ordering (e.g., `sort_unstable()` on IDs before deletion)

---

*Convention analysis: 2026-01-19*

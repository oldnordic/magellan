# Architecture Patterns: Magellan v1.1 (Correctness + Safety)

**Domain:** Code graph persistence and indexing
**Researched:** 2026-01-19
**Overall confidence:** HIGH

## Executive Summary

Magellan v1.1 introduces three correctness features that integrate with existing module boundaries:

1. **FQN-as-key refactor** - Changes how symbols are identified, primarily affecting the `ingest/` and `graph/` modules
2. **Path traversal validation** - Adds safety checks at the filesystem entry points (watcher, scanner)
3. **Transactional deletes** - Wraps existing `delete_file_facts` in proper SQLite transactions

The current architecture already has clear separation of concerns. The v1.1 features should integrate without major restructuring:

- **FQN extraction** happens in the `ingest/` module during tree-sitter traversal, stored in `SymbolFact.fqn`, used by `graph/symbols.rs` for stable ID generation
- **Path validation** belongs at entry points: `watcher.rs` (event filtering), `scan.rs` (directory walking), and `ingest/` (final guard before parsing)
- **Transactional deletes** use rusqlite's transaction API (already used in `generation/mod.rs`), applied to `delete_file_facts`

## Current Architecture

### Data Flow: watch -> indexer -> ingest -> graph -> persist

```
                       +------------------+
                       |   watcher.rs     |
                       |  (file events)   |
                       +--------+---------+
                                |
                                v
                       +------------------+
                       |   indexer.rs     |
                       |  (orchestration) |
                       +--------+---------+
                                |
        +-----------------------+-----------------------+
        |                       |                       |
        v                       v                       v
+----------------+    +----------------+    +----------------+
| scan_directory |    | reconcile_file |    |  event handler |
+-------+--------+    +-------+--------+    +-------+--------+
        |                       |                       |
        +-----------------------+-----------------------+
                                |
                                v
                       +------------------+
                       |   ingest/        |
                       | (tree-sitter)    |
                       +--------+---------+
                                |
        +-----------------------+-----------------------+
        |                       |                       |
        v                       v                       v
+----------------+    +----------------+    +----------------+
| SymbolFact     |    | ReferenceFact  |    |   CallFact    |
| (fqn field)    |    +----------------+    +----------------+
+-------+--------+
        |
        v
                       +------------------+
                       |   graph/         |
                       |  (sqlitegraph)   |
                       +--------+---------+
                                |
        +-----------------------+-----------------------+
        |                       |                       |
        v                       v                       v
+----------------+    +----------------+    +----------------+
| symbols.rs     |    | references.rs  |    |  calls.rs      |
| (symbol_id)    |    +----------------+    +----------------+
+----------------+
```

### Module Boundaries

| Module | Responsibility | Communicates With |
|--------|---------------|-------------------|
| `watcher.rs` | Filesystem events, debouncing, batching | `indexer.rs` |
| `indexer.rs` | Orchestration, reconcile coordination | `watcher.rs`, `graph/`, `ingest/` |
| `ingest/` | Language detection, tree-sitter parsing, symbol extraction | `graph/` |
| `graph/mod.rs` | Public API, entry point | `graph/*`, `ingest/` |
| `graph/schema.rs` | Node/edge payload definitions | All graph modules |
| `graph/files.rs` | File node CRUD, in-memory index | `graph/ops.rs`, `graph/query.rs` |
| `graph/symbols.rs` | Symbol node CRUD, symbol_id generation | `graph/ops.rs` |
| `graph/references.rs` | Reference node CRUD, cross-file matching | `graph/query.rs` |
| `graph/call_ops.rs` | Call node CRUD | `graph/ops.rs` |
| `graph/ops.rs` | `index_file`, `delete_file_facts`, `reconcile_file_path` | `graph/mod.rs` |
| `graph/query.rs` | Symbol queries, reference indexing | `graph/references.rs` |
| `graph/scan.rs` | Directory scanning with filtering | `graph/filter.rs` |
| `graph/filter.rs` | File filtering (gitignore, glob patterns) | `graph/scan.rs` |
| `generation/mod.rs` | Code chunk storage, SQLite side-tables | `graph/mod.rs` |

### Key Data Structures

```rust
// From ingest/mod.rs - already has fqn field!
pub struct SymbolFact {
    pub file_path: PathBuf,
    pub kind: SymbolKind,
    pub kind_normalized: String,
    pub name: Option<String>,
    pub fqn: Option<String>,        // <-- Already exists, v1 uses name
    pub byte_start: usize,
    pub byte_end: usize,
    // ... line/col fields
}

// From graph/schema.rs - SymbolNode already has symbol_id
pub struct SymbolNode {
    pub symbol_id: Option<String>,  // Generated from (language, fqn, span_id)
    pub name: Option<String>,
    pub kind: String,
    pub kind_normalized: Option<String>,
    // ... span fields
}
```

**Critical finding:** The `fqn` field already exists in `SymbolFact` and `symbol_id` generation already uses it (see `src/graph/symbols.rs:152-156`). The "FQN-as-key refactor" is primarily about **populating the FQN correctly** during tree-sitter traversal, not changing the schema.

## Feature 1: FQN Extraction and Storage

### Where FQN Extraction Happens

**Current state (v1):**
- `Parser::extract_symbol()` sets `fqn = name.clone()` (simple name only)
- This happens during `walk_tree()` traversal in `src/ingest/mod.rs`
- Each language parser inherits this pattern

**Target state (v1.1):**
- FQN should be computed from the **hierarchical context** during tree-sitter traversal
- Requires tracking parent scopes (modules, impl blocks, classes) during traversal

### Architecture Integration

```
+-------------------+
| tree-sitter parse |
+--------+----------+
         |
         v
+-------------------+     +----------------------+
|  walk_tree()      |---->|  track_scope_stack() |
|  (current)        |     |  (NEW for v1.1)      |
+-------------------+     +----------+-----------+
                                      |
                                      v
                             +-------------------+
                             | compute_fqn()     |
                             | (name + scope)    |
                             +---------+---------+
                                       |
                                       v
                             +-------------------+
                             | SymbolFact.fqn    |
                             | (already exists)  |
                             +-------------------+
```

### Implementation Location

| Location | Responsibility |
|----------|---------------|
| `src/ingest/mod.rs` | Add `ScopeStack` struct to track nesting during `walk_tree()` |
| `src/ingest/{rust,python,java,etc}.rs` | Language-specific scope tracking (e.g., `mod` for Rust, `class` for Python) |
| `src/graph/symbols.rs` | Already uses `fqn` for `symbol_id` generation (no changes needed) |
| `src/graph/schema.rs` | No schema changes needed |

### FQN Format by Language

| Language | FQN Format | Example |
|----------|------------|---------|
| Rust | `crate::module::item::name` | `my_crate::utils::parse` |
| Python | `module.Class.method` | `mypkg.MyClass.run` |
| Java | `pkg.Class.method` | `com.example.App.main` |
| C++ | `namespace::Class::method` | `std::vector::push_back` |
| JavaScript | `path/to/module.function` (path-based) | `src/utils/helpers.format` |
| TypeScript | `namespace.Class.method` or path-based | `utils.Helpers.format` |

**Note:** FQN format is per-language and must account for each language's scoping rules.

### Dependencies

- FQN extraction **does not depend on** path validation
- FQN extraction **does not depend on** transactional deletes
- Can be implemented **independently** as a pure refactoring of the ingest module

## Feature 2: Path Traversal Validation

### Where Validation Should Occur

Path validation must happen **before** any file content is accessed to prevent directory traversal attacks.

```
+------------------+      validate_path()
|  filesystem API  | <------------------+
|  (std::fs)       |                     |
+--------+---------+                     |
         ^                               |
         |                               |
+--------+---------+             +-------+--------+
|  watcher.rs     |             |  scan.rs       |
|  (event paths)  |             |  (walkdir)     |
+--------+---------+             +-------+--------+
         |                               |
         |                               |
         v                               v
+------------------+             +------------------+
| ingest/          |             | filter.rs        |
| (final guard)    |             | (should_skip)    |
+------------------+             +------------------+
```

### Validation Points

| Entry Point | Current Behavior | v1.1 Addition | Risk Level |
|-------------|------------------|---------------|------------|
| `watcher.rs::extract_dirty_paths()` | Checks `is_dir()`, skips DB files | Add path traversal check | **HIGH** - user-controlled input |
| `scan.rs::scan_directory_with_filter()` | Uses `walkdir`, applies filters | Add path traversal check | **HIGH** - user-controlled input |
| `filter.rs::should_skip()` | Checks language, gitignore | Already checks `is_file()` | **LOW** - defensive check |
| `ingest/mod.rs::extract_symbols()` | Assumes valid path | Add assert/validation | **MEDIUM** - should never fail if above guards work |

### Path Traversal Attack Patterns

```rust
// Attack patterns to validate against:
"../../../etc/passwd"           // Parent directory traversal
"/absolute/path/to/file"        // Absolute path outside root
"~/user/.ssh/config"            // Home directory expansion
"C:\\Windows\\System32\\config" // Windows absolute path
"./subdir/../../etc/passwd"     // Mixed relative/parent
"symlink_to_outside_root"       // Symlink escape
```

### Implementation Location

| Location | Validation Strategy |
|----------|-------------------|
| `watcher.rs` | Canonicalize path, verify it starts with `root_path` |
| `scan.rs` | Same as watcher, reuse validation function |
| `graph/filter.rs` | Add `is_within_root()` helper |
| New: `src/validation.rs` | Centralized path validation utilities |

### Canonicalization Strategy

```rust
// From src/graph/filter.rs:69 - already does this!
let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

// v1.1: Apply same canonicalization to all file paths before processing
fn validate_path(path: &Path, root: &Path) -> Result<PathBuf, PathValidationError> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| PathValidationError::CannotCanonicalize)?;

    if !canonical.starts_with(root) {
        return Err(PathValidationError::OutsideRoot(path.to_path_buf()));
    }

    Ok(canonical)
}
```

**Critical finding:** `filter.rs` already canonicalizes the root. Extend this pattern to all file paths.

### Dependencies

- Path validation **does not depend on** FQN extraction
- Path validation **does not depend on** transactional deletes
- Should be implemented **first** as a safety baseline

## Feature 3: Transactional Deletes

### Current Delete Flow

```
reconcile_file_path(path)
        |
        v
delete_file_facts(graph, path)
        |
        +--> 1) Find file node, collect symbol IDs
        |
        +--> 2) Delete symbols (via sqlitegraph delete_entity)
        |
        +--> 3) Delete references in file
        |
        +--> 4) Delete calls in file
        |
        +--> 5) Delete code chunks
        |
        +--> 6) Delete file node
        |
        +--> 7) Cleanup edges
```

**Current problem:** Steps 1-7 are not atomic. If the process crashes mid-delete, the database may have orphaned entities.

### Transaction Boundaries

**sqlitegraph auto-transactions:**
- Each `insert_node()`, `insert_edge()`, `delete_entity()` runs in its own transaction
- This is **fine for single operations** but **not for multi-step deletes**

**rusqlite explicit transactions (already used):**
- See `src/generation/mod.rs:110-138` for the pattern:
  ```rust
  let tx = conn.unchecked_transaction()?;
  // ... multiple operations ...
  tx.commit()?;
  ```

### Implementation Location

| Location | Transaction Strategy |
|----------|---------------------|
| `src/graph/ops.rs::delete_file_facts()` | Wrap entire operation in rusqlite transaction |
| `src/generation/mod.rs` | Already uses transactions for `store_chunks()` - reference this pattern |

### Transaction Implementation Pattern

```rust
// Based on generation/mod.rs:110-138
pub fn delete_file_facts(graph: &mut CodeGraph, path: &str) -> Result<()> {
    use crate::graph::schema::delete_edges_touching_entities;

    // Get connection for explicit transaction control
    let conn = graph.chunks.connect()?;

    // Start transaction
    let tx = conn.unchecked_transaction()
        .map_err(|e| anyhow::anyhow!("Failed to start transaction: {}", e))?;

    // ... perform all delete operations within tx ...

    // Commit on success
    tx.commit()
        .map_err(|e| anyhow::anyhow!("Failed to commit delete transaction: {}", e))?;

    Ok(())
}
```

### Why Use `chunks.connect()`?

From `src/generation/mod.rs:19-21`:
> Uses a separate rusqlite connection to the same database file for code chunk storage. This is necessary because sqlitegraph's connection is private to the crate.

**Pattern established:** Use `chunks.connect()` to get a rusqlite connection for operations requiring explicit transaction control.

### Transaction Scope

```
BEGIN TRANSACTION
  |
  +-- [1] Find file node
  +-- [2] Collect symbol IDs
  +-- [3] Delete symbols (delete_entity)
  +-- [4] Delete references
  +-- [5] Delete calls
  +-- [6] Delete chunks
  +-- [7] Delete file node
  +-- [8] Cleanup edges
  |
COMMIT (or ROLLBACK on error)
```

### Dependencies

- Transactional deletes **do not depend on** FQN extraction
- Transactional deletes **do not depend on** path validation
- Can be implemented **independently** as a refactoring of `delete_file_facts()`

## Build Order (Dependencies)

```
Phase 1: Path Traversal Validation
  |
  +-- watcher.rs: validate incoming event paths
  +-- scan.rs: validate paths during directory walk
  +-- validation.rs: new module for centralized path utilities
  |
  +-- NO dependencies on other features
  +-- Provides safety baseline for ALL operations

Phase 2: FQN Extraction (can parallel with Phase 3)
  |
  +-- ingest/mod.rs: add ScopeStack
  +-- ingest/{lang}.rs: language-specific scope tracking
  +-- NO schema changes (fqn field already exists)
  +-- Updates symbol_id generation (already uses fqn)
  |
  +-- NO dependencies on other features
  +-- Pure refactoring of ingest logic

Phase 3: Transactional Deletes (can parallel with Phase 2)
  |
  +-- graph/ops.rs: wrap delete_file_facts in transaction
  +-- Follows pattern from generation/mod.rs
  +-- NO schema changes
  |
  +-- NO dependencies on other features
  +-- Refactoring of delete logic
```

**Recommended build order:** Path validation first (safety), then FQN + deletes in parallel (can proceed safely with path guards in place).

## Component Boundary Changes

### New Modules

| Module | Purpose |
|--------|---------|
| `src/validation.rs` | Centralized path validation utilities |
| `src/ingest/scope.rs` (optional) | Scope tracking for FQN computation (could live in `mod.rs`) |

### Modified Modules

| Module | Changes | Risk |
|--------|---------|------|
| `watcher.rs` | Add path validation to `extract_dirty_paths()` | **MEDIUM** - affects watch loop |
| `scan.rs` | Add path validation before processing each path | **LOW** - isolated to scan |
| `ingest/mod.rs` | Add `ScopeStack` to `Parser`, update `extract_symbol()` | **MEDIUM** - affects all parsers |
| `ingest/{lang}.rs` | Language-specific scope tracking | **MEDIUM** - per-language complexity |
| `graph/ops.rs` | Wrap `delete_file_facts` in transaction | **LOW** - localized change |
| `graph/filter.rs` | Add `is_within_root()` helper | **LOW** - pure function addition |

### Unchanged Modules

| Module | Why Unchanged |
|--------|---------------|
| `graph/schema.rs` | FQN field already exists, no schema changes needed |
| `graph/symbols.rs` | Already uses `fqn` for symbol_id generation |
| `graph/files.rs` | File ops don't change |
| `graph/query.rs` | Query logic doesn't depend on FQN format |
| `references.rs` | References use symbol_id, not FQN directly |
| `generation/mod.rs` | Reference pattern for transactions, not modified |

## Risk Areas

### High Risk Areas

| Area | Risk | Mitigation |
|------|------|------------|
| FQN extraction changes symbol_id | Existing symbol_ids will change, breaking cross-file references | **Data migration required** - re-index all files, update references |
| Scope tracking complexity | Different languages have different scoping rules | Per-language implementation, extensive testing |
| Transaction wrapping may hide errors | Errors that were previously partial-fail become all-or-nothing | Comprehensive error testing, verify rollback behavior |

### Medium Risk Areas

| Area | Risk | Mitigation |
|------|------|------------|
| Path validation performance | Canonicalization is expensive, slows indexing | Cache canonicalized paths, validate once per path |
| Transaction contention | Long-running deletes block other operations | Keep transactions short, batch operations carefully |
| Symlink handling | Canonicalizing symlinks may behave unexpectedly | Document symlink behavior, add tests |

### Low Risk Areas

| Area | Risk | Mitigation |
|------|------|------------|
| filter.rs changes | Pure function additions, isolated | Standard unit tests |
| ops.rs transaction wrapping | Localized change, pattern exists | Test rollback scenarios |

## Data Flow Changes

### Current FQN Flow (v1)

```
tree-sitter parse
      |
      v
extract_symbol()
      |
      +-- name = "my_function"
      +-- fqn = "my_function"  (just name)
      |
      v
SymbolFact { fqn: "my_function" }
      |
      v
symbols.rs::insert_symbol_node()
      |
      +-- symbol_id = generate_symbol_id(language, "my_function", span_id)
```

### Target FQN Flow (v1.1)

```
tree-sitter parse
      |
      v
walk_tree() with ScopeStack
      |
      +-- Enter: module "utils"
      +-- Enter: impl "MyStruct"
      +-- Found: function "my_function"
      |
      v
extract_symbol()
      |
      +-- name = "my_function"
      +-- fqn = "crate::utils::MyStruct::my_function"  (computed from scope)
      |
      v
SymbolFact { fqn: "crate::utils::MyStruct::my_function" }
      |
      v
symbols.rs::insert_symbol_node()
      |
      +-- symbol_id = generate_symbol_id(language, "crate::utils::MyStruct::my_function", span_id)
```

### Current Delete Flow (v1)

```
reconcile_file_path(path)
      |
      v
delete_file_facts()
      |
      +-- [TX1] Delete symbols
      +-- [TX2] Delete references
      +-- [TX3] Delete calls
      +-- [TX4] Delete chunks
      +-- [TX5] Delete file node
      +-- [TX6] Cleanup edges
```

**Problem:** 6 separate transactions. Crash between any steps leaves inconsistent state.

### Target Delete Flow (v1.1)

```
reconcile_file_path(path)
      |
      v
delete_file_facts()
      |
      +-- BEGIN TX
      |
      +-- Delete symbols
      +-- Delete references
      +-- Delete calls
      +-- Delete chunks
      +-- Delete file node
      +-- Cleanup edges
      |
      +-- COMMIT TX (or ROLLBACK on error)
```

**Solution:** Single transaction ensures atomicity.

## Testing Strategy

### Path Validation Tests

```rust
#[test]
fn test_reject_parent_traversal() {
    let root = TempDir::new().unwrap();
    let malicious = root.path().join("../../../etc/passwd");

    let result = validate_path(&malicious, root.path());
    assert!(matches!(result, Err(PathValidationError::OutsideRoot(_))));
}

#[test]
fn test_reject_absolute_path_outside_root() {
    let root = TempDir::new().unwrap();
    let outside = Path::new("/etc/passwd");

    let result = validate_path(outside, root.path());
    assert!(matches!(result, Err(PathValidationError::OutsideRoot(_))));
}

#[test]
fn test_reject_symlink_escape() {
    // Create symlink inside root pointing outside
    // Verify validation catches it
}
```

### FQN Extraction Tests

```rust
#[test]
fn test_rust_fqn_nested_module() {
    let source = r#"
mod utils {
    pub fn parse() -> Result { }
}
"#;
    let mut parser = Parser::new().unwrap();
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source.as_bytes());

    let fact = facts.iter().find(|f| f.name.as_deref() == Some("parse")).unwrap();
    assert_eq!(fact.fqn.as_deref(), Some("utils::parse"));
}

#[test]
fn test_rust_fqn_impl_method() {
    let source = r#"
struct MyStruct;
impl MyStruct {
    pub fn new() -> Self { }
}
"#;
    let mut parser = Parser::new().unwrap();
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source.as_bytes());

    let fact = facts.iter().find(|f| f.name.as_deref() == Some("new")).unwrap();
    assert_eq!(fact.fqn.as_deref(), Some("MyStruct::new"));
}
```

### Transactional Delete Tests

```rust
#[test]
fn test_delete_file_atomic() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Index a file
    graph.index_file("test.rs", b"fn test() {}").unwrap();

    // Mock a failure during delete (inject error)
    // Verify rollback: file data should still exist

    // Successful delete should remove all traces
    graph.delete_file_facts("test.rs").unwrap();

    // Verify no orphaned entities
    assert!(graph.symbols_in_file("test.rs").unwrap().is_empty());
}
```

## Integration with Existing Patterns

### sqlitegraph Constraints

From `/home/feanor/Projects/magellan/docs/SQLITEGRAPH_GUIDE.md`:

1. **Opaque JSON payloads** - Already using `SymbolNode` with `symbol_id` field
2. **Use concrete `SqliteGraphBackend`** - Already done in `graph/mod.rs`
3. **Import `GraphBackend` trait** - Already done
4. **Manual cascade deletes** - `delete_file_facts` already handles this
5. **Maintain in-memory indexes** - `file_index` in `files.rs`

**Conclusion:** v1.1 changes are compatible with existing sqlitegraph patterns.

### Transaction Pattern Reference

From `/home/feanor/Projects/magellan/src/generation/mod.rs:110-138`:

```rust
let tx = conn.unchecked_transaction()
    .map_err(|e| anyhow::anyhow!("Failed to start transaction: {}", e))?;

for chunk in chunks {
    tx.execute(...)?;
}

tx.commit()
    .map_err(|e| anyhow::anyhow!("Failed to commit transaction: {}", e))?;
```

**Apply this exact pattern to `delete_file_facts`.**

### Filter Pattern Reference

From `/home/feanor/Projects/magellan/src/graph/filter.rs:69`:

```rust
let root = std::fs::canonicalize(root)
    .unwrap_or_else(|_| root.to_path_buf());
```

**Extend this pattern to validate all paths before processing.**

## Phase-Specific Warnings

| Phase | Topic | Warning | Mitigation |
|-------|-------|---------|------------|
| FQN extraction | Data migration | Changing symbol_id breaks all existing references | Plan full re-index, version the schema |
| FQN extraction | Language complexity | Each language has unique scoping | Implement per-language, test extensively |
| Path validation | Symlinks | Canonicalization follows symlinks, may escape root | Document behavior, add explicit symlink check |
| Transactional deletes | Error handling | Rollback may hide intermittent failures | Log all rollback attempts, investigate causes |
| Transactional deletes | Performance | Long transactions lock the database | Keep operations minimal, batch carefully |

## Sources

| File | Confidence | Notes |
|------|------------|-------|
| `/home/feanor/Projects/magellan/src/lib.rs` | HIGH | Module structure, re-exports |
| `/home/feanor/Projects/magellan/src/ingest/mod.rs` | HIGH | SymbolFact structure, FQN field exists, Parser::extract_symbol implementation |
| `/home/feanor/Projects/magellan/src/graph/symbols.rs` | HIGH | symbol_id generation already uses fqn, generate_symbol_id algorithm |
| `/home/feanor/Projects/magellan/src/graph/ops.rs` | HIGH | delete_file_facts implementation, reconcile_file_path flow |
| `/home/feanor/Projects/magellan/src/graph/filter.rs` | HIGH | FileFilter, should_skip, canonicalization pattern |
| `/home/feanor/Projects/magellan/src/generation/mod.rs` | HIGH | Transaction pattern (unchecked_transaction, commit) |
| `/home/feanor/Projects/magellan/src/watcher.rs` | HIGH | WatcherBatch, extract_dirty_paths flow |
| `/home/feanor/Projects/magellan/src/indexer.rs` | HIGH | run_watch_pipeline, process_dirty_paths |
| `/home/feanor/Projects/magellan/docs/SQLITEGRAPH_GUIDE.md` | HIGH | sqlitegraph API patterns, constraints |
| `/home/feanor/Projects/magellan/.planning/phases/02-deterministic-watch--indexing-pipeline/02-CONTEXT.md` | HIGH | Phase 2 context, deterministic guarantees |

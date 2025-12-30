# Magellan Cross-File Resolution Roadmap

**Version**: 2.0.0
**Date**: 2025-12-28
**Status**: Planning Phase

---

## Executive Summary

Extend Magellan from same-file-only symbol resolution (~50% effective precision) to **cross-file resolution with 95-98% precision** using import-aware AST analysis.

**No LSP required. No compilation required. Pure AST-based path following.**

---

## Current State Analysis

### What Magellan Does Today

| Feature | Precision | Method |
|---------|-----------|--------|
| Symbol extraction | 100% | tree-sitter AST |
| Reference locations | 100% | tree-sitter AST |
| Same-file name matching | 95% | Name comparison |
| **Cross-file resolution** | **0%** | **Not implemented** |
| **Overall effective** | **~50%** | — |

### Current Limitation

From `src/references.rs:164-167`:
```rust
let referenced_symbol = symbols.iter().find(|s| {
    s.name.as_ref().map(|n| n == symbol_name).unwrap_or(false)
})?;
```

**Only matches symbols from the same file.** No cross-file tracking.

---

## The Solution: Import-Aware Resolution

### Core Insight

**Import statements in the AST tell us exactly where symbols come from.**

```rust
use crate::b::foo;  // ← Literally says: foo comes from crate::b

fn main() {
    foo();  // ← We can now resolve this!
}
```

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   Magellan Indexing                    │
├─────────────────────────────────────────────────────────┤
│  1. Parse AST (tree-sitter)                            │
│  2. Extract symbols WITH module_path                   │
│  3. Extract imports (use, import, from)                │
│  4. Build import graph in SQLiteGraph                  │
└─────────────────────────────────────────────────────────┘
                        │
                        ↓
┌─────────────────────────────────────────────────────────┐
│                 Symbol Resolution                       │
├─────────────────────────────────────────────────────────┤
│  Query: "What does identifier 'foo' refer to?"         │
│                                                         │
│  1. Check local file → Not found?                      │
│  2. Check imports → Found "use crate::b::foo;"         │
│  3. Resolve module_path → Find file "src/b.rs"         │
│  4. Query symbols → Find "foo" in that file            │
│  5. Return: foo → src/b.rs:42                          │
└─────────────────────────────────────────────────────────┘
```

---

## Data Model Changes

### Enhanced SymbolFact

```rust
pub struct SymbolFact {
    // Existing fields
    pub file_path: PathBuf,
    pub kind: SymbolKind,
    pub name: Option<String>,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub end_line: usize,

    // NEW FIELDS for cross-file resolution
    pub module_path: String,        // "crate::foo::bar::Baz"
    pub fully_qualified: String,    // "crate::foo::bar::Baz"
    pub visibility: Visibility,      // Public, Restricted, Private
}
```

### New ImportFact

```rust
pub struct ImportFact {
    pub file_path: PathBuf,
    pub import_kind: ImportKind,
    pub import_path: Vec<String>,   // ["crate", "b", "foo"]
    pub imported_names: Vec<String>, // ["foo", "bar"]
    pub is_glob: bool,
    pub byte_start: usize,
    pub byte_end: usize,
}

pub enum ImportKind {
    UseCrate,      // use crate::module::symbol
    UseSuper,      // use super::symbol
    UseSelf,       // use self::symbol
    ExternCrate,   // use extern_crate::symbol
    PlainUse,      // use module::symbol
}
```

### SQLiteGraph Schema Additions

```sql
-- Import nodes (new)
CREATE TABLE import_nodes (
    id INTEGER PRIMARY KEY,
    file_id INTEGER,
    import_kind TEXT,
    import_path TEXT,              -- "crate::b::foo"
    imported_names TEXT,           -- JSON: ["foo", "bar"]
    is_glob BOOLEAN,
    byte_start INTEGER,
    byte_end INTEGER,
    data JSON
);

-- IMPORTS edge (new)
-- File → File or File → Module
-- Represents "this file imports from that target"
```

---

## Resolution Algorithm

```
FUNCTION resolve_symbol(file_id, identifier_name):

    // Step 1: Check local definitions
    local_symbol = find_local_symbol(file_id, identifier_name)
    IF local_symbol FOUND:
        RETURN local_symbol

    // Step 2: Check explicit imports
    imports = get_imports(file_id)
    FOR EACH import IN imports:
        IF identifier_name IN import.imported_names:
            target_file = resolve_import_path(import.import_path)
            symbol = find_symbol_in_file(target_file, identifier_name)
            IF symbol FOUND:
                RETURN symbol

    // Step 3: Check glob imports (lower confidence)
    FOR EACH import IN imports WHERE import.is_glob:
        target_file = resolve_import_path(import.import_path)
        symbol = find_public_symbol_in_file(target_file, identifier_name)
        IF symbol FOUND:
            RETURN symbol WITH lower_confidence

    // Step 4: Not found
    RETURN None
END FUNCTION
```

---

## Implementation Phases

### Phase 1: Enhanced Symbol Storage (Week 1)

**Goal**: Store symbols with module path and visibility.

**File**: `src/ingest/mod.rs`

**Tasks**:
1. [ ] Add `module_path: String` to `SymbolFact`
2. [ ] Add `fully_qualified: String` to `SymbolFact`
3. [ ] Add `visibility: Visibility` to `SymbolFact`
4. [ ] Create `Visibility` enum (Public, Restricted, Private)
5. [ ] Extract module declarations (`mod foo;`)
6. [ ] Build module path during AST walk
7. [ ] Update database schema
8. [ ] Write tests for module path extraction

**Acceptance Criteria**:
- Symbols stored with `module_path = "crate::module::submodule"`
- Fully qualified names built correctly
- Visibility detected from `pub`, `pub(crate)`, etc.

### Phase 2: Import Extraction (Week 2)

**Goal**: Extract and store import statements.

**File**: `src/ingest/imports.rs` (new)

**Tasks**:
1. [ ] Create `ImportFact` struct
2. [ ] Create `ImportKind` enum
3. [ ] Create `ImportExtractor` using tree-sitter
4. [ ] Extract `use crate::X::Y` statements
5. [ ] Extract `use super::X` statements
6. [ ] Extract `use self::X` statements
7. [ ] Extract `use extern_crate::X` statements
8. [ ] Extract glob imports `use X::*`
9. [ ] Store imports in SQLiteGraph
10. [ ] Write tests for import extraction

**Acceptance Criteria**:
- All import types extracted correctly
- Imports stored in database
- Tests pass for various import patterns

### Phase 3: Module Path Resolution (Week 3)

**Goal**: Resolve module paths to file paths.

**File**: `src/resolve/module.rs` (new)

**Tasks**:
1. [ ] Create `ModuleResolver` struct
2. [ ] Build `module_path → file_id` index
3. [ ] Resolve `crate::*` paths (absolute)
4. [ ] Resolve `super::*` paths (parent module)
5. [ ] Resolve `self::*` paths (current module)
6. [ ] Handle relative paths
7. [ ] Add module resolution to CodeGraph API
8. [ ] Write tests for module resolution

**Acceptance Criteria**:
- `crate::foo::bar` resolves to correct file
- `super::foo` resolves to parent module
- `self::foo` resolves to current module
- All tests pass

### Phase 4: Cross-File Symbol Resolution (Week 4)

**Goal**: Resolve symbols across files using imports.

**File**: `src/resolve/cross_file.rs` (new)

**Tasks**:
1. [ ] Create `resolve_symbol_cross_file()` function
2. [ ] Query import edges for file
3. [ ] Follow import paths to target files
4. [ ] Search for symbol in target files
5. [ ] Return resolved symbol with file location
6. [ ] Handle multiple matches (name collision)
7. [ ] Handle glob imports
8. [ ] Update `references.rs` to use cross-file resolution
9. [ ] Update `calls.rs` to use cross-file resolution
10. [ ] Write integration tests

**Acceptance Criteria**:
- References resolved across files
- Calls resolved across files
- Integration tests pass
- No regression in same-file resolution

### Phase 5: CLI & Query Interface (Week 5)

**Goal**: Expose cross-file resolution to users.

**File**: `src/main.rs`, `src/cli/*.rs`

**Tasks**:
1. [ ] Add `--follow-imports` flag to relevant commands
2. [ ] Update `status` to show import statistics
3. [ ] Add `imports` command to list imports
4. [ ] Update `export` to include import edges
5. [ ] Add `resolve` command for manual symbol queries
6. [ ] Update documentation

**Acceptance Criteria**:
- All CLI commands work with imports
- Documentation updated
- User can query symbol resolution

### Phase 6: Validation & Performance (Week 6)

**Goal**: Verify precision and performance.

**Tasks**:
1. [ ] Create precision validation test suite
2. [ ] Test on real Rust projects (ripgrep, etc.)
3. [ ] Benchmark resolution queries
4. [ ] Add caching if needed
5. [ ] Performance optimization
6. [ ] Update benchmarks

**Acceptance Criteria**:
- Precision >95% on validation tests
- Resolution <100ms per query
- No performance regression

---

## Precision Targets

| Scenario | Current | Target | Method |
|----------|---------|--------|--------|
| Same file | 95% | 95% | Unchanged |
| `use crate::X::Y` | 0% | 99% | Path following |
| `use super::X` | 0% | 95% | Parent module |
| `use extern_crate::X` | 0% | 90% | Cargo metadata |
| Glob imports | 0% | 85% | Search target |
| Fully qualified | 0% | 99% | AST parsing |
| **Overall** | **~50%** | **95-98%** | — |

---

## Edge Cases & Handling

### Case 1: Name Collisions

```rust
// a.rs
pub fn foo() {}

// b.rs
pub fn foo() {}

// main.rs
use crate::a::foo;
use crate::b::foo;  // ← Collision!
```

**Handling**: Most recent import wins (document this)

### Case 2: Glob Imports

```rust
use crate::module::*;

foo();  // ← Where?
```

**Handling**: Search all public symbols in target, mark as lower confidence

### Case 3: Re-exports

```rust
// a.rs
pub use crate::b::foo;
```

**Handling**: Follow re-export chain (treat as import)

### Case 4: Conditional Compilation

```rust
#[cfg(feature = "foo")]
pub fn foo() {}
```

**Handling**: Store CFG conditions, match when querying

### Case 5: Macros

```rust
vec![1, 2, 3]
```

**Handling**: Defer to Phase 2+ (complex, need expansion)

---

## Testing Strategy

### Unit Tests

```rust
// tests/module_resolution_tests.rs
test_resolve_crate_absolute_path()
test_resolve_super_relative_path()
test_resolve_self_relative_path()
test_resolve_extern_crate()

// tests/import_extraction_tests.rs
test_extract_simple_use()
test_extract_use_crate()
test_extract_use_super()
test_extract_glob_import()

// tests/cross_file_resolution_tests.rs
test_resolve_cross_file_symbol()
test_resolve_via_reexport()
test_resolve_glob_import()
test_name_collision_handling()
```

### Integration Tests

```rust
// Create test project:
// src/
//   main.rs  (uses crate::a::foo)
//   a.rs     (pub fn foo())
//   b.rs     (pub fn foo())

test_resolve_finds_correct_foo()
test_resolution_with_multiple_matches()
test_export_includes_import_edges()
```

### Real-World Validation

1. Index open-source Rust projects
2. Sample 100 random symbol references
3. Compare with ground truth (rust-analyzer)
4. Calculate precision

**Target Projects**:
- [ ] ripgrep
- [ ] bat
- [ ] hyper
- [ ] tokio

---

## Performance Considerations

### Caching Strategy

```rust
pub struct ResolutionCache {
    // (file_id, symbol_name) → symbol_id
    symbol_cache: HashMap<(u64, String), u64>,

    // file_id → Vec<import_id>
    import_cache: HashMap<u64, Vec<u64>>,

    // module_path → file_id
    module_cache: HashMap<String, u64>,
}
```

### Query Optimization

1. **Index lookups**: O(1) for module paths
2. **Bounded traversal**: Max import depth = 10
3. **Lazy loading**: Only resolve when queried
4. **Batch queries**: Resolve multiple symbols at once

### Benchmarks

| Operation | Target | Notes |
|-----------|--------|-------|
| Symbol extraction | 50K ops/sec | Current baseline |
| Import extraction | 50K ops/sec | Similar to symbols |
| Cross-file resolve | <100ms | Per query |
| Export with imports | <5 sec | For 500 files |

---

## Backward Compatibility

### Database Migration

```sql
-- Add new columns to existing nodes
ALTER TABLE graph_entities ADD COLUMN module_path TEXT;
ALTER TABLE graph_entities ADD COLUMN visibility TEXT;

-- Migration script for existing data
UPDATE graph_entities
SET module_path = 'crate',
    visibility = 'Public'
WHERE kind IN ('rust_function', 'rust_struct', ...);
```

### API Compatibility

```rust
// Old API still works
let symbols = graph.symbols_in_file(path)?;

// New API for cross-file
let symbol = graph.resolve_symbol(file_id, "foo")?;
let symbols = graph.resolve_symbol_all_files("foo")?;
```

---

## Success Criteria

- [ ] Cross-file resolution implemented for Rust
- [ ] Precision >95% on validation tests
- [ ] Performance <100ms per query
- [ ] All existing tests pass (no regression)
- [ ] Real-world projects indexed successfully
- [ ] Documentation complete

---

## Future Enhancements (Out of Scope)

- [ ] Macro expansion and resolution
- [ ] Generic specialization tracking
- [ ] Trait impl resolution
- [ ] Async/await transformation tracking
- [ ] Procedural macro expansion

---

## Dependencies

**None** - Uses existing tree-sitter and sqlitegraph

---

## Related Documents

- [CROSS_FILE_RESOLUTION.md](./CROSS_FILE_RESOLUTION.md) - Detailed design
- [MANUAL.md](./MANUAL.md) - User manual
- [CHANGELOG.md](./CHANGELOG.md) - Version history

---

**Document Version**: 2.0.0
**Last Updated**: 2025-12-28
**Author**: AI Research + User Requirements
**Status**: DRAFT - Pending Approval

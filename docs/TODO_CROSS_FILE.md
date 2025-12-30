# Magellan Cross-File Resolution TODO

**Linked Document**: [CROSS_FILE_ROADMAP.md](./CROSS_FILE_ROADMAP.md)
**Last Updated**: 2025-12-28

---

## Status Legend

- [ ] TODO - Not started
- [~] IN PROGRESS - Started but not complete
- [x] DONE - Completed
- [!] BLOCKED - Waiting on dependency

---

## Phase 1: Enhanced Symbol Storage (Week 1)

### 1.1 Data Model Changes
- [ ] Add `Visibility` enum to `src/ingest/mod.rs`
  - [ ] `Public` - `pub`
  - [ ] `Restricted` - `pub(crate)`, `pub(super)`, `pub(in path)`
  - [ ] `Private` - no visibility modifier
- [ ] Add `module_path: String` to `SymbolFact`
- [ ] Add `fully_qualified: String` to `SymbolFact`
- [ ] Add `visibility: Visibility` to `SymbolFact`

### 1.2 Module Declaration Extraction
- [ ] Extract `mod foo;` declarations
- [ ] Extract `mod foo { }` inline modules
- [ ] Build module path during AST walk
- [ ] Handle nested modules (e.g., `mod a { mod b { } }`)
- [ ] Handle `mod` with path attribute (e.g., `#[path = "file.rs"]`)

### 1.3 Visibility Detection
- [ ] Detect `pub` keyword
- [ ] Detect `pub(crate)` keyword
- [ ] Detect `pub(super)` keyword
- [ ] Detect `pub(in crate::foo)` keyword
- [ ] Default to `Private` if no visibility modifier

### 1.4 Database Schema
- [ ] Add `module_path` column to graph_entities
- [ ] Add `visibility` column to graph_entities
- [ ] Update JSON schema documentation
- [ ] Create migration script for existing databases

### 1.5 Tests
- [ ] Test module path extraction for simple modules
- [ ] Test module path extraction for nested modules
- [ ] Test visibility detection
- [ ] Test fully qualified name construction
- [ ] Test backward compatibility with old schema

**Phase 1 Completion Criteria**:
- Symbols stored with module path
- Visibility correctly detected
- All tests pass

---

## Phase 2: Import Extraction (Week 2)

### 2.1 Import Data Structures
- [ ] Create `src/ingest/imports.rs`
- [ ] Define `ImportFact` struct
- [ ] Define `ImportKind` enum
  - [ ] `UseCrate`
  - [ ] `UseSuper`
  - [ ] `UseSelf`
  - [ ] `ExternCrate`
  - [ ] `PlainUse`

### 2.2 ImportExtractor Implementation
- [ ] Create `ImportExtractor` struct
- [ ] Implement tree-sitter parser setup
- [ ] Implement `extract_imports()` function
- [ ] Implement `walk_tree_for_imports()` function
- [ ] Implement `extract_use_statement()` function

### 2.3 Import Pattern Extraction
- [ ] Extract `use crate::X::Y;`
- [ ] Extract `use super::X;`
- [ ] Extract `use self::X;`
- [ ] Extract `use extern_crate::X;`
- [ ] Extract `use X::Y;` (plain)
- [ ] Extract `use X::{Y, Z};` (named imports)
- [ ] Extract `use X::*;` (glob)

### 2.4 Database Storage
- [ ] Add imports to SQLiteGraph
- [ ] Create import_nodes table
- [ ] Create IMPORTS edges
- [ ] Store import metadata in JSON

### 2.5 Tests
- [ ] Test simple use extraction
- [ ] Test crate-relative use extraction
- [ ] Test super-relative use extraction
- [ ] Test extern crate use extraction
- [ ] Test named import extraction (`{a, b}`)
- [ ] Test glob import extraction (`*`)
- [ ] Test complex nested imports

**Phase 2 Completion Criteria**:
- All import types extracted
- Imports stored in database
- Tests pass for all patterns

---

## Phase 3: Module Path Resolution (Week 3)

### 3.1 ModuleResolver Structure
- [ ] Create `src/resolve/module.rs`
- [ ] Define `ModuleResolver` struct
- [ ] Add module_path → file_id index

### 3.2 Path Resolution Functions
- [ ] Implement `resolve_crate_path()` - `crate::*` → absolute
- [ ] Implement `resolve_super_path()` - `super::*` → parent
- [ ] Implement `resolve_self_path()` - `self::*` → current
- [ ] Implement `resolve_extern_crate()` - external dependencies

### 3.3 Relative Path Handling
- [ ] Handle relative paths in mod declarations
- [ ] Handle `#[path = "..."]` attribute
- [ ] Handle nested module paths

### 3.4 Index Building
- [ ] Build module index during indexing
- [ ] Update index on file changes
- [ ] Persist index to database

### 3.5 CodeGraph API
- [ ] Add `resolve_module()` to CodeGraph
- [ ] Add `get_file_by_module()` to CodeGraph
- [ ] Add `list_modules()` to CodeGraph

### 3.6 Tests
- [ ] Test crate path resolution
- [ ] Test super path resolution
- [ ] Test self path resolution
- [ ] Test nested module resolution
- [ ] Test extern crate resolution
- [ ] Test relative path edge cases

**Phase 3 Completion Criteria**:
- Module paths resolve correctly
- All path types supported
- CodeGraph API works

---

## Phase 4: Cross-File Symbol Resolution (Week 4)

### 4.1 Resolution Engine
- [ ] Create `src/resolve/cross_file.rs`
- [ ] Define `ResolvedSymbol` struct
  - [ ] symbol_id
  - [ ] file_id
  - [ ] confidence (0.0 to 1.0)
  - [ ] resolution_method

### 4.2 Core Resolution Algorithm
- [ ] Implement `resolve_symbol_cross_file()`
- [ ] Step 1: Check local definitions
- [ ] Step 2: Check explicit imports
- [ ] Step 3: Check glob imports
- [ ] Step 4: Return None if not found

### 4.3 Import Following
- [ ] Implement `follow_import_path()`
- [ ] Handle single-name imports
- [ ] Handle named imports (`{a, b}`)
- [ ] Handle glob imports (`*`)

### 4.4 Name Collision Handling
- [ ] Detect when multiple matches exist
- [ ] Return all matches with metadata
- [ ] Allow user to disambiguate
- [ ] Document "most recent wins" behavior

### 4.5 Integration with Existing Code
- [ ] Update `ReferenceExtractor` to use cross-file resolution
- [ ] Update `CallExtractor` to use cross-file resolution
- [ ] Maintain backward compatibility

### 4.6 Tests
- [ ] Test cross-file simple resolution
- [ ] Test cross-file via crate import
- [ ] Test cross-file via super import
- [ ] Test cross-file via glob import
- [ ] Test name collision handling
- [ ] Test symbol not found case
- [ ] Test circular import handling

**Phase 4 Completion Criteria**:
- References resolved across files
- Calls resolved across files
- Integration tests pass
- No regression in same-file resolution

---

## Phase 5: CLI & Query Interface (Week 5)

### 5.1 New CLI Commands
- [ ] Add `imports` command
  - [ ] List all imports for a file
  - [ ] Show import graph
- [ ] Add `resolve` command
  - [ ] Resolve symbol by name
  - [ ] Show all matches
  - [ ] Show resolution path

### 5.2 Existing Command Updates
- [ ] Update `status` to show import statistics
  - [ ] Total imports
  - [ ] Imports per file
  - [ ] Circular import detection
- [ ] Update `export` to include import edges
- [ ] Add `--follow-imports` flag where appropriate

### 5.3 Query API
- [ ] Add `CodeGraph::resolve_symbol()`
- [ ] Add `CodeGraph::resolve_symbol_all_files()`
- [ ] Add `CodeGraph::get_imports()`
- [ ] Add `CodeGraph::find_references_cross_file()`

### 5.4 Documentation
- [ ] Update MANUAL.md with import features
- [ ] Update README.md with examples
- [ ] Add import resolution examples
- [ ] Document limitations

### 5.5 Tests
- [ ] Test imports CLI command
- [ ] Test resolve CLI command
- [ ] Test --follow-imports flag
- [ ] Test export with imports

**Phase 5 Completion Criteria**:
- All CLI commands work
- Documentation updated
- User can query resolution

---

## Phase 6: Validation & Performance (Week 6)

### 6.1 Precision Validation
- [ ] Create validation test suite
- [ ] Test on ripgrep codebase
- [ ] Sample 100 random references
- [ ] Compare with rust-analyzer
- [ ] Calculate precision metric
- [ ] Target: >95%

### 6.2 Performance Benchmarking
- [ ] Benchmark import extraction
- [ ] Benchmark module resolution
- [ ] Benchmark cross-file resolution
- [ ] Benchmark large codebase (1000+ files)
- [ ] Target: <100ms per query

### 6.3 Caching Implementation (if needed)
- [ ] Implement symbol cache
- [ ] Implement import cache
- [ ] Implement module cache
- [ ] Add cache invalidation

### 6.4 Real-World Testing
- [ ] Index bat repository
- [ ] Index hyper repository
- [ ] Index tokio repository
- [ ] Verify no crashes
- [ ] Verify reasonable performance

### 6.5 Documentation Updates
- [ ] Document precision results
- [ ] Document performance characteristics
- [ ] Document known limitations
- [ ] Add troubleshooting section

**Phase 6 Completion Criteria**:
- Precision >95%
- Performance <100ms
- Real-world projects work
- Documentation complete

---

## General Tasks

### Code Quality
- [ ] Maintain <300 LOC per module
- [ ] Zero compiler warnings
- [ ] All tests pass
- [ ] Documentation on public API

### Database
- [ ] Schema versioning
- [ ] Migration scripts
- [ ] Backup/restore testing

### Integration
- [ ] Test with existing Magellan installations
- [ ] Test database upgrade path
- [ ] Test with large codebases

---

## Completed

*None yet - this is the initial planning document.*

---

## Summary

- **Total Phases**: 6
- **Estimated Duration**: 6 weeks
- **Total Tasks**: ~90
- **Completed**: 0
- **In Progress**: 0
- **Pending**: 90

---

**Last Updated**: 2025-12-28
**Target Release**: Magellan 0.2.0

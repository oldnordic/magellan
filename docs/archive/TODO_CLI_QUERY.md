# Magellan CLI Query Commands - TODO

**Phase**: CLI Query Enhancement
**Last Updated**: 2025-12-28 (ALL PHASES COMPLETE)
**Current State**: ✅ Complete
**Parent Plan**: `docs/CLI_QUERY_PLAN.md`

---

## Overview

Add query commands to Magellan CLI to expose existing library query capabilities.

---

## Progress Summary

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: Core Query Command | ✅ Complete | 4/4 tasks |
| Phase 2: Find Command | ✅ Complete | 3/3 tasks |
| Phase 3: References Command | ✅ Complete | 3/3 tasks |
| Phase 4: Files Command | ✅ Complete | 2/2 tasks |
| Phase 5: Help Text Update | ✅ Complete | 1/1 tasks |
| **Total** | **✅ Complete** | **13/13 tasks** |

---

## Phase 1: Core Query Command

**Goal**: Implement `magellan query --db --file [--kind]`

### Task 1.1: Create query_cmd.rs Module
**Status**: ✅ Complete
**File**: `src/query_cmd.rs`
**Actual**: 114 LOC
**Completed**: 2025-12-28
**Description**: New module for query command logic

**Deliverables**:
- [x] `run_query()` function accepting db_path, file_path, optional kind
- [x] Parse file path argument
- [x] Parse kind argument to SymbolKind enum (case-insensitive)
- [x] Call `CodeGraph::symbols_in_file_with_kind()`
- [x] Format output as human-readable table

**Output format**:
```
/path/to/file.rs:
  Line 12:  Function     main
  Line 45:  Class        Point
  Line 78:  Function     distance
```

**Verification**:
- [x] `magellan query --db test.db --file src/main.rs` shows all symbols
- [x] `magellan query --db test.db --file src/main.rs --kind Function` shows only functions
- [x] Invalid file path shows clear error
- [x] Case-insensitive kind matching works

---

### Task 1.2: Add Query Command to main.rs
**Status**: ✅ Complete
**File**: `src/main.rs`
**Actual**: ~70 LOC changes
**Completed**: 2025-12-28
**Description**: Add Query variant to Command enum and parsing

**Deliverables**:
- [x] Add `Query { db_path: PathBuf, file_path: PathBuf, kind: Option<SymbolKind> }` to Command enum
- [x] Add parsing logic in `parse_args()` for "query" command
- [x] Parse `--db`, `--file`, `--kind` arguments
- [x] Call `run_query()` in main()
- [x] Error handling for missing required arguments

**Verification**:
- [x] `cargo check` passes
- [x] `magellan --help` shows query command

---

### Task 1.3: Add SymbolKind Parsing Helper
**Status**: ✅ Complete
**File**: `src/query_cmd.rs`
**Actual**: 36 LOC (parse_symbol_kind function)
**Completed**: 2025-12-28
**Description**: Parse string to SymbolKind (case-insensitive)

**Deliverables**:
- [x] `parse_symbol_kind()` function with case-insensitive matching
- [x] Support aliases: "fn" → Function, "trait" → Interface

**Mappings**:
- "function" / "fn" → Function
- "method" → Method
- "class" / "struct" → Class
- "interface" / "trait" → Interface
- "enum" → Enum
- "module" / "mod" → Module
- "union" → Union
- "namespace" / "ns" → Namespace
- "type" / "typealias" → TypeAlias

**Verification**:
- [x] "class" → Some(SymbolKind::Class)
- [x] "CLASS" → Some(SymbolKind::Class)
- [x] "struct" → Some(SymbolKind::Class)
- [x] "unknown" → None (invalid)

---

### Task 1.4: Query Command TDD Tests
**Status**: ✅ Complete
**File**: `tests/cli_query_tests.rs`
**Actual**: 290 LOC, 6 tests
**Completed**: 2025-12-28
**Description**: Integration tests for query command

**Tests**:
- [x] test_query_shows_all_symbols_in_file
- [x] test_query_filters_by_kind
- [x] test_query_case_insensitive_kind
- [x] test_query_nonexistent_file
- [x] test_query_empty_file
- [x] test_query_output_format

**Verification**:
- [x] All 6 tests pass

---

## Phase 2: Find Command

**Goal**: Implement `magellan find --db --name [--path]`

### Task 2.1: Create find_cmd.rs Module
**Status**: ✅ Complete
**File**: `src/find_cmd.rs`
**Actual**: 141 LOC
**Completed**: 2025-12-28
**Description**: New module for find command logic

**Deliverables**:
- [x] `run_find()` function accepting db_path, name, optional path
- [x] If path provided: call `symbol_id_by_name()` for that file
- [x] If path omitted: search all files
- [x] Display symbol location and metadata
- [x] Handle multiple matches

**Output format**:
```
Found "main":
  File:     /path/to/file.rs
  Kind:     Function
  Location: Line 12, Column 0
  Node ID:  123
```

---

### Task 2.2: Add Find Command to main.rs
**Status**: ✅ Complete
**File**: `src/main.rs`
**Actual**: ~40 LOC changes
**Completed**: 2025-12-28

**Deliverables**:
- [x] Add `Find { db_path, name, path }` variant to Command enum
- [x] Add parsing for "find" command
- [x] Parse `--db`, `--name`, `--path` arguments
- [x] Call `run_find()` in main()

---

### Task 2.3: Find Command TDD Tests
**Status**: ✅ Complete
**File**: `tests/cli_query_tests.rs`
**Actual**: 230 LOC, 4 tests
**Completed**: 2025-12-28

**Tests**:
- [x] test_find_symbol_by_name
- [x] test_find_in_specific_file
- [x] test_find_all_files
- [x] test_find_symbol_not_found

**Verification**:
- [x] All 4 tests pass

---

## Phase 3: References Command

**Goal**: Implement `magellan refs --db --name [--path] [--direction]`

### Task 3.1: Create refs_cmd.rs Module
**Status**: ✅ Complete
**File**: `src/refs_cmd.rs`
**Actual**: 69 LOC
**Completed**: 2025-12-28
**Description**: New module for refs command logic

**Deliverables**:
- [x] `run_refs()` function accepting db_path, name, optional path, direction
- [x] Parse direction: "in" → callers_of_symbol, "out" → calls_from_symbol
- [x] Default direction: "in"
- [x] Format reference list

**Output format**:
```
References TO "main":
  From: helper (Function) at /path/to/file.rs:45
  From: process (Function) at /path/to/other.rs:12
```

---

### Task 3.2: Add Refs Command to main.rs
**Status**: ✅ Complete
**File**: `src/main.rs`
**Actual**: ~50 LOC changes
**Completed**: 2025-12-28

**Deliverables**:
- [x] Add `Refs { db_path, name, path, direction }` variant
- [x] Add parsing for "refs" command
- [x] Parse `--db`, `--name`, `--path`, `--direction` arguments

---

### Task 3.3: Refs Command TDD Tests
**Status**: ✅ Complete
**File**: `tests/cli_query_tests.rs`
**Actual**: 180 LOC, 3 tests
**Completed**: 2025-12-28

**Tests**:
- [x] test_refs_incoming_calls
- [x] test_refs_outgoing_calls
- [x] test_refs_symbol_not_found

**Verification**:
- [x] All 3 tests pass

---

## Phase 4: Files Command

**Goal**: Implement `magellan files --db`

### Task 4.1: Add Files Command to main.rs
**Status**: ✅ Complete
**File**: `src/main.rs`
**Actual**: 60 LOC (inline handler)
**Completed**: 2025-12-28

**Deliverables**:
- [x] Add `Files { db_path }` variant to Command enum
- [x] Add parsing for "files" command
- [x] Call `CodeGraph::all_file_nodes()`
- [x] Display file list with count

**Output format**:
```
30 indexed files:
  /home/feanor/Projects/magellan/src/lib.rs
  /home/feanor/Projects/magellan/src/main.rs
  ...
```

---

### Task 4.2: Files Command TDD Tests
**Status**: ✅ Complete
**File**: `tests/cli_query_tests.rs`
**Actual**: 75 LOC, 2 tests
**Completed**: 2025-12-28

**Tests**:
- [x] test_files_lists_all
- [x] test_files_empty_database

**Verification**:
- [x] All 2 tests pass

---

## Phase 5: Help Text Update

**Goal**: Update usage help with new commands

### Task 5.1: Update print_usage()
**Status**: ✅ Complete
**File**: `src/main.rs`
**Actual**: ~20 LOC changes
**Completed**: 2025-12-28

**Deliverables**:
- [x] Add query command to usage
- [x] Add find command to usage
- [x] Add refs command to usage
- [x] Add files command to usage
- [x] Update command descriptions

**Verification**:
- [x] `magellan` (no args) shows complete usage
- [x] All commands documented

---

## References

| Document | Path |
|----------|------|
| Plan | `docs/CLI_QUERY_PLAN.md` |
| Contract | `docs/CONTRACT.md` |
| Main TODO | `docs/TODO.md` |
| Existing CodeGraph | `src/graph/mod.rs` (lines 114-226) |
| Query Module | `src/graph/query.rs` |
| Calls Module | `src/graph/calls.rs` |
| SymbolKind | `src/ingest/mod.rs` (lines 19-45) |

---

*Created: 2025-12-28*

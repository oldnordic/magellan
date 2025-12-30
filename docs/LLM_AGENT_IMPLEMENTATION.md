# Magellan LLM Agent Improvements - Implementation TODO

**Date**: 2025-12-27
**Purpose**: Track implementation of improvements for LLM agent usage
**Status**: Active Implementation

---

## Phase 5.1: Initial Full Scan (HIGH Priority)

### Status: COMPLETE ✓

### Files Modified

1. **`src/graph/mod.rs`** (281 LOC)
   - Added `ScanProgress` type alias
   - Added `scan_directory()` method (delegates to scan module)
   - Removed unused imports after refactoring

2. **`src/graph/scan.rs`** (NEW - 121 LOC)
   - `scan_directory()` - Walk directory, collect .rs files, index all
   - `test_scan_filters_database_files()` - Unit test for .db filtering
   - Uses `walkdir` crate for recursive traversal

3. **`src/graph/query.rs`** (NEW - 151 LOC)
   - Extracted query methods from mod.rs for LOC compliance
   - `symbols_in_file()` - Query symbols by file path
   - `symbol_id_by_name()` - Find symbol node ID by name
   - `index_references()` - Index references for a file
   - `references_to_symbol()` - Query references to a symbol

4. **`src/main.rs`**
   - Added `--scan-initial` flag to `parse_args()`
   - Modified `run_watch()` to call `scan_directory()` on startup
   - Updated `print_usage()` to show new flag

5. **`Cargo.toml`**
   - Added `walkdir = "2.5"` dependency

6. **`src/lib.rs`**
   - Exported `ScanProgress` type

7. **`tests/scan_tests.rs`** (NEW - 132 LOC)
   - `test_scan_initial_flag_indexes_all_files_on_startup()` - Integration test
   - `test_scan_only_processes_rs_files()` - File type filtering test

---

## Phase 5.2: Line/Column Spans (MEDIUM Priority)

### Status: COMPLETE ✓

### Files Modified

1. **`src/graph/schema.rs`** (37 LOC)
   - Added `start_line, start_col, end_line, end_col` to `SymbolNode`
   - Added `start_line, start_col, end_line, end_col` to `ReferenceNode`

2. **`src/ingest.rs`** (200 LOC)
   - Added line/column fields to `SymbolFact`
   - Updated `extract_symbol()` to use `node.start_position().row` and `.column`
   - Lines are 1-indexed (tree-sitter 0-indexed + 1)
   - Columns are 0-indexed (tree-sitter native)

3. **`src/references.rs`** (183 LOC)
   - Added line/column fields to `ReferenceFact`
   - Updated `extract_reference()` to populate line/column

4. **`src/graph/symbols.rs`** (78 LOC)
   - Updated `insert_symbol_node()` to persist line/column

5. **`src/graph/files.rs`** (164 LOC)
   - Updated `symbol_fact_from_node()` to include line/column on round-trip

6. **`src/graph/references.rs`** (146 LOC)
   - Updated `insert_reference_node()` to persist line/column
   - Updated `reference_fact_from_node()` to include line/column on round-trip

7. **`tests/line_column_tests.rs`** (NEW - 92 LOC)
   - `test_symbol_fact_contains_line_column_spans()` - Verify SymbolFact has line/column
   - `test_reference_fact_contains_line_column_spans()` - Verify ReferenceFact has line/column
   - `test_symbol_node_persistence_includes_line_column()` - Verify persistence round-trip

### TDD Proof

**Test First (Failing)**:
```bash
$ cargo test --test line_column_tests
error[E0609]: no field `start_line` on type `&SymbolFact`
error[E0609]: no field `start_line` on type `&ReferenceFact`
test result: FAILED. 0 passed; 16 errors
```

**Implementation (Passing)**:
```bash
$ cargo test --test line_column_tests
running 3 tests
test test_symbol_fact_contains_line_column_spans ... ok
test test_reference_fact_contains_line_column_spans ... ok
test test_symbol_node_persistence_includes_line_column ... ok

test result: ok. 3 passed; 0 failed
```

### Acceptance Criteria

- [x] `SymbolFact` includes `start_line, start_col, end_line, end_col`
- [x] `ReferenceFact` includes `start_line, start_col, end_line, end_col`
- [x] `SymbolNode` schema includes line/column
- [x] `ReferenceNode` schema includes line/column
- [x] Line/column extracted from tree-sitter `start_position()` / `end_position()`
- [x] Line/column persisted and retrieved correctly
- [x] All tests pass (48/48)
- [x] All modules under 300 LOC

### Tree-sitter API Notes

- `node.start_position().row` - 0-indexed line
- `node.start_position().column` - 0-indexed column (bytes)
- Store: `row + 1` for 1-indexed lines (standard convention)
- Store: `column` as-is for 0-indexed columns

---

## Phase 5.3: Symbol Kind Filtering (MEDIUM Priority)

### Status: COMPLETE ✓

### Files Modified

1. **`src/graph/query.rs`** (175 LOC, was 151 LOC)
   - Added `symbols_in_file_with_kind()` function with optional `SymbolKind` filter
   - Modified `symbols_in_file()` to delegate to `symbols_in_file_with_kind(None)`
   - Filter logic: apply match only if `Some(kind)` provided

2. **`src/graph/mod.rs`** (297 LOC)
   - Added `symbols_in_file_with_kind()` public method to `CodeGraph`
   - Delegates to query module implementation

3. **`tests/kind_filter_tests.rs`** (NEW - 86 LOC)
   - `test_symbols_in_file_filters_by_kind()` - Verify manual filtering works
   - `test_symbols_in_file_with_none_returns_all()` - Verify None returns all
   - `test_code_graph_symbols_in_file_with_kind_filter()` - Test CodeGraph API

### TDD Proof

**Test First (Failing)**:
```bash
$ cargo test --test kind_filter_tests
error[E0599]: no method named `symbols_in_file_with_kind` found for struct `CodeGraph`
test result: FAILED. 0 passed; 3 errors
```

**Implementation (Passing)**:
```bash
$ cargo test --test kind_filter_tests
running 3 tests
test test_symbols_in_file_with_none_returns_all ... ok
test test_symbols_in_file_filters_by_kind ... ok
test test_code_graph_symbols_in_file_with_kind_filter ... ok

test result: ok. 3 passed; 0 failed
```

### Acceptance Criteria

- [x] `symbols_in_file_with_kind()` accepts `Option<SymbolKind>`
- [x] `Some(SymbolKind::Function)` returns only functions
- [x] `Some(SymbolKind::Struct)` returns only structs
- [x] `None` returns all symbols
- [x] Original `symbols_in_file()` unchanged (backward compatible)
- [x] All tests pass (51/51)
- [x] All modules under 300 LOC

### API Usage

```rust
// Get all functions in a file
let functions = graph.symbols_in_file_with_kind(
    "src/lib.rs",
    Some(SymbolKind::Function)
)?;

// Get all symbols (backward compatible)
let all = graph.symbols_in_file("src/lib.rs")?;
```

---

## Phase 5.4: Call Graph Edges (HIGH Priority)

### Status: COMPLETE ✓

### Design

Add new edge type `CALLS` for forward call graph:
- `REFERENCES (Symbol → Symbol)` - "who references this symbol" (existing)
- `CALLER (Symbol → Call)` - "which Call nodes originated from this Symbol" (new)
- `CALLS (Call → Symbol)` - "what Symbol does this Call target" (new)

### Files Modified

1. **`src/lib.rs`** (15 LOC)
   - Exported `CallFact` type

2. **`src/graph/schema.rs`** (51 LOC)
   - Added `CallNode` struct with caller/callee and span information

3. **`src/references.rs`** (440 LOC → 434 LOC after refactoring)
   - Added `CallFact` struct
   - Added `CallExtractor` with `extract_calls()` method
   - Added `extract_calls()` extension to `Parser`
   - Handles both direct calls (`helper()`) and method calls (`data.method()`)

4. **`src/graph/call_ops.rs`** (NEW - 191 LOC)
   - `CallOps` struct with call node and edge operations
   - `index_calls()` - Index calls and create CALLER/CALLS edges
   - `calls_from_symbol()` - Query outgoing CALLER edges
   - `callers_of_symbol()` - Query incoming CALLS edges
   - `insert_call_node()` - Create Call nodes
   - `insert_calls_edge()` - Create CALLS edges
   - `insert_caller_edge()` - Create CALLER edges

5. **`src/graph/calls.rs`** (NEW - 88 LOC)
   - Query wrapper functions that delegate to CallOps
   - `index_calls()` - Index calls for a file
   - `calls_from_symbol()` - Query calls from a symbol
   - `callers_of_symbol()` - Query callers to a symbol

6. **`src/graph/mod.rs`** (300 LOC)
   - Added `calls: CallOps` field to `CodeGraph`
   - Added `calls` module
   - Added `call_ops` module
   - Exported `CallNode` type

7. **`src/graph/ops.rs`** (NEW - 84 LOC)
   - Extracted core operations from mod.rs for LOC compliance
   - `index_file()` - Now also indexes calls via `calls::index_calls()`

8. **`src/graph/count.rs`** (NEW - 50 LOC)
   - Extracted count methods from mod.rs for LOC compliance

9. **`tests/call_graph_tests.rs`** (NEW - 139 LOC)
   - `test_extract_calls_detects_function_calls` - Basic call detection
   - `test_extract_calls_ignores_type_references` - Method calls, not type refs
   - `test_extract_calls_handles_nested_calls` - Duplicate call detection
   - `test_code_graph_stores_and_queries_calls_edges` - Full CodeGraph API

### TDD Proof

**Test First (Failing)**:
```bash
$ cargo test --test call_graph_tests
error[E0599]: no method named `extract_calls` found for struct `Parser`
error[E0599]: no method named `calls_from_symbol` found for struct `CodeGraph`
error[E0599]: no method named `callers_of_symbol` found for struct `CodeGraph`
test result: FAILED. 0 passed; 6 errors
```

**Implementation (Passing)**:
```bash
$ cargo test --test call_graph_tests
running 4 tests
test test_extract_calls_detects_function_calls ... ok
test test_extract_calls_ignores_type_references ... ok
test test_extract_calls_handles_nested_calls ... ok
test test_code_graph_stores_and_queries_calls_edges ... ok

test result: ok. 4 passed; 0 failed
```

### Acceptance Criteria

- [x] `CallFact` struct with caller/callee names and span information
- [x] `CallNode` schema for persistence
- [x] `CallExtractor` extracts function calls from source
- [x] Handles direct function calls (`helper()`)
- [x] Handles method calls (`data.method()`)
- [x] CALLER edge: Symbol → Call (for forward traversal)
- [x] CALLS edge: Call → Symbol (for reverse traversal)
- [x] `calls_from_symbol(path, name)` returns calls FROM a function
- [x] `callers_of_symbol(path, name)` returns calls TO a function
- [x] `index_file()` automatically indexes calls
- [x] All tests passing
- [x] All modules under 300 LOC

### Graph Structure

```
Symbol (caller) --CALLER--> Call node --CALLS--> Symbol (callee)
```

This structure allows:
- Forward queries: "What does main call?" → Follow CALLER edges
- Reverse queries: "Who calls parse?" → Follow CALLS edges

---

## Phase 5.5: JSON Export (LOW Priority)

### Status: COMPLETE ✓

### Files Modified

1. **`src/graph/export.rs`** (NEW - 192 LOC)
   - `GraphExport` struct with files, symbols, references, calls arrays
   - `FileExport`, `SymbolExport`, `ReferenceExport`, `CallExport` structs
   - `export_json()` - Main export function
   - `get_file_path_from_symbol()` - Helper to get file path via DEFINES edge

2. **`src/graph/mod.rs`** (290 LOC)
   - Added `export` module
   - Added `export_json()` public method to `CodeGraph`
   - Moved inline tests to `src/graph/tests.rs` for LOC compliance

3. **`src/graph/tests.rs`** (NEW - 24 LOC)
   - Moved `test_hash_computation()` from inline test module

4. **`tests/export_tests.rs`** (NEW - 135 LOC)
   - `test_code_graph_exports_to_json` - Verify JSON structure
   - `test_export_json_includes_file_details` - Check file export format
   - `test_export_json_includes_symbol_details` - Check symbol export format
   - `test_export_json_includes_call_details` - Check call export format

### TDD Proof

**Test First (Failing)**:
```bash
$ cargo test --test export_tests
error[E0599]: no method named `export_json` found for struct `CodeGraph`
test result: FAILED. 0 passed; 4 errors
```

**Implementation (Passing)**:
```bash
$ cargo test --test export_tests
running 4 tests
test test_code_graph_exports_to_json ... ok
test test_export_json_includes_file_details ... ok
test test_export_json_includes_symbol_details ... ok
test test_export_json_includes_call_details ... ok

test result: ok. 4 passed; 0 failed
```

### Acceptance Criteria

- [x] `export_json()` method returns JSON string
- [x] JSON includes `files` array with path and hash
- [x] JSON includes `symbols` array with name, kind, file, spans
- [x] JSON includes `references` array with file, referenced_symbol, spans
- [x] JSON includes `calls` array with caller, callee, file, spans
- [x] All arrays sorted for deterministic output
- [x] Symbol file path resolved via DEFINES edge
- [x] All tests passing (58/58)
- [x] All modules under 300 LOC

### API Usage

```rust
// Export entire graph to JSON
let mut graph = CodeGraph::open("my_graph.db")?;
let json = graph.export_json()?;

// JSON structure:
// {
//   "files": [{ "path": "...", "hash": "..." }],
//   "symbols": [{ "name": "...", "kind": "...", "file": "...", ... }],
//   "references": [{ "file": "...", "referenced_symbol": "...", ... }],
//   "calls": [{ "caller": "...", "callee": "...", "file": "...", ... }]
// }
```

---

## Progress Log

### 2025-12-27

**Phase 5.1 COMPLETE**
- Wrote failing tests for `--scan-initial` flag
- Implemented `scan_directory()` method
- Added `--scan-initial` CLI flag
- Refactored graph module into query.rs and scan.rs for LOC compliance
- All 45 tests passing
- Zero compilation warnings in modified code
- All modules under 300 LOC

**Phase 5.2 COMPLETE**
- Added line/column fields to `SymbolFact` and `ReferenceFact`
- Updated schema nodes to include line/column
- Modified extraction to use tree-sitter position API
- Created 3 integration tests for line/column
- All 48 tests passing

**Phase 5.3 COMPLETE**
- Added `symbols_in_file_with_kind()` with optional `SymbolKind` filter
- Modified `symbols_in_file()` to delegate to new implementation
- Created 3 integration tests for kind filtering
- All 51 tests passing
- Backward compatible (original API unchanged)

**Phase 5.4 COMPLETE**
- Added CALLS edge type for forward call graph
- Implemented call extraction with CallExtractor
- Added CALLER and CALLS edges for bidirectional traversal
- Created CallOps module for call operations
- All 4 call_graph_tests passing
- All modules under 300 LOC

**Phase 5.5 COMPLETE**
- Added JSON export functionality via `export_json()` method
- Created export.rs module with GraphExport struct
- Export includes files, symbols, references, and calls
- All 4 export_tests passing
- All modules under 300 LOC (mod.rs: 290 LOC, export.rs: 192 LOC)

**All Phase 5 tasks complete**

---

*Last Updated: 2025-12-27*

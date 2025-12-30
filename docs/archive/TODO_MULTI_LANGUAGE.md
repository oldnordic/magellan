# Magellan Multi-Language Support TODO

**Phase**: Multi-Language Implementation
**Last Updated**: 2025-12-28 (Phase 6 Complete)
**Current State**: ✅ Complete
**Parent Plan**: `MULTI_LANGUAGE_SUPPORT_PLAN.md`

---

## Overview

Extend Magellan from Rust-only to match Splice's language support:
- Python (`.py`)
- C (`.c`, `.h`)
- C++ (`.cpp`, `.hpp`, `.cc`, `.cxx`)
- Java (`.java`)
- JavaScript (`.js`, `.mjs`, `.cjs`)
- TypeScript (`.ts`, `.tsx`)

**Contract Change Required**: `docs/CONTRACT.md` line 103 currently specifies `"Supported language: Rust (only)"`

---

## Progress Summary

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: Infrastructure | ✅ Complete | 5/5 tasks |
| Phase 2: Python Support | ✅ Complete | 3/3 tasks |
| Phase 3: C/C++ Support | ✅ Complete | 5/5 tasks |
| Phase 4: Java Support | ✅ Complete | 3/3 tasks |
| Phase 5: JS/TS Support | ✅ Complete | 5/5 tasks |
| Phase 6: Integration | ✅ Complete | 5/5 tasks |
| **Total** | **✅ Complete** | **26/26 tasks** |

---

## Phase 1: Infrastructure (Foundation)

**Goal**: Set up language detection and generic SymbolKind

### Task 1.1: Create Language Detection Module
**Status**: ✅ Complete
**File**: `src/ingest/detect.rs`
**Actual**: 194 LOC (includes tests)
**Completed**: 2025-12-28

**Deliverables**:
- [x] `Language` enum with 7 variants (Rust, Python, C, Cpp, Java, JavaScript, TypeScript)
- [x] `detect_language(path: &Path) -> Option<Language>` function
- [x] Extension → Language mapping table (15 extensions total)
- [x] Returns `None` for unknown extensions (no guessing)

**Verification**:
- [x] Test: `file.rs` → `Some(Language::Rust)`
- [x] Test: `file.py` → `Some(Language::Python)`
- [x] Test: `file.cpp` → `Some(Language::Cpp)`
- [x] Test: `file.unknown` → `None`
- [x] Test: Case sensitivity (`.RS` → `None`)

---

### Task 1.2: Export Detection Module
**Status**: ✅ Complete
**File**: `src/ingest/mod.rs`
**Actual**: 5 LOC
**Completed**: 2025-12-28

**Deliverables**:
- [x] `pub mod detect;` added to `src/ingest/mod.rs`
- [x] `pub use detect::{Language, detect_language};` re-export
- [x] Also re-exported from `src/lib.rs` for public API

**Verification**:
- [x] `cargo check` passes
- [x] `use magellan::Language;` compiles

---

### Task 1.3: Language Detection TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/detect.rs` (inline tests)
**Actual**: 15 tests in #[cfg(test)] module
**Completed**: 2025-12-28

**Deliverables**:
- [x] Test module created with 15 test cases
- [x] All extensions tested (rs, py, c, h, cpp, hpp, cc, cxx, java, js, mjs, cjs, ts, tsx)
- [x] Unknown extensions return `None`
- [x] Path handling works (directories, absolute paths)

**Verification**:
- [x] `cargo test detect` passes (15 tests)
- [x] All tests passing

---

### Task 1.4: Refactor SymbolKind to Generic
**Status**: ✅ Complete
**File**: `src/ingest/mod.rs`, `src/graph/files.rs`
**Actual**: ~50 LOC changes across 5 files
**Completed**: 2025-12-28
**Description**: Replace Rust-specific SymbolKind with language-agnostic enum

**Current SymbolKind**:
```rust
pub enum SymbolKind {
    Function,
    Struct,      // Rust-specific
    Enum,
    Trait,       // Rust-specific
    Method,
    Module,      // Rust-specific
    Unknown,
}
```

**New SymbolKind**:
```rust
pub enum SymbolKind {
    Function,
    Method,
    Class,       // Covers: Rust struct, Python class, Java class, JS class
    Interface,   // Covers: Rust trait, Java interface, TS interface
    Enum,
    Module,      // Covers: Rust mod, Python module, Java package, JS ES module
    Union,       // C/C++ union
    Namespace,   // C++ namespace, TS namespace
    TypeAlias,   // TypeScript type, Rust type alias
    Unknown,
}
```

**Deliverables**:
- [x] `SymbolKind` enum refactored to generic version
- [x] Rust extraction updated to use new mapping:
  - `struct_item` → `Class` (was `Struct`)
  - `trait_item` → `Interface` (was `Trait`)
- [x] Existing tests updated to match new mapping
- [x] Deserialization mapping updated in `src/graph/files.rs`

**Verification**:
- [x] All existing Rust extraction tests pass (71 tests)
- [x] Database migration not required (stored as strings in JSON)

---

### Task 1.5: Add tree-sitter Parser Dependencies
**Status**: ✅ Complete
**File**: `Cargo.toml`
**Completed**: 2025-12-28
**Description**: Add tree-sitter parser packages for all languages

**Deliverables**:
- [x] `tree-sitter-python = "0.21"`
- [x] `tree-sitter-c = "0.21"`
- [x] `tree-sitter-cpp = "0.21"`
- [x] `tree-sitter-java = "0.21"`
- [x] `tree-sitter-javascript = "0.21"`
- [x] `tree-sitter-typescript = "0.21"`

**Verification**:
- [x] `cargo build` succeeds
- [x] All parsers compile without errors

---

## Phase 2: Python Support

**Goal**: First non-Rust language, validates architecture

### Task 2.1: Python Symbol Extraction
**Status**: ✅ Complete
**File**: `src/ingest/python.rs`
**Actual**: 290 LOC (includes 12 tests)
**Completed**: 2025-12-28
**Description**: Extract Python symbols using tree-sitter-python

**Deliverables**:
- [x] `PythonParser` struct with tree-sitter-python parser
- [x] `extract_symbols(path, source) → Vec<SymbolFact>` method
- [x] Python node mappings:
  - `function_definition` → `SymbolKind::Function`
  - `class_definition` → `SymbolKind::Class`
  - `decorated_definition` → recurse to child
  - `async` detection → functions are extracted (async in node type)
- [x] Name extraction from `identifier` nodes

**Verification**:
- [x] `def foo(): pass` → Function("foo")
- [x] `class Bar:` → Class("Bar")
- [x] `async def baz():` → Function("baz")

---

### Task 2.2: Python TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/python.rs` (inline tests)
**Actual**: 12 tests in #[cfg(test)] module
**Completed**: 2025-12-28
**Description**: TDD tests for Python symbol extraction

**Deliverables**:
- [x] Test: Empty file → no symbols
- [x] Test: Syntax error → graceful handling
- [x] Test: Simple function → extracted
- [x] Test: Simple class → extracted
- [x] Test: Multiple symbols → all extracted
- [x] Test: Async function → detected
- [x] Test: Decorated function → extracted
- [x] Test: Nested class → flat extraction
- [x] Test: Method in class → Function kind (flat extraction)
- [x] Test: Byte spans within bounds
- [x] Test: Line/column positions

**Verification**:
- [x] `cargo test python` passes (12 tests)
- [x] All tests passing

---

### Task 2.3: Add Python to Language Dispatch
**Status**: ✅ Complete
**File**: `src/graph/ops.rs`
**Actual**: ~25 LOC changes
**Completed**: 2025-12-28
**Description**: Add Python to Parser's language dispatch

**Deliverables**:
- [x] `Language::Python` case in dispatch table
- [x] Calls `PythonParser::extract_symbols()`
- [x] Returns `Vec<SymbolFact>`
- [x] Scanner updated to use `detect_language()` instead of hardcoded `.rs`

**Verification**:
- [x] Integration test: `.py` file scanned and indexed
- [x] All 83 tests passing (71 + 12 new)

---

## Phase 3: C/C++ Support

### Task 3.1: C Symbol Extraction
**Status**: ✅ Complete
**File**: `src/ingest/c.rs`
**Actual**: 205 LOC (includes 9 tests)
**Completed**: 2025-12-28
**Description**: Extract C symbols using tree-sitter-c

**Deliverables**:
- [x] `CParser` struct with tree-sitter-c parser
- [x] Node mappings:
  - `function_definition` → `SymbolKind::Function`
  - `struct_specifier` → `SymbolKind::Class`
  - `union_specifier` → `SymbolKind::Union`
  - `enum_specifier` → `SymbolKind::Enum`
- [x] Recursive name extraction (handles nested function_declarator nodes)

**Verification**:
- [x] `struct Foo { int x; };` → Class("Foo")
- [x] `union Bar { int x; float y; };` → Union("Bar")
- [x] `enum Baz { A, B };` → Enum("Baz")

---

### Task 3.2: C TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/c.rs` (inline tests)
**Actual**: 9 tests in #[cfg(test)] module
**Completed**: 2025-12-28

**Tests**:
- [x] test_extract_simple_function
- [x] test_extract_struct
- [x] test_extract_enum
- [x] test_extract_union
- [x] test_extract_multiple_symbols
- [x] test_empty_file
- [x] test_syntax_error_returns_empty
- [x] test_byte_spans_within_bounds
- [x] test_line_column_positions

**Verification**:
- [x] `cargo test ingest::c` passes (9 tests)

---

### Task 3.3: C++ Symbol Extraction
**Status**: ✅ Complete
**File**: `src/ingest/cpp.rs`
**Actual**: 360 LOC (includes 12 tests)
**Completed**: 2025-12-28
**Description**: Extract C++ symbols using tree-sitter-cpp

**Deliverables**:
- [x] `CppParser` struct with tree-sitter-cpp parser
- [x] Node mappings:
  - `function_definition` → `SymbolKind::Function`
  - `class_specifier` → `SymbolKind::Class`
  - `struct_specifier` → `SymbolKind::Class`
  - `namespace_definition` → `SymbolKind::Namespace`
  - `template_declaration` → skipped (walk recurses to child)
- [x] Handles `namespace_identifier` (different from `type_identifier`)

**Verification**:
- [x] `class Foo {};` → Class("Foo")
- [x] `namespace NS {}` → Namespace("NS")
- [x] `template<typename T> class Bar {};` → Class("Bar")

---

### Task 3.4: C++ TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/cpp.rs` (inline tests)
**Actual**: 12 tests in #[cfg(test)] module
**Completed**: 2025-12-28

**Tests**:
- [x] test_extract_simple_function
- [x] test_extract_class
- [x] test_extract_struct
- [x] test_extract_namespace
- [x] test_extract_template_class
- [x] test_extract_nested_namespace
- [x] test_extract_multiple_symbols
- [x] test_empty_file
- [x] test_syntax_error_returns_empty
- [x] test_byte_spans_within_bounds
- [x] test_line_column_positions
- [x] test_template_function

**Verification**:
- [x] `cargo test ingest::cpp` passes (12 tests)

---

### Task 3.5: Add C/C++ to Language Dispatch
**Status**: ✅ Complete
**File**: `src/graph/ops.rs`
**Actual**: ~20 LOC changes
**Completed**: 2025-12-28

**Deliverables**:
- [x] `CParser` import added
- [x] `CppParser` import added
- [x] `Language::C` case in dispatch table
- [x] `Language::Cpp` case in dispatch table

**Verification**:
- [x] `.c`, `.h`, `.cpp`, `.hpp`, `.cc`, `.cxx` files indexed via language detection
- [x] All 119 tests passing (98 + 9 new C + 12 new C++)

---

## Phase 4: Java Support

### Task 4.1: Java Symbol Extraction
**Status**: ✅ Complete
**File**: `src/ingest/java.rs`
**Actual**: 315 LOC (includes 11 tests)
**Completed**: 2025-12-28
**Description**: Extract Java symbols using tree-sitter-java

**Deliverables**:
- [x] `JavaParser` struct with tree-sitter-java parser
- [x] Node mappings:
  - `method_declaration` → `SymbolKind::Method`
  - `class_declaration` → `SymbolKind::Class`
  - `interface_declaration` → `SymbolKind::Interface`
  - `enum_declaration` → `SymbolKind::Enum`
  - `package_declaration` → `SymbolKind::Module`
- [x] Handles `scoped_identifier` for package names (e.g., com.example)

**Verification**:
- [x] `class Test {}` → Class("Test")
- [x] `interface Foo {}` → Interface("Foo")
- [x] `package com.example;` → Module("com.example")

---

### Task 4.2: Java TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/java.rs` (inline tests)
**Actual**: 11 tests in #[cfg(test)] module
**Completed**: 2025-12-28

**Tests**:
- [x] test_extract_class
- [x] test_extract_interface
- [x] test_extract_enum
- [x] test_extract_method
- [x] test_extract_package
- [x] test_extract_multiple_symbols
- [x] test_empty_file
- [x] test_syntax_error_returns_empty
- [x] test_byte_spans_within_bounds
- [x] test_line_column_positions
- [x] test_nested_class

**Verification**:
- [x] `cargo test ingest::java` passes (11 tests)

---

### Task 4.3: Add Java to Language Dispatch
**Status**: ✅ Complete
**File**: `src/graph/ops.rs`
**Actual**: ~10 LOC changes
**Completed**: 2025-12-28

**Deliverables**:
- [x] `JavaParser` import added
- [x] `Language::Java` case in dispatch table

**Verification**:
- [x] `.java` files indexed via language detection
- [x] All 130 tests passing (119 + 11 new Java)

---

## Phase 5: JavaScript/TypeScript Support

### Task 5.1: JavaScript Symbol Extraction
**Status**: ✅ Complete
**File**: `src/ingest/javascript.rs`
**Actual**: 300 LOC (includes 11 tests)
**Completed**: 2025-12-28
**Description**: Extract JavaScript symbols using tree-sitter-javascript

**Deliverables**:
- [x] `JavaScriptParser` struct with tree-sitter-javascript parser
- [x] Node mappings:
  - `function_declaration` → `SymbolKind::Function`
  - `class_declaration` → `SymbolKind::Class`
  - `method_definition` → `SymbolKind::Method`
- [x] Skip `arrow_function` (anonymous - not extracted)
- [x] Handle `export_statement` (skipped, walk recurses to child)

**Verification**:
- [x] `function foo() {}` → Function("foo")
- [x] `class Bar {}` → Class("Bar")
- [x] `export function baz() {}` → Function("baz")

---

### Task 5.2: JavaScript TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/javascript.rs` (inline tests)
**Actual**: 11 tests in #[cfg(test)] module
**Completed**: 2025-12-28

**Tests**:
- [x] test_extract_function
- [x] test_extract_class
- [x] test_extract_method
- [x] test_extract_export_function
- [x] test_extract_export_class
- [x] test_extract_export_default
- [x] test_extract_multiple_symbols
- [x] test_empty_file
- [x] test_syntax_error_returns_empty
- [x] test_byte_spans_within_bounds
- [x] test_line_column_positions

**Verification**:
- [x] `cargo test ingest::javascript` passes (11 tests)

---

### Task 5.3: TypeScript Symbol Extraction
**Status**: ✅ Complete
**File**: `src/ingest/typescript.rs`
**Actual**: 380 LOC (includes 14 tests)
**Completed**: 2025-12-28
**Description**: Extract TypeScript symbols using tree-sitter-typescript

**Deliverables**:
- [x] `TypeScriptParser` struct with TypeScript parser
- [x] Node mappings (all JS nodes +):
  - `interface_declaration` → `SymbolKind::Interface`
  - `type_alias_declaration` → `SymbolKind::TypeAlias`
  - `enum_declaration` → `SymbolKind::Enum`
  - `internal_module` → `SymbolKind::Namespace` (TS `namespace`)
- [x] Handle `export_statement` (skipped, walk recurses to child)
- [x] Handles `type_identifier` for type names

**Verification**:
- [x] `interface Foo {}` → Interface("Foo")
- [x] `type Bar = string;` → TypeAlias("Bar")
- [x] `namespace NS {}` → Namespace("NS")

---

### Task 5.4: TypeScript TDD Tests
**Status**: ✅ Complete
**File**: `src/ingest/typescript.rs` (inline tests)
**Actual**: 14 tests in #[cfg(test)] module
**Completed**: 2025-12-28

**Tests**:
- [x] test_extract_function
- [x] test_extract_class
- [x] test_extract_interface
- [x] test_extract_type_alias
- [x] test_extract_enum
- [x] test_extract_namespace
- [x] test_extract_generic_class
- [x] test_extract_export_interface
- [x] test_extract_export_type
- [x] test_extract_multiple_symbols
- [x] test_empty_file
- [x] test_syntax_error_returns_empty
- [x] test_byte_spans_within_bounds
- [x] test_line_column_positions

**Verification**:
- [x] `cargo test ingest::typescript` passes (14 tests)

---

### Task 5.5: Add JS/TS to Language Dispatch
**Status**: ✅ Complete
**File**: `src/graph/ops.rs`
**Actual**: ~20 LOC changes
**Completed**: 2025-12-28

**Deliverables**:
- [x] `JavaScriptParser` import added
- [x] `TypeScriptParser` import added
- [x] `Language::JavaScript` case in dispatch table
- [x] `Language::TypeScript` case in dispatch table

**Verification**:
- [x] `.js`, `.mjs`, `.cjs`, `.ts`, `.tsx` files indexed via language detection
- [x] All 155 tests passing (130 + 11 new JS + 14 new TS)

---

## Phase 6: Integration & Cleanup

### Task 6.1: Update Scanner for Multi-Language
**Status**: ✅ Complete
**File**: `src/graph/scan.rs`
**Actual**: ~15 LOC changes
**Completed**: 2025-12-28
**Description**: Replace hardcoded `.rs` filter

**Implementation**:
- Replaced hardcoded `.rs` check with `detect_language(path).is_some()`
- Updated documentation to reference "supported source files" instead of "Rust files"

**Verification**:
- [x] Scanner finds `.py` files (tested)
- [x] Scanner skips unknown files (`.txt`, `.md`, `.db`)

---

### Task 6.2: Update Watcher for Multi-Language
**Status**: ✅ Complete
**File**: `src/watch_cmd.rs`
**Actual**: ~10 LOC changes
**Completed**: 2025-12-28
**Description**: Replace hardcoded `.rs` filter

**Current**:
```rust
if !path_str.ends_with(".rs") {
    continue;
}
```

**New**:
```rust
if detect_language(&event.path).is_none() {
    continue;
}
```

**Verification**:
- [x] Watcher processes all supported file types
- [x] Watcher ignores unsupported files

---

### Task 6.3: Update CLI Help Text
**Status**: ✅ Complete
**File**: `src/main.rs`
**Actual**: 2 LOC changes
**Completed**: 2025-12-28
**Description**: Remove "for Rust projects" from help

**Current**:
```rust
eprintln!("Magellan - Codebase mapping tool for Rust projects");
```

**New**:
```rust
eprintln!("Magellan - Multi-language codebase mapping tool");
```

**Verification**:
- [x] `magellan --help` shows updated text

---

### Task 6.4: Update CONTRACT.md
**Status**: ✅ Complete
**File**: `docs/CONTRACT.md`
**Actual**: ~30 LOC changes
**Completed**: 2025-12-28
**Description**: Update supported languages list

**Current**:
```
**Supported language:** Rust (only)
```

**New**:
```
**Supported languages:** Rust, Python, C, C++, Java, JavaScript, TypeScript
```

**Verification**:
- [x] CONTRACT.md reflects new scope
- [x] All 7 languages listed

---

### Task 6.5: Integration Test
**Status**: ✅ Complete
**File**: `tests/multi_language_integration_tests.rs`
**Actual**: 350 LOC (includes 4 tests)
**Completed**: 2025-12-28
**Description**: End-to-end test with mixed codebase

**Deliverables**:
- [x] Create temporary codebase with mixed languages
- [x] Run `CodeGraph::scan_directory()`
- [x] Verify all files indexed
- [x] Verify correct symbol counts per file
- [x] Verify cross-language queries work

**Test Structure**:
```
/tmp/test_codebase/
  main.rs         (3 symbols)
  utils.py        (2 symbols)
  config.java     (2 symbols)
  logic.js        (2 symbols)
  types.ts        (2 symbols)
```

**Verification**:
- [x] 7 files indexed (includes .cpp, .c)
- [x] All symbol counts verified per language
- [x] 4 integration tests passing

---

## References

| Document | Path |
|----------|------|
| Plan | `docs/MULTI_LANGUAGE_SUPPORT_PLAN.md` |
| Contract | `docs/CONTRACT.md` |
| Main TODO | `docs/TODO.md` |
| Splice reference | `/home/feanor/Projects/splice/docs/LANGUAGE_DETECTION_PLAN.md` |

---

*Created: 2025-12-28*

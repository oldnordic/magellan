# Multi-Language Reference & Call Extraction Plan

**Date**: 2025-12-30
**Status**: IN PROGRESS
**Purpose**: Add reference extraction and call graph support for all 7 supported languages

---

## Problem Statement

Currently Magellan supports **symbol extraction** for 7 languages (Rust, Python, C, C++, Java, JavaScript, TypeScript), but:

1. **Reference extraction** is hardcoded to Rust only (`src/graph/references.rs`)
2. **Call graph indexing** is hardcoded to Rust only (`src/graph/call_ops.rs`, `src/graph/ops.rs:95-98`)

This breaks rename refactoring and call graph analysis for non-Rust languages.

---

## Current State

### Symbol Extraction (✅ Complete)
| Language | Parser | Symbol Extraction | Tests |
|----------|--------|-------------------|-------|
| Rust | `Parser` / `tree_sitter_rust` | ✅ | ✅ |
| Python | `PythonParser` / `tree_sitter_python` | ✅ | ✅ |
| C | `CParser` / `tree_sitter_c` | ✅ | ✅ |
| C++ | `CppParser` / `tree_sitter_cpp` | ✅ | ✅ |
| Java | `JavaParser` / `tree_sitter_java` | ✅ | ✅ |
| JavaScript | `JavaScriptParser` / `tree_sitter_javascript` | ✅ | ✅ |
| TypeScript | `TypeScriptParser` / `tree_sitter_typescript` | ✅ | ✅ |

### Reference Extraction (❌ Rust-only)
| Language | Reference Extraction | Status |
|----------|---------------------|--------|
| Rust | `Parser::extract_references()` | ✅ |
| Python | `PythonParser::extract_references()` | ❌ Missing |
| C | `CParser::extract_references()` | ❌ Missing |
| C++ | `CppParser::extract_references()` | ❌ Missing |
| Java | `JavaParser::extract_references()` | ❌ Missing |
| JavaScript | `JavaScriptParser::extract_references()` | ❌ Missing |
| TypeScript | `TypeScriptParser::extract_references()` | ❌ Missing |

### Call Graph Extraction (❌ Rust-only)
| Language | Call Extraction | Status |
|----------|-----------------|--------|
| Rust | `Parser::extract_calls()` | ✅ |
| Python | `PythonParser::extract_calls()` | ❌ Missing |
| C | `CParser::extract_calls()` | ❌ Missing |
| C++ | `CppParser::extract_calls()` | ❌ Missing |
| Java | `JavaParser::extract_calls()` | ❌ Missing |
| JavaScript | `JavaScriptParser::extract_calls()` | ❌ Missing |
| TypeScript | `TypeScriptParser::extract_calls()` | ❌ Missing |

---

## Architecture

### Current (Broken) Flow

```
index_file()
  ├─> detect_language() ────────✅ Works (all 7 languages)
  ├─> extract_symbols() ────────✅ Works (language dispatch in ops.rs)
  ├─> index_references() ───────❌ Uses Parser::new() (Rust only!)
  └─> index_calls() ────────────❌ Explicitly Rust-only
```

### Target Flow

```
index_file()
  ├─> detect_language() ────────✅ All 7 languages
  ├─> extract_symbols() ────────✅ All 7 languages
  ├─> index_references() ────────✅ All 7 languages (NEW)
  └─> index_calls() ────────────✅ All 7 languages (NEW)

index_references()
  └─> match language {
          Rust => RustParser::extract_references()
          Python => PythonParser::extract_references()
          C => CParser::extract_references()
          ...
      }

index_calls()
  └─> match language {
          Rust => RustParser::extract_calls()
          Python => PythonParser::extract_calls()
          C => CParser::extract_calls()
          ...
      }
```

---

## Implementation Strategy

### Phase 1: Add Language Dispatch to Reference Extraction

**File**: `src/graph/references.rs`

**Changes**:
1. Import language-specific parsers
2. Replace `Parser::new()` with language dispatch
3. Move Rust reference extraction logic to use `RustParser` pattern

### Phase 2: Add Language Dispatch to Call Extraction

**Files**:
- `src/graph/call_ops.rs` - Update to use language dispatch
- `src/graph/ops.rs` - Remove explicit Rust-only check

### Phase 3: Implement Language-Specific Reference Extractors

For each language, add `extract_references()` method to its parser:

| Language | File | Reference Nodes to Track |
|----------|------|--------------------------|
| Python | `src/ingest/python.rs` | identifier references in expressions/function calls |
| C | `src/ingest/c.rs` | identifier references, function calls |
| C++ | `src/ingest/cpp.rs` | identifier references, member access, function calls |
| Java | `src/ingest/java.rs` | identifier references, method invocations |
| JavaScript | `src/ingest/javascript.rs` | identifier references, function calls, member expressions |
| TypeScript | `src/ingest/typescript.rs` | identifier references, function calls, property access |

### Phase 4: Implement Language-Specific Call Extractors

For each language, add `extract_calls()` method to its parser.

---

## File Modifications

| File | Changes |
|------|---------|
| `src/graph/references.rs` | Add language dispatch for `index_references()` |
| `src/graph/call_ops.rs` | Add language dispatch for `index_calls()` |
| `src/graph/ops.rs` | Remove Rust-only check, enable calls for all languages |
| `src/ingest/python.rs` | Add `extract_references()` and `extract_calls()` |
| `src/ingest/c.rs` | Add `extract_references()` and `extract_calls()` |
| `src/ingest/cpp.rs` | Add `extract_references()` and `extract_calls()` |
| `src/ingest/java.rs` | Add `extract_references()` and `extract_calls()` |
| `src/ingest/javascript.rs` | Add `extract_references()` and `extract_calls()` |
| `src/ingest/typescript.rs` | Add `extract_references()` and `extract_calls()` |

---

## Testing Strategy

For each language:
1. Create test file with known symbol references
2. Index the file
3. Query for references to each symbol
4. Verify byte offsets are correct
5. Verify reference count is correct

---

## Success Criteria

1. ✅ All 7 languages have reference extraction
2. ✅ All 7 languages have call graph extraction
3. ✅ Rename refactoring works for Python, C, C++, Java, JS, TS
4. ✅ Call graph queries work for all languages
5. ✅ All new code has tests
6. ✅ Zero new compiler warnings

---

*Created: 2025-12-30*

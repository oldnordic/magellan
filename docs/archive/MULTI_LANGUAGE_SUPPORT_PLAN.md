# Magellan Multi-Language Support - Implementation Plan

**Date**: 2025-12-28
**Status**: 🚧 PLANNED
**Purpose**: Extend Magellan from Rust-only to match Splice's language support

---

## Goal

Add multi-language symbol extraction and indexing to Magellan, matching the language support already implemented in Splice.

**Philosophy**: "Truth lives in execution" — use real tree-sitter parsers for each language, no heuristics, no guessing.

**Scope Change Required**: CONTRACT.md currently specifies "Supported language: Rust (only)" — this plan requires explicit scope approval.

---

## Current State (Rust-Only)

### Hard-coded Filters Found

| File | Line | Hard-code |
|------|------|-----------|
| `src/ingest.rs` | 61 | `tree_sitter_rust::language()` |
| `src/watch_cmd.rs` | 68 | `if !path_str.ends_with(".rs")` |
| `src/graph/scan.rs` | 46 | `if path.extension() == Some(OsStr::new("rs"))` |
| `src/main.rs` | 14 | `"Codebase mapping tool **for Rust projects**"` |

### Current SymbolKind (Rust-Specific)

```rust
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,        // Rust-specific
    Method,
    Module,       // Rust-specific
    Unknown,
}
```

**Problem**: This enum doesn't map cleanly to other languages:
- Python: `def` → Function, `class` → Class (no Struct/Enum/Trait)
- Java: `interface` (not Trait), `package` (not Module)
- C/C++: `union`, `namespace`
- JavaScript/TypeScript: `class`, `interface`, `type` alias

---

## Target Languages (Matching Splice)

| Language | Extensions | Parser Package | Status |
|----------|-------------|----------------|--------|
| Rust | `.rs` | tree-sitter-rust | ✅ Existing |
| Python | `.py` | tree-sitter-python | ❌ To implement |
| C | `.c`, `.h` | tree-sitter-c | ❌ To implement |
| C++ | `.cpp`, `.hpp`, `.cc`, `.cxx` | tree-sitter-cpp | ❌ To implement |
| Java | `.java` | tree-sitter-java | ❌ To implement |
| JavaScript | `.js`, `.mjs`, `.cjs` | tree-sitter-javascript | ❌ To implement |
| TypeScript | `.ts`, `.tsx` | tree-sitter-typescript | ❌ To implement |

---

## Architecture Design

### Option 1: Generic SymbolKind (Recommended)

Replace Rust-specific `SymbolKind` with language-agnostic enum:

```rust
pub enum SymbolKind {
    // Universal concepts
    Function,
    Method,
    Class,          // Covers: Rust struct, Python class, Java class, JS class
    Interface,      // Covers: Rust trait, Java interface, TS interface
    Enum,
    Module,         // Covers: Rust mod, Python module, Java package, JS ES module
    Variable,       // Covers: const, let, var (for future reference extraction)

    // Language-specific (when needed)
    Union,          // C/C++ union
    Namespace,      // C++ namespace
    TypeAlias,      // TypeScript type, Rust type alias

    Unknown,
}
```

**Mapping Table**:

| Language | Construct | Magellan SymbolKind |
|----------|-----------|---------------------|
| Rust | `fn` | Function |
| Rust | `struct` | Class |
| Rust | `trait` | Interface |
| Rust | `enum` | Enum |
| Rust | `mod` | Module |
| Python | `def` | Function |
| Python | `class` | Class |
| Java | `class` | Class |
| Java | `interface` | Interface |
| Java | `enum` | Enum |
| Java | `package` | Module |
| C | `struct` | Class |
| C++ | `class` | Class |
| C++ | `namespace` | Namespace |
| JS | `function` | Function |
| JS | `class` | Class |
| TS | `interface` | Interface |

### Option 2: Per-Language SymbolKind (Not Recommended)

Keep `SymbolKind` generic but store language-specific kind in JSON data field:

```rust
// Symbol node data field:
{
  "name": "foo",
  "kind": "Function",       // Generic (for queries)
  "language_kind": "trait"  // Language-specific (raw from tree-sitter)
}
```

**Rejected**: Adds complexity, harder to query across languages.

---

## File Structure

### New Files to Create

| File | LOC Estimate | Purpose |
|------|--------------|---------|
| `src/ingest/detect.rs` | ~100 | Language detection from file extension |
| `src/ingest/python.rs` | ~300 | Python symbol extraction |
| `src/ingest/c.rs` | ~250 | C symbol extraction |
| `src/ingest/cpp.rs` | ~300 | C++ symbol extraction |
| `src/ingest/java.rs` | ~300 | Java symbol extraction |
| `src/ingest/javascript.rs` | ~300 | JavaScript symbol extraction |
| `src/ingest/typescript.rs` | ~350 | TypeScript symbol extraction |
| `tests/language_detection_tests.rs` | ~100 | TDD tests for detect.rs |
| `tests/python_symbol_tests.rs` | ~150 | TDD tests for Python |
| `tests/c_symbol_tests.rs` | ~100 | TDD tests for C |
| `tests/cpp_symbol_tests.rs` | ~120 | TDD tests for C++ |
| `tests/java_symbol_tests.rs` | ~150 | TDD tests for Java |
| `tests/javascript_symbol_tests.rs` | ~150 | TDD tests for JavaScript |
| `tests/typescript_symbol_tests.rs` | ~180 | TDD tests for TypeScript |

**Total New Code**: ~2,950 LOC

### Files to Modify

| File | Changes |
|------|---------|
| `src/ingest/mod.rs` | Add `pub mod detect;` and language modules |
| `src/ingest.rs` | Refactor to generic `SymbolKind`, add language dispatch |
| `src/lib.rs` | Re-export new modules |
| `src/graph/scan.rs` | Replace `.rs` filter with `detect_language()` dispatch |
| `src/watch_cmd.rs` | Replace `.rs` filter with `detect_language()` dispatch |
| `src/main.rs` | Update help text to remove "for Rust projects" |
| `Cargo.toml` | Add tree-sitter parser dependencies |
| `docs/CONTRACT.md` | Update "Supported language" section |

---

## Implementation Phases

### Phase 1: Infrastructure (Foundation)

**Goal**: Set up language detection and generic SymbolKind

| Task | File | Changes |
|------|------|---------|
| 1.1 | `src/ingest/detect.rs` | Create `Language` enum + `detect_language()` function |
| 1.2 | `src/ingest/mod.rs` | Add `pub mod detect;` |
| 1.3 | `tests/language_detection_tests.rs` | TDD tests for all extensions |
| 1.4 | `src/ingest.rs` | Refactor `SymbolKind` to generic enum |
| 1.5 | `Cargo.toml` | Add tree-sitter parser packages |

**Exit Criteria**:
- `detect_language("test.py")` returns `Some(Language::Python)`
- `detect_language("test.unknown")` returns `None`
- All 7 tree-sitter parsers compile

### Phase 2: Python Support

**Goal**: First non-Rust language, validates architecture

| Task | File | Changes |
|------|------|---------|
| 2.1 | `src/ingest/python.rs` | Extract `def`, `class`, `async def` |
| 2.2 | `tests/python_symbol_tests.rs` | TDD tests for Python symbols |
| 2.3 | `src/ingest.rs` | Add Python to language dispatch |

**Python tree-sitter nodes**:
- `function_definition` → SymbolKind::Function
- `class_definition` → SymbolKind::Class
- `decorated_definition` → recurse to child
- `identifier` → name extraction
- `async` → detect (stored in data field)

**Exit Criteria**:
- `test.py` with `def foo():` → indexed as Function
- `test.py` with `class Bar:` → indexed as Class
- Scanner picks up `.py` files

### Phase 3: C/C++ Support

**Goal**: Add C and C++ (share some patterns)

| Task | File | Changes |
|------|------|---------|
| 3.1 | `src/ingest/c.rs` | Extract `struct`, `union`, `enum`, `function` |
| 3.2 | `tests/c_symbol_tests.rs` | TDD tests for C symbols |
| 3.3 | `src/ingest/cpp.rs` | Extract `class`, `namespace`, `template` |
| 3.4 | `tests/cpp_symbol_tests.rs` | TDD tests for C++ symbols |
| 3.5 | `src/ingest.rs` | Add C/C++ to language dispatch |

**C tree-sitter nodes**:
- `function_definition` → SymbolKind::Function
- `struct_specifier` → SymbolKind::Class
- `union_specifier` → SymbolKind::Union
- `enum_specifier` → SymbolKind::Enum
- `field_declaration` → skip (struct members)

**C++ tree-sitter nodes**:
- `function_definition` → SymbolKind::Function
- `class_specifier` → SymbolKind::Class
- `struct_specifier` → SymbolKind::Class
- `namespace_definition` → SymbolKind::Namespace
- `template_declaration` → recurse to child

**Exit Criteria**:
- `test.c` with `struct Foo {}` → indexed as Class
- `test.cpp` with `class Bar {}` → indexed as Class
- `test.cpp` with `namespace NS {}` → indexed as Namespace

### Phase 4: Java Support

**Goal**: Add Java with `interface` and `package`

| Task | File | Changes |
|------|------|---------|
| 4.1 | `src/ingest/java.rs` | Extract `class`, `interface`, `enum`, `package` |
| 4.2 | `tests/java_symbol_tests.rs` | TDD tests for Java symbols |
| 4.3 | `src/ingest.rs` | Add Java to language dispatch |

**Java tree-sitter nodes**:
- `method_declaration` → SymbolKind::Method
- `class_declaration` → SymbolKind::Class
- `interface_declaration` → SymbolKind::Interface
- `enum_declaration` → SymbolKind::Enum
- `package_declaration` → SymbolKind::Module

**Exit Criteria**:
- `Test.java` with `class Test {}` → indexed as Class
- `Test.java` with `interface Foo {}` → indexed as Interface

### Phase 5: JavaScript/TypeScript Support

**Goal**: Add JS and TS (TSX support)

| Task | File | Changes |
|------|------|---------|
| 5.1 | `src/ingest/javascript.rs` | Extract `function`, `class`, `export` |
| 5.2 | `tests/javascript_symbol_tests.rs` | TDD tests for JS symbols |
| 5.3 | `src/ingest/typescript.rs` | Extract TS-specific (`interface`, `type`, `enum`) |
| 5.4 | `tests/typescript_symbol_tests.rs` | TDD tests for TS symbols |
| 5.5 | `src/ingest.rs` | Add JS/TS to language dispatch |

**JavaScript tree-sitter nodes**:
- `function_declaration`, `function_expression` → SymbolKind::Function
- `class_declaration`, `class_expression` → SymbolKind::Class
- `method_definition` → SymbolKind::Method
- `arrow_function` → skip (anonymous, no name)

**TypeScript tree-sitter nodes**:
- All JS nodes +:
- `interface_declaration` → SymbolKind::Interface
- `type_alias_declaration` → SymbolKind::TypeAlias
- `enum_declaration` → SymbolKind::Enum
- `internal_module` → SymbolKind::Namespace (TS `namespace`)
- `export_statement` → recurse to child

**TSX variants**:
- Use `tree_sitter_typescript::language_tsx()` for `.tsx` files

**Exit Criteria**:
- `test.js` with `function foo() {}` → indexed as Function
- `test.ts` with `interface Foo {}` → indexed as Interface
- `test.tsx` parsed with TSX parser

### Phase 6: Integration & Cleanup

**Goal**: Remove hard-coded filters, update documentation

| Task | File | Changes |
|------|------|---------|
| 6.1 | `src/graph/scan.rs` | Replace `.rs` filter with `detect_language()` |
| 6.2 | `src/watch_cmd.rs` | Replace `.rs` filter with `detect_language()` |
| 6.3 | `src/main.rs` | Update help text |
| 6.4 | `docs/CONTRACT.md` | Update supported languages list |
| 6.5 | Integration tests | Multi-language codebase scanning |

**Exit Criteria**:
- `magellan watch` indexes `.py`, `.java`, `.js`, `.ts` files
- `magellan scan` finds all supported languages
- CONTRACT.md reflects new scope

---

## Testing Strategy

### TDD Approach (Per Language)

For each language:

1. **Write failing test first**
2. **Prove test fails** with expected error
3. **Implement minimal extraction**
4. **Prove test passes**
5. **No blanket #[allow]** — fix root causes

### Test Coverage (Per Language)

| Test Case | Purpose |
|-----------|---------|
| Empty file | Returns empty vec |
| Syntax error | Graceful handling (no crash) |
| Single function | Name and span extracted |
| Single class | Name and span extracted |
| Multiple symbols | All extracted |
| Nested symbols | Flat extraction (no hierarchy) |
| Language-specific construct | e.g., `async def`, `namespace`, `interface` |
| Export handling | e.g., `export function`, `export class` |

### Integration Test

Create temporary codebase with mixed languages:
```
/tmp/mixed_codebase/
  main.rs         (Rust)
  utils.py        (Python)
  config.java     (Java)
  logic.js        (JavaScript)
  types.ts        (TypeScript)
```

Run `magellan scan` and verify all files indexed.

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| SymbolKind mapping ambiguity | Use "closest match" principle, document mapping table |
| tree-sitter parser inconsistencies | Test each parser with real code samples |
| Performance degradation | Benchmark with large codebases, optimize if needed |
| Scope creep | CONTRACT.md requires explicit approval — freeze after initial 7 languages |

---

## Dependencies

### Cargo.toml Additions

```toml
# Existing
tree-sitter = "0.21"
tree-sitter-rust = "0.21"

# To add
tree-sitter-python = "0.21"
tree-sitter-c = "0.21"
tree-sitter-cpp = "0.21"
tree-sitter-java = "0.21"
tree-sitter-javascript = "0.21"
tree-sitter-typescript = "0.21"
```

---

## Success Criteria

1. ✅ All 7 languages supported (Rust, Python, C, C++, Java, JS, TS)
2. ✅ `detect_language()` maps 15 extensions correctly
3. ✅ Scanner finds all supported file types
4. ✅ Symbol extraction TDD tests pass (minimum 7 tests per language)
5. ✅ Integration test passes (mixed codebase)
6. ✅ CONTRACT.md updated with new scope
7. ✅ Zero new compiler warnings
8. ✅ All files ≤ 300 LOC

---

## References

- Splice language detection: `/home/feanor/Projects/splice/docs/LANGUAGE_DETECTION_PLAN.md`
- Splice TypeScript implementation: `/home/feanor/Projects/splice/src/ingest/typescript.rs`
- tree-sitter grammars: https://tree-sitter.github.io/tree-sitter/

---

*Created: 2025-12-28*

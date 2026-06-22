# CFG Limitations

**Version:** 3.1.7

Magellan stores CFG blocks and typed CFG edges, but its default CFG extraction is
source-structure based. It is useful for navigation, coverage correlation, and
local reasoning, but it is not a compiler-grade semantic CFG.

## Supported By Default

Default extraction is based on tree-sitter syntax trees for supported source
languages:

- Rust
- Python
- C
- C++
- Java
- JavaScript
- TypeScript

Directory scans ignore unsupported extensions.

## What The Default CFG Does Well

- Function-level block extraction where the parser supports it.
- Branch, loop, return, fallthrough, and selected conditional edges.
- Short-circuit `&&` / `||` modeling.
- Rust `?` early-return paths.
- Match guard edges.
- CFG block hashes and statement snippets.
- 4D coordinate columns in SQLite CFG storage:
  - `coord_x`
  - `coord_y`
  - `coord_z`
  - `coord_t`

## Known Limits

- No full type checking.
- No macro expansion.
- No borrow-checker or lifetime reasoning.
- No interprocedural compiler optimization model.
- Dynamic dispatch, reflection, monkey patching, and generated code may be
  incomplete.
- C/C++ CFG uses LLVM IR when clang is available at runtime; falls back to
  tree-sitter when clang is absent. Java CFG uses bytecode when javac is
  available; falls back to tree-sitter otherwise.

## External Tools (Runtime Detection)

No special build flags are required. Magellan detects `clang` and `javac` on
`PATH` at indexing time.

- **clang present:** C/C++ CFGs and call edges come from LLVM IR. Results match
  the compiler's view of the code, including macro expansion.
- **clang absent:** C/C++ CFGs come from tree-sitter. Macro-heavy code and
  preprocessor conditionals are less accurate.
- **javac present:** Java CFGs come from bytecode.
- **javac absent:** Java CFGs come from tree-sitter.

## Coverage

Coverage ingestion maps LCOV data onto CFG coverage side tables. Coverage data
does not make CFG extraction more semantically precise; it records observed
execution over the CFG data Magellan has.

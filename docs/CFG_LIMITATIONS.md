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
- Bytecode/IR-level CFG is optional and feature-gated where available.

## Optional External Tools

Build with:

```bash
cargo build --features external-tools-cfg
```

This enables extra C/C++ and Java CFG extraction paths through installed
external tools. The default build does not require those tools.

## Coverage

Coverage ingestion maps LCOV data onto CFG coverage side tables. Coverage data
does not make CFG extraction more semantically precise; it records observed
execution over the CFG data Magellan has.

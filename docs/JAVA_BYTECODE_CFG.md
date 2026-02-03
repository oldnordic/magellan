# Java Bytecode CFG (Optional Feature)

**Feature:** `bytecode-cfg` (Phase 44)
**Status:** Optional enhancement
**Dependency:** Java bytecode library

## Overview

Magellan supports optional Java bytecode-based Control Flow Graph extraction. This provides more precise CFG than AST-based analysis for Java code.

## Why Bytecode CFG?

Bytecode-based CFG is more accurate than AST-based CFG for Java because:

1. **Compiler-generated control flow**: Synthetic bridge methods, lambda desugaring
2. **Exception handling**: Try/catch/finally blocks create implicit edges
3. **Compiler optimizations**: Dead code elimination, branch optimization visible
4. **Complete control flow**: No syntactic ambiguities from complex expressions

## Comparison

| Aspect | AST CFG (Phase 42) | Bytecode CFG (Phase 44) |
|--------|-------------------|------------------------|
| **Required** | Java source code | Compiled .class files |
| **Precision** | Syntactic | Semantic (after javac) |
| **Exception edges** | Approximated | Exact |
| **Lambda bodies** | Nested syntax | Full desugared CFG |
| **Compilation step** | None (tree-sitter) | Requires javac |
| **Feature flag** | Always available | `--features bytecode-cfg` |

## Enabling the Feature

### For Users

```bash
# Build Magellan with bytecode CFG support
cargo build --release --features bytecode-cfg

# When using Magellan, bytecode CFG is automatically used for .class files
magellan index --project . --with-cfg
```

### For Developers

```toml
# In Cargo.toml
[dependencies]
magellan = { version = "2.0", features = ["bytecode-cfg"] }
```

## Limitations

1. **Requires javac**: Source code must be compiled first
2. **Java-only**: Other JVM languages (Kotlin, Scala) not supported without adaptation
3. **Optional**: Not required for Magellan to work; AST CFG is fallback
4. **External dependency**: Adds dependency on Java bytecode library

## When to Use

### Use Bytecode CFG When:

- Analyzing compiled Java libraries (no source available)
- Precise exception flow analysis needed
- Analyzing lambda-heavy code (desugared control flow)
- Building tools that require exact control flow

### Use AST CFG When:

- Source code is the primary input
- Java compilation step is undesirable
- Analyzing mixed-language projects
- Binary size must be minimal

## Implementation Notes

- Uses java_asm library (Rust-based Java bytecode parser)
- Reuses cfg_blocks/cfg_edges schema from Phase 42
- Feature-gated: only compiled when `bytecode-cfg` feature enabled
- Graceful degradation: falls back to AST CFG if bytecode unavailable

**Note**: The org.ow2.asm library (Java-based) is referenced for documentation,
but the Rust implementation uses equivalent Rust libraries. Phase 44-02 will
implement the actual bytecode CFG extraction logic.

## Future Enhancements

- Kotlin bytecode support
- Scala bytecode support
- Incremental CFG updates (detect class file changes)
- Integration with Mirage path enumeration

## References

- ASM Documentation: https://asm.ow2.io/
- ASM Analysis Guide: https://asm.ow2.io/asm70-guide.pdf
- Phase 44 Plans: `.planning/phases/44-bytecode-cfg-java/`
- Phase 42 (AST CFG): `.planning/phases/42-ast-cfg-rust/`

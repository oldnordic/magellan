# CFG Limitations in Magellan

**Version:** 3.1.6+
**Last Updated:** 2026-04-21
**Schema Version:** 10

## Overview

Magellan extracts Control Flow Graphs (CFGs) from source code using AST-based analysis. This enables intra-procedural analysis such as:

- Cyclomatic complexity computation
- Path enumeration within functions
- Dominance analysis
- Program slicing (limited)
- 4D spatial coordinate mapping of control flow

**Important:** CFG extraction is language-specific and has known limitations. This document explains what's supported, what's not, and future plans.

---

## What is AST-based CFG?

AST-based CFG extraction constructs control flow graphs by analyzing the Abstract Syntax Tree (AST) rather than compiler intermediate representations (MIR, LLVM IR, JVM bytecode).

**Advantages:**
- Works on stable toolchains (no nightly compiler required)
- No external binary dependencies (single binary distribution)
- Fast extraction (tree-sitter is optimized for parsing)
- Multi-language support (via tree-sitter grammars)
- Typed edges between blocks with 4D spatial coordinates for advanced analysis

**Trade-offs:**
- Less precise than compiler IR (missing macro expansion, generic monomorphization)
- Limited to syntax-level control flow (no runtime semantics)
- Cannot see compiler-generated code (async desugaring, closure transforms)

---

## Rust CFG Support

### Supported Constructs

Magellan correctly extracts CFG for these Rust constructs:

| Construct | Support | Notes |
|-----------|---------|-------|
| `if / else if / else` | Full | All branches tracked, merge blocks identified |
| `loop` | Full | Infinite loop with `break` exit paths |
| `while` | Full | Conditional loop with conditional exit |
| `for` | Full | Iterator loops with `break`/`continue` |
| `match` | Full | Pattern matching with all arms |
| `match` guards (`if x > 0`) | Full | Guard blocks with ConditionalTrue/ConditionalFalse edges |
| `return` | Full | Early return paths tracked |
| `break` | Full | Loop break edges |
| `continue` | Full | Loop continue edges |
| `?` operator (try) | Full | Success path (fallthrough) and error path (Return edge) |
| `&&` / `||` (short-circuit) | Full | Desugared into conditional blocks with merge blocks |
| `await` | Partial | Treated as yield point (state machine edges not visible) |
| `closure` expressions | Partial | Block structure tracked, capture analysis not done |
| `block` expressions | Full | Statement blocks tracked |
| function calls | Full | Call targets recorded (inter-procedural edges) |

**Example: Short-circuit operators**

```rust
fn short_circuit_example(a: bool, b: bool) -> bool {
    a && b
}
```

This function produces a CFG with:
- Entry block
- Block for `a` with ConditionalTrue edge to block for `b`, ConditionalFalse edge to merge
- Block for `b` with Fallthrough edge to merge
- Merge block

**Example: Try operator**

```rust
fn try_example(x: Option<i32>) -> Option<i32> {
    let y = x?;
    Some(y * 2)
}
```

This function produces a CFG with:
- Entry block
- Block for `x?` with success path (Fallthrough) to `y * 2` block
- Error path (Return edge) from `x?` block directly to function return
- Merge block for `Some(y * 2)`

**Example: Match guards**

```rust
fn guard_example(x: Option<i32>) -> i32 {
    match x {
        Some(v) if v > 0 => v * 2,
        Some(v) => v,
        None => 0,
    }
}
```

This function produces a CFG with:
- Entry block
- Match entry block
- `match_guard` block for `v > 0` with ConditionalTrue to arm body, ConditionalFalse to next arm
- Match merge block

### Not Supported (Rust)

These constructs have **limited or incorrect** CFG extraction:

| Construct | Limitation | Impact |
|-----------|------------|--------|
| **Macro expansion** | AST doesn't see expanded code | CFG misses macro-generated control flow |
| **Generic monomorphization** | AST doesn't see type-specific code | CFG assumes generic structure (may not match runtime) |
| **async / await desugaring** | State machine not visible | CFG treats `await` as simple yield (missing state machine edges) |
| **Closure captures** | Capture analysis not done | CFG cannot reason about closure side effects |
| **Trait method dispatch** | Dynamic dispatch not resolved | CFG assumes all trait methods are callable |
| **panic! / assert!** | Treated as simple terminators | Cannot distinguish unwinding vs abort |
| **loop labels** | Label resolution not implemented | Nested breaks may target wrong loop |
| **goto (via macros)** | No real goto in Rust | N/A (Rust has no goto) |

**Example: Macro expansion not visible**

```rust
macro_rules! my_loop {
    ($body:expr) => {
        loop { $body; break; }
    };
}

fn example() {
    my_loop!(println!("Hello"));
}
```

**CFG shows:** Empty block (macro call is opaque)
**Reality:** Infinite loop with break (visible only after expansion)

**Workaround:** Pre-process code with `cargo expand` before indexing (not integrated).

### Precision Comparison

| Analysis Type | Precision | Example: `vec![1, 2, 3]` |
|---------------|-----------|--------------------------|
| **AST-based CFG** | Syntax-level | Single macro call node |
| **MIR (Rust)** | Compiler IR | Multiple basic blocks, allocation, push operations |
| **LLVM IR** | Optimized IR | Optimized allocation, vectorized operations |

**For most use cases:** AST-based CFG is sufficient (cyclomatic complexity, path enumeration, dominance analysis).

**For precise analysis:** Use MIR-based tools (Mirage, Charon) when available.

---

## C/C++ CFG Support

### Current Status: AST-based (Same limitations as Rust)

Magellan's Phase 42 implementation works for **C and C++** via tree-sitter grammars.

**Supported:**
- `if / else if / else`
- `for / while / do-while`
- `switch / case / break`
- `return / goto / continue`
- Function calls

**Not Supported:**
- Macro expansion (#define, preprocessor)
- Template specialization
- Virtual function dispatch resolution
- Exception handling (try/catch)

**Future:** Phase 43 (optional LLVM IR integration) would enable precise CFG for C/C++ using Clang's IR output.

---

## Java CFG Support

### Current Status: AST-based (Same limitations as Rust)

Magellan's Phase 42 implementation works for **Java** via tree-sitter grammar.

**Supported:**
- `if / else if / else`
- `for / while / do-while`
- `switch / case / break`
- `return / continue`
- Method calls
- `try / catch / finally` (basic)

**Not Supported:**
- Exception flow (precise exception paths not tracked)
- Lambda desugaring (treated as method calls)
- Generic type erasure (AST only sees generic structure)
- Synthetic bridge methods (compiler-generated)

**Future:** Phase 44 (optional bytecode CFG) would enable precise CFG using ASM library for JVM bytecode analysis.

---

## Edge Types and 4D Coordinates

### CFG Edge Types

The modern CFG extractor (`cfg_edges_extract.rs`) stores typed edges between blocks:

| Edge Type | Description |
|-----------|-------------|
| `Fallthrough` | Sequential execution to next block |
| `ConditionalTrue` | Branch taken when condition is true |
| `ConditionalFalse` | Branch taken when condition is false |
| `Return` | Early return or error path (`?` operator) |
| `BackEdge` | Loop back edge (continue, loop iteration) |
| `Call` | Function call to another function |
| `Goto` | Unconditional jump |

### 4D Spatial Coordinates

Each CFG block is annotated with 4D coordinates for advanced analysis:

| Coordinate | Meaning | Use Case |
|------------|---------|----------|
| `coord_x` | Dominator depth | How deeply nested in control flow |
| `coord_y` | Loop nesting | How many loops enclose this block |
| `coord_z` | Branch distance | Distance from nearest branch point |
| `coord_t` | Temporal ordering | Execution order within function |

**Querying coordinates:**

```bash
# Get most complex blocks (highest branch distance)
mirage --db .magellan/magellan.db --output json cfg --function "function_name" | \
  jq '.data.blocks | sort_by(.coord_z) | reverse | .[0:5]'

# Find blocks in specific coordinate ranges
llmgrep --db .magellan/magellan.db search --query ".*" \
  --depth-range-x 0-5 --depth-range-y 0-3 --depth-range-z 0-10
```

---

## When to Use CFG Data

### Good Use Cases

AST-based CFG is appropriate for:

- **Cyclomatic complexity:** Counting decision points for complexity metrics
- **Path enumeration:** Finding execution paths within a function (bounded)
- **Dominance analysis:** Identifying dominator blocks for optimization hints
- **Code coverage:** Identifying uncovered branches (when combined with test data)
- **Dead code detection:** Finding unreachable blocks within functions
- **Refactoring safety:** Checking if a code change affects control flow
- **4D coordinate analysis:** Finding high-complexity regions, deeply nested code

### Poor Use Cases

AST-based CFG is **NOT appropriate** for:

- **Precise data flow analysis:** Cannot track variable assignments across macro boundaries
- **Escape analysis:** Cannot determine lifetimes after compiler optimization
- **Inlining decisions:** Cannot predict if compiler will inline a function
- **Exception safety analysis:** Cannot see exception propagation in C++/Java
- **Async/await reasoning:** Cannot see state machine edges in Rust async code

**Alternative:** Use language-specific tools for these analyses:
- Rust: Miri, clippy, rustc MIR
- C/C++: clang static analyzer, Coverity
- Java: SpotBugs, Error Prone

---

## Future Improvements

### Phase 43: LLVM IR CFG for C/C++ (Optional)

**Goal:** Add optional LLVM IR-based CFG extraction for C/C++ using Clang.

**Status:** Planned (infrastructure only)

**Approach:**
1. Compile C/C++ to LLVM IR using Clang
2. Parse LLVM IR to extract basic blocks
3. Store in same `cfg_blocks` table (schema v10)

**Benefits:**
- Precise CFG (sees macro expansion, template specialization)
- Optimized IR (compiler-generated code visible)

**Trade-offs:**
- Requires Clang integration (external dependency)
- Optional feature flag (not required for Magellan)
- Increases build complexity

**See:** `.planning/phases/43-llvm-cfg-cpp/README.md`

### Phase 44: JVM Bytecode CFG for Java (Optional)

**Goal:** Add optional bytecode-based CFG extraction for Java using ASM library.

**Status:** Planned (infrastructure only)

**Approach:**
1. Compile Java to .class files (javac)
2. Parse bytecode using ASM library
3. Store in same `cfg_blocks` table (schema v10)

**Benefits:**
- Precise CFG (compiler-generated code visible)
- Exception flow tracking

**Trade-offs:**
- Requires compiled .class files (extra build step)
- Optional feature flag (not required for Magellan)
- Binary size increase (~100KB)

**See:** `.planning/phases/44-bytecode-cfg-java/README.md`

### Stable MIR Integration (Rust, Future)

**Goal:** Native MIR extraction using Rust's stable_mir crate.

**Status:** Blocked on stable_mir publication (expected 2025H2)

**Tracking:** https://rust-lang.github.io/rust-project-goals/2025h1/stable-mir.html

**Benefits:**
- Compiler-precise CFG (no AST limitations)
- Macro expansion visible
- Generic monomorphization visible
- Async/await state machine visible

**Trade-offs:**
- Requires stable_mir dependency (when published)
- May require nightly Rust initially
- Schema extension for MIR-specific features

---

## Comparison Table

| Language | Current CFG | Future CFG | Notes |
|----------|-------------|------------|-------|
| **Rust** | AST-based (v3.1.6+) | Stable MIR (future) | AST CFG sufficient for most use cases |
| **C/C++** | AST-based (v3.1.6+) | LLVM IR (Phase 43, optional) | Optional enhancement via Clang |
| **Java** | AST-based (v3.1.6+) | Bytecode (Phase 44, optional) | Optional enhancement via ASM |
| **JavaScript** | AST-based (v3.1.6+) | None planned | AST CFG is standard for JS |
| **TypeScript** | AST-based (v3.1.6+) | None planned | AST CFG is standard for TS |
| **Python** | AST-based (v3.1.6+) | None planned | AST CFG is standard for Python |
| **Go** | AST-based (v3.1.6+) | None planned | AST CFG is standard for Go |

---

## API Reference

### Querying CFG Data

**Get CFG for a function:**

```bash
mirage --db .magellan/magellan.db --output json cfg --function "function_name"
```

**Get all blocks with a specific edge type:**

```sql
SELECT * FROM cfg_edges WHERE edge_type = 'ConditionalTrue';
```

**Get blocks with high complexity (high coord_z):**

```sql
SELECT b.id, b.kind, b.coord_x, b.coord_y, b.coord_z, b.coord_t,
       e.name as function_name
FROM cfg_blocks b
JOIN graph_entities e ON b.function_id = e.id
WHERE b.coord_z > 5
ORDER BY b.coord_z DESC;
```

**Get CFG statistics for a file:**

```sql
SELECT kind, COUNT(*) as count
FROM cfg_blocks
WHERE function_id IN (
    SELECT id FROM graph_entities WHERE file_path = 'src/main.rs'
)
GROUP BY kind;
```

### CFG Block Schema

**Table:** `cfg_blocks`

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER | Primary key |
| `function_id` | INTEGER | Foreign key to graph_entities (function) |
| `kind` | TEXT | Block kind (Entry, If, Else, Loop, While, For, MatchArm, MatchGuard, And, Or, Try, Await, Closure, etc.) |
| `terminator` | TEXT | Terminator kind (Fallthrough, Conditional, Goto, Return, etc.) |
| `coord_x` | INTEGER | Dominator depth |
| `coord_y` | INTEGER | Loop nesting level |
| `coord_z` | INTEGER | Branch distance |
| `coord_t` | INTEGER | Temporal ordering |
| `span_start` | INTEGER | Byte offset of block start |
| `span_end` | INTEGER | Byte offset of block end |

**Table:** `cfg_edges`

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER | Primary key |
| `from_block_id` | INTEGER | Source block |
| `to_block_id` | INTEGER | Target block |
| `edge_type` | TEXT | Edge type (Fallthrough, ConditionalTrue, ConditionalFalse, Return, BackEdge, Call, Goto) |
| `function_id` | INTEGER | Owning function |

**Indexes:**
- `idx_cfg_blocks_function_id` (function_id)
- `idx_cfg_blocks_span` (span_start, span_end)
- `idx_cfg_blocks_terminator` (terminator)
- `idx_cfg_edges_function_id` (function_id)
- `idx_cfg_edges_from_block` (from_block_id)
- `idx_cfg_edges_to_block` (to_block_id)

---

## FAQ

**Q: Why not use MIR/LLVM IR/JVM bytecode from the start?**

A: AST-based approach works on stable toolchains, requires no external dependencies, and enables CFG extraction for all languages via tree-sitter. IR-based approaches are language-specific, require compiler integration, and are optional enhancements (Phases 43-44).

**Q: Will my CFG be wrong if I use macros?**

A: CFG for macro-expanded code will be incomplete. The macro call itself will appear as a single node, but the expanded control flow will not be visible. Pre-process with `cargo expand` if you need precise CFG.

**Q: Can I use CFG for code coverage?**

A: Yes, but with limitations. AST-based CFG can identify all branches in source code, but cannot see compiler-generated branches (e.g., async state machine edges). Combine with runtime coverage tools for complete coverage.

**Q: Is cyclomatic complexity accurate?**

A: For syntax-level constructs, yes. Cyclomatic complexity based on AST CFG counts decision points (if, match, loops) visible in source code. This is the standard definition used by most tools (e.g., radon, lizard).

**Q: When will Stable MIR be available?**

A: The stable_mir project is a 2025H1 goal for the Rust project. Track progress at: https://rust-lang.github.io/rust-project-goals/2025h1/stable-mir.html

**Q: Should I wait for Phase 43/44 before using CFG?**

A: No. AST-based CFG (Phase 42) is sufficient for most use cases. Phases 43-44 are optional enhancements for users who need IR-level precision.

**Q: What are 4D coordinates used for?**

A: The 4D coordinates (coord_x, coord_y, coord_z, coord_t) enable spatial analysis of control flow. You can find deeply nested code, high-complexity regions, and analyze execution paths geometrically. Use Mirage or llmgrep with coordinate range filters.

**Q: How do I know if my database has 4D coordinates?**

A: Check with: `sqlite3 .magellan/magellan.db "PRAGMA table_info(cfg_blocks);" | grep coord`. If missing, rebuild your database with `rm .magellan/magellan.db && magellan watch --root ./src --db .magellan/magellan.db --scan-initial`.

---

## Reporting Issues

If you encounter incorrect CFG extraction:

1. **Check if it's a known limitation:** See "Not Supported" sections above
2. **Search existing issues:** https://github.com/your-repo/magellan/issues
3. **File a new issue:** Include:
   - Source code example
   - Expected CFG structure
   - Actual CFG output (`mirage --db .magellan/magellan.db --output json cfg --function "function_name"`)
   - Language and version (`rustc --version`, `clang --version`, etc.)

---

## References

- **Phase 42 Plan:** `.planning/phases/42-ast-cfg-rust/42-04-PLAN.md`
- **Phase 42 Implementation (deprecated):** `src/graph/cfg_extractor.rs`
- **Phase 42 Implementation (current):** `src/graph/cfg_edges_extract.rs`
- **Phase 42 Research:** `.planning/phases/42-ast-cfg-rust/42-RESEARCH.md` (archived)
- **Schema v10:** `src/graph/db_compat.rs` (ensure_cfg_schema)
- **Phase 43 (LLVM IR):** `.planning/phases/43-llvm-cfg-cpp/README.md`
- **Phase 44 (Bytecode):** `.planning/phases/44-bytecode-cfg-java/README.md`

---

*Document Version:* 2.0
*Last Updated:* 2026-04-21
*Maintained By:* Magellan Project

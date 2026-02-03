# Gitignore-Aware Indexing

## Problem

Currently Magellan indexes **everything** in the watched directory, including:
- Dependencies (from `Cargo.lock` if indexing project root)
- Build artifacts (`target/`)
- Documentation files
- Generated code

This contaminates the call graph with noise.

## Proposed Solution

Make Magellan `.gitignore`-aware: exclude files matching `.gitignore` patterns during indexing.

### Default Behavior (Smart Defaults)

When `--root` points to a project root:

**Index by default:**
- `src/` - main source code
- `tests/` - test files
- `lib.rs`, `mod.rs` files
- Common source directories per language

**Exclude by default:**
- `target/`, `build/`, `out/`, `dist/` - build artifacts
- `node_modules/`, `vendor/` - dependencies
- `.git/`, `.codemcp/` - metadata
- `Cargo.lock` - lock files
- `*.db`, `*.db-shm`, `*.db-wal` - databases
- Documentation directories

### Explicit Control

```bash
# Smart mode (respects .gitignore)
magellan watch --root ./src --db .codemcp/codegraph.db --gitignore-aware

# Explicit include/exclude
magellan watch --root . --db .codemcp/codegraph.db \
  --include "src/**" \
  --exclude "target/**" \
  --exclude "**/tests/**"

# Config file
magellan watch --root . --db .codemcp/codegraph.db --config .magellanignore
```

### .magellanignore File

Similar to `.gitignore` but Magellan-specific:

```
# Include patterns
src/**/*.rs
tests/**/*.rs

# Exclude patterns
target/**
vendor/**
**/generated/**
**/*.generated.rs
```

---

## Charon vs Built-in MIR Extraction

### Current State

- **Mirage** requires Charon for MIR extraction (CFG analysis)
- Charon is an external dependency with installation friction
- Charon has compatibility issues (nightly Rust, compilation failures)

### Decision: External vs Built-in

| Factor | Use Charon | Build Into Magellan |
|--------|------------|---------------------|
| **Installation** | ❌ External dep, friction | ✅ Single binary |
| **Maintenance** | ✅ Leverages existing work | ❌ More code to maintain |
| **Control** | ❌ At their mercy | ✅ Full control |
| **MIR Stability** | ❌ They track rustc changes | ❌ Same problem either way |
| **Performance** | ✅ Optimized for extraction | ✅ Can optimize for our needs |
| **Integration** | ❌ Separate process | ✅ Direct library call |

### Recommendation: **Build Into Magellan**

**Rationale:**

1. **Single Binary Philosophy** - The toolset (Magellan, llmgrep, Mirage, splice) aims to be self-contained
2. **No Installation Friction** - `cargo install magellan` just works
3. **Targeted Extraction** - Magellan only needs what's useful for call graph + CFG
4. **Consistent Database** - No intermediate files, direct to sqlitegraph

**Implementation Approach:**

```
Phase 1: Gitignore awareness (immediate)
Phase 2: Evaluate MIR extraction options
  - Option A: Fork Charon, strip down to essentials
  - Option B: Use rustc-driver directly (what Charon does)
  - Option C: Minimal MIR extraction for just CFG edges
Phase 3: Integrate into Magellan watch
```

### Alternative: Minimal "Charon Lite"

Instead of full Charon, build a minimal MIR extractor that only captures:

```rust
// What we actually need for call graph + CFG
struct FunctionInfo {
    name: String,
    span: Span,
    calls: Vec<SymbolId>,     // call graph
    blocks: Vec<BasicBlock>,  // CFG
    edges: Vec<CFGEdge>,      // control flow
}
```

**Don't need:**
- Full type information
- Trait resolution
- Lifetime analysis
- Most of what Charon extracts

---

## Implementation Priority

1. **Phase 1: Gitignore Awareness** (High priority, low complexity)
   - Parse `.gitignore` patterns
   - Apply during file scanning
   - Add `--gitignore-aware` flag (default: true)

2. **Phase 2: .magellanignore Support** (Medium priority)
   - Project-specific include/exclude rules
   - Fallback when `.gitignore` doesn't exist

3. **Phase 3: MIR Extraction Decision** (Requires research)
   - Benchmark Charon vs rustc-direct
   - Prototype "Charon Lite"
   - Decide integration approach

---

## Open Questions

1. Should `--gitignore-aware` be the default?
   - Pro: Less noise in databases by default
   - Con: Might exclude files user wants indexed

2. How to handle `.gitignore` in watched directories?
   - Parse once at startup?
   - Watch for changes and re-parse?

3. Per-language smart defaults?
   - Rust: `src/`, `tests/`, exclude `target/`
   - Python: `src/`, exclude `__pycache__/`, `.venv/`
   - JavaScript: `src/`, exclude `node_modules/`, `dist/`

---

## Related Issues

- Mirage needs Charon for CFG analysis
- Charon installation failures block Mirage usage
- Contaminated indexes include dependencies

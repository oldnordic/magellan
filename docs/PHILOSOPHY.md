# Design Philosophy

**Last Updated:** 2026-02-10
**Version:** v2.2.1

The core principles that guide Magellan's development.

---

## Part I: The LLM Era Manifesto

### The Problem

LLMs are forced to work with Unix text-era tools:

```
grep -r "function foo" .          # Returns 50 lines of text
cat src/main.rs                    # 10,000 tokens of context
sed 's/foo/bar/g' src/main.rs     # Hope it works
```

**Result**: Context bloat → Early compaction → **Guessing**

---

### Why IntelliJ Works for Java (And Why That Doesn't Scale)

IntelliJ refactoring works "mostly" for Java because **Java lets it**.

Not because JetBrains is magic.

#### What Java Has (That Others Don't)

| Feature | Java | C/C++ | Python | JS/TS | Rust |
|---------|------|-------|--------|-------|------|
| Single compiler pipeline | ✅ | ❌ | ❌ | ❌ | ✅ |
| Explicit symbol tables | ✅ | ❌ | ❌ | ❌ | ❌ |
| Nominal typing everywhere | ✅ | ❌ | ❌ | ❌ | Partial |
| No macros | ✅ | ❌ | ✅ | ✅ | ❌ |
| No conditional compilation | ✅ | ✅ | ❌ | ✅ | ❌ |
| Minimal metaprogramming | ✅ | ❌ | ✅ | ❌ | ❌ |
| One canonical AST | ✅ | ❌ | ❌ | ❌ | ❌ |

**Result**: Java gives IntelliJ a stable semantic contract to lean on.

#### Why Everything Else Feels Flaky

| Language | Problem |
|----------|---------|
| C/C++ | Macros, headers, conditional compilation = shifting reality |
| Python | Dynamic imports, monkey patching, runtime mutation |
| JS/TS | Bundlers, transpilers, multiple module systems |
| Rust | Macros, cfg flags, hygiene, proc macros |
| Mixed repos | Tools guess which "language truth" applies |

#### What IDE Refactors Do Outside Java

They rely on:
- **Heuristics** - "Probably this is a reference"
- **Partial semantic models** - "Good enough for common cases"
- **Best-effort guesses** - "Users tolerate silent failures"
- **Survivorship bias** - "It usually works" ≠ "It's correct"

```
IntelliJ: "Trust us, we figured it out."
Magellan: "Here's the proof. Or nothing happens."
```

That's not correctness. That's survivorship bias.

---

### The Approach That Scales

```
AST → byte spans → deterministic edits
          ↓
   Explicit scope rules
          ↓
   Reverse-order application
          ↓
   Compiler/LSP as gatekeeper
          ↓
   Rollback on failure
```

#### Why This Works Across Languages

| Approach | Java | C/C++ | Python | JS/TS | Rust |
|----------|------|-------|--------|-------|------|
| IntelliJ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| Magellan | ✅ | ✅ | ✅ | ✅ | ✅ |

Not because Magellan is smarter. Because Magellan **doesn't guess**.

#### The Difference

| IDE Refactor | Magellan Workflow |
|---------------|-------------------|
| Heuristics | Spans |
| Partial semantic | Full AST |
| Silent failures | Rollback |
| "Usually works" | "Works or nothing" |
| Trust the tool | Auditable proof |

---

### For the LLM Era: This Is Existential

```
Guessing = hallucination
Silent corruption = poisoned context
No audit trail = cannot debug
```

If an LLM + Magellan + Splice + enforced workflow can't refactor safely:

**That's not your fault. That's the language telling the truth.**

Some refactors **should** fail. Some code **should** be flagged as unsafe.

---

### What LLMs Need

#### NOT This (10,000+ tokens)
```
$ cat src/main.rs
// 500 lines of Rust code...
// LLM must: parse syntax, find symbols, track relationships
// Result: Context bloat, compaction, hallucination risk
```

#### BUT This (12 tokens)
```
$ magellan query --file src/main.rs --kind Function
{"symbol":"main","kind":"Function","line":42}
{"symbol":"parse_args","kind":"Function","line":156}
{"symbol":"run","kind":"Function","line":223}
```

---

### The Thesis

> **"Text is more tokens. Facts are answers."**

Magellan exists to give LLMs **answers, not search results**.

#### Before: LLM Cognitive Overload
```
LLM: "Find all callers of function foo"

Tool (grep): Returns 5000 lines of code
LLM: Must parse each line, track context, guess relationships
Result: 50,000 tokens, compaction, mistakes
```

#### After: Structured Facts
```
LLM: "Find all callers of function foo"

Magellan: [{"from":"bar","file":"src/lib.rs","line":42},
           {"from":"baz","file":"src/main.rs","line":156}]

LLM: Receives exact answer, 50 tokens
Result: No parsing, no guessing, correct operation
```

---

### The Unix Philosophy, Updated

```
1970: "Write programs to handle text streams."
2020: "Write programs to handle STRUCTURED FACTS."

Magellan: | Parse → Graph | Query → JSON |
Old Unix:  | Grep → Text  | Awk → Hope |
```

---

### The Compactness Advantage

| Query | grep Output | Magellan Output | Token Savings |
|-------|-------------|-----------------|---------------|
| "Functions in file" | ~500 lines | 3 JSON objects | 98% |
| "Callers of X" | ~200 lines | 5 JSON objects | 95% |
| "All symbols" | ~5000 lines | 150 JSON objects | 97% |
| "Rename impact" | ~1000 lines | 1 diff summary | 99% |

---

### Manifesto

1. **Text is the enemy** of token-efficient LLM operations
2. **Facts are answers** that don't require parsing
3. **Graph is intelligence** that captures relationships
4. **Structure is compact** - JSON < Source Code
5. **Verification is truth** - AST-based > Text-based

Magellan = **SQLiteGraph + TreeSitter = Facts for LLMs**

---

## Part II: Design Principles

### 1. Graph First, Text Second

**Principle:** Code is relationships, not just text.

**What this means:**
- Symbol → Symbol relationships (calls, references) are first-class
- File → Symbol relationships define structure
- Position data enables precise tooling
- Text search (grep) is a fallback, not the primary interface

**Why:**
- Text search can't distinguish between definition and usage
- Graph queries answer "who calls this" in O(1), grep takes O(n)
- Call graphs enable impact analysis, dead code detection, cycle detection
- Structure matters more than raw text for understanding codebases

**Trade-off:**
- Requires indexing step before queries (unlike grep)
- No substring search on symbol content (use grep for that)

---

### 2. Deterministic Output

**Principle:** Same input, same output. Always.

**What this means:**
- BLAKE3 hashes for stable SymbolId (32 characters, 128 bits)
- Sorted output for consistent ordering
- No randomized data structures
- Reproducible across runs

**Why:**
- Scriptable output for automation
- Diff-friendly for version control
- Cacheable results
- LLMs can rely on stable identifiers

**Example:**
```bash
# Same symbol always gets same ID
$ magellan find --db ./magellan.db --name main --output json
{
  "symbol_id": "a1b2c3d4e5f678901234567890123ab",  # Stable across re-index
  ...
}
```

---

### 3. AST via Tree-Sitter

**Principle:** Use robust parsers, not regex.

**What this means:**
- Tree-sitter grammars for all 7 supported languages
- Extracts: functions, classes, methods, enums, modules, calls, references
- Hierarchical AST nodes for structure analysis
- Position-accurate (byte offsets, line/column)

**Why:**
- Regex breaks on nested structures, comments, strings
- Tree-sitter handles edge cases (macros, generics, templates)
- Multi-language support with consistent API
- Recovery from syntax errors (keeps parsing)

**Trade-off:**
- No semantic analysis (type checking, name resolution)
- Cross-crate resolution limited to symbol names
- Some language-specific features need manual handling

---

### 4. CLI as Stable Interface

**Principle:** The command line is the API.

**What this means:**
- Every command has stable flags and output format
- JSON output schema is versioned
- Exit codes are meaningful
- Help text is comprehensive

**Why:**
- LLMs can generate shell commands more reliably than in-process code
- Easy to compose with other tools (jq, awk, splice)
- Language-agnostic integration
- No FFI/ABI compatibility concerns

**Example LLM Workflow:**

```bash
# LLM generates these commands deterministically
magellan watch --root ./src --db .codemcp/codegraph.db --scan-initial
llmgrep --db .codemcp/codegraph.db search --query "process" --output json
splice rename --symbol abc123 --file src/lib.rs --to new_process_name
```

Each command:
- Has stable, documented interface
- Returns parseable JSON
- Can be composed into workflows

---

### 5. Batteries Included for Graph Analysis

**Principle:** Don't make users implement graph algorithms.

**What's included:**

| Algorithm | Complexity | Use Case |
|-----------|------------|----------|
| Reachability | O(V + E) | Impact analysis, API exploration |
| Dead Code | O(V + E) | Cleanup, coverage gaps |
| Cycles (SCC) | O(V + E) | Refactoring mutual recursion |
| Condensation | O(V + E) | Topological ordering |
| Path Enumeration | Bounded | Test coverage analysis |
| Program Slice | O(V + E) | Bug isolation, refactoring safety |

**Why:**
- Graph algorithms are easy to get wrong
- Well-tested implementations save time
- Users can focus on their domain, not computer science

---

### 6. Dual Backend Strategy

**Principle:** Choice matters. Different workloads need different storage.

| SQLite Backend | Native V2 Backend |
|----------------|-------------------|
| Proven reliability | O(1) KV lookups |
| SQL tooling ecosystem | Smaller file sizes |
| Unlimited scale | Embedded with graph data |
| Familiar to developers | Cross-process KV communication |

**Unified API:**
```bash
# Same commands work with both backends
magellan watch --root . --db ./codegraph.db --scan-initial

# All operations identical
magellan find --db ./codegraph.db --name main
magellan reachable --db ./codegraph.db --symbol main
```

**Why not just pick one?**
- No single solution is optimal for all workloads
- Users understand their needs better than we do
- Migration path lets users start safe (SQLite) and optimize later (Native V2)

---

### 7. Pragmatic Constraints

**Principle:** Acknowledge limitations rather than over-engineering.

#### Known Limitations

| Limitation | Why It Exists | Plan |
|------------|---------------|------|
| No semantic analysis | AST-only keeps it simple | Not planned - use LSP servers |
| No cross-crate resolution | Name-based is sufficient for most | Name-based only |
| No incremental parsing | Full re-parse is fast enough | Not planned |
| One writer at a time | Simplifies concurrency | Acceptable for single-machine |

#### Semantic Versioning

**Breaking changes** (major version bump):
- API signature changes
- Removed features
- Export schema changes

**NOT breaking** (minor/patch):
- Performance improvements
- Bug fixes
- New features
- Documentation updates

---

### 8. Correct Over Clever

**Principle:** Simple, correct code beats clever but bug-prone optimizations.

#### Example: Symbol ID Resolution

**We could** use:
- Complex name resolution with scope tracking
- Cross-crate symbol table merging
- Incremental compilation integration

**Instead:**
- BLAKE3 hash of (file path, name, kind, position)
- Simple string-based FQN lookup
- Name-based call graph resolution

**Why:**
- Complexity introduces bugs
- Deterministic output matters more
- Works across 7 languages without language-specific logic
- Good enough for 99% of use cases

---

### 9. Developer Experience Matters

**Principle:** The tool should be pleasant to use.

#### Clear Error Messages

```bash
# Bad
ERROR: Symbol not found

# Good
ERROR: Symbol "main" not found in database
Hint: Use --ambiguous to show all candidates, or re-index with --scan-initial
```

#### Sensible Defaults

```bash
# Don't force users to make choices for common cases
magellan watch --root . --db ./magellan.db  # Sensible defaults
# Gitignore-aware: enabled
# Debounce: 500ms
# Scan initial: disabled (use flag)
```

#### Comprehensive Help

```bash
magellan --help           # Global help
magellan watch --help     # Command-specific help
magellan query --explain   # Selector cheat sheet
```

---

### 10. Community Over Code

**Principle:** The ecosystem matters more than any single tool.

#### Integration with Other Tools

```
┌─────────────────────────────────────────────────┐
│                  The Ecosystem                   │
│                                                  │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
│  │Magellan │─→│codegraph│←─│ splice  │         │
│  └─────────┘  │  .db   │  └─────────┘         │
│              └────┬────┘                       │
│  ┌─────────┐      │                           │
│  │ llmgrep │─────┘                           │
│  └─────────┘                                  │
│  ┌─────────┐                                  │
│  │ mirage  │          (CFG analysis)          │
│  └─────────┘                                  │
│                   ↓                             │
│  ┌───────────────────────────────────────────┐ │
│  │         OdinCode (LLM Editor)              │ │
│  │      Coordinates tool usage               │ │
│  └───────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

**Why:**
- Each tool does one thing well
- Shared database format enables composition
- LLM can orchestrate tools via CLI
- Users can adopt tools incrementally

---

## Summary

Magellan's design philosophy:

**For the LLM Era:**
1. Text is the enemy of token-efficient operations
2. Facts are answers that don't require parsing
3. Graph is intelligence that captures relationships
4. Structure is compact - JSON < Source Code
5. Verification is truth - AST-based > Text-based

**Core Design Principles:**
1. **Graph First** — Relationships over text
2. **Deterministic Output** — Stable, reproducible results
3. **AST via Tree-Sitter** — Robust, multi-language parsing
4. **CLI as Interface** — Stable, composable commands
5. **Batteries Included** — 6 graph algorithms built-in
6. **Dual Backend** — Choice between SQLite and Native V2
7. **Pragmatic Constraints** — Honest about limitations
8. **Correct Over Clever** — Simple beats bug-prone
9. **Developer Experience** — Pleasant to use
10. **Community Over Code** — Ecosystem focus

These principles guide every decision in Magellan's development. When in doubt, we refer back to these principles rather than following trends or hype.

---

## Further Reading

- [README.md](../README.md) - Quick start and command reference
- [MANUAL.md](../MANUAL.md) - Comprehensive operator manual
- [INTEGRATION.md](INTEGRATION.md) - Working with other tools in the ecosystem
- [PERFORMANCE.md](PERFORMANCE.md) - Benchmarks and optimization
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues and solutions

---

*Created: 2025-12-30*
*Updated: 2026-02-10*

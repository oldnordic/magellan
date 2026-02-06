# Philosophy: Tools for the LLM Era

## The Problem

LLMs are forced to work with Unix text-era tools:

```
grep -r "function foo" .          # Returns 50 lines of text
cat src/main.rs                    # 10,000 tokens of context
sed 's/foo/bar/g' src/main.rs     # Hope it works
```

**Result**: Context bloat → Early compaction → **Guessing**

---

## Why IntelliJ Works for Java (And Why That Doesn't Scale)

IntelliJ refactoring works "mostly" for Java because **Java lets it**.

Not because JetBrains is magic.

### What Java Has (That Others Don't)

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

### Why Everything Else Feels Flaky

| Language | Problem |
|----------|---------|
| C/C++ | Macros, headers, conditional compilation = shifting reality |
| Python | Dynamic imports, monkey patching, runtime mutation |
| JS/TS | Bundlers, transpilers, multiple module systems |
| Rust | Macros, cfg flags, hygiene, proc macros |
| Mixed repos | Tools guess which "language truth" applies |

### What IDE Refactors Do Outside Java

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

## The Approach That Scales

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

### Why This Works Across Languages

| Approach | Java | C/C++ | Python | JS/TS | Rust |
|----------|------|-------|--------|-------|------|
| IntelliJ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| Magellan | ✅ | ✅ | ✅ | ✅ | ✅ |

Not because Magellan is smarter. Because Magellan **doesn't guess**.

### The Difference

| IDE Refactor | Magellan Workflow |
|---------------|-------------------|
| Heuristics | Spans |
| Partial semantic | Full AST |
| Silent failures | Rollback |
| "Usually works" | "Works or nothing" |
| Trust the tool | Auditable proof |

---

## For the LLM Era: This Is Existential

```
Guessing = hallucination
Silent corruption = poisoned context
No audit trail = cannot debug
```

If an LLM + Magellan + Splice + enforced workflow can't refactor safely:

**That's not your fault. That's the language telling the truth.**

Some refactors **should** fail. Some code **should** be flagged as unsafe.

### Why This Path Is The Only One

| Requirement | Why It Matters |
|-------------|----------------|
| Explicit spans | LLMs can verify before mutating |
| Atomic operations | No partial corruption |
| Rollback on failure | Bad state doesn't persist |
| Auditable history | You can debug what happened |
| Measurable correctness | Ship improvements, not vibes |

---

## The Reality

| Tool | Era | Output | Token Cost | LLM Must |
|------|-----|-------|------------|----------|
| grep | 1970s | Unstructured text | High | Parse, filter, guess |
| sed | 1970s | Text transforms | High | Hope regex is correct |
| awk | 1970s | Text processing | High | Write parsing logic |
| **Magellan** | **2020s** | **Structured facts** | **Low** | **Query, receive truth** |

## What LLMs Need

### NOT This (10,000+ tokens)
```
$ cat src/main.rs
// 500 lines of Rust code...
// LLM must: parse syntax, find symbols, track relationships
// Result: Context bloat, compaction, hallucination risk
```

### BUT This (12 tokens)
```
$ magellan query --file src/main.rs --kind Function
{"symbol":"main","kind":"Function","line":42}
{"symbol":"parse_args","kind":"Function","line":156}
{"symbol":"run","kind":"Function","line":223}
```

## The Thesis

> **"Text is more tokens. Facts are answers."**

Magellan exists to give LLMs **answers, not search results**.

### Before: LLM Cognitive Overload
```
LLM: "Find all callers of function foo"

Tool (grep): Returns 5000 lines of code
LLM: Must parse each line, track context, guess relationships
Result: 50,000 tokens, compaction, mistakes
```

### After: Structured Facts
```
LLM: "Find all callers of function foo"

Magellan: [{"from":"bar","file":"src/lib.rs","line":42},
           {"from":"baz","file":"src/main.rs","line":156}]

LLM: Receives exact answer, 50 tokens
Result: No parsing, no guessing, correct operation
```

## What This Enables

### Precise Operations
```json
// NOT: "search for foo and replace with bar"
// BUT: "replace symbol at span (1234, 1237) with bar"

{
  "operation": "rename",
  "span": {"start": 1234, "end": 1237},
  "file": "src/main.rs",
  "old": "foo",
  "new": "bar",
  "confidence": "certain"  // AST-verified
}
```

### Cross-File Intelligence
```
$ magellan callers --name Database --to_json
[
  {"file":"src/main.rs","line":"42","function":"main"},
  {"file":"src/lib.rs","line":"156","function":"connect"},
  {"file":"src/db/mod.rs","line":"23","function":"init"}
]

LLM: Knows EXACTLY where Database is used
No: Reading 50 files
No: Hoping to find all references
No: Missing cross-file calls
```

### Compact Context
```
What LLM gets:                  What LLM doesn't need:
────────────────────────────────────────────────────
{ symbol, kind, location }      10,000 lines of code
{ caller, callee, span }        Function implementations
{ file, hash, timestamp }      Comments, whitespace
{ references: [...] }          Irrelevant context
```

## The Unix Philosophy, Updated

```
1970: "Write programs to handle text streams."
2020: "Write programs to handle STRUCTURED FACTS."

Magellan: | Parse → Graph | Query → JSON |
Old Unix:  | Grep → Text  | Awk → Hope |
```

## For codemcp / LLM Agents

### Without Magellan
```
Agent: "Rename function foo to bar"
1. grep -r "foo" . → 5000 results
2. Filter manually → 200 real calls
3. Hope for no string literals
4. Apply sed → Cross fingers
5. Run tests → Fix misses
```

### With Magellan
```
Agent: "Rename function foo to bar"
1. magellan refs --name foo --to_json
   → [{"file":"src/main.rs","line":42,"span":[1234,1237]}, ...]
2. magellan rename --name foo --to bar --verify
   → Applied 142 refs, rejected 2 (shadowing)
3. Done. Tests pass.
```

## The Compactness Advantage

| Query | grep Output | Magellan Output | Token Savings |
|-------|-------------|-----------------|---------------|
| "Functions in file" | ~500 lines | 3 JSON objects | 98% |
| "Callers of X" | ~200 lines | 5 JSON objects | 95% |
| "All symbols" | ~5000 lines | 150 JSON objects | 97% |
| "Rename impact" | ~1000 lines | 1 diff summary | 99% |

## This Changes the Game

When LLMs receive **structured facts** instead of **unstructured text**:

- ✅ Less context bloat
- ✅ Fewer hallucinations
- ✅ More precise operations
- ✅ Faster responses
- ✅ Verifiable correctness

## The Goal

> **"Make LLMs stop guessing. Make them stop reading.**
> **Give them facts. Give them structure. Give them truth."**

---

## Manifesto

1. **Text is the enemy** of token-efficient LLM operations
2. **Facts are answers** that don't require parsing
3. **Graph is intelligence** that captures relationships
4. **Structure is compact** - JSON < Source Code
5. **Verification is truth** - AST-based > Text-based

Magellan = **SQLiteGraph + TreeSitter = Facts for LLMs**

---

*Created: 2025-12-30*

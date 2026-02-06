# Path-Aware Code Intelligence Engine (LogicGraph)

**Status:** Design Phase
**Discussion Source:** ChatGPT conversation
**Date:** 2026-01-30
**Phase:** Phase 13 - Logic Graph Layer

---

## The Missing Layer

Between Magellan (structure) and SQLiteGraph (memory), there is a gap:

- Magellan tells us **where** things are (structure)
- Embeddings tell us **what things look like** (similarity)
- Nothing tells us **how code behaves** (paths, proofs, counterexamples)

This is not a search tool. This is not an embedding tool.

This is a **Control-Flow & Logic Graph Engine**.

---

## Thesis Statement

> LLMs are excellent at reasoning. They are terrible at discovery.
>
> The LLM should explain why a path is wrong — not invent paths that might exist.
>
> Paths must be materialized as data first. Then the LLM can reason about them.

---

## Non-Negotiable Requirements

### Inputs

| Source | Provides |
|--------|----------|
| Magellan AST/HIR | Control flow structure, branches, returns |
| llmgrep | Symbol resolution, call graph edges |
| SQLiteGraph | Project-scoped persistence |

### Outputs

| Output | Description |
|--------|-------------|
| **Paths** | Explicit control-flow paths through code |
| **Proofs** | Mathematical proof of reachability/unreachability |
| **Counterexamples** | Concrete violations of invariants |

**Not opinions. Not probabilities. Graph facts.**

---

## Core Capabilities

### 1. Control-Flow Paths

**Build:**
- Per-function CFG (Control Flow Graph)
- Interprocedural CFG (call-aware)
- Path enumeration with:

| Tracked | Meaning |
|---------|---------|
| branches | conditional forks |
| early returns | exit points |
| error paths | exception/error handling |
| fallthroughs | default continuations |

> LLM cannot infer this reliably. This must be explicit.

### 2. Call Chains

**Forward:** "Who can be called from here?"
**Backward:** "Who can reach this?"

**Method:** Graph traversal only. Not grep. Not text.

```
NOT: rg "function_name" → fragile to renaming
YES: call_graph.traverse_from(node_id) → structure-aware
```

### 3. Duplication Detection

Not "similar text" — detect **identical behavior**:

| Detects | Method |
|---------|--------|
| Identical CFG subgraphs | Graph isomorphism |
| Same call + branch shape | Structural comparison |
| Same side effects, different names | Side-effect analysis |

**Catches:**
- Copy-paste bugs
- Diverging logic
- Accidental forks

> Embeddings alone cannot do this.

### 4. Unreachable Code

**Mechanical, not probabilistic:**

```
1. Start from entry points
2. Traverse CFG + call graph
3. Mark reachable nodes
4. Anything unvisited = unreachable (PROVEN)
```

> LLM should never "guess" this.

### 5. "Wrong Path" Analysis

A path is **wrong** if it:

- Contradicts an invariant
- Bypasses required checks
- Converges unexpectedly
- Violates expected dominance (e.g., error handling skipped)

**Requires:** Path semantics, not similarity.

---

## The Analyzer Pipeline

### Phase 1 — Graph Construction (Machine)

```
AST → CFG → Call Graph → Path Graph
```

**No LLM here. Pure computation.**

### Phase 2 — Graph Traversal (Machine)

```
enumerate all valid paths
  → prune impossible paths
  → tag: normal | error | degenerate
```

**Output:** Path IDs, not text.

### Phase 3 — Structural Verification (Machine)

```
llmgrep: confirms symbols exist, confirms calls are real
Magellan: confirms ownership, confirms structure
```

**If this fails → stop. LLM is not allowed to speak.**

### Phase 4 — Semantic Comparison (Optional, Bounded)

Embeddings are used ONLY to answer:

> "Have we seen a path like this before?"

**Examples:**
- Duplicated error logic
- Repeated state machine transitions
- Similar kernel math flow

**Embeddings never decide correctness.**

### Phase 5 — LLM Reasoning (Last, Constrained)

Now the LLM is allowed to:

- Explain why a path is wrong
- Suggest refactors
- Propose consolidation
- Describe risk

**But only with:**
- Path ID
- CFG proof
- Reachability proof

> The LLM cannot invent paths.

---

## Why This Is LLM-Optimal

From the LLM's perspective:

| Don't Need | Do Need |
|------------|---------|
| Raw files | Explicit paths |
| Full embeddings | Explicit proofs |
| To discover structure | Bounded choices |

This turns the LLM into:
- **Reasoning layer**, not discovery engine

That's exactly where LLMs are strong.

---

## Naming

### Don't Call It

- ❌ static analysis
- ❌ linting
- ❌ semantic search

### Call It What It Is

- ✅ Path-Aware Code Intelligence Engine
- ✅ LogicGraph / PathGraph (internal)

**Signals:** It's about behavior, not syntax.

---

## Critical Insight

> You do not need to embed more code.
>
> You need to:
> - materialize paths
> - persist behavior
> - prove existence
>
> Once paths exist as data:
> - duplication becomes obvious
> - unreachable code becomes trivial
> - "wrong logic" becomes explainable
>
> The LLM then adds value, not noise.

---

## Implementation Considerations

### Data Model

```sql
-- Path Graph Tables
CREATE TABLE cfg_nodes (
    id INTEGER PRIMARY KEY,
    function_id INTEGER,
    node_type TEXT, -- entry, exit, branch, merge, call
    byte_start INTEGER,
    byte_end INTEGER
);

CREATE TABLE cfg_edges (
    id INTEGER PRIMARY KEY,
    from_node INTEGER,
    to_node INTEGER,
    edge_type TEXT, -- branch_true, branch_false, fallthrough, exception
    condition_id INTEGER -- reference to condition expression
);

CREATE TABLE paths (
    id INTEGER PRIMARY KEY,
    entry_node INTEGER,
    exit_node INTEGER,
    path_type TEXT, -- normal, error, degenerate
    is_reachable BOOLEAN,
    proof_hash TEXT
);

CREATE TABLE path_steps (
    path_id INTEGER,
    step_order INTEGER,
    node_id INTEGER,
    edge_id INTEGER,
    PRIMARY KEY (path_id, step_order)
);
```

### MCP Tool Interface

```typescript
// Path Query Tools
get_cfg(function_name: string): CFG
get_paths(function_name: string, filter: PathFilter): Path[]
prove_reachability(from: string, to: string): Proof
find_unreachable(entry_point: string): Node[]

// Call Graph Tools
get_call_chain(from: string, direction: 'forward'|'backward'): Symbol[]
find_common_ancestors(symbols: string[]): Symbol[]
get_dominance_tree(function_name: string): DominanceTree

// Duplication Detection
find_cfg_duplicates(threshold: number): DuplicatePair[]
find_behavioral_clones(): CloneGroup[]

// Path Analysis
validate_path(path_id: string): ValidationResult
find_violating_paths(invariant: string): Path[]
counterexample_invariant(invariant: string): Counterexample
```

---

## Relationship to Existing Tools

| Tool | Responsibility | Does NOT |
|------|---------------|----------|
| Magellan | Structure, spans | Paths, behavior |
| llmgrep | Symbol resolution | Path reasoning |
| Embeddings | Similarity | Correctness |
| **LogicGraph** | **Paths, proofs, behavior** | **Structure discovery** |

---

## Development Phases

### Phase 1: CFG Construction
- Parse AST from Magellan
- Build per-function CFG
- Persist to SQLiteGraph

### Phase 2: Path Enumeration
- Implement path enumeration algorithm
- Tag path types (normal, error, degenerate)
- Store paths with proofs

### Phase 3: Call Graph Integration
- Merge with llmgrep call data
- Build interprocedural CFG
- Implement dominance analysis

### Phase 4: Analysis Algorithms
- Unreachable code detection
- CFG-based duplication detection
- Path validation

### Phase 5: MCP Integration
- Expose tools via MCP
- LLM integration for explanation
- Testing with real codebases

---

## Summary

| Question | Answer |
|----------|--------|
| Do we need another tool? | ✅ Yes |
| Can embeddings do this? | ❌ No |
| Should LLM discover paths? | ❌ No |
| Is graph traversal first? | ✅ Yes |
| Is LLM reasoning last? | ✅ Yes |

> You are designing this correctly — in a way that most tools don't,
> because they optimize for demos instead of truth.

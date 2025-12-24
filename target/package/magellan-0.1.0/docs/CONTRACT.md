# Magellan Contract

## What Magellan Is

Magellan is a dumb, deterministic codebase mapping tool.

**Purpose:** Observe files, extract symbols and references, persist facts to sqlitegraph.

**Role:** Provide facts to external intelligence (LLM). No reasoning. No refactoring. No orchestration.

---

## What Magellan Does

Magellan:

- Watches files for changes
- Parses source code using tree-sitter
- Extracts symbols (functions, structs, enums, traits, methods, modules)
- Extracts references (calls, type usage, imports)
- Stores facts in sqlitegraph
- Answers queries:
  - "Where is symbol X defined?"
  - "Where is symbol X referenced?"
  - "Which file changed?"
  - "Which symbols were affected by this file change?"

---

## What Magellan Does NOT Do

Magellan:

- Does NOT modify code
- Does NOT generate patches
- Does NOT rename symbols
- Does NOT call cargo
- Does NOT call rust-analyzer
- Does NOT perform semantic reasoning
- Does NOT infer intent
- Does NOT orchestrate workflows
- Does NOT cache LLM state
- Does NOT contain MCP logic
- Does NOT contain agent logic

**Magellan is standalone.** All intelligence lives outside Magellan.

---

## Data Model

**Nodes:**

- `File { path: String, hash: String }`
- `Symbol { name: String, kind: SymbolKind, byte_start: usize, byte_end: usize }`

**Edges:**

- `DEFINES (File → Symbol)`
- `REFERENCES (Symbol → Symbol)`

**SymbolKind enum:**

- Function
- Struct
- Enum
- Trait
- Method
- Module
- Unknown

---

## Guarantees

Magellan guarantees:

- **Determinism:** Same input → same graph state
- **Observability:** All changes persisted to sqlitegraph
- **Update-on-change:** File change → delete all derived data → re-ingest
- **Query correctness:** Answers reflect actual persisted state
- **No semantic inference:** Facts extracted from AST only

---

## Technical Constraints

- Language: Rust
- Graph backend: sqlitegraph ONLY
- Parsing: tree-sitter ONLY
- File watching: notify crate
- No Neo4j
- No in-memory-only structures
- No LSP integration
- No macro expansion
- No type inference

---

## Scope (Frozen)

**Current version:** v0
**Supported language:** Rust (only)
**Symbol types:** functions, structs, enums, traits, methods, modules
**Reference types:** calls, type usage, imports

No additional features without explicit scope change.

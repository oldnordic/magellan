# Context: LogicGraph/PathGraph Design

**Created:** 2026-01-30
**Purpose:** Regain context for LogicGraph/PathGraph development

---

## Quick Context

You are designing a **Path-Aware Code Intelligence Engine** (LogicGraph/PathGraph) that integrates with Magellan.

### What It Does

- **Materializes paths** as data (not embeddings, not text)
- **Provides proofs** of reachability/unreachability
- **Detects duplication** via CFG isomorphism (not text similarity)
- **Finds unreachable code** via graph traversal (not guessing)

### What It Does NOT Do

- Static analysis/linting
- Semantic search
- Symbol discovery (Magellan does this)

---

## Database Decision

**Extend `codegraph.db`** — NOT separate database.

**Why?**
- Atomic updates: When file changes → delete symbol + delete paths + recompute in ONE transaction
- JOIN queries work: `SELECT * FROM graph_entities ge JOIN control_flow_paths p ON p.function_id = ge.id`
- Single source of truth
- Magellan already extends the DB (edges, labels, chunks, HNSW)

---

## Proposed Schema Extension

```sql
-- CFG Nodes (per function)
CREATE TABLE cfg_nodes (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id  INTEGER NOT NULL,     -- References graph_entities.id
    node_type    TEXT NOT NULL,        -- 'entry', 'exit', 'branch', 'merge', 'call', 'return'
    byte_start   INTEGER,
    byte_end     INTEGER
);

CREATE TABLE cfg_edges (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    from_node_id INTEGER NOT NULL,
    to_node_id   INTEGER NOT NULL,
    edge_type    TEXT NOT NULL,        -- 'branch_true', 'branch_false', 'fallthrough', 'exception'
    data         TEXT NOT NULL
);

CREATE TABLE control_flow_paths (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id   INTEGER NOT NULL,
    path_hash     TEXT NOT NULL,
    entry_node    INTEGER NOT NULL,
    exit_node     INTEGER NOT NULL,
    path_type     TEXT NOT NULL,        -- 'normal', 'error', 'degenerate'
    is_reachable  BOOLEAN NOT NULL
);

CREATE TABLE path_steps (
    path_id      INTEGER NOT NULL,
    step_order   INTEGER NOT NULL,
    node_id      INTEGER NOT NULL,
    edge_id      INTEGER,
    PRIMARY KEY (path_id, step_order)
);

CREATE TABLE call_chain_blast_zone (
    changed_function   TEXT NOT NULL,
    affected_function TEXT NOT NULL,
    depth             INTEGER NOT NULL,
    is_error_path     BOOLEAN NOT NULL,
    proof_id          INTEGER
);
```

---

## Key Files

- **This project:** `/home/feanor/Projects/magellan/`
- **Related design doc:** `docs/PATH_AWARE_CODE_INTELLIGENCE_DESIGN.md`
- **Architecture reference:** `docs/MAGELLAN_ARCHITECTURE.md`
- **Codemcp integration:** `/home/feanor/Projects/codemcp/`

---

## Next Steps

1. **Phase 1:** CFG Construction
   - Parse AST from Magellan's tree-sitter data
   - Build per-function CFG
   - Persist to `codegraph.db`

2. **Phase 2:** Path Enumeration
   - Implement path enumeration algorithm
   - Tag path types (normal, error, degenerate)

3. **Phase 3:** MCP Integration
   - Expose tools via MCP
   - LLM gets Path IDs + proofs to reason about

---

## CLI Commands (Proposed)

```bash
magellan paths --db .codemcp/codegraph.db --function run_init
magellan unreachable --db .codemcp/codegraph.db --entry main
magellan blast-zone --db .codemcp/codegraph.db --symbol run_init
magellan cfg --db .codemcp/codegraph.db --function run_init
```

---

## Key Insight

> "You do not need to embed more code. You need to:
> - materialize paths
> - persist behavior
> - prove existence"

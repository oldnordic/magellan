# Magellan Temporal Snapshots And GNN Vulnerability Scan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** First harden Magellan's bounded-output and CFG foundations, then add a sqlitegraph-only temporal storage layer, and finally build a graph-native vulnerability scanning pipeline on top of that temporal graph, with CVE-backed corpus ingestion deferred to the final phase.

**Architecture:** Keep Magellan's current single-snapshot symbol/reference/call/CFG extraction pipeline, but first make that pipeline dependable and cheap for agents to query. Add bounded-output/token-budget support to Magellan's high-level context commands, verify and repair the AST-derived CFG path that Mirage depends on, then persist explicit repository snapshots and per-snapshot graph facts in the same SQLite database. Build temporal queries and barcode-style persistence analysis first; then layer deterministic candidate generation, validation bookkeeping, and a small embedded GNN over the temporal/static graph. Do not resurrect the removed GeometricDB/4D path.

**Tech Stack:** Rust, sqlitegraph, rusqlite, tree-sitter, git2, Magellan side tables, Mirage CFG/ICFG output, optional later local sandbox runner. For Rust, AST-derived CFG remains the owned baseline. Higher-fidelity MIR-backed extraction is a future optional provider, not a phase-1 dependency.

---

## Current State Summary

- Magellan already stores current graph facts in sqlitegraph tables (`graph_entities`, `graph_edges`) plus side tables (`cfg_blocks`, `cfg_edges`, `code_chunks`, `ast_nodes`, `source_documents`, `candidate_facts`, `telemetry_events`).
- `CodeGraph::index_file()` in `src/graph/ops.rs` is a current-state ingest pipeline. It deletes stale facts for one file, then inserts current facts.
- `magellan_meta.project_metadata` stores manifest-derived JSON from `src/manifest.rs`.
- `candidate_facts` and `source_documents` already provide a useful evidence/review substrate for future vulnerability findings.
- The legacy 4D CFG coordinate path has been removed and must not be used as a temporal foundation.
- `mirage` currently consumes Magellan CFG rows directly from `cfg_blocks`, so Mirage compatibility must be treated as a prerequisite whenever Magellan CFG storage changes.

## Non-Goals

- Do not add any transformer-first architecture to the detection core.
- Do not use external APIs or internet-backed enrichment in the first implementation phases.
- Do not depend on CVE ingestion for the initial temporal graph or local vulnerability scanner.
- Do not change Magellan's current symbol/reference/call extraction semantics unless required for stable temporal identity.
- Do not assume the current AST-derived CFG path is correct until it is explicitly verified against Mirage's needs.
- Do not make the first Rust implementation depend on unstable rustc/MIR internals.

## Hard Constraints

- SQLite `.db` remains the supported primary storage path.
- sqlitegraph remains the graph substrate; temporal state must be expressed in sqlitegraph-compatible tables and queries.
- Existing single-snapshot commands (`find`, `refs`, `status`, `chunks`, `cfg`, etc.) must keep working.
- New temporal features must be additive, not destructive to current local DBs.
- Existing DBs with old `cfg_blocks` legacy columns must remain readable.
- Mirage's `cfg`/`icfg` workflows must continue to work against Magellan-produced `cfg_blocks` before temporal work proceeds.
- Rust AST-derived CFG must be good enough to support temporal queries, deterministic candidate generation, and local ranking before any MIR upgrade is attempted.

## Phased Delivery Order

1. Bounded-output/token-budget support for Magellan context/navigation commands
2. Verify and repair the AST-derived CFG pipeline and Mirage compatibility
3. Temporal snapshot substrate
4. Stable identity across snapshots
5. Temporal sweep and barcode queries
6. Deterministic vulnerability candidate generation
7. Validation/evidence tables and workflows
8. Embedded GNN scorer and path beam search
9. CVE-backed corpus ingestion and internet-assisted seeding

---

### Task 1: Lock The Temporal Architecture

**Files:**
- Modify: `docs/MAGELLAN_ARCHITECTURE.md`
- Modify: `docs/INVARIANTS.md`
- Modify: `CHANGELOG.md`
- Create: `docs/MAGELLAN_TEMPORAL_GNN_PLAN.md`

- [ ] Document that Magellan is a current-state graph plus side tables today, and that temporal support will be implemented with explicit repository snapshot tables rather than implicit coordinate fields.
- [ ] Add invariants for temporal mode:
  - snapshot identity is explicit
  - stable symbol identity is separate from SQLite entity IDs
  - current-state queries stay supported
  - historical queries never mutate current facts
- [ ] Record that CVE seeding is a deferred phase requiring networked/source-backed workflows.

### Task 2: Add Bounded-Output And Token-Budget Controls

**Files:**
- Modify: `src/context/*.rs`
- Modify: `src/navigate*.rs`
- Modify: `src/cli.rs`
- Modify: `src/cli/parsers/*.rs`
- Modify: `docs/MAGELLAN_ARCHITECTURE.md`
- Modify: `MANUAL.md`
- Test: `tests/*context*`

- [ ] Add a user-facing token/size budget control for Magellan's high-context commands.
- [ ] Start with commands that agents will actually use during implementation:
  - `navigate`
  - `context symbol`
  - `context impact`
  - `context summary`
- [ ] Support a deterministic budget model such as:
  - `--token-budget <N>`
  - `--detail concise|normal|deep`
  - `--concise` as a preset, not a separate ad hoc path
- [ ] Truncation order must be stable and documented.
- [ ] Output must remain valid JSON when JSON modes are requested.
- [ ] This task is a prerequisite because the later temporal/GNN work will otherwise burn too much model context during implementation and review.

### Task 3: Verify And Repair AST-Derived CFG Output

**Files:**
- Modify: `src/graph/cfg_extractor.rs`
- Modify: `src/graph/cfg_edges_extract/*.rs`
- Modify: `src/graph/cfg_ops.rs`
- Modify: `tests/*cfg*`
- Modify: `../mirage/src/storage/sqlite_backend.rs`
- Modify: `../mirage/src/storage/operations.rs`
- Modify: `../mirage/src/storage/mirage_db.rs`
- Test: `../mirage/tests/*`

- [ ] Treat Magellan CFG correctness as a prerequisite, not an assumption.
- [ ] Verify current AST-derived CFG output against real Mirage consumers:
  - block ordering
  - edge typing
  - branch representation
  - loop/back-edge handling
  - `cfg_condition` filtering
- [ ] Treat AST-derived CFG as the required Rust baseline. Do not block this phase on MIR integration.
- [ ] Repair Mirage's SQLite readers so they no longer depend on removed legacy `coord_*` columns.
- [ ] Add regression tests that prove:
  - Magellan writes usable CFG rows
  - Mirage can load them
  - path enumeration over those rows still works
- [ ] Keep room for a future provider model where Rust MIR, clang CFG, Soot, or other external analyzers can populate the same `cfg_blocks` / `cfg_edges` tables without changing downstream temporal/GNN logic.
- [ ] Do not start temporal work until this layer is trustworthy, because later barcode/GNN work depends on CFG quality.

### Task 4: Add Repository Snapshot Tables

**Files:**
- Modify: `src/graph/db_compat.rs`
- Modify: `src/migrate_cmd.rs`
- Modify: `tests/migration_tests.rs`
- Modify: `src/graph/mod.rs`
- Test: `tests/migration_tests.rs`

- [ ] Add schema bootstrap and migration support for:
  - `repo_snapshots`
  - `repo_snapshot_parents`
  - `file_versions`
  - `symbol_versions`
  - `edge_versions`
- [ ] Keep schema naming boring and queryable; prefer explicit columns over JSON blobs for keys used in joins and filters.
- [ ] Record enough commit metadata for deterministic history traversal:
  - repo root
  - commit OID
  - parent OIDs
  - author time
  - commit time
  - tree OID
  - commit message summary
- [ ] Make v18 the first schema version that introduces true temporal support.
- [ ] Add migration tests for:
  - fresh DB creation
  - upgrade from existing v17 DBs
  - re-opening older DBs without losing current-state behavior

### Task 5: Introduce Stable Symbol Identity

**Files:**
- Modify: `src/graph/schema.rs`
- Modify: `src/graph/symbol_lookup.rs`
- Modify: `src/graph/symbol_index.rs`
- Modify: `src/graph/ops.rs`
- Modify: `src/common.rs`
- Test: `src/graph/ops.rs`
- Test: `tests/span_tests.rs`

- [ ] Define a stable temporal symbol key separate from transient `graph_entities.id`.
- [ ] Base the first implementation on:
  - normalized repo-relative path
  - canonical FQN
  - normalized kind
  - span/body hash fallback
- [ ] Store that key in current symbol data and in `symbol_versions`.
- [ ] Ensure re-indexing of unchanged symbols yields the same stable key.
- [ ] Add regression tests for:
  - unchanged function across snapshots
  - moved line span but same semantic symbol
  - rename/refactor fallback behavior

### Task 6: Add Snapshot Ingestion Primitives

**Files:**
- Modify: `src/graph/files.rs`
- Modify: `src/graph/ops.rs`
- Modify: `src/indexer/watch.rs`
- Modify: `src/service/mod.rs`
- Modify: `src/service/meta_db.rs`
- Create: `src/temporal/mod.rs`
- Create: `src/temporal/snapshots.rs`
- Create: `src/temporal/ingest.rs`
- Test: `tests/watch_integration.rs`

- [ ] Keep `index_file()` as the current-state ingest path.
- [ ] Add a separate snapshot ingest path that can index a checked-out worktree or sampled commit into temporal tables without replacing the current live graph.
- [ ] Avoid trying to make sqlitegraph entities themselves versioned in the first pass. Persist temporal joins in the new snapshot tables first.
- [ ] Add APIs to:
  - register a snapshot
  - diff file versions against parent
  - map current extracted symbols/edges to snapshot rows
- [ ] Ensure snapshot ingest can skip unchanged files by content hash.

### Task 7: Build `temporal-sweep`

**Files:**
- Create: `src/temporal_sweep_cmd.rs`
- Modify: `src/main.rs`
- Modify: `src/cli.rs`
- Modify: `src/cli/parsers/*.rs`
- Create: `src/temporal/worktrees.rs`
- Test: `tests/temporal_sweep_tests.rs`

- [ ] Implement `magellan temporal-sweep --db <db> --repo <path>`.
- [ ] Use detached temporary worktrees, not in-place checkout.
- [ ] Support sampling policies:
  - every commit
  - every N commits
  - tags only
  - merge commits only
  - date window
- [ ] For each sampled commit:
  - create worktree
  - ingest snapshot
  - persist file/symbol/edge versions
  - tear down worktree
- [ ] Emit machine-readable output summarizing number of commits, snapshots, files, and symbol versions ingested.

### Task 8: Add Temporal Query Commands

**Files:**
- Create: `src/temporal_query_cmd.rs`
- Modify: `src/main.rs`
- Modify: `src/cli.rs`
- Create: `src/temporal/query.rs`
- Test: `tests/temporal_query_tests.rs`

- [ ] Implement initial commands:
  - `magellan temporal-status`
  - `magellan as-of --commit <oid> <existing query>`
  - `magellan temporal-barcode --symbol <stable-id>`
  - `magellan temporal-barcode --scc`
- [ ] First barcode outputs should cover:
  - symbol lifetime
  - edge lifetime
  - SCC lifetime
- [ ] Keep output deterministic and local; no probabilistic ranking in this phase.

### Task 9: Add SCC And Persistence Analysis

**Files:**
- Modify: `src/graph/algorithms.rs`
- Create: `src/temporal/persistence.rs`
- Create: `src/temporal/scc.rs`
- Test: `tests/temporal_query_tests.rs`

- [ ] Compute per-snapshot SCCs from call/import/reference subgraphs.
- [ ] Persist SCC lineage keys across adjacent snapshots using stable members rather than raw row IDs.
- [ ] Expose:
  - born-at commit
  - died-at commit
  - lifetime length
  - churn count for recurring SCCs
- [ ] This phase supplies the temporal stability signal needed later by the GNN and by user-facing architecture queries.

### Task 10: Add Deterministic Vulnerability Candidate Generation

**Files:**
- Create: `src/vuln/mod.rs`
- Create: `src/vuln/candidates.rs`
- Create: `src/vuln/patterns.rs`
- Modify: `src/main.rs`
- Modify: `src/cli.rs`
- Test: `tests/vuln_candidate_tests.rs`

- [ ] Implement `magellan vuln-scan --db <db>` as a deterministic rule/pattern layer first.
- [ ] Inputs:
  - symbol graph
  - call graph
  - CFG blocks/edges
  - optional Mirage path exports
- [ ] Initial candidate classes:
  - command injection sinks with untrusted flow candidates
  - path traversal candidates
  - unsafe deserialization candidates
  - shell/process misuse candidates
  - authz bypass shape candidates
- [ ] Persist findings into a dedicated table, not directly into `candidate_facts`.
- [ ] Also create bridge rows into `candidate_facts` when the finding is ready for review.

### Task 11: Add Validation And Evidence Tables

**Files:**
- Modify: `src/graph/db_compat.rs`
- Create: `src/vuln/validation.rs`
- Create: `src/vuln/evidence.rs`
- Modify: `src/service/candidates.rs`
- Test: `tests/vuln_validation_tests.rs`

- [ ] Add tables for:
  - `vuln_candidates`
  - `vuln_evidence`
  - `validation_runs`
  - `training_events`
- [ ] Distinguish statuses:
  - `pending`
  - `heuristic_only`
  - `validated`
  - `rejected`
  - `uncertain`
- [ ] Store path evidence, symbol evidence, snapshot context, and temporal persistence stats for each candidate.
- [ ] Do not add exploit generation yet; just shape the storage and status lifecycle cleanly.

### Task 12: Add Embedded GNN Storage And Runtime

**Files:**
- Create: `src/gnn/mod.rs`
- Create: `src/gnn/model.rs`
- Create: `src/gnn/features.rs`
- Create: `src/gnn/infer.rs`
- Create: `src/gnn/storage.rs`
- Modify: `src/graph/db_compat.rs`
- Test: `tests/gnn_tests.rs`

- [ ] Keep the first model tiny and CPU-local.
- [ ] Add schema for:
  - `gnn_models`
  - `node_embeddings`
  - `path_scores`
  - `gnn_checkpoints`
- [ ] First feature set should come from existing local facts:
  - symbol kind
  - fan-in/fan-out
  - CFG block/edge counts
  - conditional density
  - temporal stability/lifetime
  - caller/callee neighborhood stats
- [ ] Support pure inference first; training can begin with replayed local labeled events later.

### Task 13: Add Structural Beam Search Over CFG/ICFG Paths

**Files:**
- Create: `src/vuln/beam.rs`
- Modify: `src/vuln/mod.rs`
- Modify: `src/gnn/infer.rs`
- Test: `tests/vuln_beam_tests.rs`

- [ ] Implement structural beam search, not text generation beam search.
- [ ] Start from candidate nodes/functions and expand top-K suspicious outgoing CFG/ICFG paths.
- [ ] Score expansions with:
  - heuristic prior
  - GNN node/path score
  - temporal persistence weight
- [ ] Output ranked suspicious paths with evidence, not just raw nodes.

### Task 14: Add Local Training Event Loop

**Files:**
- Create: `src/gnn/train.rs`
- Create: `src/vuln/training.rs`
- Test: `tests/gnn_training_tests.rs`

- [ ] Consume validated/rejected findings as local supervision.
- [ ] Record:
  - positive path patterns
  - negative path patterns
  - snapshot-local and repo-local context
- [ ] Keep the first implementation as offline local updates from stored events.
- [ ] Do not add automatic continuous training until the event schema and rollback story are stable.

### Task 15: Add User-Facing Commands

**Files:**
- Modify: `src/main.rs`
- Modify: `src/cli.rs`
- Modify: `src/cli/parsers/*.rs`
- Create: `src/vuln_scan_cmd.rs`
- Create: `src/temporal_status_cmd.rs`
- Test: `tests/status_tests.rs`

- [ ] Add commands:
  - `magellan temporal-sweep`
  - `magellan temporal-status`
  - `magellan temporal-barcode`
  - `magellan vuln-scan`
  - later: `magellan vuln-train`
- [ ] Human output should stay concise.
- [ ] JSON output should include stable IDs and snapshot identifiers so downstream local tools can reuse results.

### Task 16: Documentation And Invariants Refresh

**Files:**
- Modify: `docs/MAGELLAN_ARCHITECTURE.md`
- Modify: `docs/INVARIANTS.md`
- Modify: `MANUAL.md`
- Modify: `CHANGELOG.md`

- [ ] Update the storage section to describe:
  - current-state tables
  - temporal snapshot tables
  - vulnerability/GNN tables
- [ ] Add explicit invariants for:
  - stable temporal identity
  - additive snapshot ingest
  - no hidden dependence on legacy 4D paths
- [ ] Document that CVE seeding is not part of the initial offline implementation.

### Task 17: Deferred Internet-Backed CVE Seeding

**Files:**
- Future create: `src/vuln/cve_import.rs`
- Future docs: `docs/CVE_IMPORT.md`

- [ ] This phase is intentionally last.
- [ ] Required capabilities:
  - internet/source access
  - corpus provenance rules
  - normalization of CVE/fix metadata into local snapshot/path signatures
- [ ] Import only after the local temporal graph, candidate schema, validation schema, and training event loop already exist.
- [ ] Initial target:
  - ingest confirmed vuln/fix pairs
  - map them to stable local path/symbol/CFG signature shapes
  - seed `training_events` with high-confidence labels

---

## Key Design Decisions

- **Temporal is explicit, not implicit.**
  Current graph facts are snapshots of “now.” Time must be modeled with explicit repository snapshots and version tables, not inferred from ad hoc fields.

- **The implementation workflow itself must be token-aware.**
  Before major temporal work begins, Magellan should expose deterministic bounded-output controls so agents can query the graph without wasting context.

- **Stable identity is mandatory.**
  SQLite entity IDs are transient. Temporal analysis, barcode queries, and GNN training all require a stable symbol identity layer.

- **CFG correctness comes before temporal sophistication.**
  If Mirage cannot trust Magellan's AST-derived CFG rows, then temporal sweep, persistence analysis, and GNN ranking will all be built on bad graph structure.

- **Rust baseline first, MIR later.**
  For Rust, AST-derived CFG is the owned and portable baseline. MIR-backed CFG is a future fidelity upgrade if toolchain stability and maintenance cost become acceptable, but the first temporal/GNN implementation must not depend on rustc internals.

- **Detection core is graph-native.**
  The first learned model should be a local GNN over call/CFG/temporal structure, not a transformer.

- **Validation is a first-class data model.**
  Even before exploit automation exists, the schema must distinguish heuristic candidates from validated findings.

- **CVE seeding is not phase 1.**
  It depends on external data acquisition and should be added only after the local graph/training substrate is correct.

## Risks

- Stable symbol identity across refactors is the hardest correctness problem.
- The current Magellan→Mirage CFG handoff may still have correctness gaps and must be verified explicitly.
- Snapshot ingestion can become too expensive without hash-based skipping and sampling controls.
- Temporal SCC lineage matching can over-fragment if the lineage key is too brittle.
- GNN feature design can become noisy if built before the validation/event schema is stable.

## Exit Criteria

- bounded-output Magellan context/navigation commands can be used safely by agents under explicit budgets.
- Magellan-produced CFG rows are verified to load and behave correctly in Mirage.
- `temporal-sweep` can ingest sampled commit history into local sqlitegraph-backed snapshot tables.
- `temporal-barcode` can report lifetimes for symbols and SCCs.
- `vuln-scan` can emit deterministic candidates with stored evidence.
- Local GNN inference can rank candidates/paths using only local graph data.

---

## Addendum — Autonomous Optimization Loop

**Date:** 2026-06-21

The GNN scorer feeds a fully autonomous nightly optimization loop running on local hardware with a local LLM. No API cost per attempt.

### Loop Design

```
night starts
  GNN scans CFG patterns across all indexed projects
  → nominates candidates: function + pattern match + confidence score
  → filtered by temporal-barcode: high-churn functions prioritized
     (low-churn stable functions skipped — lower payoff)

  for each candidate:
    local LLM receives: GNN pattern label + magellan navigate context
    LLM creates git branch: optimize/<fn-name>-<pattern>
    LLM rewrites function body via splice edit
    cargo test --workspace runs
    if green:
      mirage cfg on new version → verify CFG actually simplified
      cargo bench (criterion) → record before/after timing
      dhat → record allocation delta
      git diff --stat → lines added/removed
      magellan context affected → verify no callers broken
      branch stays, flagged "validated"
    if red:
      failed candidate metadata persisted as negative training signal
      branch deleted silently

morning:
  report generated per project
```

### Per-Branch Report Schema

```
Function:     <name>  (<file>:<line>)
Pattern:      <GNN pattern label>  (confidence: X.XX)
Churn rank:   #N of M candidates (temporal-barcode)

Before:
  lines:        N
  cfg blocks:   N  (mirage)
  complexity:   N  (mirage cyclomatic)
  time:         Xns ± Yns  (criterion, n=1000)
  allocations:  N  (dhat-rs)
  peak memory:  XKB

After:
  lines:        N  (±delta)
  cfg blocks:   N  (±delta)
  complexity:   N  (±delta)
  time:         Xns ± Yns  (±% change)
  allocations:  N  (±delta)
  peak memory:  XKB (±%)

Tests:    N pass / 0 fail
Callers:  N symbols checked, 0 regressions
Branch:   optimize/<fn-name>-<pattern>  [ready for review]
```

### Measurement Stack

| Tool | Measures | Notes |
|------|----------|-------|
| `criterion` | wall time, statistical confidence | warmup + outlier detection, HTML report |
| `dhat-rs` | heap allocations, peak memory | exact counts, not estimates |
| `cargo-llvm-lines` | LLVM IR lines per function | catches monomorphization bloat |
| `mirage cfg` | CFG block count, cyclomatic complexity | before/after structural comparison |
| `git diff --stat` | lines added/removed, files touched | automatic from branch diff |
| `magellan context affected` | caller regression check | verifies no callers broken by signature/behavior change |

### Self-Improving Signal

After N nights of runs the loop accumulates a dataset:

```
(GNN pattern, CFG shape before, CFG shape after, test result, benchmark delta)
```

This becomes additional training signal — patterns that consistently produce green tests and benchmark improvements get higher confidence weights. Patterns that produce red tests get penalized. The GNN improves on the actual codebase without synthetic data.

Failed branches should not disappear completely. Persist at least:
- candidate symbol
- pattern label
- CFG shape summary before attempt
- test/benchmark failure mode
- commit and branch metadata

That negative evidence is part of the training set even when the branch itself is discarded.

### Rust CFG Reality Check

For Rust, the loop initially runs on AST-derived CFG because that is the part Magellan can own locally and keep stable across toolchains. This has known blind spots:
- macro expansion details
- async/await lowering
- `?` desugaring and some temporary/drop paths
- MIR-only reachability and cleanup edges

That is acceptable for phase 1 as long as:
- Mirage can consume the CFG deterministically
- the optimization loop treats CFG as a nomination signal, not a correctness oracle
- tests, benchmarks, and caller checks remain the actual validation gates

If a future MIR provider is added, it should normalize into the same storage model so temporal sweep, barcode analysis, candidate generation, and GNN inference do not need a second architecture.

---

## Addendum: Embedding Geometry Insights from ColBERT Regularization Research

*Added 2026-06-21 after reading: "Party is over: regularizing ColBERT models to fix efficient ANN methods" (Chaffin, LightOn 2026)*

### Direct Relevance to HopGraph

`magellan hopgraph` uses nomic-embed-text embeddings indexed via HNSW (cosine similarity). Modern embedding models — including nomic-embed-text — are highly anisotropic: mean pairwise cosine similarity ~0.8-0.95. All vectors collapse into a narrow cone.

**Problem:** HNSW partitions the embedding space to accelerate search. When all vectors are in one cone, random hyperplanes can't split the cone — every function lands in the same bucket. Candidate recall degrades without any visible error.

**Quick fix (applies now):** Mean-center embeddings at query and insert time. Subtract corpus mean vector before inserting into HNSW and before querying. The article's data shows this drops mean cosine similarity from 0.95 → ~0.001, making random projections useful. In magellan terms: store corpus mean in the hopgraph metadata table at index time; subtract before every vector comparison.

**Better fix (training-time):** STE-based regularization trains the embedding model to concentrate discriminative information into fewer effective dimensions rather than spreading noise across all 768. Not applicable to a pre-trained nomic model without fine-tuning.

### Insight for GNN Feature Geometry

The article's stable rank measurements (per-document ~5 effective dimensions, corpus-wide ~15) apply to our 13 structural features too. Complexity and cfg_blocks are correlated (~0.85 Pearson in our data). The effective dimensionality is 4-6, not 13.

**Implication for GCN layer:** The 1-layer GCN (13 → 45 features via message passing) expands feature dimensionality but doesn't concentrate discriminative information. The ColBERT result shows that *fewer* effective dimensions improve downstream projection quality. For our cross-project transfer problem, we should project DOWN, not up: PCA to 4-6 components before training, not GCN expansion.

However: the article also shows naive PCA is catastrophic (destroys 93% of quality). The model must *learn* which dimensions to concentrate into — not post-hoc compression. For us this means: when we have enough labeled data across multiple projects, train a learned dimensionality reduction (autoencoder or supervised projection) rather than applying PCA.

### Insight for the Autonomous Optimization Loop

The STE (Straight-Through Estimator) technique in the article is directly applicable to the loop:

In the optimization loop, the "discrete selection" step is: does a function pass the benchmark threshold? `benchmark_delta > speedup_threshold` is a hard 0/1 selection. Its gradient is zero (flat everywhere except the boundary). This breaks end-to-end gradient flow from "loop outcome" back to "GNN nomination weights."

STE fix: during backprop, treat the threshold comparison as identity. Let the gradient of the outcome loss flow through the threshold as if `I(benchmark_delta > threshold)` were a no-op. This enables the self-improving signal (GNN pattern, outcome) → GNN weight update to work as a true gradient signal rather than a reinforcement learning proxy.

### Stable Rank as a Quality Metric for Snapshots

The article measures per-document stable rank (how many effective dimensions each document's token set spans). Analogously, we can compute the stable rank of each git snapshot's feature matrix:

```python
# For each snapshot, compute feature matrix of functions at that point
# Compute singular values, then stable rank = (sum(sv))^2 / sum(sv^2)
# Stable rank increasing over time → code becoming more diverse/complex
# Stable rank decreasing → code converging, simplification underway
```

This gives a temporal signal orthogonal to LOC and hash churn: the geometric complexity of the codebase at each snapshot. A sudden stable rank drop is a simplification event regardless of which specific functions changed.

### Why This Works Better Than openevolve

openevolve mutates randomly and scores a fitness function — high noise, no graph grounding. This loop:
- Starts from graph evidence (GNN only nominates CFG shapes matching known-reducible patterns)
- Uses temporal data to prioritize high-value targets
- Uses real tests as the oracle, not a synthetic fitness function
- Accumulates real benchmark deltas, not proxy scores
- Runs entirely local — no cloud, no API cost per attempt
- CVE import remains deferred and clearly separated as an external-data phase.

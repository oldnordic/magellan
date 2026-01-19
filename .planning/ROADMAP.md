# ROADMAP: Magellan (v1)

## Overview

Magellan v1 is a deterministic, local-first codebase mapping CLI. The roadmap focuses on making Magellan’s outputs and identifiers **contract-grade** (schema-versioned, span-aware, stable IDs) while keeping watch mode **reliable under real editor workloads**.

**Planning depth:** comprehensive (config.json)

---

## Phases

### Phase 1 — Persistence Compatibility Baseline

**Goal:** Users can run Magellan against a chosen SQLite DB with explicit compatibility guarantees.

**Dependencies:** None

**Requirements:** DB-01, DB-02

**Plans:** 4 plans

Plans:
- [x] 01-01-PLAN.md — Pin sqlitegraph v1.0.0 (crates.io) + lockfile
- [x] 01-02-PLAN.md — Add read-only preflight + two-phase CodeGraph open
- [x] 01-03-PLAN.md — Add magellan_meta + tests for no partial mutation
- [x] 01-04-PLAN.md — Gap closure: align Phase 1 docs + verification report

**Success Criteria (observable):**
1. User can build/run Magellan with **sqlitegraph v1.0.0 pinned from crates.io**, with reproducible dependency resolution via committed `Cargo.lock` (no hidden drift).
2. When user passes `--db <FILE>`, Magellan records a schema version in the database.
3. When user opens an incompatible/older DB, Magellan refuses to proceed with a clear, deterministic error (and does not partially mutate the DB).

---

### Phase 2 — Deterministic Watch & Indexing Pipeline

**Goal:** Users can continuously index a repo via watch mode and trust it to behave deterministically under file event storms.

**Dependencies:** Phase 1

**Requirements:** WATCH-01, WATCH-02, WATCH-03, WATCH-04, WATCH-05A

**Plans:** 3 plans

Plans:
- [ ] 02-01-PLAN.md — Per-file reconcile + delete_file_facts (true idempotency; no ghost nodes)
- [ ] 02-02-PLAN.md — Debounced batching + buffer events during scan-initial (storm determinism)
- [ ] 02-03-PLAN.md — Include/exclude rules + structured diagnostics (skip reasons + per-file errors)

**Success Criteria (observable):**
1. User runs `watch` and observes a complete **scan-initial baseline** before any incremental updates are applied.
2. When user saves a file repeatedly (editor storm), Magellan coalesces the events deterministically and produces the same resulting indexed state as a clean re-run.
3. User can apply ignore/include/exclude rules and can see which files were skipped and why via structured diagnostics.
4. User can re-index the same file multiple times and the DB reflects only the latest derived facts (no ghost nodes/edges).
5. When a file cannot be read/parsed, watch continues running and the failure is captured as structured diagnostics (with deterministic stderr output for human consumption).

---

### Phase 3 — CLI Output Contract (Schema + Determinism + Stdout Discipline)

**Goal:** Users can script Magellan reliably because every command has deterministic, schema-versioned JSON output with clean stdout/stderr separation.

**Dependencies:** Phase 2

**Requirements:** OUT-01, OUT-02, OUT-03, WATCH-05B

**Success Criteria (observable):**
1. For every user-facing command, user can request JSON output (or get JSON in machine mode) that includes `schema_version` and documented fields.
2. On unchanged inputs, running the same command twice produces JSON output with deterministic ordering (no HashMap-order drift).
3. Stdout contains only machine-consumable data; logs/diagnostics never contaminate stdout (they go to stderr).

---

### Phase 4 — Canonical Span Model + Span-Aware Results

**Goal:** Users can treat Magellan’s “points into source code” as a consistent coordinate system across languages and files.

**Dependencies:** Phase 3

**Requirements:** ID-01, OUT-04

**Success Criteria (observable):**
1. Any result that points into source code includes byte offsets and line/col, with explicit range semantics (half-open) that are consistent across all commands.
2. For non-ASCII source files, user can still use returned byte ranges safely (no invalid UTF-8 slicing assumptions) and line/col mapping matches what editors display.
3. Users can compare spans across runs and see they refer to the same lexical region when the underlying file content is unchanged.

---

### Phase 5 — Stable Identity + Execution Tracking

**Goal:** Users can correlate runs and results across time using stable IDs and per-run `execution_id`.

**Dependencies:** Phase 4

**Requirements:** OUT-05, ID-02, ID-03, ID-04, DB-03

**Success Criteria (observable):**
1. Every run produces an `execution_id` that appears in CLI JSON outputs and is recorded in the database.
2. For unchanged inputs, `span_id` values remain stable across repeated runs.
3. For unchanged inputs, `symbol_id` values remain stable across repeated runs.
4. Each result payload that represents a “match” includes stable identifiers (`match_id`, `span_id`, `symbol_id`) where applicable, allowing downstream tooling to de-duplicate and diff results.

---

### Phase 6 — Query UX (Definitions, References, Call Graph, File Listing)

**Goal:** Users can query the indexed graph from the CLI with deterministic, span-aware, ID-stable results.

**Dependencies:** Phase 5

**Requirements:** QRY-01, QRY-02, QRY-03, QRY-04

**Success Criteria (observable):**
1. User can look up symbol definitions by name/kind and receive span-aware results with stable IDs.
2. User can look up references to a symbol and receive span-aware results with stable IDs.
3. User can query callers/callees and results are deterministically ordered and include stable IDs.
4. User can list symbols (and counts) for a given file/module.

---

### Phase 7 — Deterministic Exports (JSON/JSONL, DOT, CSV)

**Goal:** Users can export the graph into stable, diff-friendly formats for downstream tooling.

**Dependencies:** Phase 5

**Requirements:** EXP-01, EXP-02, EXP-03

**Success Criteria (observable):**
1. User can export a full graph snapshot to JSON/JSONL with stable IDs and deterministic ordering.
2. User can export DOT (Graphviz) for caller→callee graphs and render it with standard Graphviz tooling.
3. User can export CSV for core entities/edges (symbols, references, calls) with stable IDs suitable for spreadsheets/pipelines.

---

### Phase 8 — Validation Hooks (Pre/Post) Surfaced via JSON

**Goal:** Users can verify indexing correctness and get actionable diagnostics when invariants fail.

**Dependencies:** Phases 2, 3, 5

**Requirements:** DB-04

**Success Criteria (observable):**
1. User can enable validation and receive a pre-run manifest/checksum summary and post-run invariant results in JSON output.
2. When a post-run invariant fails, Magellan exits non-zero and returns structured diagnostics that identify the failure class.
3. Validation results are tied to an `execution_id` so the user can correlate diagnostics to a specific run.

---

### Phase 9 — Interop Export (SCIP / LSIF)

**Goal:** Users can emit interoperable index formats with documented encoding and identity rules.

**Dependencies:** Phases 4, 5, 7

**Requirements:** EXP-04

**Success Criteria (observable):**
1. User can export to SCIP and/or LSIF with documented position encoding and symbol identity rules.
2. A standard SCIP/LSIF consumer can parse the exported artifact without format errors.

---

## Progress

| Phase | Name | Status |
|------:|------|--------|
| 1 | Persistence Compatibility Baseline | Complete |
| 2 | Deterministic Watch & Indexing Pipeline | Planned |
| 3 | CLI Output Contract | Planned |
| 4 | Canonical Span Model + Span-Aware Results | Planned |
| 5 | Stable Identity + Execution Tracking | Planned |
| 6 | Query UX | Planned |
| 7 | Deterministic Exports | Planned |
| 8 | Validation Hooks | Planned |
| 9 | Interop Export (SCIP / LSIF) | Planned |

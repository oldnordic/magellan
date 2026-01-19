# Requirements: Magellan

**Defined:** 2026-01-18
**Core Value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

## v1 Requirements

### Output Contract (CLI JSON)

- [ ] **OUT-01**: Every user-facing command supports `--output json` (or defaults to JSON in machine mode) with a **schema_version** field and explicit, documented fields.
- [ ] **OUT-02**: CLI output is **deterministic**: stable ordering of arrays/records (sorted by stable keys) and no HashMap iteration order leaks.
- [ ] **OUT-03**: Enforce stdout discipline: **stdout = data only**, **stderr = logs/diagnostics**.
- [ ] **OUT-04**: Every match/result that points into source is **span-aware**, returning byte offsets and line/col with explicit range semantics.
- [ ] **OUT-05**: Every response includes stable identifiers where applicable: `execution_id`, and per-result `match_id` / `span_id` / `symbol_id`.

### Spans & Identity

- [ ] **ID-01**: Define a canonical span model (UTF-8 byte offsets with half-open ranges) and include conversion rules for line/col in outputs.
- [ ] **ID-02**: `span_id` is stable across runs for unchanged inputs and is derived deterministically from (canonical path, byte_start, byte_end, and content hash policy).
- [ ] **ID-03**: `symbol_id` is stable across runs for unchanged inputs and is derived deterministically from (language, fully-qualified symbol name policy, and defining span or content hash).
- [ ] **ID-04**: `execution_id` is generated for every run and is recorded in both outputs and the database.

### Watch / Indexing

- [x] **WATCH-01**: `watch` supports **scan-initial + then watch** as the default behavior (unless user opts out) and produces deterministic baseline indexing.
- [x] **WATCH-02**: File events are **debounced and coalesced** deterministically to handle editor storms without nondeterministic ordering.
- [x] **WATCH-03**: Support ignore/include/exclude rules (gitignore-style plus explicit CLI globs) and report skip reasons in diagnostics.
- [x] **WATCH-04**: Indexing updates are idempotent: re-indexing a file fully replaces its derived facts (no ghost nodes/edges).
- [x] **WATCH-05A**: Per-file failures do not stop watch mode; errors are recorded as **structured diagnostics** and surfaced deterministically in human-readable output (stderr).
- [ ] **WATCH-05B**: Structured watch diagnostics are exposed via the **Phase 3 JSON output contract** (schema-versioned + deterministic ordering + stdout/stderr discipline).

### Queries

- [ ] **QRY-01**: Provide definition lookup (by symbol name/kind) returning span-aware results and stable IDs.
- [ ] **QRY-02**: Provide reference lookup (by symbol) returning span-aware results and stable IDs.
- [ ] **QRY-03**: Provide callers/callees queries returning stable IDs and deterministic ordering.
- [ ] **QRY-04**: Provide file/module listing (symbols + counts) for a given file.

### Exports

- [ ] **EXP-01**: Export graph snapshot to JSON/JSONL (nodes + edges) with stable IDs and deterministic ordering.
- [ ] **EXP-02**: Export DOT (Graphviz) for caller→callee graphs and/or selected subgraphs.
- [ ] **EXP-03**: Export CSV for core entities/edges (symbols, references, calls) with stable IDs.
- [ ] **EXP-04**: Export to SCIP and/or LSIF with documented position encoding and symbol identity rules.

### Persistence & Validation

- [ ] **DB-01**: Upgrade sqlitegraph dependency to **sqlitegraph v1.0.0** (crates.io pinned; reproducible via committed `Cargo.lock`).
- [ ] **DB-02**: Record DB schema version and enforce compatibility at open time.
- [ ] **DB-03**: Add an execution log table that records every run with `execution_id`, tool version, args, root, DB path, timings, and outcome.
- [ ] **DB-04**: Add validation hooks: pre-run input manifest/checksums and post-run invariants (no orphan edges, expected edge types, etc.), with output surfaced via JSON.

## v2 Requirements

### Multi-workspace / Scaling

- **SCALE-01**: Multi-root workspaces (index multiple roots in one run/DB).
- **SCALE-02**: Cross-repo graphs and DB merge workflows.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Semantic analysis / type checking | Magellan is “facts only” by design |
| LSP server / IDE language features | CLI-only tool; no server responsibility |
| Async runtime / background thread pools | Determinism + simplicity over concurrency |
| Web APIs / network services | Local developer tool; offline by default |
| Automatic database cleanup | DB lifecycle is explicit and user-controlled |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DB-01 | Phase 1 | Satisfied |
| DB-02 | Phase 1 | Satisfied |
| WATCH-01 | Phase 2 | Complete |
| WATCH-02 | Phase 2 | Complete |
| WATCH-03 | Phase 2 | Complete |
| WATCH-04 | Phase 2 | Complete |
| WATCH-05A | Phase 2 | Complete |
| WATCH-05B | Phase 3 | Pending |
| OUT-01 | Phase 3 | Pending |
| OUT-02 | Phase 3 | Pending |
| OUT-03 | Phase 3 | Pending |
| ID-01 | Phase 4 | Pending |
| OUT-04 | Phase 4 | Pending |
| OUT-05 | Phase 5 | Pending |
| ID-02 | Phase 5 | Pending |
| ID-03 | Phase 5 | Pending |
| ID-04 | Phase 5 | Pending |
| DB-03 | Phase 5 | Pending |
| QRY-01 | Phase 6 | Pending |
| QRY-02 | Phase 6 | Pending |
| QRY-03 | Phase 6 | Pending |
| QRY-04 | Phase 6 | Pending |
| EXP-01 | Phase 7 | Pending |
| EXP-02 | Phase 7 | Pending |
| EXP-03 | Phase 7 | Pending |
| DB-04 | Phase 8 | Pending |
| EXP-04 | Phase 9 | Pending |

**Coverage:**
- v1 requirements: 25 total
- Mapped to phases: 25
- Unmapped: 0 ✓

---
*Requirements defined: 2026-01-18*
*Last updated: 2026-01-18 after initial definition*

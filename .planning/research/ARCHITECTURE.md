# Architecture Research

**Domain:** Deterministic codebase mapping / code graph indexing (CLI + watcher + SQLite)
**Researched:** 2026-01-18
**Confidence:** MEDIUM

> Scope: How code graph / codebase mapping systems are typically structured, and how to enhance an existing indexing + watch architecture with **deterministic structured output**, **stable span IDs**, **validation hooks**, and **execution logs**.

## Standard Architecture

### System Overview

In practice, “code graph” systems are a pipeline with a durable store and one or more output formats. The pipeline is typically designed to support **(a) initial full scan** and **(b) incremental updates** from a watcher.

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              CLI / API Layer                             │
│  (commands, config, JSON output, exit codes, reproducible runs)           │
│   ┌────────────┐  ┌────────────┐  ┌──────────────┐  ┌────────────────┐   │
│   │ index      │  │ watch      │  │ query/export │  │ verify/doctor  │   │
│   └─────┬──────┘  └─────┬──────┘  └──────┬───────┘  └───────┬────────┘   │
├─────────┴───────────────┴────────────────┴──────────────────┴───────────┤
│                           Orchestration Layer                             │
│  job planning, batching, determinism policy, retries, cancellation         │
│   ┌─────────────────────────────┐     ┌──────────────────────────────┐    │
│   │ Indexing Pipeline Scheduler │     │ Watch Event Router/Debouncer │    │
│   └──────────────┬──────────────┘     └───────────────┬──────────────┘    │
├──────────────────┴────────────────────────────────────┴──────────────────┤
│                        Analysis / Extraction Layer                         │
│  file discovery → parse → extract symbols/spans → normalize → link edges   │
│   ┌───────────────┐  ┌───────────────┐  ┌─────────────────────────────┐   │
│   │ File walker   │→ │ Tree-sitter    │→ │ Language adapter/extractor  │   │
│   │ (ignore, fs)  │  │ parse/queries  │  │ (symbols, refs, structure)  │   │
│   └───────────────┘  └───────────────┘  └──────────────┬──────────────┘   │
│                                                         │
│                                            ┌────────────▼─────────────┐   │
│                                            │ Determinism & ID service │   │
│                                            │ (stable IDs, sorting,    │   │
│                                            │ encoding, canonical paths)│   │
│                                            └────────────┬─────────────┘   │
├────────────────────────────────────────────┴──────────────────────────────┤
│                      Persistence + Outputs + Validation                    │
│   ┌──────────────┐  ┌───────────────────┐  ┌──────────────────────────┐   │
│   │ SQLite graph │  │ Export/JSON writer│  │ Validators + Hook Runner  │   │
│   │ (facts/edges)│  │ (deterministic)   │  │ (invariants, compilers)   │   │
│   └──────┬───────┘  └─────────┬─────────┘  └─────────────┬────────────┘   │
│          │                    │                          │
│   ┌──────▼────────┐   ┌───────▼────────┐        ┌────────▼─────────┐      │
│   │ Execution log │   │ Snapshots       │        │ Reports/errors   │      │
│   │ (audit trail) │   │ (replay/debug)  │        │ (JSON/console)   │      │
│   └───────────────┘   └─────────────────┘        └──────────────────┘      │
└──────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| **CLI / Commands** | UX contract: flags/config, exit codes, output formatting, reproducible runs | Subcommands (index/watch/query/export/verify); stable JSON schemas |
| **Indexing Orchestrator** | Turns “index this workspace” into a deterministic set of jobs | Job graph: discovery → parse → extract → normalize → persist → validate |
| **Watcher Router/Debouncer** | Converts file events into “reindex these files” jobs | Debounce + coalesce; backpressure; restart safety |
| **File discovery** | Enumerates files + filters + canonicalizes paths | Ignore rules; path normalization; stable traversal order |
| **Parser layer** | Produces parse trees / syntax nodes with spans | Tree-sitter incremental parsing (robust to syntax errors) |
| **Language adapters** | Extract language-specific symbols/references/structure | Per-language adapters; may be partial/heuristic |
| **Determinism & ID service** | Defines stable IDs and output ordering rules | Canonical path IDs + span IDs + symbol signature rules |
| **Graph persistence** | Stores entities and relationships durably | SQLite tables for symbols, occurrences, edges, metadata |
| **Output subsystem** | Exports JSON (and/or protobuf) deterministically | Canonical JSON, stable ordering, schema versioning |
| **Validation subsystem** | Proves invariants and runs external hooks | Internal checks + compiler/LSP hooks + format validation |
| **Execution logging / audit** | Records what happened and why | Append-only operation log with inputs/outputs, durations, errors |

## Recommended Project Structure (Magellan-aligned)

Magellan already has clear separation (CLI wiring, watcher, tree-sitter ingest, sqlite persistence). For “deterministic structured output + verification”, the common architectural evolution is to make these cross-cutting concerns **first-class modules** rather than scattered behavior.

Suggested refinement:

```
src/
├── main.rs                  # CLI entrypoint
├── lib.rs                   # Library API surface
├── indexer.rs               # High-level indexing orchestration
├── watcher.rs               # Watch loop + file event routing
├── watch_cmd.rs             # CLI command: watch
├── query_cmd.rs             # CLI command: query
├── verify_cmd.rs            # CLI command: verify
├── graph/                   # persistence, schema, queries
│   ├── mod.rs
│   ├── schema.rs            # explicit schema/versioning helpers
│   ├── store.rs             # write model: insert/update
│   ├── read_model.rs        # query-facing views (optional)
│   └── migrations/          # if using migrations
├── ingest/                  # language adapters and tree-sitter integration
│   ├── mod.rs
│   ├── rust.rs
│   ├── python.rs
│   ├── js_ts.rs
│   └── ...
├── ids/                     # NEW: stable identifiers
│   ├── mod.rs
│   ├── path_id.rs           # canonical path → stable file_id
│   ├── span_id.rs           # span identity rules
│   └── symbol_id.rs         # symbol identity rules (signature-based)
├── output/                  # NEW: structured outputs
│   ├── mod.rs
│   ├── json.rs              # canonical JSON writer + schema version
│   └── export.rs            # export commands (graph snapshot, etc.)
├── validate/                # NEW: validation and hooks
│   ├── mod.rs
│   ├── invariants.rs        # internal consistency checks
│   ├── hooks.rs             # external validation hooks
│   └── report.rs            # normalized diagnostics
└── opslog/                  # NEW: execution logging
    ├── mod.rs
    ├── model.rs             # operation record schema
    ├── sink.rs              # write to sqlite/jsonl
    └── correlation.rs       # trace_id/run_id/span correlation
```

### Structure Rationale

- **ids/**: Stable IDs are a global contract. Keeping them centralized prevents “ID drift” (different modules inventing different identities).
- **output/**: Deterministic output is a serialization and ordering problem, not an indexing problem. Separate it so you can test it (golden files) without the indexer.
- **validate/**: Validation is a pipeline stage with explicit inputs/outputs (diagnostics), and must be runnable on-demand (`magellan verify`) and optionally as hooks after indexing.
- **opslog/**: Execution logs are an architectural backbone for determinism debugging (“why did this index differ?”). Treat them as a product feature.

## Architectural Patterns

### Pattern 1: Deterministic pipeline with explicit normalization stage

**What:** Split “extract” from “normalize”:
- extraction is allowed to be language/adapter-specific and noisy
- normalization applies deterministic rules: canonical paths, sorted outputs, consistent encoding, stable ID assignment

**When to use:** Always, when “correctness/determinism” is a v1 goal.

**Trade-offs:**
- + Dramatically improves reproducibility and makes diffs meaningful
- + Allows upgrading adapters without breaking output stability (if normalization contract is stable)
- − Adds a stage and some overhead

**Example (conceptual):**
```text
raw_occurrences (unordered, adapter-specific)
    → normalize_occurrences (sort, encode, dedupe)
    → assign_ids (stable)
    → persist + emit
```

### Pattern 2: Stable IDs via content/structure-addressing (not DB row IDs)

**What:** IDs used in outputs and logs should not be “sqlite autoincrement row IDs”. Use stable identities derived from:
- canonical file identity
- stable span coordinates
- stable symbol signatures

**When to use:** When you need deterministic JSON, replayable logs, and compatibility across re-index runs.

**Trade-offs:**
- + Output diffs stay stable across runs
- + Enables merging / comparing indexes
- − Requires careful versioning (ID algorithm changes must be treated as breaking)

**Ecosystem anchors:**
- **SCIP** uses string symbol identifiers with a defined grammar (designed for robustness/debuggability) and requires explicit position encodings in documents. (See `scip.proto` and `DESIGN.md`.)
- **Kythe** uses VNames (language/corpus/root/path/signature) with conventions that encourage consistent generation for the same input. (See Kythe schema reference.)

### Pattern 3: Streaming-friendly outputs (document-bounded batches)

**What:** Emit/commit work in document/file-scoped batches so that:
- large repos can be indexed without holding everything in memory
- watcher updates can reindex “one file at a time”

**When to use:** Tree-sitter + watcher architectures, and any system aiming at monorepo scale.

**Trade-offs:**
- + Natural fit for incremental watch
- + Easier crash recovery (commit per file)
- − Cross-file linking may require a second pass or deferred edge resolution

**Ecosystem anchor:**
- **LSIF** explicitly supports streaming and introduces begin/end events for documents to allow consumers to process without holding the full dump in memory. (See LSIF spec.)

### Pattern 4: First-class validation hooks (internal invariants + external toolchain)

**What:** Validation is a pipeline stage that can be run:
- after full index
- after each watch update
- on-demand via `verify`

Split into:
1) **Internal invariants**: referential integrity, span validity, ordering invariants
2) **External hooks**: language toolchain checks (e.g., `cargo check`, `tsc --noEmit`, `python -m py_compile`), or LSP diagnostics snapshots

**When to use:** When correctness is a primary product promise and you want fast detection of regressions.

**Trade-offs:**
- + “Fail fast” on broken index invariants
- + Users can trust output and automation
- − External hooks can be slow and environment-dependent; must be configurable

### Pattern 5: Append-only execution log with correlation IDs

**What:** Every run produces an execution record:
- `run_id` (one CLI invocation)
- `operation_id` per stage (scan/parse/extract/normalize/persist/validate)
- `inputs` (workspace root, config, tool versions, changed files)
- `outputs` (counts, timings, diagnostics, produced artifacts)

**When to use:** When you need determinism debugging and auditability.

**Trade-offs:**
- + Enables reproducibility investigations
- + Allows “explain why index changed” features
- − Requires log schema/versioning and careful size management

## Data Flow

### Request Flow (index)

```
User: magellan index --root <R> --db <DB> [--json <OUT>]
  ↓
Config resolution (explicit + defaults)
  ↓
File discovery (canonical paths, stable traversal ordering)
  ↓
Per-file job loop (or parallel batches)
  ↓
Tree-sitter parse → language adapter extraction
  ↓
Normalization + stable ID assignment
  ↓
Persistence transaction (SQLite)
  ↓
Optional export (deterministic JSON)
  ↓
Validation (internal invariants → external hooks)
  ↓
Finalize run record (execution log)
```

### Request Flow (watch)

```
User: magellan watch --root <R> --db <DB>
  ↓
FS event stream
  ↓
Debounce + coalesce + ignore rules
  ↓
Incremental reindex job(s) for affected files
  ↓
Same per-file pipeline: parse → extract → normalize → persist
  ↓
Incremental validation policy (configurable)
  ↓
Emit event/report + append execution log entries
```

### Key Data Flows (what to make explicit)

1. **Source → Parse trees:** the parser must provide stable spans (byte/char) and handle syntax errors. Tree-sitter’s incremental parsing + robustness to errors is designed for this environment.
2. **Parse trees → Raw facts:** language adapters produce symbol defs, refs, and relationships.
3. **Raw facts → Canonical facts:** normalization ensures deterministic ordering, deduping, and stable IDs.
4. **Canonical facts → DB:** persistence is the “source of truth” for later querying and for generating exports.
5. **DB → Structured output:** exports should come from DB read-model queries (or canonical facts) with deterministic ordering.
6. **Everything → Execution log:** logs should record both inputs and observed outputs (counts, hashes) for reproducibility.

## Suggested Build Order (roadmap implications)

This ordering reflects typical dependency structure and avoids rework.

1. **Define output contracts first (schemas + determinism policy)**
   - JSON schema versioning, canonical ordering rules, explicit position encoding (bytes vs UTF-16 etc.).
   - Pick the stable ID strategy and document it.
   - Rationale: everything else (watch, validation, logs) depends on a stable “what are we producing?” contract.

2. **Implement stable identifiers (span IDs, symbol IDs) and canonicalization utilities**
   - Canonical path normalization, file IDs, and span identity.
   - Rationale: stable IDs are the glue for deterministic exports and for correlating logs/validation.

3. **Deterministic export layer (from DB / canonical facts)**
   - Build “export JSON” without changing indexing logic too much.
   - Add golden-file tests for ordering and schema stability.
   - Rationale: you want a reliable observable output to validate all later refactors.

4. **Execution logging (operation model + correlation + storage)**
   - Log pipeline stages, counts, hashes, and versions.
   - Rationale: helps debug determinism issues and validate watcher behavior.

5. **Validation subsystem (internal invariants first, external hooks second)**
   - Start with invariants that are fast and portable.
   - Add optional toolchain hooks with clear configuration and timeouts.
   - Rationale: internal invariants catch correctness regressions early; external hooks add confidence but require more ops support.

6. **Integrate into watcher/incremental pipeline**
   - Ensure incremental updates produce the same canonical outputs as full scans.
   - Define validation policy for watch mode (e.g., “cheap invariants on every change; expensive hooks on demand”).

7. **Hardening: crash/restart safety + replay/debug tooling**
   - Ability to replay a run from logs, or compare two runs for determinism deltas.

## Scaling Considerations

| Scale (repo size / churn) | Architecture Adjustments |
|---|---|
| Small repo, low churn | Single-process, sequential indexing; validate on completion |
| Medium repo, active dev | Parallel per-file jobs, DB transactions per batch; watcher debouncing; cheap invariants on each update |
| Large monorepo | Sharded indexing by directory/package; incremental per-file parse cache; separate read-model tables; optional background validation |

### Scaling Priorities

1. **First bottleneck:** parsing + extraction CPU time
   - Fix: per-language query optimization, caching, parallelism, incremental parsing reuse.
2. **Second bottleneck:** DB write amplification / contention
   - Fix: batch writes, WAL mode tuning, reduce redundant updates via file content hashing.

## Anti-Patterns

### Anti-Pattern 1: Using DB row IDs as public/stable IDs

**What people do:** Emit `symbols.id` or `edges.id` from SQLite as identifiers in JSON/logs.

**Why it’s wrong:** Row IDs change across runs; exports become non-deterministic; logs can’t be correlated across reruns.

**Do this instead:** Define a stable ID scheme (file_id + span + symbol signature), keep DB row IDs as internal implementation details.

### Anti-Pattern 2: Mixing “normalization/determinism” into language adapters

**What people do:** Each adapter invents its own ordering and ID rules.

**Why it’s wrong:** Output stability breaks when adapters change; cross-language consistency becomes impossible.

**Do this instead:** Centralize normalization + determinism policy in a shared module.

### Anti-Pattern 3: Always-on heavy validation in watch mode

**What people do:** Run expensive compilers/lints on every file change.

**Why it’s wrong:** Watcher becomes sluggish; developers disable it; you lose the very feedback loop watch mode is for.

**Do this instead:** Configure tiers: cheap invariants always, heavy hooks on-demand or on debounce windows.

## Integration Points

### External Services / Formats

| Service/Format | Integration Pattern | Notes |
|---|---|---|
| Tree-sitter | Parser + query-based extraction | Built for incremental parsing and robustness under syntax errors | 
| LSIF (optional inspiration) | Streaming “document-bounded” output patterns | LSIF uses per-document begin/end events to support streaming consumption | 
| SCIP (optional inspiration) | Stable string IDs + explicit position encoding + document-scoped payload | SCIP explicitly supports streaming and avoids integer IDs for robustness/debuggability |
| Kythe (optional inspiration) | “VName” identity and a rich edge schema | Emphasizes consistent identity and a typed edge/fact schema |

### Internal Boundaries (Magellan)

| Boundary | Communication | Notes |
|---|---|---|
| `watcher` ↔ `indexer` | job queue / API calls | Watch should reuse the same indexing pipeline stages as full index |
| `ingest/*` ↔ `ids/*` | data structures + stable ID functions | Adapters should never invent IDs; they emit raw spans/symbol info |
| `graph/*` ↔ `output/*` | read-model queries → serialized output | Outputs should be generated deterministically from persisted state |
| `validate/*` ↔ `opslog/*` | diagnostics + run records | Validation should emit structured diagnostics + link to operation IDs |

## Sources

- SCIP repository (protocol + design rationale):
  - https://github.com/sourcegraph/scip
  - https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto
  - https://raw.githubusercontent.com/sourcegraph/scip/main/DESIGN.md
- LSIF index format specification (streaming, document-bounded events, monikers):
  - https://raw.githubusercontent.com/microsoft/language-server-protocol/main/indexFormat/specification.md
- Kythe schema reference (VName conventions; nodes/edges model):
  - https://kythe.io/docs/schema/
- Tree-sitter documentation (incremental parsing, robustness goals):
  - https://tree-sitter.github.io/tree-sitter/

---
*Architecture research for: deterministic codebase mapping tools (Magellan v1 correctness/determinism)*
*Researched: 2026-01-18*

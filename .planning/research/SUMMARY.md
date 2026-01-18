# Project Research Summary

**Project:** Magellan
**Domain:** Deterministic, local-first codebase mapping CLI (watcher + tree-sitter AST fact extraction + SQLite-backed graph + JSON/NDJSON outputs)
**Researched:** 2026-01-18
**Confidence:** MEDIUM

## Executive Summary

Magellan is best treated as an **embedded, deterministic indexing pipeline**: file discovery → parse (tree-sitter) → extract raw facts (defs/refs/calls) → **normalize** (canonical paths, stable ordering, stable IDs, span model) → persist (SQLite) → export/query. Experts build these tools by making the *output contract* (schema + ordering + coordinate system) explicit first, then ensuring every ingestion path (one-shot index and watch mode) funnels through the same normalization rules.

The research strongly recommends making **determinism a product feature, not a side effect**: ban HashMap order leaks, ban timestamps in primary JSON, enforce stdout discipline (data only), and adopt a **content-addressed ID strategy** (file_id/span_id/symbol_id) that is stable across reruns on unchanged inputs. This is what unlocks downstream automation (diffing, caching, audit trails, refactor tooling) and keeps the tool trustworthy.

The main risks are predictable and avoidable: “stable IDs” that are actually unstable (DB row IDs / tree-sitter node IDs), span bugs from mixed coordinate systems (byte vs char, 0-based vs 1-based, inclusive vs exclusive), watcher nondeterminism from event storms/unbounded queues, and JSON outputs that aren’t a real contract (no versioning/schema). The mitigation is phased: lock the schema + normalization rules early, add golden tests for determinism, harden span handling (including non-ASCII fixtures), and only then layer on execution logging and validation hooks.

## Key Findings

### Recommended Stack

Magellan’s constraints (local CLI, deterministic, no async runtime, SQLite persistence, tree-sitter parsing) align cleanly with a **Rust 2021 + tree-sitter + SQLite** stack. The key “stack” decision is less about frameworks and more about **determinism tooling**: path normalization, stable ID hashing, deterministic ordering, and schema validation/snapshot testing.

**Core technologies:**
- **Rust 2021**: single static CLI binary — strong ecosystem for deterministic tooling and tree-sitter integration.
- **tree-sitter (0.26.3)**: CST parsing with byte spans and 0-based row/col points — fast, robust under syntax errors.
- **SQLite (embedded) + WAL**: durable, inspectable local store — good fit for watcher + query workloads (avoid WAL on network FS).
- **rusqlite (0.38.0)**: sync SQLite access — fits “no async runtime” constraint.
- **serde (1.0.228) + serde_json (1.0.149)**: structured output model — enables schema-first JSON.
- **clap (4.5.54)**: CLI UX contract — explicit flags and subcommands.
- **tracing (0.1.44) + tracing-subscriber (0.3.22)**: structured logging — supports JSON logs without stdout contamination.

Supporting recommendations that materially affect correctness:
- **notify (8.2.0) + notify-debouncer-mini (0.7.0)**: watcher correctness — debounced, coalesced batches with explicit ordering.
- **blake3 (1.8.3)**: stable content-addressed IDs — avoids DB rowid churn.
- **camino (1.2.2)**: UTF-8 path normalization — reduces cross-platform drift.
- **schemars (1.2.0) + jsonschema (0.40.0)**: explicit output contract — validate JSON in CI.

### Expected Features

The feature research is blunt: for a deterministic mapping CLI, users expect **repeatability, baseline-before-watch, and span-aware facts** as table stakes. Magellan’s differentiator should be that it behaves like an *API-grade indexer*: stable IDs, schema-versioned JSON, and trust-building verification.

**Must have (table stakes):**
- Deterministic indexing results (ordering + IDs + counts) with canonical paths.
- Initial full baseline scan before incremental updates.
- Watch mode with create/modify/delete, debouncing, and idempotent DB updates.
- Per-file error tolerance: keep running; emit structured diagnostics.
- Ignore rules + include/exclude filters (gitignore-like parity).
- Language detection + per-language configuration.
- Span-aware outputs everywhere (byte offsets + line/col + explicit encoding + half-open range rules).
- Core graph facts: definitions + references; plus caller→callee edges.
- Query surface via CLI: definition, references, callers/callees, list symbols in file, export.
- Structured JSON output + stable exit codes + reproducible run metadata.

**Should have (competitive):**
- Stable IDs for execution/match/span/symbol with clear derivation rules.
- Schema-versioned deterministic outputs (JSON and NDJSON/JSONL where streaming helps).
- Validation hooks (pre/post) and execution logging/replayability.
- Export to interoperable formats (SCIP/LSIF) after the span+ID model is locked.

**Defer (v2+):**
- Multi-root workspaces / DB merging workflows.
- Pluggable external language adapters (plugin API).
- Semantic/type-aware augmentation (explicitly out of scope for v1).

### Architecture Approach

Magellan should be organized around a **deterministic indexing pipeline** with a first-class normalization stage and clear cross-cutting modules:

**Major components:**
1. **CLI/Commands** — stable flags, exit codes, stdout/stderr discipline, schema_version in every response.
2. **Indexing orchestrator** — schedules discovery→parse→extract→normalize→persist→validate deterministically.
3. **Watcher + debouncer** — turns file events into deterministic reindex jobs (coalesce, order, backpressure).
4. **Ingest/language adapters** — extract raw symbols/refs/calls; do *not* invent IDs or ordering.
5. **Determinism & ID service** — canonical path rules; stable ordering; file_id/span_id/symbol_id.
6. **Graph persistence (SQLite)** — store facts/edges + per-file content hashes + run metadata.
7. **Output subsystem** — deterministic JSON/NDJSON writer, schema generation, export commands.
8. **Validation subsystem** — internal invariants first; external toolchain hooks optional/configurable.
9. **Execution log (opslog)** — append-only run/operation records with correlation IDs.

### Critical Pitfalls

1. **“Stable IDs” that aren’t stable** — avoid DB row IDs and tree-sitter `Node::id()`; use content-addressed IDs and document derivation rules.
2. **Mixed span coordinate systems** — canonicalize on UTF-8 byte offsets with exclusive end; document 0/1-based conversions; add span invariants and tests.
3. **Non-ASCII panics / invalid slicing** — don’t slice Rust `String` with byte offsets; treat source as bytes; validate UTF-8 boundaries before converting.
4. **“Deterministic output” that still changes** — nested map ordering, filesystem order leaks, timestamps/paths; enforce deterministic serialization and path normalization.
5. **JSON output without a contract / stdout contamination** — every JSON payload needs schema_version + consistent error shape; stdout must remain machine output only.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 0: Watcher determinism hygiene (foundation)
**Rationale:** Watch mode already exists, but event storms and unbounded queues can invalidate all later determinism guarantees.
**Delivers:** Debounced/coalesced event batches, explicit per-batch ordering, bounded backlog policy, and deterministic “scan initial then watch” semantics.
**Addresses:** watch mode reliability; reproducible incremental updates.
**Avoids:** event storms, nondeterministic intermediate states, runaway memory (Pitfall 8).

### Phase 1: Output contract first (schema + stdout discipline + path normalization)
**Rationale:** Everything downstream (IDs, logs, validation, exports) depends on a stable output shape and semantics.
**Delivers:** Schema-versioned JSON/NDJSON conventions; stdout-only-data rule; canonical path representation (workspace-relative + optional absolute).
**Addresses:** deterministic structured JSON outputs; ignore/include/exclude semantics in outputs; stable exit codes.
**Avoids:** “JSON-shaped but not a contract”, stdout contamination, path identity bugs (Pitfalls 5/6/9).

### Phase 2: Span model hardening + stable ID service
**Rationale:** Stable IDs require an unambiguous span model; span bugs are expensive to fix after consumers exist.
**Delivers:** Canonical span definition (UTF-8 bytes, exclusive end, documented line/col base), non-ASCII safety, and content-addressed IDs (file_id/span_id/symbol_id/match_id/execution_id) with a published derivation recipe.
**Addresses:** stable identifiers; span-aware outputs everywhere.
**Avoids:** unstable IDs, mixed coordinate systems, non-ASCII panics (Pitfalls 1–3).

### Phase 3: Deterministic exports + golden tests + schema validation gates
**Rationale:** You need a reliable observable output before refactoring deeper internals.
**Delivers:** Deterministic export layer (JSON + optional NDJSON); snapshot/golden tests; JSON Schema validation in CI.
**Addresses:** reproducible outputs; diff-friendly exports; regression safety net.
**Avoids:** silent drift in ordering/fields; nondeterministic serializer leaks (Pitfall 4/5).

### Phase 4: Execution logging (opslog) + validation hooks
**Rationale:** Once IDs and schema are stable, logs/validation become actionable instead of noise.
**Delivers:** Append-only execution log with run/operation records; pre/post invariants; configurable external hooks (tiered so watch remains fast).
**Addresses:** trust, auditability, “why did this change?” debugging.
**Avoids:** validation that checks the wrong thing; heavy validation in watch mode (Pitfalls 7/Anti-pattern 3).

### Phase 5: Query UX hardening + ambiguity reporting
**Rationale:** Name-collision mislinks are the main correctness risk under “facts-only” constraints.
**Delivers:** Consistent resolution policy, explicit ambiguity flags/candidates in outputs, query-time guarantees/filters (max-results, deterministic ordering).
**Addresses:** safer automation; reduced “confidently wrong” graphs.
**Avoids:** wrong edges from naive name-based resolution (Pitfall 10).

### Phase 6 (v1.x / v2+): Interop exports + scaling work
**Rationale:** SCIP/LSIF export and monorepo scaling should happen after the ID/span contract is stable.
**Delivers:** Optional SCIP/LSIF export; performance work (parallel parse with deterministic merge; WAL tuning); richer validation modes.
**Addresses:** ecosystem integration and large-repo viability.
**Avoids:** having to rewrite identity/encoding later.

### Phase Ordering Rationale

- Output semantics (schema, stdout discipline, path normalization) must be locked before IDs/logs/validation, otherwise every consumer breaks.
- Stable IDs must be content/structure-addressed; do not retrofit later (high migration cost).
- Watch mode must reuse the same normalization + persistence path as full index; otherwise determinism collapses.
- Validation hooks should be tiered (cheap invariants always; heavy hooks on-demand) to keep watch mode usable.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 0 (watcher debouncing/backpressure):** OS/editor event semantics vary; needs fixture-based stress testing and careful queue policy design.
- **Phase 2 (symbol identity rules across languages):** fully-qualified naming is language-specific; needs explicit collision strategy and “ambiguity” UX.
- **Phase 6 (SCIP/LSIF export):** requires adopting precise encoding/range conventions; plan should validate chosen mapping early.

Phases with standard patterns (skip research-phase):
- **Phase 1 (schema-versioned JSON + stdout discipline):** established CLI best practice; implementable without deep domain exploration.
- **Phase 3 (snapshot + schema validation gates):** well-trodden in Rust CLI tooling (insta/jsonschema patterns).

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Concrete crate versions + official tree-sitter/SQLite docs cited; aligns with Magellan constraints. |
| Features | MEDIUM | Strong consensus patterns (SCIP/LSIF/Semgrep/ripgrep), but some items depend on Magellan’s current implementation details and user workflow validation. |
| Architecture | MEDIUM | Architecture patterns are standard, but exact module boundaries/DB schema implications require repo-specific review during roadmap. |
| Pitfalls | MEDIUM-HIGH | Failure modes are well-known; includes Magellan-specific concerns referenced from internal audit notes. |

**Overall confidence:** MEDIUM

### Gaps to Address

- **Exact “symbol_id” naming/qualification strategy per language:** decide how to incorporate containers/modules (and how to represent ambiguity) without over-promising semantic correctness.
- **Path policy for non-UTF8 files and symlinks:** choose reject vs encode-bytes policy and test cross-platform behavior.
- **DB schema evolution/migrations plan:** if stable IDs become first-class columns, define migration strategy and versioning rules.
- **Determinism acceptance tests:** define repo fixtures and “rerun → byte-for-byte identical output” gates for both index and watch modes.

## Sources

### Primary (HIGH confidence)
- Tree-sitter official docs — incremental parsing + robust error handling: https://tree-sitter.github.io/tree-sitter/
- tree-sitter Rust bindings docs — `Node::id` uniqueness limits and `Point` 0-based coords:
  - https://docs.rs/tree-sitter/latest/tree_sitter/struct.Node.html
  - https://docs.rs/tree-sitter/latest/tree_sitter/struct.Point.html
- SQLite WAL mode constraints (including network filesystem caveats): https://www.sqlite.org/wal.html
- RFC 8785 — JSON Canonicalization Scheme (reference for canonical JSON needs): https://www.rfc-editor.org/rfc/rfc8785

### Secondary (MEDIUM confidence)
- Sourcegraph SCIP (symbol identity + position encoding):
  - https://github.com/sourcegraph/scip
  - https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto
  - https://raw.githubusercontent.com/sourcegraph/scip/main/DESIGN.md
- Microsoft LSIF specification (streaming document-bounded patterns): https://raw.githubusercontent.com/microsoft/language-server-protocol/main/indexFormat/specification.md
- Ripgrep guide (ignore/filter and CLI contract patterns): https://raw.githubusercontent.com/BurntSushi/ripgrep/master/GUIDE.md
- Semgrep CLI reference (exit codes, structured output conventions, autofix warnings): https://semgrep.dev/docs/cli-reference

### Tertiary (LOW confidence)
- SARIF v2.1.0 Appendix F (determinism guidance; conceptually similar failure modes): https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html

---
*Research completed: 2026-01-18*
*Ready for roadmap: yes*

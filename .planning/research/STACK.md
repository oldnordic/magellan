# Stack Research

**Domain:** Deterministic codebase mapping CLI (watchers + AST extraction + SQLite-backed graph) with structured JSON outputs
**Researched:** 2026-01-18
**Confidence:** MEDIUM

This stack is scoped to a **local, deterministic CLI** (no web APIs, no LSP, no async runtime required) that needs:
- deterministic structured JSON output (stable ordering)
- stable identifiers (`execution_id`, `match_id`, `span_id`)
- span-aware matches (byte + line/col)
- validation hooks (checksums pre/post)
- execution logging (machine-readable)

> Version verification approach: crate versions below were verified via `cargo search <crate> --limit 1` in this repository environment (crates.io index).

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust (edition) | 2021 | Single static CLI binary | Strong ecosystem for deterministic tooling; excellent tree-sitter bindings; easy distribution; predictable performance. |
| tree-sitter | 0.26.3 | Concrete syntax tree (CST) parsing + spans | Tree-sitter is the de-facto standard for fast, robust, multi-language parsing in developer tools; supports byte offsets + row/col points. (Official docs confirm incremental parsing + error-tolerant design.) |
| SQLite (embedded) | (system lib) | Durable local persistence | Embedded, deterministic, debuggable with standard tools; WAL mode improves concurrency for watcher + query workloads. |
| rusqlite | 0.38.0 | SQLite access from Rust | Synchronous, ergonomic SQLite wrapper (fits “no async runtime” constraint) and works well for embedded CLI data stores. |
| sqlitegraph | 1.0.0 | SQLite-backed deterministic graph storage | Fits the “SQLite-backed graph DB” requirement while staying embedded and deterministic (no separate DB service). |
| serde | 1.0.228 | Structured data model (de)serialization | Standard Rust serialization framework; integrates with schema generation and canonicalization strategies. |
| serde_json | 1.0.149 | JSON output for CLI | Widely adopted; stable API; good interoperability with jq/NDJSON tooling. |
| clap | 4.5.54 | CLI argument parsing | De-facto standard Rust CLI parser; supports subcommands and stable UX patterns. |
| tracing | 0.1.44 | Structured execution logging | Structured spans/events make it easy to emit both human logs and machine-readable JSON logs without ad-hoc logging. |
| tracing-subscriber | 0.3.22 | Logging sinks + JSON formatting | Provides JSON formatting and filtering; supports deterministic “event per line” logs suitable for ingestion. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| notify | 8.2.0 | Cross-platform filesystem watcher | Use as the watcher backbone. Pair with a debouncer to coalesce bursts and enforce deterministic batch boundaries. |
| notify-debouncer-mini | 0.7.0 | Event debouncing/coalescing | Use to enforce deterministic “scan batches” (e.g., coalesce events for N ms, then process in sorted path order). |
| crossbeam-channel | 0.5.15 | Deterministic internal message passing | Use for watcher → indexer pipeline; avoids async runtime while supporting multi-threaded producers/consumers. |
| walkdir | 2.5.0 | Deterministic initial scans | Use for initial crawl; always sort discovered paths before processing for deterministic results. |
| camino | 1.2.2 | UTF-8, normalized paths | Use to avoid platform-specific/non-UTF8 path issues in JSON outputs; improves reproducibility across OSes. |
| blake3 | 1.8.3 | Fast hashing for stable IDs + checksums | Use for content-addressed identifiers (e.g., span_id) and pre/post validation digests. Fast, portable, and widely used. |
| uuid | 1.19.0 | Run-scoped IDs (execution_id) | Use UUIDv7 (time-sortable) **if available/desired** for “unique per execution” identifiers. For strict determinism across replays, don’t derive execution_id from time; use a content hash or explicit seed. (See “ID strategy” below.) |
| ulid | 1.2.1 | Alternative run-scoped IDs | Use if you want lexicographically sortable IDs with minimal overhead. Same determinism warning as UUID applies if generated from time/random. |
| indexmap | 2.13.0 | Deterministic map iteration (insertion-ordered) | Use when you need predictable iteration order but don’t want to sort keys each time. Still prefer explicit sorting for “schema-ish” objects. |
| schemars | 1.2.0 | JSON Schema generation | Use to publish a stable schema for CLI outputs; helps downstream tooling validate output shape across versions. |
| jsonschema | 0.40.0 | JSON Schema validation (tests/hooks) | Use in CI/tests to validate emitted JSON against schemars-generated schema (or a frozen schema). |
| serde_path_to_error | 0.1.20 | Better JSON decode errors | Use for clear “path to failure” diagnostics when validating/reading JSON outputs. |
| serde_jcs | 0.1.0 | JSON Canonicalization Scheme (JCS) | Optional: use if you want RFC 8785 canonical JSON for hashing/signing. Evaluate fit: JCS has numeric/Unicode constraints; may be stricter than needed for CLI output. |
| time | 0.3.45 | Timestamps (execution logging) | Use for timestamps in logs/metadata. For determinism, keep timestamps out of stable IDs and “canonical” outputs. |
| anyhow | 1.0.100 | Error handling | Good ergonomics for CLI tools; pairs well with tracing for context-rich errors. |
| thiserror | 2.0.18 | Typed error enums | Use for library-like modules with stable error categories (especially for machine-readable error output). |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo-nextest | Fast, isolated tests | Recommended for CI speed when snapshot testing JSON outputs and running multi-language fixtures. (Version not verified here.) |
| insta | Snapshot tests | Use for deterministic JSON output snapshots (with canonicalization/sorting). Verified crate version 1.46.1. |
| assert_cmd + predicates | CLI integration testing | Standard approach for testing CLI output and exit codes. Verified: assert_cmd 2.1.2, predicates 3.1.3. |
| cargo fmt / clippy | Style + lint gates | Essential for long-term maintainability and consistent diffs.

## Deterministic Output + ID Strategy (Prescriptive)

### Output format

**Recommendation:** support both:
1. **JSON (single object)** for “one-shot” commands (`magellan query`, `magellan stats`, etc.).
2. **NDJSON / JSONL (one JSON object per line)** for streaming results (watcher events, matches, spans).

Why:
- NDJSON composes well with unix tools and avoids buffering huge arrays.
- “One record per line” is a natural unit for stable `match_id` / `span_id` and for tracing correlation.

### Deterministic ordering

**Rule:** never rely on `HashMap` iteration order for emitted JSON.

Preferred patterns:
- Use **BTreeMap** for key-sorted maps.
- Or use **IndexMap** when insertion order is intentional and controlled.
- Always sort arrays by a stable tuple (e.g., `file_path`, `byte_start`, `byte_end`, `symbol_kind`, `symbol_name`).

### Stable identifiers

Use **two classes of IDs**:

1) **Run-scoped ID** (`execution_id`): unique identifier for *this invocation*.
- If strict determinism across replays is required: derive from a deterministic seed (e.g., hash of CLI args + repo root canonical path + tool version + start “logical clock”).
- If uniqueness is sufficient: UUIDv7/ULID is fine.

2) **Content-addressed IDs** (stable across runs for the same codebase state):
- `file_id = blake3(normalized_repo_relative_path)`
- `span_id = blake3(file_id || byte_start || byte_end || node_kind)`
- `symbol_id = blake3(language || fully_qualified_name || def_span_id)`
- `match_id = blake3(execution_id || stable_match_fields_sorted)` (or purely content-based if you want cross-run identity)

**Why content-addressing:** it’s deterministic, easy to validate, and avoids “DB rowid leaks” (rowids vary across rebuilds/vacuums/imports).

### Span model

Emit both:
- `byte_start`, `byte_end` (tree-sitter-native, deterministic)
- `start`: `{ line, column }`, `end`: `{ line, column }` (tree-sitter `Point` is 0-based; document normalization to 1-based if desired)

Do not compute line/col by rescanning text unless you must; prefer tree-sitter’s built-in row/col ranges. If you need independent verification, use `line-col` or a rope (`ropey`) as a secondary implementation.

### Canonical JSON (optional)

If you need stable hashing/signing over JSON payloads, implement (or adopt) a canonicalization scheme.

**Best-practice reference:** RFC 8785 JSON Canonicalization Scheme (JCS) requires:
- no whitespace
- deterministic key sorting
- strict string/number rules (I-JSON subset)

Use JCS only if you truly need cross-implementation canonicalization. For Magellan’s internal determinism, **stable sorting + fixed serializer settings** is often sufficient.

## Installation

```bash
# Core (Rust)
cargo add clap@4.5.54 serde@1.0.228 serde_json@1.0.149 tracing@0.1.44 tracing-subscriber@0.3.22

# Watcher + parsing + storage
cargo add notify@8.2.0 notify-debouncer-mini@0.7.0 tree-sitter@0.26.3 rusqlite@0.38.0 sqlitegraph@1.0.0

# Determinism + IDs + paths
cargo add blake3@1.8.3 camino@1.2.2 indexmap@2.13.0 uuid@1.19.0 ulid@1.2.1

# Validation/schema (optional)
cargo add schemars@1.2.0 jsonschema@0.40.0 serde_path_to_error@0.1.20

# Test-only
cargo add -D insta@1.46.1 assert_cmd@2.1.2 predicates@3.1.3 tempfile@3.24.0
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| notify + debouncer | watchexec (8.0.1) | Use watchexec if you want “run commands on changes” orchestration. For a library-like watcher feeding an indexer pipeline, notify gives finer control and fewer semantics baked in. |
| rusqlite (sync) | sqlx (0.9.0-alpha.1) | Use sqlx if you already have an async runtime and need compile-time checked queries at scale. For a deterministic CLI with “no async runtime”, rusqlite is the better fit. |
| Content-addressed IDs (blake3) | UUID/ULID everywhere | Use UUID/ULID everywhere only if you don’t need stability across rebuilds. Content-addressing is better for deterministic mapping and reproducible diffs. |
| schemars + jsonschema | Hand-written schema docs | Hand-written schema may be fine if the output is small and stable. For a tool meant to be integrated into pipelines, machine-checkable schema reduces breakage. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `HashMap` in JSON outputs | Iteration order is not stable; breaks deterministic output and hashing | `BTreeMap`, or `IndexMap` + explicit ordering guarantees |
| “Pretty JSON” as the only mode | Pretty-printing introduces whitespace differences; not canonical for hashing/diffs | Provide `--format json` (compact) and optionally `--pretty` separately |
| DB rowids as stable IDs | Rebuilds/imports/vacuum can change rowids; breaks cross-run stability | Content-addressed IDs (blake3) and/or explicit stable IDs stored as columns |
| LSP-based extraction | Adds non-determinism (toolchain/version/environment), heavier dependencies, slower startup | Tree-sitter-based deterministic extraction (already aligned with project constraints) |
| Async runtime “just because” | Increases complexity and can hide scheduling nondeterminism; also violates project constraint | Use threads + channels (crossbeam) and keep ordering explicit |
| Ad-hoc text grep for “matches” | False positives/negatives; no span provenance | AST-backed spans via tree-sitter; store byte ranges + node kinds |

## Stack Patterns by Variant

**If you must guarantee determinism across machines (CI vs dev laptops):**
- Normalize paths to repo-relative UTF-8 (camino)
- Use fixed newline handling (treat file content as bytes; line/col derived from tree-sitter points)
- Sort all emitted arrays by explicit keys
- Use content-addressed IDs only (avoid time/random-based IDs)

**If you need maximum throughput on large monorepos:**
- Keep WAL enabled for SQLite; ensure checkpoints are controlled
- Parallelize parsing per file (rayon 1.11.0) but merge results in a deterministic order (sort before write)
- Prefer streaming NDJSON output to avoid huge allocations

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| tree-sitter@0.26.3 | tree-sitter-<language> grammars | Grammar crates should be pinned and upgraded intentionally; grammar changes can alter node kinds and thus affect stable IDs. |
| rusqlite@0.38.0 | system SQLite | SQLite WAL behavior depends on filesystem and concurrent readers; avoid WAL on network filesystems (SQLite docs). |
| serde_json@1.0.149 | schemars@1.2.0 | Works well; ensure schema is generated from the same Rust types used to serialize output. |

## Confidence Notes (per recommendation)

- **HIGH:** crate versions pulled from crates.io via cargo search; SQLite/tree-sitter official docs used for behavioral claims.
- **MEDIUM:** “standard 2025 approach” claims (ecosystem norms) are based on widely adopted Rust tooling patterns, but web ecosystem verification was blocked (no WebSearch budget).

## Sources

- Tree-sitter official documentation — overview and design goals (incremental parsing, robust error handling): https://tree-sitter.github.io/tree-sitter/
- SQLite official docs — WAL mode behavior and constraints (incl. network filesystem caveat), page updated 2025-05-31: https://www.sqlite.org/wal.html
- SQLite official docs — database file format and WAL references: https://www.sqlite.org/fileformat.html
- RFC 8785 — JSON Canonicalization Scheme (JCS): https://www.rfc-editor.org/rfc/rfc8785
- Crate versions verified locally via `cargo search <crate> --limit 1` (crates.io index)

---
*Stack research for: deterministic codebase mapping / code graph tools*
*Researched: 2026-01-18*

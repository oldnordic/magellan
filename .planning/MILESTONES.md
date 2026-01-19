# Project Milestones: Magellan

## v1.0 Magellan (Shipped: 2026-01-19)

**Delivered:** Deterministic, local-first codebase mapping CLI with schema-versioned JSON outputs, stable IDs, watch mode, validation hooks, and SCIP export.

**Phases completed:** 1-9 (29 plans total)

**Key accomplishments:**

- Deterministic watch mode with debounced event batching and idempotent file updates
- Schema-versioned JSON output with stdout/stderr discipline and deterministic ordering
- Stable span and symbol IDs (span_id, symbol_id, execution_id) with content-addressed generation
- CLI query surface: definitions, references, callers/callees, file listing
- Export formats: JSON, JSONL, DOT (Graphviz), CSV, SCIP
- Validation hooks (pre/post-run) with orphan detection and structured JSON diagnostics
- SCIP export for Sourcegraph interoperability

**Stats:**

- ~18,000 lines of Rust
- 9 phases, 29 plans
- 26 days from project start to ship (2025-12-24 → 2026-01-19)

**Git range:** full project

**What's next:** v1.1 to address concerns from codebase audit (performance improvements, FQN support, sqlitegraph caching)

---

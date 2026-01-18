# Magellan

## What This Is

Magellan is a deterministic codebase mapping CLI for local developers. It watches source trees, extracts AST-level facts (symbols, references, and call relationships) across 7 languages, and persists them into a searchable SQLite-backed graph database.

## Core Value

Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

## Requirements

### Validated

- ✓ Watch directories for file create/modify/delete and process events deterministically — existing
- ✓ Extract AST-level symbol facts (functions/classes/methods/enums/modules) for 7 languages — existing
- ✓ Extract reference facts and call graph edges (caller → callee) across indexed files — existing
- ✓ Persist graph data to SQLite via sqlitegraph and support query-style CLI access — existing
- ✓ Export graph data for downstream tooling — existing
- ✓ Continue running on unreadable/invalid files (per-file errors don’t kill the watcher) — existing
- ✓ Clean shutdown on SIGINT/SIGTERM — existing

### Active

- [ ] Standardize v1 CLI outputs as structured JSON with explicit schemas (for query/find/refs/get/export)
- [ ] Guarantee stable identifiers in outputs (`execution_id`, `match_id`, `span_id`) for downstream tooling
- [ ] Make every match/span output span-aware (byte offsets + line/col) and deterministically ordered
- [ ] Add validation hooks (checksums + pre/post verification) and execution logging for every run

### Out of Scope

- Semantic analysis or type checking — explicitly not a goal
- LSP server or editor language features — CLI-only v1
- Async runtimes or background thread pools — keep deterministic + simple
- Configuration files — prefer CLI flags only
- Web APIs / network services — local tool only
- Automatic database cleanup — user controls DB lifecycle
- Multi-root workspaces (multiple roots in one run/DB) — out of scope for v1

## Context

- Primary users: local developers running Magellan against their own repositories during development and refactoring.
- Magellan is intentionally “facts only”: it extracts syntactic/AST-level facts via tree-sitter and persists them; it does not attempt semantic understanding.
- The repository is a brownfield codebase with an existing CLI + library split and a persistence core built on sqlitegraph.
- Required v1 query surface includes:
  - find symbol definitions
  - find symbol references
  - callers/callees
  - file/module listing
  - graph export (JSON/DOT/CSV)

## Constraints

- **Interface**: CLI commands are the primary interface — keep flags explicit and stable
- **DB location**: User chooses DB path via `--db <FILE>` — no hidden defaults
- **Correctness**: Prioritize correctness and determinism over micro-optimizations
- **Determinism**: Deterministic ordering in outputs and scans (sorted paths/results)
- **Span fidelity**: Outputs must include byte offsets and line/col where applicable
- **Languages**: Rust, Python, Java, JavaScript, TypeScript, C, C++
- **No config files**: CLI flags only; no `.env` or config-driven behavior

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| CLI-first tool for local developers | Keeps scope tight; enables scripting + integration | — Pending |
| Use tree-sitter for AST fact extraction | Cross-language parsing with deterministic syntax trees | — Pending |
| Persist facts in SQLite-backed graph (sqlitegraph) | Portable, inspectable, queryable local store | — Pending |
| `--db` flag required for DB path selection | No hidden state; supports repeatable runs | — Pending |
| Watch does “scan initial + then watch” | v1 must produce a complete baseline before incremental updates | — Pending |
| Structured JSON output with stable IDs + span-aware fields | Enables deterministic downstream tooling and safe automation | — Pending |
| Deterministic ordering everywhere (sorted outputs) | Diff-friendly, reproducible automation | — Pending |
| Validation hooks + execution logging with `execution_id` | Verifiability + audit trail for runs and refactors | — Pending |

---
*Last updated: 2026-01-18 after initialization*

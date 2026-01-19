# Magellan

## What This Is

Magellan is a deterministic codebase mapping CLI for local developers. It watches source trees, extracts AST-level facts (symbols, references, and call relationships) across 7 languages, and persists them into a searchable SQLite-backed graph database with contract-grade JSON outputs and stable IDs.

## Core Value

Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

## Current Milestone: v1.1 Correctness + Safety

**Goal:** Fix correctness issues (FQN collisions), harden security (path traversal), and ensure data integrity (transactional deletes)

**Target features:**
- FQN-as-key refactor: Switch all maps to FQN→symbol_id, treat simple names as display only
- Path traversal validation: Ensure resolved paths cannot escape root directory
- Delete ops transactional safety: Wrap delete_file_facts in explicit transaction + assertions
- SCIP round-trip tests: Export then parse SCIP to verify format correctness
- Docs hardening: Document .db location recommendations

## Requirements

### Validated

- ✓ Watch directories for file create/modify/delete and process events deterministically — v1.0
- ✓ Extract AST-level symbol facts (functions/classes/methods/enums/modules) for 7 languages — v1.0
- ✓ Extract reference facts and call graph edges (caller → callee) across indexed files — v1.0
- ✓ Persist graph data to SQLite via sqlitegraph and support query-style CLI access — v1.0
- ✓ Export graph data for downstream tooling (JSON/JSONL/DOT/CSV/SCIP) — v1.0
- ✓ Continue running on unreadable/invalid files (per-file errors don't kill the watcher) — v1.0
- ✓ Clean shutdown on SIGINT/SIGTERM — v1.0
- ✓ CLI outputs as structured JSON with explicit schemas (schema_version) — v1.0
- ✓ Stable identifiers in outputs (execution_id, match_id, span_id, symbol_id) — v1.0
- ✓ Span-aware outputs (byte offsets + line/col) with deterministic ordering — v1.0
- ✓ Validation hooks (checksums + pre/post verification) and execution logging — v1.0

### Active

- [ ] FQN-as-key refactor: Switch all maps to FQN→symbol_id, treat simple names as display only — v1.1
- [ ] Path traversal validation: Ensure resolved paths cannot escape root directory — v1.1
- [ ] Delete ops transactional safety: Wrap delete_file_facts in explicit transaction + assertions — v1.1
- [ ] SCIP round-trip tests: Export then parse SCIP to verify format correctness — v1.1
- [ ] Docs hardening: Document .db location recommendations — v1.1

### Active (v1.2+)

- [ ] sqlitegraph caching for reference indexing (deferred)
- [ ] Persist file index (deferred)
- [ ] Cross-file reference accuracy tests (deferred)
- [ ] Nested .gitignore file support
- [ ] Multi-root workspaces (v2)

### Out of Scope

- Semantic analysis or type checking — explicitly not a goal
- LSP server or editor language features — CLI-only v1
- Async runtimes or background thread pools — keep deterministic + simple
- Configuration files — prefer CLI flags only
- Web APIs / network services — local tool only
- Automatic database cleanup — user controls DB lifecycle
- Multi-root workspaces (multiple roots in one run/DB) — out of scope for v1
- LSIF export — deprecated in favor of SCIP

## Context

**Current State (v1.0 shipped):**
- ~18,000 lines of Rust
- 9 phases, 29 plans completed over 26 days
- Tech stack: Rust 2021, tree-sitter, sqlitegraph v1.0.0, SCIP 0.6.1

- Primary users: local developers running Magellan against their own repositories during development and refactoring.
- Magellan is intentionally "facts only": it extracts syntactic/AST-level facts via tree-sitter and persists them; it does not attempt semantic understanding.
- Required v1 query surface includes:
  - find symbol definitions
  - find symbol references
  - callers/callees
  - file/module listing
  - graph export (JSON/JSONL/DOT/CSV/SCIP)

**Known concerns from codebase audit:**
- Symbol name collisions in `symbol_name_to_id` HashMap (first match wins)
- Legacy deprecated single-event Watcher API methods
- Incomplete FQN (simple name instead of hierarchical)
- Full symbol scan for references on every file change
- SCIP export lacks integration tests

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
| CLI-first tool for local developers | Keeps scope tight; enables scripting + integration | ✓ Good |
| Use tree-sitter for AST fact extraction | Cross-language parsing with deterministic syntax trees | ✓ Good |
| Persist facts in SQLite-backed graph (sqlitegraph) | Portable, inspectable, queryable local store | ✓ Good |
| `--db` flag required for DB path selection | No hidden state; supports repeatable runs | ✓ Good |
| Watch does "scan initial + then watch" | v1 must produce a complete baseline before incremental updates | ✓ Good |
| Structured JSON output with stable IDs + span-aware fields | Enables deterministic downstream tooling and safe automation | ✓ Good |
| Deterministic ordering everywhere (sorted outputs) | Diff-friendly, reproducible automation | ✓ Good |
| Validation hooks + execution logging with `execution_id` | Verifiability + audit trail for runs and refactors | ✓ Good |
| SCIP export for interoperability | Sourcegraph standard; LSIF deprecated | ✓ Good |
| Simple symbol names (not FQN) for v1 | Hierarchical names deferred; FQN requires AST traversal | ⚠️ Revisit for v1.1 |

---
*Last updated: 2026-01-19 after v1.1 milestone initialization*

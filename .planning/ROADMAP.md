# Roadmap: Magellan

## Milestones

- ✅ **v1.0 MVP** - Phases 1-9 (shipped 2025-12-XX)
- ✅ **v1.1 Correctness + Safety** - Phases 10-13 (shipped 2026-01-20)
- 📋 **v1.2 Performance** - Planned

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-9) - SHIPPED 2025-12-XX</summary>

v1.0 delivered deterministic codebase mapping with tree-sitter AST extraction, SQLite graph persistence, file watching, and multi-format export (JSON/NDJSON/DOT/CSV/SCIP).

</details>

<details>
<summary>✅ v1.1 Correctness + Safety (Phases 10-13) - SHIPPED 2026-01-20</summary>

**Milestone Goal:** Fix correctness issues (FQN collisions), harden security (path traversal), and ensure data integrity (transactional deletes).

**Phases:** 10-13 (20 plans total)
- Phase 10: Path Traversal Validation (4 plans) - Security baseline
- Phase 11: FQN Extraction (6 plans) - Correctness foundation
- Phase 12: Transactional Deletes (6 plans) - Data integrity
- Phase 13: SCIP Tests + Documentation (4 plans) - Validation and documentation

**Shipped:**
- Path traversal validation at all entry points (watcher, scan, indexing)
- Symlink rejection for paths outside project root
- FQN-based symbol lookup eliminating name collisions
- Row-count assertions for delete operation verification
- SCIP export with round-trip test coverage
- Security documentation in README and MANUAL

**See:** `.planning/milestones/v1.1-ROADMAP.md` for full details

</details>

### 📋 v1.2 Performance (Planned)

**Milestone Goal:** Improve indexing performance through caching and incremental optimization.

**Deferred to v1.2:** PERF-01, PERF-02, XREF-01, GIT-01

## Progress

**Execution Order:**
Phases execute in numeric order: 10 → 11 → 12 → 13

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-9 | v1.0 | 29/29 | Complete | 2025-12-XX |
| 10-13 | v1.1 | 20/20 | Complete | 2026-01-20 |

**v1.1 Progress:** [██████████] 100% (20/20 plans) — **SHIPPED**

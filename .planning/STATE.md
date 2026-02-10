# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-10)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** v2.3 Tool Migration & Core Quality milestone

## Current Position

Phase: 070-magellan-core-quality
Plan: 04 (complete)
Status: Tracing migration complete - zero eprintln! in library code
Last activity: 2026-02-10 — Replaced all 8 remaining eprintln! with debug!/warn!/error! macros

Progress: [████████░░] 85% (5/6 phases complete, Phase 70-04 complete)

**Completed Milestones:**
- v1.0 Magellan - Phases 1-9 (shipped 2026-01-19)
- v1.1 Correctness + Safety - Phases 10-13 (shipped 2026-01-20)
- v1.2 Unified JSON Schema - Phase 14 (shipped 2026-01-22)
- v1.3 Performance - Phase 15 (shipped 2026-01-22)
- v1.4 Bug Fixes & Correctness - Phases 16-19 (shipped 2026-01-22)
- v1.5 Symbol Identity - Phases 20-26 (shipped 2026-01-23)
- v1.6 Quality & Bugfix - Phases 27-28 (shipped 2026-02-04)
- v1.7 Concurrency & Thread Safety - Phases 29-33 (shipped 2026-02-04)
- v1.8 CFG and Metrics - Phases 34-35 (shipped 2026-01-31)
- v1.9 AST & Graph Algorithms - Phases 36-44 (shipped 2026-02-04)
- v2.0 Native V2 Backend Migration - Phases 46-55 (shipped 2026-02-08)
- v2.1 Backend Parity Completion - Phases 56-59 (shipped 2026-02-08)
- v2.2 Code Quality & Cross-File Relations - Phases 60-65 (shipped 2026-02-09)

## Performance Metrics

**Velocity:**
- Total plans completed: 196 (v1.0 through 069-04)
- Average duration: ~10 min
- Total execution time: ~32.1 hours

**By Milestone:**

| Milestone | Phases | Plans | Total | Avg/Plan |
|-----------|--------|-------|-------|----------|
| v2.0 | 46-55 | 55 | ~10h | ~11 min |
| v2.1 | 56-59 | 13 | ~4h | ~18 min |
| v2.2 | 60-65 | 14 | ~3h | ~12 min |

**Recent Trend:**
- Last 6 plans: [9 min, 8 min, 12 min, 8 min, 15 min, 10 min, 14 min, 7 min, 10 min, 8 min, 10 min, 8 min]
- Trend: Stable (consistent execution pattern)

*Updated after each plan completion*
| Phase 070-magellan-core-quality P04 | 3 min | 3 tasks | 3 files |
| Phase 070-magellan-core-quality P03 | 8 min | 2 tasks | 3 files |
| Phase 069-mirage-storage-trait P04 | 8 min | 3 tasks | 3 files |
| Phase 069-mirage-storage-trait P03 | 10 min | 3 tasks | 2 files |
| Phase 069-mirage-storage-trait P02 | 6 min | 2 tasks | 3 files |
| Phase 069-mirage-storage-trait P01 | 7 min | 2 tasks | 2 files |
| Phase 070-magellan-core-quality P01 | 45 min | 1 task | 1 file |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

**v2.2 Decisions (shipped 2026-02-09):**
- Cross-file reference indexing verified with integration tests
- refs and find commands return multi-file results correctly
- Cross-file call indexing with symbol_facts from all database symbols
- name_to_ids fallback enables simple-name matching across files for method calls
- refs command --direction in/out flags correctly show cross-file call relationships
- Cross-file import edges using DEFINES edge type (Import->File)
- Module index rebuild after file deletion for accurate resolutions
- Import infrastructure with ImportExtractor for Rust, ImportOps for graph storage
- ModuleResolver for path resolution
- ModulePathCache provides O(1) module lookups during indexing
- Dual backend abstraction via GraphBackend trait enables compile-time backend selection
- ChunkStore KV support unified across both backends
- Native V2 backend uses clustered adjacency for 10x graph traversal performance

**v2.3 Decisions (from research, 66-01, 69-02, 070-01, 070-02, 070-03, and 070-04):**
- **Phase 070-01**: Remove unsafe downcasting from algorithms.rs - use GraphBackend trait API instead (070-01)
- **Phase 070-01**: Backend-agnostic algorithms: BFS for reachability, Tarjan's for SCC, DFS for path enumeration (070-01)
- **Phase 070-02**: Use tracing crate for structured logging - keep user-facing warnings as eprintln! (070-02)
- **Phase 070-02**: Default log level to WARN, overridable via RUST_LOG env var for debugging (070-02)
- **Phase 070-02**: Add #[instrument] macro to core graph operations for automatic span tracking (070-02)
- **Phase 070-04**: Complete tracing migration - zero eprintln! in library code (070-04)
- **Phase 070-04**: Use debug! for verbose diagnostics in debug_assertions, warn! for non-fatal failures, error! for storage errors (070-04)
- Use `magellan::migrate_backend_cmd::detect_backend_format()` for backend detection (don't reimplement)
- Follow llmgrep's `Backend::detect_and_open()` pattern for backend abstraction
- Complete CLI flag exposures first (quick wins) before architectural rewrites
- Mirage storage trait is the largest effort (2-3 weeks) and blocks advanced features
- Magellan core quality fixes needed for accurate cross-file resolution and call tracking
- **Phase 070-03**: Follow references.rs two-pass indexing pattern - query backend.entity_ids() directly (070-03)
- **Phase 070-03**: call_ops.rs symbol_ids parameter used for edge resolution, not symbol_fact building (070-03)
- Made CLI subcommands optional (Option<Command>) to support --detect-backend without subcommand (66-01)
- Use clap alias attribute for flag aliases without code duplication (66-01: --purpose for --label)
- Match splice's exact JSON format: {"backend":"...","database":"..."} for consistency (66-01)
- **Phase 69-02**: Use dual backend pattern: Backend enum for CFG, GraphBackend for entity queries (69-02)
- **Phase 69-02**: Feature flag alignment: backend-sqlite/backend-native-v2 instead of sqlite/native-v2 (69-02)
- **Phase 69-02**: storage() returns Backend enum for CFG operations, backend() returns GraphBackend (69-02)
- **Phase 69-03**: Delegate to Magellan's run_migrate_backend() for migration instead of reimplementing (69-03)
- **Phase 69-03**: Support in-place migration with same input/output database path (69-03)
- **Phase 69-04**: Backend parity tests verify SQLite and native-v2 return identical results (69-04)
- **Phase 69-04**: Integration tests cover all 15 mirage CLI commands (69-04)

### Pending Todos

**v2.3 Phase Planning:**
- Phase 66: CLI Flag Exposure (llmgrep --detect-backend, --purpose; mirage --detect-backend)
- Phase 67: llmgrep watch command (pub/sub)
- Phase 68: Splice --impact-graph flag exposure
- Phase 69: Mirage storage trait rewrite (backend-agnostic, KV storage)
- Phase 70: Magellan core quality fixes (unsafe downcasting, debug output, cross-file bugs)
- Phase 71: Mirage advanced commands (diff, hotpaths, icfg, --incremental)

### Blockers/Concerns

**Research Findings (from `.planning/research/SUMMARY.md`):**

**Gaps to Address:**
- sqlitegraph pub/sub API: Not verified if complete enough for llmgrep watch command
- Import extraction performance: Unknown impact on indexing time (for Phase 70 if needed)
- Mirage storage trait scope: Unclear if CFG analysis requires different storage patterns than semantic search
- hotpaths vs Hotspots terminology: Migration plan asks for `hotpaths` but mirage has `Hotspots` - need clarification

**Tech Debt to Address:**
- Mirages direct `rusqlite` usage prevents backend abstraction (Phase 69)
- Magellans cross-file reference resolution may have orphan references (Phase 70)
- Magellans caller/callee tracking has race conditions during concurrent updates (Phase 70)
- ~~Unsafe downcasting in `src/graph/algorithms.rs` (Phase 70)~~ **COMPLETED (070-01)**
- ~~Debug `eprintln!` statements in production code (Phase 70)~~ **COMPLETED (070-02)**

## Session Continuity

Last session: 2026-02-10 (Phase 070-04: Complete tracing migration)
Stopped at: Completed 070-04 - Replaced all 8 remaining eprintln! with debug!/warn!/error! macros
Resume file: None - plan complete
Blockers: None

## v2.3 Roadmap Summary

**Phases:** 6 (Phases 66-71)

| Phase | Name | Complexity | Dependencies |
|-------|------|------------|--------------|
| 66 | CLI Flag Exposure | Low | None |
| 67 | llmgrep Watch | Medium | 66 |
| 68 | Splice Impact Graph | Low | None |
| 69 | Mirage Storage Trait | High | None |
| 70 | Magellan Core Quality | Medium | None |
| 71 | Mirage Advanced Commands | Medium | 69, 70 |

**Research-Based:**
- All phases based on completed research in `.planning/research/`
- Research confidence: HIGH for stack, features, architecture, pitfalls

**Key Insights:**
- splice is 70% migrated (quick wins available)
- llmgrep is 50% migrated (CLI flags missing)
- mirage is 20% migrated (major rewrite needed)
- Magellan has critical bugs affecting dependent tools

---

*Last updated: 2026-02-10*

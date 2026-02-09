# Roadmap: Magellan

## Overview

Magellan is a deterministic codebase mapping CLI for local developers. This roadmap tracks the v2.2 Code Quality & Cross-File Relations milestone, which completes cross-file reference indexing, re-enables caller/callee tracking, eliminates unwrap() panic points, splits the 2874-line main.rs into focused modules, and completes backend abstraction for full Native V2 parity.

## Milestones

- ✅ **v1.0 Magellan** - Phases 1-9 (shipped 2026-01-19)
- ✅ **v1.1 Correctness + Safety** - Phases 10-13 (shipped 2026-01-20)
- ✅ **v1.2 Unified JSON Schema** - Phase 14 (shipped 2026-01-22)
- ✅ **v1.3 Performance** - Phase 15 (shipped 2026-01-22)
- ✅ **v1.4 Bug Fixes & Correctness** - Phases 16-19 (shipped 2026-01-22)
- ✅ **v1.5 Symbol Identity** - Phases 20-26 (shipped 2026-01-23)
- ✅ **v1.6 Quality & Bugfix** - Phases 27-28 (shipped 2026-02-04)
- ✅ **v1.7 Concurrency & Thread Safety** - Phases 29-33 (shipped 2026-02-04)
- ✅ **v1.8 CFG and Metrics** - Phases 34-35 (shipped 2026-01-31)
- ✅ **v1.9 AST & Graph Algorithms** - Phases 36-44 (shipped 2026-02-04)
- ✅ **v2.0 Native V2 Backend Migration** - Phases 46-55 (shipped 2026-02-08)
- ✅ **v2.1 Backend Parity Completion** - Phases 56-59 (shipped 2026-02-08) - [Archived](.planning/milestones/v2.1-ROADMAP.md)
- 🚧 **v2.2 Code Quality & Cross-File Relations** - Phases 60-65 (in progress)

---

<details>
<summary>✅ v2.0 Native V2 Backend Migration (Phases 46-55) - SHIPPED 2026-02-08</summary>

### Phase 46: Backend Abstraction Foundation
**Goal:** Code uses backend-agnostic types enabling compile-time backend selection via feature flag
**Plans:** 6/6 complete

### Phase 47: Data Migration & Compatibility
**Goal:** Users can migrate existing SQLite databases to Native V2 format without data loss
**Plans:** 5/5 complete

### Phase 48: Native V2 Performance Features
**Goal:** Graph traversal achieves 10x performance improvement through clustered adjacency and KV store
**Plans:** 5/5 complete

### Phase 49: Pub/Sub Integration
**Goal:** Watcher mode uses pub/sub events for real-time cache invalidation
**Plans:** 5/5 complete

### Phase 49.5: Native V2 Test Fixes
**Goal:** Fix test failures that occur with native-v2 feature enabled
**Plans:** 3/3 complete

### Phase 50: Testing & Documentation
**Goal:** All CLI commands work identically on both backends with comprehensive documentation
**Plans:** Fulfilled by phases 47, 49, 49.5, 54

### Phase 51: Fix Native V2 Compilation Errors
**Goal:** Native V2 backend compiles without errors and all features work correctly
**Plans:** 3/3 complete

### Phase 52: Eliminate Native-V2 Stubs
**Goal:** Replace all SQLite stub implementations with proper KV store storage in native-v2 mode
**Plans:** 7/7 complete

### Phase 53: Fix Native-V2 Database Initialization
**Goal:** Fix critical bug where magellan fails to initialize new databases in native-v2 mode
**Plans:** 3/3 complete

### Phase 54: CLI Backend Detection and Dual Query Methods
**Goal:** Fix CLI commands to work with both SQLite and Native-V2 backends
**Plans:** 5/5 complete

### Phase 55: KV Data Storage Migration
**Goal:** Update indexing pipeline to store all metadata in KV storage when using native-v2 backend
**Plans:** 8/8 complete

</details>

---

<details>
<summary>✅ v2.1 Backend Parity Completion (Phases 56-59) - SHIPPED 2026-02-08</summary>

See [.planning/milestones/v2.1-ROADMAP.md](.planning/milestones/v2.1-ROADMAP.md) for details.

**Plans completed:** 13/13
</details>

---

### 🚧 v2.2 Code Quality & Cross-File Relations (In Progress)

**Milestone Goal:** Fix cross-file reference indexing, re-enable caller/callee tracking, improve code quality (reduce unwrap() calls, split main.rs), and complete backend abstraction for full Native V2 parity.

#### Phase 60: Import Infrastructure & Module Resolution ✅
**Goal**: System extracts import statements and builds module path index for cross-file symbol resolution
**Depends on**: Nothing (v2.2 foundation)
**Requirements**: XREF-03
**Success Criteria** (what must be TRUE):
  1. ImportExtractor extracts `use`, `import`, `from` statements during indexing
  2. Import nodes stored in database with IMPORTS metadata from files; module resolution enables Phase 61 to create edges to defining symbols
  3. ModuleResolver resolves `crate::`, `super::`, `self::` paths to file IDs
  4. Module path cache (module_path → file_id) enables efficient lookups
**Plans:** 1/1
- [x] 60-01-PLAN.md — Import extraction infrastructure (ImportFact, ImportNode, ImportExtractor, ImportOps, ModuleResolver)
**Status**: Complete 2026-02-09

#### Phase 61: Cross-File Symbol Resolution ✅
**Goal**: Cross-file references and call relationships are resolved and indexed across all files
**Depends on**: Phase 60
**Requirements**: XREF-01, XREF-02
**Success Criteria** (what must be TRUE):
  1. References indexed across all files in codebase (inter-file relationships)
  2. `refs` command returns multi-file results with file paths, lines, columns
  3. CALLS edges created across file boundaries during indexing
  4. `refs --direction in/out` shows call relationships from/to all files
**Plans:** 3/3
- [x] 61-01-PLAN.md — Import nodes create DEFINES edges to resolved files
- [x] 61-02-PLAN.md — Cross-file call indexing and querying
- [x] 61-03-PLAN.md — Cross-file reference indexing verification
**Status**: Complete 2026-02-09

#### Phase 62: CLI Exposure & Query Updates
**Goal**: CLI commands expose cross-file resolution with clear, structured output
**Depends on**: Phase 61
**Requirements**: None (exposes Phase 61 functionality)
**Success Criteria** (what must be TRUE):
  1. `refs` command shows references from all files in codebase
  2. `find` command returns multi-file results with correct locations
  3. `query` command includes cross-file relationships in results
  4. Cross-file reference tests pass (tests/backend_migration_tests.rs:63-75)
**Plans**: TBD

#### Phase 63: Error Handling Quality - Critical Paths
**Goal**: User-facing code paths have no unwrap() panic points
**Depends on**: Nothing (can run parallel to Phase 60-62)
**Requirements**: QUAL-01
**Success Criteria** (what must be TRUE):
  1. unwrap() removed from all command modules (refs_cmd, find_cmd, query_cmd, etc.)
  2. unwrap() removed from graph operations (symbols, references, calls, etc.)
  3. unwrap() removed from parser ingestion (pool.rs, indexer.rs)
  4. Errors include context via `.context()` or `.with_context()`
  5. Clippy passes with `-- -W clippy::unwrap_used` on critical paths
**Plans**: TBD

#### Phase 64: Code Organization & Backend Abstraction
**Goal**: main.rs split into focused modules and backend abstraction completed
**Depends on**: Nothing (can run parallel to Phase 60-63)
**Requirements**: QUAL-03, BACK-01, BACK-02
**Success Criteria** (what must be TRUE):
  1. main.rs reduced to < 300 lines (from current 2874 lines)
  2. src/cli.rs created with argument parsing logic
  3. src/version.rs created with version information
  4. GraphBackend trait has no SQLite-specific method signatures
  5. Backend-specific code behind `#[cfg(feature = "...")]` gates
**Plans**: TBD

#### Phase 65: Performance & Validation
**Goal**: Codebase quality verified with comprehensive testing and benchmarking
**Depends on**: Phase 61 (resolution implementation), Phase 63 (error handling), Phase 64 (backend abstraction)
**Requirements**: QUAL-02, BACK-03
**Success Criteria** (what must be TRUE):
  1. Clippy passes with `-- -W clippy::unwrap_used` on entire codebase
  2. All CLI query commands work identically on both backends
  3. Cross-file reference indexing works on Native V2 backend
  4. Caller/callee tracking works on Native V2 backend
  5. Integration tests pass for both backends
**Plans**: TBD

---

## Progress

**Execution Order:**
Phases execute in numeric order: 60 → 61 → 62 → 63 → 64 → 65
(Phases 63-64 can run in parallel with 60-62)

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 46-55 | v2.0 | 55/55 | Complete | 2026-02-08 |
| 56-59 | v2.1 | 13/13 | Complete | 2026-02-08 |
| 60. Import Infrastructure | v2.2 | 1/1 | Complete | 2026-02-09 |
| 61. Cross-File Resolution | v2.2 | 3/3 | Complete | 2026-02-09 |
| 62. CLI Exposure | v2.2 | 0/TBD | Not started | - |
| 63. Error Handling Quality | v2.2 | 0/TBD | Not started | - |
| 64. Code Organization | v2.2 | 0/TBD | Not started | - |
| 65. Performance & Validation | v2.2 | 0/TBD | Not started | - |

---

*Last updated: 2026-02-09*

# Roadmap: Magellan

## Overview

Magellan is a deterministic codebase mapping CLI for local developers. This roadmap tracks the v2.3 Tool Migration & Core Quality milestone, which completes Native-V2 backend migration across three dependent tools (llmgrep, splice, mirage) and addresses core quality issues in Magellan itself.

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
- ✅ **v2.1 Backend Parity Completion** - Phases 56-59 (shipped 2026-02-08)
- ✅ **v2.2 Code Quality & Cross-File Relations** - Phases 60-65 (shipped 2026-02-09)
- ✅ **v2.3 Tool Migration & Core Quality** - Phases 66-69 (shipped 2026-02-10)

---

## v2.3 Tool Migration & Core Quality (PLANNING)

**Milestone Goal:** Complete Native-V2 backend migration for external tools (llmgrep, splice, mirage) and fix Magellan core issues (unsafe downcasting, debug output, cross-file resolution bugs).

**Context:**
- **splice** is 70% migrated (snapshots, batch editing, verify done)
- **llmgrep** is 50% migrated (backend abstraction works, CLI flags missing)
- **mirage** is 20% migrated (uses direct SQLite queries, no storage trait)
- **Magellan** has critical bugs affecting cross-file reference resolution and caller/callee tracking

**Based on Research:** `.planning/research/SUMMARY.md`, `.planning/research/FEATURES.md`, `.planning/research/ARCHITECTURE.md`, `.planning/research/PITFALLS.md`, `.planning/research/STACK.md`

---

### Phase 66: CLI Flag Exposure (llmgrep, mirage)

**Goal:** All tools provide consistent `--detect-backend` flag for runtime backend detection and llmgrep exposes `--purpose` search mode.

**User Value:** Users can query which backend a database uses without running a full command, enabling automated tooling and debugging. Purpose-based semantic search finds code by functional role.

**Depends on:** Nothing (uses existing `magellan::migrate_backend_cmd::detect_backend_format()`)

**Complexity:** Low

**Requirements Addressed:**
- TOOL-01: All tools support `--detect-backend` flag
- TOOL-02: llmgrep `--purpose` search mode

**Success Criteria:**
1. User runs `llmgrep --detect-backend --db codegraph.db` and receives output "native-v2" or "sqlite"
2. User runs `mirage --detect-backend --db codegraph.db` and receives output "native-v2" or "sqlite"
3. User runs `llmgrep search --purpose "authentication" --db codegraph.db` and receives results using label-based search
4. All tools output consistent format strings: "native-v2" and "sqlite" (exact lowercase match)

**Files:**
- `/home/feanor/Projects/llmgrep/src/main.rs` - Add global `--detect-backend` flag, `--purpose` flag to search command
- `/home/feanor/Projects/mirage/src/main.rs` - Add global `--detect-backend` flag
- Reference: `/home/feanor/Projects/splice/src/main.rs:414` - Splice's implementation (already done)

**Avoids:** Pitfall #3 - CLI flags not exposed for implemented features

**Plans:** 1/1 complete
**Status:** ✅ Complete (shipped 2026-02-10)

---

### Phase 67: llmgrep Watch Command

**Goal:** llmgrep provides real-time database updates via pub/sub infrastructure.

**User Value:** Developers can monitor semantic search results continuously as they edit code, without re-running queries manually.

**Depends on:** Phase 66 (backend detection verified first), sqlitegraph 1.5.7 pub/sub API completeness

**Complexity:** Medium

**Requirements Addressed:**
- TOOL-03: llmgrep `watch` command for real-time pub/sub updates

**Success Criteria:**
1. User runs `llmgrep watch --query "Widget" --db codegraph.db` and receives initial results
2. When a file is modified (matching symbols added/removed), new results appear automatically
3. Watch mode exits cleanly on SIGINT/SIGTERM
4. Watch mode works with both SQLite and native-v2 backends

**Files:**
- `/home/feanor/Projects/llmgrep/src/main.rs` - Add watch command to Command enum
- `/home/feanor/Projects/llmgrep/src/watch_cmd.rs` - NEW: watch command implementation using sqlitegraph pub/sub

**Research Flag:** Verify sqlitegraph pub/sub API completeness before implementation

**Plans:** 1/1
**Plan list:**
- [ ] 067-01-PLAN.md — Create watch_cmd.rs module with pub/sub implementation and wire up Watch command
**Status:** ✅ Complete (shipped 2026-02-10)

---

### Phase 68: Splice Impact Graph Exposure

**Goal:** splice provides consistent `--impact-graph` flag across relevant commands for DOT graph visualization.

**User Value:** Developers can visualize the impact of refactors before applying them, enabling safer code changes.

**Depends on:** Nothing (internal `execute_impact_graph()` already exists)

**Complexity:** Low

**Requirements Addressed:**
- TOOL-04: splice `--impact-graph` flag exposed on relevant commands

**Success Criteria:**
1. User runs `splice rename --symbol foo --to bar --impact-graph --preview --db codegraph.db` and receives DOT graph output
2. User runs `splice refs --name main --path src/main.rs --impact-graph --db codegraph.db` and receives DOT graph output
3. User runs `splice reachable --symbol main --path src/main.rs --impact-graph --db codegraph.db` and receives DOT graph output
4. User runs `splice patch --symbol foo --file src/lib.rs --with new.rs --impact-graph --preview --db codegraph.db` and receives DOT graph output
5. Impact graph shows caller/callee relationships affected by the change
6. DOT output is parseable by graphviz tools (dot, xdot)

**Files:**
- `/home/feanor/Projects/splice/src/cli/mod.rs` - Add `--impact-graph` flag to patch command
- `/home/feanor/Projects/splice/src/main.rs` - Wire up impact_graph in execute_patch and main()
- `/home/feanor/Projects/splice/tests/integration/impact_graph_tests.rs` - NEW: verification tests
- Reference: `/home/feanor/Projects/splice/src/main.rs:4746-4758` - rename impact_graph pattern
- Reference: `/home/feanor/Projects/splice/src/main.rs:148` - `execute_impact_graph()` function (already exists)

**Avoids:** Pitfall #3 - CLI flags not exposed for implemented features

**Notes:**
- `rename`, `refs`, `reachable` already have `--impact-graph` (verified in research)
- `apply-files` excluded (text-only, no --db parameter, would require significant scope)
- `patch` added (symbol-based like `rename`, has --db parameter)

**Plans:** 1/1
**Plan list:**
- [ ] 068-01-PLAN.md — Add `--impact-graph` to patch command and create verification tests
**Status:** ✅ Complete (shipped 2026-02-10)

---

### Phase 69: Mirage Storage Trait Rewrite

**Goal:** mirage provides backend-agnostic storage trait, replacing direct `rusqlite` usage throughout codebase.

**User Value:** mirage works with both SQLite and native-v2 backends, enabling performance benefits and new features (diff, incremental, hotpaths, icfg).

**Depends on:** Nothing (foundational work for Phase 71)

**Complexity:** High (2-3 weeks)

**Requirements Addressed:**
- TOOL-05: mirage backend-agnostic storage trait
- TOOL-06: mirage KV storage backend for CFG data
- TOOL-07: mirage `migrate` command

**Success Criteria:**
1. mirage opens databases using `Backend::detect_and_open()` pattern (like llmgrep)
2. CFG blocks stored in KV format: `cfg:func:{function_id}` (following magellan's KV key pattern)
3. CFG edges computed from block terminators (not stored, computed on-demand)
4. User runs `mirage migrate --from sqlite --to native-v2 --db codegraph.db` and data converts successfully
5. All existing commands (cfg, paths, dominators, loops) work identically on both backends
6. No direct `rusqlite::Connection` usage remains in mirage codebase

**Files:**
- `/home/feanor/Projects/mirage/src/storage/mod.rs` - Rewrite to backend-agnostic storage trait
- `/home/feanor/Projects/mirage/src/storage/sqlite.rs` - NEW: SQLite backend implementation
- `/home/feanor/Projects/mirage/src/storage/kv.rs` - NEW: Native-V2 KV backend implementation
- `/home/feanor/Projects/mirage/src/migrate_cmd.rs` - NEW: migrate command
- Reference: `/home/feanor/Projects/llmgrep/src/backend/mod.rs:149` - `Backend::detect_and_open()` pattern to copy

**Addresses:** Pitfall #2 - Direct SQLite usage prevents backend abstraction

**Research Flag:** Storage trait design needs deeper research during planning

**Plans:** 4/4
**Plan list:**
- [x] 069-01-PLAN.md — Create StorageTrait and backend implementations (SQLite + KV) ✅ Complete (shipped 2026-02-10)
- [x] 069-02-PLAN.md — Migrate all commands to use Backend instead of Connection ✅ Complete (shipped 2026-02-10)
- [x] 069-03-PLAN.md — Implement migrate command and --detect-backend ✅ Complete (shipped 2026-02-10)
- [x] 069-04-PLAN.md — Verify backend parity and create integration tests ✅ Complete (shipped 2026-02-10)
**Status:** ✅ Complete (shipped 2026-02-10)

---

### Phase 70: Magellan Core Quality Fixes

**Goal:** Fix critical bugs in Magellan that affect cross-file reference resolution and caller/callee tracking.

**User Value:** Accurate call graph analysis with no orphan references or calls, enabling reliable refactoring tools.

**Depends on:** Nothing (can run in parallel with Phase 69)

**Complexity:** Medium

**Requirements Addressed:**
- CORE-01: Fix unsafe downcasting in `src/graph/algorithms.rs`
- CORE-02: Remove debug `eprintln!` statements from production code
- CORE-03: Use GraphBackend trait consistently (eliminate direct `rusqlite` usage)
- CORE-04: Fix cross-file reference resolution (two-pass indexing)
- CORE-05: Fix caller/callee edge consistency (transactional re-indexing)

**Success Criteria:**
1. `validate_graph()` returns zero `ORPHAN_REFERENCE` errors on cross-file codebases
2. `validate_graph()` returns zero `ORPHAN_CALL_NO_CALLER` and `ORPHAN_CALL_NO_CALLEE` errors
3. No `unsafe downcast_*()` calls remain in `src/graph/algorithms.rs`
4. All `eprintln!` debug statements replaced with proper logging (tracing/log crate)
5. Both backends (SQLite and native-v2) produce identical cross-file reference results

**Files:**
- `src/graph/algorithms.rs:92-100` - Remove unsafe downcasting
- `src/graph/references.rs` - Implement two-pass indexing for cross-file references
- `src/graph/call_ops.rs` - Implement transactional re-indexing for calls
- `src/graph/validation.rs` - Verify orphan detection passes

**Addresses:** Pitfalls #1, #2 - Orphan references, call edge inconsistency

**Plans:** 3/3
**Plan list:**
- [ ] 070-01-PLAN.md — Remove unsafe downcasting from algorithms.rs
- [ ] 070-02-PLAN.md — Replace all eprintln! with tracing logging
- [ ] 070-03-PLAN.md — Verify and fix cross-file reference/call resolution
**Status:** Pending (plans created 2026-02-10)

---

### Phase 71: Mirage Advanced Commands

**Goal:** mirage implements new features enabled by KV storage: diff, hotpaths, icfg commands and `--incremental` flag.

**User Value:** CFG diff between code versions, incremental analysis of changed functions only, most-traversed path detection, and inter-procedural analysis.

**Depends on:** Phase 69 (storage trait required), Phase 70 (Magellan core fixes enable accurate cross-function analysis)

**Complexity:** Medium

**Requirements Addressed:**
- TOOL-08: mirage `diff` command for CFG comparison between snapshots
- TOOL-09: mirage `--incremental` flag on paths command
- TOOL-10: mirage `hotpaths` command for most-traversed path detection
- TOOL-11: mirage `icfg` command for inter-procedural CFG

**Success Criteria:**
1. User runs `mirage diff --function "process" --before snapshot_v1 --after snapshot_v2` and sees CFG differences
2. User runs `mirage paths --incremental --since HEAD~1 --db codegraph.db` and only changed functions are analyzed
3. User runs `mirage hotpaths --db codegraph.db` and receives most-traversed execution paths
4. User runs `mirage icfg --entry "main" --depth 3 --db codegraph.db` and receives inter-procedural CFG
5. All commands work with both SQLite and native-v2 backends

**Files:**
- `/home/feanor/Projects/mirage/src/diff_cmd.rs` - NEW: diff command
- `/home/feanor/Projects/mirage/src/hotpaths_cmd.rs` - NEW: hotpaths command
- `/home/feanor/Projects/mirage/src/icfg_cmd.rs` - NEW: icfg command
- `/home/feanor/Projects/mirage/src/paths.rs` - Add `--incremental` flag

**Clarification Needed:** `hotpaths` vs existing `Hotspots` command - are these the same or different?

**Plans:** —
**Status:** Pending

---

## Phase Dependencies (v2.3)

```
Phase 66: CLI Flag Exposure
    |
    v
Phase 67: llmgrep Watch
    |
    v
Phase 68: Splice Impact Graph
    |
    +------------------+
    |                  |
    v                  v
Phase 69: Mirage   Phase 70: Magellan
    Storage Trait      Core Quality
    |                  |
    +------------------+
             |
             v
    Phase 71: Mirage Advanced Commands
```

**Parallel Execution Opportunities:**
- Phase 66, 67, 68 are independent and can run in any order (or in parallel)
- Phase 69 (Mirage Storage) and Phase 70 (Magellan Core) can run in parallel

---

## Progress Tracking (v2.3)

| Phase | Name | Plans | Status |
|-------|------|-------|--------|
| 66 | CLI Flag Exposure | 1/1 | ✅ Complete |
| 67 | llmgrep Watch | 2/2 | ✅ Complete |
| 68 | Splice Impact Graph | 1/1 | ✅ Complete |
| 69 | Mirage Storage Trait | 4/4 | Pending |
| 70 | Magellan Core Quality | — | Pending |
| 71 | Mirage Advanced Commands | — | Pending |

---

## Milestone Success Criteria (v2.3)

The v2.3 milestone is complete when:

1. **Cross-Tool Consistency:** All three tools (llmgrep, splice, mirage) provide `--detect-backend` flag with identical output formats

2. **Backend Parity:**
   - llmgrep: All commands work identically on SQLite and native-v2
   - splice: All commands work identically on SQLite and native-v2
   - mirage: All commands work identically on SQLite and native-v2

3. **New Features Delivered:**
   - llmgrep: `watch` command with real-time pub/sub updates
   - llmgrep: `--purpose` search mode for label-based semantic search
   - splice: `--impact-graph` flag for DOT visualization
   - mirage: `diff`, `hotpaths`, `icfg` commands and `--incremental` flag

4. **Magellan Core Quality:**
   - Zero orphan references in cross-file codebases
   - Zero orphan calls in call graph
   - No unsafe downcasting in algorithms
   - No debug output in production code
   - Consistent GraphBackend trait usage throughout

5. **Documentation:** All tool README.md files updated with native-v2 notes

---

<details>
<summary>Archived Milestones (v1.0 - v2.2)</summary>

### ✅ v2.2 Code Quality & Cross-File Relations (SHIPPED 2026-02-09)

**Milestone Goal:** Fix cross-file reference indexing, re-enable caller/callee tracking, improve code quality (reduce unwrap() calls, split main.rs), and complete backend abstraction for full Native V2 parity.

#### Phase 60: Import Infrastructure & Module Resolution ✅
**Goal**: System extracts import statements and builds module path index for cross-file symbol resolution
**Plans:** 1/1 complete

#### Phase 61: Cross-File Symbol Resolution ✅
**Goal**: Cross-file references and call relationships are resolved and indexed across all files
**Plans:** 3/3 complete

#### Phase 62: CLI Exposure & Query Updates ✅
**Goal**: CLI commands expose cross-file resolution with clear, structured output
**Plans:** 1/1 complete

#### Phase 63: Error Handling Quality ✅
**Goal**: User-facing code paths have no unwrap() panic points
**Plans:** 1/1 complete

#### Phase 64: Code Organization & Backend Abstraction ✅
**Goal**: main.rs split into focused modules and backend abstraction completed
**Plans:** 5/5 complete

#### Phase 65: Performance & Validation ✅
**Goal**: Codebase quality verified with comprehensive testing and benchmarking
**Plans:** 3/3 complete

### ✅ v2.1 Backend Parity Completion (SHIPPED 2026-02-08)
**Plans:** 13/13 complete

### ✅ v2.0 Native V2 Backend Migration (SHIPPED 2026-02-08)
**Plans:** 55/55 complete

### ✅ v1.9 AST & Graph Algorithms (SHIPPED 2026-02-04)
**Plans:** 9/9 complete

### ✅ v1.8 CFG and Metrics (SHIPPED 2026-01-31)
**Plans:** 2/2 complete

### ✅ v1.7 Concurrency & Thread Safety (SHIPPED 2026-02-04)
**Plans:** 5/5 complete

### ✅ v1.6 Quality & Bugfix (SHIPPED 2026-02-04)
**Plans:** 2/2 complete

### ✅ v1.5 Symbol Identity (SHIPPED 2026-01-23)
**Plans:** 7/7 complete

### ✅ v1.4 Bug Fixes & Correctness (SHIPPED 2026-01-22)
**Plans:** 4/4 complete

### ✅ v1.3 Performance (SHIPPED 2026-01-22)
**Plans:** 1/1 complete

### ✅ v1.2 Unified JSON Schema (SHIPPED 2026-01-22)
**Plans:** 1/1 complete

### ✅ v1.1 Correctness + Safety (SHIPPED 2026-01-20)
**Plans:** 4/4 complete

### ✅ v1.0 Magellan (SHIPPED 2026-01-19)
**Plans:** 9/9 complete

</details>

---

*Last updated: 2026-02-10*

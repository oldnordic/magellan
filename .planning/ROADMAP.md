# Roadmap: Magellan

## Overview

Magellan is a deterministic codebase mapping CLI for local developers. This roadmap tracks the v2.1 Backend Parity Completion milestone, which ensures all CLI query commands and ChunkStore methods work correctly with both SQLite and Native-V2 backends.

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

## ✅ v2.1 Backend Parity Completion (SHIPPED 2026-02-08)

**Milestone Goal:** Ensure all CLI query commands and ChunkStore methods work with both SQLite and Native-V2 backends using TDD methodology.

**Approach:** Test-Driven Development - each phase follows TDD: write failing test on Native-V2, fix code, verify both backends.

**Status:** Complete - All 4 phases (56-59) finished with comprehensive test coverage.

---

### Phase 56: get_chunks_for_file() KV Support ✅

**Goal:** ChunkStore.get_chunks_for_file() works correctly on Native-V2 backend using KV store queries.

**Status:** Complete - 2026-02-08

**Depends on:** Phase 55

**Requirements:** CHUNK-01 ✅

**Success Criteria** (what must be TRUE):
1. ✅ User can run `magellan chunks --db <native-v2-db>` and get correct output
2. ✅ User can retrieve all code chunks for a specific file on Native-V2 backend
3. ✅ Cross-backend test verifies identical results for SQLite vs Native-V2

**Plans:** 2/3 complete (56-02 skipped as redundant)

**Implementation:**
- KV prefix scan using `chunk:{escaped_path}:` pattern
- Colon escaping with `::` for file paths containing colons
- Sort by `byte_start` for consistent ordering
- Early return pattern with SQLite fallback

**Commits:**
- e961e1d: test(56-01): add failing test
- 7b25126: feat(56-01): add KV prefix scan support
- b79e738: refactor(56-01): remove unused mut
- 05c65e6: docs(56-01): complete plan
- fdb1c0b: docs(56-03): update NATIVE-V2.md

Plans:
- [x] 56-01: Write failing test for get_chunks_for_file() on Native-V2 ✅
- [x] 56-02: Add KV prefix scan support to get_chunks_for_file() ✅
- [x] 56-03: Update documentation ✅

---

### Phase 57: get_chunk_by_span() Verification ✅

**Goal:** ChunkStore.get_chunk_by_span() verified to work correctly on both backends.

**Status:** Complete - 2026-02-08

**Depends on:** Phase 56

**Requirements:** CHUNK-03 ✅

**Success Criteria** (what must be TRUE):
1. ✅ User can retrieve a code chunk by exact byte span on Native-V2 backend
2. ✅ Cross-backend test verifies identical results for SQLite vs Native-V2
3. ✅ Edge cases (missing chunks, overlapping spans) handled correctly

**Plans:** 2/2 complete

**Note:** `get_chunk_by_span()` already has KV support (lines 461-485 in src/generation/mod.rs). This phase verified correctness with comprehensive tests.

**Commits:**
- f13aa14: test(57-01): add cross-backend tests for get_chunk_by_span()
- 4f7cc42: docs(57-01): complete get_chunk_by_span() verification plan
- 3e4daf3: test(57-02): add edge case tests for get_chunk_by_span()
- d5ad3ac: docs(57-02): document ChunkStore KV support and get_chunk_by_span()
- d470667: docs(57-02): complete edge case testing and documentation plan

Plans:
- [x] 57-01: Write cross-backend test for get_chunk_by_span() ✅
- [x] 57-02: Verify edge cases and fix any issues ✅

---

### Phase 58: CLI Command Parity - Chunk Queries ✅

**Goal:** `magellan chunks`, `magellan get`, and `magellan get-file` commands work identically on both backends.

**Status:** Complete - 2026-02-08

**Depends on:** Phase 56, Phase 57

**Requirements:** QUERY-01 ✅, QUERY-02 ✅, QUERY-03 ✅

**Success Criteria** (what must be TRUE):
1. ✅ User can run `magellan chunks` on Native-V2 and see all code chunks
2. ✅ User can run `magellan get --file <path> --span <start>:<end>` on Native-V2
3. ✅ User can run `magellan get-file <path>` on Native-V2
4. ✅ All three commands produce identical JSON output on SQLite vs Native-V2

**Plans:** 3/3 complete

**Commits:**
- 8d19e83: test(58-01): add CLI integration tests for magellan chunks command
- 2fe7a7e: test(58-02): add CLI integration tests for magellan get command
- a5fb532: docs(58-02): complete CLI integration tests plan for magellan get command

Plans:
- [x] 58-01: Test `magellan chunks` command on both backends ✅
- [x] 58-02: Test `magellan get` command on both backends ✅
- [x] 58-03: Test `magellan get-file` command on both backends ✅

---

### Phase 59: CLI Command Parity - AST Queries + Test Suite ✅

**Goal:** `magellan ast`, `magellan find-ast` commands work identically on both backends; comprehensive cross-backend test suite exists.

**Status:** Complete - 2026-02-08

**Depends on:** Phase 58

**Requirements:** QUERY-04 ✅, QUERY-05 ✅, VERIFY-01 ✅

**Success Criteria** (what must be TRUE):
1. ✅ User can run `magellan ast --file <path>` on Native-V2 and see AST nodes
2. ✅ User can run `magellan find-ast --kind <kind>` on Native-V2 and get filtered results
3. ✅ Cross-backend test suite exercises all query commands with representative data
4. ✅ Test suite can be run with `--features native-v2` flag

**Plans:** 4/4 complete

**Plans:**
- [x] 59-01: Test `magellan ast` command on both backends
- [x] 59-02: Test `magellan find-ast` command on both backends
- [x] 59-03: Create comprehensive cross-backend test suite
- [x] 59-04: Run full test suite and verify parity

---

## Progress

**Execution Order:**
Phases execute in numeric order: 56 → 57 → 58 → 59

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 46-55 | v2.0 | 55/55 | Complete | 2026-02-08 |
| 56. get_chunks_for_file() KV Support | v2.1 | 2/3 | Complete | 2026-02-08 |
| 57. get_chunk_by_span() Verification | v2.1 | 2/2 | Complete | 2026-02-08 |
| 58. CLI Command Parity - Chunk Queries | v2.1 | 3/3 | Complete | 2026-02-08 |
| 59. CLI Command Parity - AST Queries + Test Suite | v2.1 | 4/4 | Complete | 2026-02-08 |

---

## v2.1 Requirements Coverage

| Requirement | Phase | Status |
|-------------|-------|--------|
| QUERY-01: magellan chunks command | 58 | ✅ Complete |
| QUERY-02: magellan get command | 58 | ✅ Complete |
| QUERY-03: magellan get-file command | 58 | ✅ Complete |
| QUERY-04: magellan ast command | 59 | ✅ Complete |
| QUERY-05: magellan find-ast command | 59 | ✅ Complete |
| CHUNK-01: get_chunks_for_file() KV support | 56 | ✅ Complete |
| CHUNK-03: get_chunk_by_span() KV support | 57 | ✅ Complete |
| VERIFY-01: Cross-backend test suite | 59 | Pending |

**Coverage:** 8/8 requirements mapped (100%)

---

*Last updated: 2026-02-08*

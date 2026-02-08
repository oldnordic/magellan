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
- ✅ **v2.1 Backend Parity Completion** - Phases 56-59 (shipped 2026-02-08) - [Archived](.planning/milestones/v2.1-ROADMAP.md)

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

## Progress

**Execution Order:**
Phases execute in numeric order.

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 46-55 | v2.0 | 55/55 | Complete | 2026-02-08 |
| 56-59 | v2.1 | 13/13 | Complete | 2026-02-08 |

---

*Last updated: 2026-02-08*

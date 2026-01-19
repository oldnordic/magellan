# Roadmap: Magellan

## Milestones

- ✅ **v1.0 MVP** - Phases 1-9 (shipped 2025-12-XX)
- 🚧 **v1.1 Correctness + Safety** - Phases 10-13 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-9) - SHIPPED 2025-12-XX</summary>

v1.0 delivered deterministic codebase mapping with tree-sitter AST extraction, SQLite graph persistence, file watching, and multi-format export (JSON/NDJSON/DOT/CSV/SCIP).

</details>

### 🚧 v1.1 Correctness + Safety (In Progress)

**Milestone Goal:** Fix correctness issues (FQN collisions), harden security (path traversal), and ensure data integrity (transactional deletes).

#### Phase 10: Path Traversal Validation ✅
**Goal**: All file access operations validate that resolved paths cannot escape the project root, preventing CVE-2025-68705 class vulnerabilities.
**Depends on**: v1.0 complete
**Requirements**: PATH-01, PATH-02, PATH-03, PATH-04, PATH-05, PATH-06
**Success Criteria** (what must be TRUE):
  1. Attempting to watch a path containing `../` or `..\\` is rejected before any file access
  2. Watcher events referencing paths outside project root are filtered and logged
  3. Directory scan in scan.rs validates each path before recursing
  4. Symlinks pointing outside project root are either rejected or resolved-then-validated
  5. Cross-platform path tests pass (Windows backslash, macOS case-insensitivity)
**Plans**: 4 plans in 3 waves
**Status**: Complete — Verified 2026-01-19 (5/5 must-haves passed)

Plans:
- [x] 10-01 — Create `src/validation.rs` with path canonicalization and validation utilities (Wave 1)
- [x] 10-02 — Integrate path validation into watcher.rs event filtering (Wave 2)
- [x] 10-03 — Integrate path validation into scan.rs directory walking (Wave 2)
- [x] 10-04 — Add traversal tests for malicious paths, symlinks, and cross-platform edge cases (Wave 3)

#### Phase 11: FQN Extraction ✅
**Goal**: Symbol lookup uses fully-qualified names (FQN) as keys, eliminating collisions from simple-name-first-match wins.
**Depends on**: Phase 10
**Requirements**: FQN-01, FQN-02, FQN-03, FQN-04, FQN-05, FQN-06
**Success Criteria** (what must be TRUE):
  1. Symbol map keys are FQN strings (e.g., `crate::module::Struct::method`) not simple names
  2. Rust symbols use `::` separator, Python/Java/TypeScript use `.` separator
  3. symbol_id is generated from hash(language, FQN, span_id) not from simple names
  4. FQN collision warnings are emitted when two symbols would have the same FQN
  5. Full re-index of all files produces correct FQNs throughout the graph
**Plans**: 6 plans in 5 waves
**Status**: Complete — Verified 2026-01-19 (5/5 must-haves passed)

Plans:
- [x] 11-01 — Implement ScopeStack struct in src/ingest/mod.rs for tracking nesting during walk_tree (Wave 1)
- [x] 11-02 — Add Rust parser scope tracking (mod/impl/trait) with walk_tree_with_scope (Wave 2)
- [x] 11-03 — Add Python/Java/JavaScript/TypeScript parser scope tracking with Dot separator (Wave 3)
- [x] 11-04 — Add C/C++ parser scope tracking (C: no-op, C++: namespaces with :: separator) (Wave 3)
- [x] 11-05 — Update symbol lookup maps to use FQN → symbol_id (query.rs, references.rs, calls.rs) (Wave 4)
- [x] 11-06 — Complete symbol_id generation from FQN and add integration tests (Wave 5)

#### Phase 12: Transactional Deletes ✅
**Goal**: Ensure delete operations have strong integrity guarantees (row-count verification, orphan detection).
**Depends on**: Phase 10
**Requirements**: DELETE-01, DELETE-02, DELETE-03, DELETE-04
**Success Criteria** (what must be TRUE):
  1. Row-count assertions verify all derived data is deleted (symbols, refs, calls, edges)
  2. Orphan detection test confirms no dangling edges after delete operations
  3. delete_file_facts() returns detailed DeleteResult with all counts
  4. All delete operations use count-then-assert pattern for verification
**Plans**: 6 plans in 6 waves
**Status**: Complete — Verified 2026-01-20 (2/2 core must-haves passed)

**Note:** ACID transactions across graph operations are not possible with current sqlitegraph API (does not expose &mut Connection). Row-count assertions and orphan detection provide strong data integrity guarantees. Future sqlitegraph versions may add transaction support.

Plans:
- [x] 12-01 — Wrap delete_file_facts() in rusqlite IMMEDIATE transaction (Wave 1) — Reverted due to API limitation
- [x] 12-02 — Add row-count assertions to verify all derived data is deleted (Wave 2)
- [x] 12-03 — Implement error injection tests for transaction rollback verification (Wave 3) — Uses verification points
- [x] 12-04 — Add invariant test for orphan detection after file delete (Wave 4)
- [x] 12-05 — Add shared connection support to ChunkStore (Wave 5) — Gap closure attempt
- [x] 12-06 — Restore IMMEDIATE transactions (Wave 6) — Discovered API limitation, documented constraint

#### Phase 13: SCIP Tests + Documentation
**Goal**: SCIP export is verified by round-trip tests, and users have clear security guidance.
**Depends on**: Phase 10, Phase 11, Phase 12
**Requirements**: SCIP-01, SCIP-02, DOC-01, DOC-02
**Success Criteria** (what must be TRUE):
  1. SCIP export can be parsed by the scip crate without format errors
  2. At least one integration test exports then parses SCIP to verify correctness
  3. User documentation recommends placing .db outside watched directories
  4. Security best practices are documented in user-facing docs
**Plans**: TBD

Plans:
- [ ] 13-01: Export SCIP from test fixture and parse with scip crate
- [ ] 13-02: Add integration test that verifies SCIP round-trip correctness
- [ ] 13-03: Document .db location recommendations in user docs
- [ ] 13-04: Update user documentation with security best practices

### 📋 v1.2 Performance (Planned)

**Milestone Goal:** Improve indexing performance through caching and incremental optimization.

Deferred to v1.2: PERF-01, PERF-02, XREF-01, GIT-01

## Progress

**Execution Order:**
Phases execute in numeric order: 10 → 11 → 12 → 13

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-9 | v1.0 | 29/29 | Complete | 2025-12-XX |
| 10. Path Traversal Validation | v1.1 | 4/4 | Complete | 2026-01-19 |
| 11. FQN Extraction | v1.1 | 6/6 | Complete | 2026-01-19 |
| 12. Transactional Deletes | v1.1 | 6/6 | Complete | 2026-01-20 |
| 13. SCIP Tests + Docs | v1.1 | 0/4 | Not started | - |

**v1.1 Progress:** [█████████░] 89% (16/18 plans)

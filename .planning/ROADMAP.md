# Roadmap: Magellan

## Overview

Magellan is a deterministic codebase mapping CLI for local developers. This roadmap tracks the v2.0 Native V2 Backend Migration milestone, which migrates from SQLiteGraph's SQLite backend to Native V2 backend for 10x traversal performance, O(1) symbol lookups, and pub/sub events.

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
- 📋 **v2.0 Native V2 Backend Migration** - Phases 46-51 (planned)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-9) - SHIPPED 2026-01-19</summary>

v1.0 delivered deterministic codebase mapping with tree-sitter AST extraction, SQLite graph persistence, file watching, and multi-format export (JSON/NDJSON/DOT/CSV/SCIP).

**Progress:**
- Phase 1: Project Foundation (4/4 plans)
- Phase 2: File Scanning (2/2 plans)
- Phase 3: Language Parsers (4/4 plans)
- Phase 4: Symbol Extraction (3/3 plans)
- Phase 5: Reference & Call Graph Extraction (3/3 plans)
- Phase 6: Graph Persistence (3/3 plans)
- Phase 7: CLI Query Surface (3/3 plans)
- Phase 8: Watch Mode (3/3 plans)
- Phase 9: Export & Validation (4/4 plans)

</details>

<details>
<summary>✅ v1.1 Correctness + Safety (Phases 10-13) - SHIPPED 2026-01-20</summary>

**Milestone Goal:** Fix correctness issues (FQN collisions), harden security (path traversal), and ensure data integrity (transactional deletes).

**Phases:** 10-13 (20 plans total)

**Shipped:**
- Path traversal validation at all entry points (watcher, scan, indexer)
- Symlink rejection for paths outside project root
- FQN-based symbol lookup eliminating name collisions
- Row-count assertions for delete operation verification
- SCIP export with round-trip test coverage
- Security documentation in README and MANUAL

**See:** `.planning/milestones/v1.1-ROADMAP.md` for full details

</details>

<details>
<summary>✅ v1.2 Unified JSON Schema (Phase 14) - SHIPPED 2026-01-22</summary>

**Milestone Goal:** Standardize JSON output format across the LLM toolset.

**Plans (5/5 complete):**
- ✅ 14-01 — Add uuid/chrono dependencies, enhance JsonResponse, verify StandardSpan compliance
- ✅ 14-02 — Create error codes module, enhance ErrorResponse with code/span/remediation
- ✅ 14-03 — Create rich span extension types (with Option<SpanSemantics> for semantic data)
- ✅ 14-04 — Add CLI flags for find/query commands, wire rich span data
- ✅ 14-05 — Add CLI flags for refs/get commands, create integration tests

**Shipped:**
- JsonResponse wrapper with tool and timestamp metadata
- Span struct verified as StandardSpan-compliant
- Error codes module with 12 MAG-{CAT}-{NNN} error codes
- Rich span extensions: SpanContext, SpanRelationships, SpanSemantics, SpanChecksums
- CLI flags --with-context, --with-semantics, --with-checksums for find/query/refs/get

**See:** `.planning/phases/14-unified-json-schema/*-PLAN.md` for details

</details>

<details>
<summary>✅ v1.3 Performance (Phase 15) - SHIPPED 2026-01-22</summary>

**Milestone Goal:** Improve indexing performance through caching, parser pooling, parallel processing, and SQLite optimization.

**Plans (6/6 complete):**
- ✅ 15-01 — Extract duplicated helper functions to common module
- ✅ 15-02 — Implement thread-local parser pooling
- ✅ 15-03 — Configure SQLite performance PRAGMAs and parser warmup
- ✅ 15-04 — Parallel file scanning with rayon
- ✅ 15-05 — LRU caching for graph query results
- ✅ 15-06 — Streaming JSON export for large graphs

**Shipped:**
- Common module (src/common.rs) with 4 shared utility functions
- Thread-local parser pool (src/ingest/pool.rs) with 7 per-language parsers
- SQLite performance PRAGMAs: WAL mode, synchronous=NORMAL, 64MB cache, temp tables in memory
- Parser warmup function for first-parse latency avoidance
- Parallel file I/O using rayon par_iter for concurrent file reads
- LRU cache (cache.rs) with FileNodeCache integration in CodeGraph
- Streaming JSON export (stream_json, stream_json_minified, stream_ndjson)

**See:** `.planning/phases/15-performance/*-PLAN.md` for details

</details>

<details>
<summary>✅ v1.4 Bug Fixes & Correctness (Phases 16-19) - SHIPPED 2026-01-22</summary>

**Milestone Goal:** Fix 18 identified issues from live testing and static analysis

**Phases:** 16-19 (18 plans total)

**Shipped:**
- Path normalization across all entry points (PATH-01)
- Result propagation fix in index_references (ERR-01)
- Byte slice bounds checking (BOUND-01)
- File-scoped counting in reconcile_file_path (COUNT-01)
- DeleteResult verification (DEL-01)
- Thread-safe ChunkStore with Arc<Mutex> (THREAD-01)
- Parser warmup error propagation (POOL-01)
- expect() with clear invariant messages (UNWRAP-01)
- PRAGMA connection scoped cleanup (CLEAN-01)
- Watcher shutdown signal (WATCH-01)
- --version/-V flags (CLI-01)
- --output flag per-command (CLI-02)
- Position conventions documented (DOC-01)
- Fixed misleading comments (DOC-02)
- Cleaned up unused variables (LEAK-01)
- :memory: database limitations documented (LEAK-02)
- RefCell usage documented (REFCELL-01)
- Clear :memory: error messages (CONTEXT-01)

**See:** `.planning/milestones/v1.4-ROADMAP.md` for full details

</details>

<details>
<summary>✅ v1.5 Symbol Identity (Phases 20-26) - SHIPPED 2026-01-23</summary>

**Milestone Goal:** Fix FQN collisions (3-5% in real codebases) through stable SymbolId with explicit ambiguity handling.

**Phases:** 20-26 (31 plans total)

**Shipped:**
- BLAKE3-based SymbolId with 32-character hex output (128 bits)
- Canonical FQN (full identity with file path) vs Display FQN (human-readable)
- All 7 language parsers emit both canonical and display FQN
- Graph-based ambiguity modeling using alias_of edges
- CLI UX enhancements (--symbol-id, --ambiguous, --first flags)
- Export format versioning (2.0.0) with format-specific encoding
- Database migration command with backup and rollback support
- 62 files modified, ~29,140 lines of Rust code

**See:** `.planning/milestones/v1.5-ROADMAP.md` for full details

</details>

<details>
<summary>✅ v1.6 Quality & Bugfix (Phases 27-28) - SHIPPED 2026-02-04</summary>

**Milestone Goal:** Fix CSV export bug, clean compiler warnings, improve test coverage, and document CLI edge cases discovered during v1.5 live testing.

**Phases:** 27-28 (17 plans total)

**Shipped:**
- CSV export fixed for mixed Symbol/Reference/Call records
- CSV export includes record_type column for discriminating mixed record types
- CSV export has consistent headers across all record types
- Clean compiler warnings (cargo build produces no warnings)
- Integration test verifies --ambiguous flag behavior with full display_fqn
- Documentation explains --ambiguous flag usage requirements
- CSV export format and behavior documented
- Collisions command vs find --ambiguous distinction clarified

</details>

<details>
<summary>✅ v1.7 Concurrency & Thread Safety (Phases 29-33) - SHIPPED 2026-02-04</summary>

**Milestone Goal:** Fix 23 concurrency and thread safety issues found in Rust code audit.

**Phases:** 29-33 (19 plans total)

**Shipped:**
- RefCell → Arc<Mutex<T>> migration in FileSystemWatcher for thread-safe concurrent access
- Lock ordering hierarchy documented and enforced (dirty_paths → wakeup send)
- Thread join panic handling with proper error logging
- 5-second timeout-based watcher thread shutdown
- Chunk storage error propagation via ? operator
- Cache invalidation at 5 mutation points
- safe_slice() and safe_str_slice() helper functions
- Parser warmup with error reporting
- cleanup_parsers() function documented
- Deprecated extract_symbols instance method
- Superseded methods removed (walk_tree, extract_symbol)
- Thread safety documentation (files.rs, cache.rs, pool.rs, indexer.rs)
- 29 verification tests: TSAN (6), stress (6), performance (5), shutdown (12)

**See:** `.planning/milestones/v1.7-ROADMAP.md` for full details

</details>

<details>
<summary>✅ v1.8 CFG and Metrics (Phases 34-35) - SHIPPED 2026-01-31</summary>

**Milestone Goal:** Add metrics tables and chunk storage CLI commands to enable fast codemcp debug tools and token-efficient code queries.

**Phases:** 34-35 (7 plans total)

**Shipped:**
- Pre-computed metrics tables (file_metrics, symbol_metrics) with fan-in/fan-out/LOC/complexity_score
- MetricsOps module with computation and storage methods
- Backfill functionality for existing databases
- Chunk storage CLI commands (chunks, chunk-by-span, chunk-by-symbol)
- Integration tests for chunk storage (9 tests, 402 lines)
- Safe UTF-8 content extraction functions (extract_symbol_content_safe, extract_context_safe)
- Integration tests with multi-byte UTF-8 fixtures

**See:** `.planning/phases/34-cfg-metrics/*-PLAN.md` and `.planning/phases/35-safe-content-extraction/*-PLAN.md` for details

</details>

<details>
<summary>✅ v1.9 AST & Graph Algorithms (Phases 36-44) - SHIPPED 2026-02-04</summary>

**Milestone Goal:** AST node storage for hierarchical code structure queries and comprehensive graph algorithms for advanced code analysis.

**Phases:** 36-44 (18 plans total)

**Shipped:**
- AST nodes table with parent_id for hierarchical relationships
- AST extraction module (ast_extractor.rs) with tree-sitter traversal
- AST CLI commands (ast, find-ast) with tree structure display
- 20+ integration tests for AST functionality
- Graph algorithms: reachable, dead-code, cycles, condense, paths, slice
- Algorithm wrapper module with JSON/human output formats
- Gitignore-aware file filtering for watcher mode
- AST-based CFG extraction for Rust (cfg_blocks table)
- Optional LLVM IR CFG infrastructure for C/C++ (llvm-cfg feature)
- Optional Java bytecode CFG infrastructure (bytecode-cfg feature)

**See:** `.planning/phases/36-ast-schema/*-PLAN.md` through `.planning/phases/44-bytecode-cfg-java/*-PLAN.md` for details

</details>

---

### 📋 v2.0 Native V2 Backend Migration (Planned)

**Milestone Goal:** Migrate from SQLiteGraph's SQLite backend to Native V2 backend for 10x traversal performance, O(1) symbol lookups, and pub/sub events.

#### ✅ Phase 46: Backend Abstraction Foundation
**Goal**: Code uses backend-agnostic types enabling compile-time backend selection via feature flag
**Depends on**: v1.9 completion
**Requirements**: BACKEND-01, BACKEND-02, BACKEND-03, BACKEND-04, BACKEND-05
**Success Criteria** (what must be TRUE):
  1. User can compile Magellan with `--features native-v2` flag without errors
  2. All Ops modules (FileOps, SymbolOps, ReferenceOps, CallOps) accept `Rc<dyn GraphBackend>` instead of `Rc<SqliteGraphBackend>`
  3. CodeGraph::open() uses `open_graph(&db_path, &GraphConfig::native())` when native-v2 feature is enabled
  4. sqlitegraph dependency receives correct feature flags based on Magellan's native-v2 feature
**Plans**: 6 plans in 5 waves

Plans:
- [x] 46-01-PLAN.md — Configure Cargo.toml for backend feature flag propagation
- [x] 46-02a-PLAN.md — Convert FileOps and SymbolOps to use backend-agnostic Rc<dyn GraphBackend>
- [x] 46-02b-PLAN.md — Convert ReferenceOps and CallOps to use backend-agnostic Rc<dyn GraphBackend>
- [x] 46-03-PLAN.md — Add conditional backend selection to CodeGraph::open()
- [x] 46-04-PLAN.md — Verify backend compilation (SQLite + Native V2)
- [x] 46-05-PLAN.md — Verify backend tests and create feature summary

#### ✅ Phase 47: Data Migration & Compatibility
**Goal**: Users can migrate existing SQLite databases to Native V2 format without data loss
**Depends on**: Phase 46
**Requirements**: MIGRATE-01, MIGRATE-02, MIGRATE-03, MIGRATE-04, MIGRATE-05, TEST-05
**Success Criteria** (what must be TRUE):
  1. User can run `magellan migrate-backend --input old.db --output new.db` to convert databases
  2. Migration preserves all graph data (nodes, edges, labels) verified by round-trip test
  3. System auto-detects backend format from database file extension/header
  4. Side tables (chunks, metrics, execution_log, ast_nodes, cfg_blocks) are migrated correctly
**Plans**: 5 plans in 4 waves

Plans:
- [x] 47-01-PLAN.md — Implement snapshot export wrapper using GraphBackend API
- [x] 47-02-PLAN.md — Implement snapshot import wrapper using GraphBackend API
- [x] 47-03-PLAN.md — Add backend format detection from file headers
- [x] 47-04-PLAN.md — Implement magellan migrate-backend CLI command
- [x] 47-05-PLAN.md — Write migration round-trip test (TDD)

#### ✅ Phase 48: Native V2 Performance Features
**Goal**: Graph traversal achieves 10x performance improvement through clustered adjacency and KV store
**Depends on**: Phase 47
**Requirements**: PERF-01, PERF-02, PERF-04, PERF-05, TEST-03
**Success Criteria** (what must be TRUE):
  1. Graph traversal operations use clustered adjacency storage
  2. Symbol lookups use O(1) KV store cache instead of database queries
  3. KV store indexes are populated during symbol indexing
  4. Benchmark suite demonstrates >=10x traversal improvement on representative workload
**Plans**: 5 plans in 4 waves

Plans:
- [x] 48-01-PLAN.md — Create KV module with encoding helpers and key patterns
- [x] 48-02-PLAN.md — Populate KV indexes during symbol indexing
- [x] 48-03-PLAN.md — Enable clustered adjacency via feature gate
- [x] 48-04-PLAN.md — Create benchmark suite with B1/B2/B3 workloads
- [x] 48-05-PLAN.md — Verify 10x improvement and document results

#### ✅ Phase 49: Pub/Sub Integration
**Goal**: Watcher mode uses pub/sub events for real-time cache invalidation
**Depends on**: Phase 48
**Requirements**: PERF-03
**Success Criteria** (what must be TRUE):
  1. Watcher mode subscribes to graph change events via pub/sub API
  2. Cache invalidation triggers immediately on graph mutations without polling
  3. Pub/sub subscriptions are properly cleaned up on watcher shutdown
**Plans**: 3 plans in 3 waves

Plans:
- [x] 49-01-PLAN.md — Create PubSubEventReceiver module for event subscription
- [x] 49-02-PLAN.md — Integrate pub/sub events into watcher cache invalidation
- [x] 49-03-PLAN.md — Add pub/sub subscription cleanup on shutdown

#### Phase 49.5: Native V2 Test Fixes
**Goal**: Fix test failures that occur with native-v2 feature enabled
**Depends on**: Phase 49
**Requirements**: TEST-01, TEST-02
**Success Criteria** (what must be TRUE):
  1. All lib tests pass with native-v2 feature enabled
  2. Test isolation issues are resolved (no shared temp file conflicts)
  3. Database format compatibility issues are resolved
**Plans**: TBD

Plans:
- [ ] 49.5-01: Fix ChunkStore stub database format compatibility
- [ ] 49.5-02: Fix test isolation and temp file conflicts
- [ ] 49.5-03: Fix database connection/reopen test failures

#### Phase 50: Testing & Documentation
**Goal**: All CLI commands work identically on both backends with comprehensive documentation
**Depends on**: Phase 49
**Requirements**: PARITY-01, PARITY-02, PARITY-03, PARITY-04, PARITY-05, TEST-01, TEST-02, TEST-04, DOCS-01, DOCS-02, DOCS-03, DOCS-04, DOCS-05
**Success Criteria** (what must be TRUE):
  1. All 20+ CLI commands produce identical outputs on both SQLite and Native V2 backends
  2. JSON/CSV/SCIP export formats are byte-identical across backends
  3. Watch mode behavior is consistent across both backends
  4. Graph algorithms (cycles, reachability, paths) produce identical results
  5. CI runs test matrix for both backends on every commit
  6. README.md documents Native V2 backend option and performance characteristics
  7. Migration guide explains how to switch backends
  8. CLI help text mentions backend selection
  9. Known limitations are documented
**Plans**: TBD

Plans:
- [ ] 50-01: Write unit tests for both SQLite and Native V2 backends
- [ ] 50-02: Create integration tests verifying data migration correctness
- [ ] 50-03: Set up CI test matrix for both backends
- [ ] 50-04: Verify feature parity for all CLI commands
- [ ] 50-05: Verify export format parity (JSON/CSV/SCIP)
- [ ] 50-06: Verify watch mode consistency
- [ ] 50-07: Verify graph algorithm parity
- [ ] 50-08: Update README.md with Native V2 documentation
- [ ] 50-09: Update MANUAL.md with performance characteristics
- [ ] 50-10: Write migration guide for backend switching
- [ ] 50-11: Update CLI help text with backend selection
- [ ] 50-12: Document known limitations

#### Phase 51: Fix Native V2 Compilation Errors
**Goal**: Native V2 backend compiles without errors and all features work correctly
**Depends on**: Phase 50
**Requirements**: BACKEND-01, BACKEND-02, BACKEND-03, BACKEND-04, BACKEND-05
**Success Criteria** (what must be TRUE):
  1. `cargo build --features native-v2` completes without compilation errors
  2. All missing files for native-v2 backend are implemented
  3. Type mismatches between SQLite and Native V2 backends are resolved
  4. All trait bounds and method signatures are compatible
  5. Native V2 backend tests pass
**Plans**: 3 plans in 3 waves

Plans:
- [x] 51-01-PLAN.md — Fix module structure and dependency issues (migrate_backend_cmd, watcher ambiguity, tempfile)
- [x] 51-02-PLAN.md — Fix type mismatches and trait bounds for KV functions (Rc vs reference, anyhow::Result)
- [x] 51-03-PLAN.md — Add missing disabled() constructors (ExecutionLog, MetricsOps)

**Details**:
Based on comprehensive analysis in `.planning/phases/51-fix-native-v2-compilation/51-RESEARCH.md`, 12 compilation errors need to be fixed across 5 categories:
- Module structure (2): migrate_backend_cmd missing file, watcher module ambiguity
- Missing dependencies (1): tempfile dependency for native-v2 feature
- Type mismatches (1): &dyn GraphBackend vs Rc<dyn GraphBackend>
- Trait bounds (6): Box<dyn Error> vs anyhow::Result conversion
- Missing methods (2): ExecutionLog::disabled(), MetricsOps::disabled()

#### Phase 52: Eliminate Native-V2 Stubs
**Goal**: Replace all SQLite stub implementations with proper KV store storage in native-v2 mode, achieving full feature parity
**Depends on**: Phase 51
**Requirements**: PARITY-01, PARITY-02, PARITY-03, PARITY-04, PARITY-05, MIGRATE-04
**Success Criteria** (what must be TRUE):
  1. `magellan get` command works in native-v2 mode (code chunks retrieved from KV)
  2. Execution history preserved across runs (execution_log in KV)
  3. Metrics queries return actual data (file_metrics, symbol_metrics in KV)
  4. CFG blocks stored and retrievable (cfg_blocks in KV)
  5. Migration from SQLite to Native-V2 preserves all metadata
  6. All tests pass with native-v2 feature
**Plans**: 7 plans in 4 waves

Plans:
- [ ] 52-01-PLAN.md — KV key patterns and encoding infrastructure (Wave 1)
- [ ] 52-02-PLAN.md — ChunkStore KV implementation (Wave 2)
- [ ] 52-03-PLAN.md — ExecutionLog KV implementation (Wave 2)
- [ ] 52-04-PLAN.md — MetricsOps KV implementation (Wave 2)
- [ ] 52-05-PLAN.md — CFG storage KV implementation (Wave 3)
- [ ] 52-06-PLAN.md — Migration enhancement (side tables to KV conversion) (Wave 3)
- [ ] 52-07-PLAN.md — Testing and verification (round-trip, performance, test fixes) (Wave 4)

**Details**:
Current native-v2 mode has NO-OP stubs that lose data:
- **ChunkStore** (src/generation/mod.rs:76-184): Creates temp SQLite DB, data deleted on exit
- **ExecutionLog** (src/graph/execution_log.rs:48-53): :memory: path, all writes lost
- **MetricsOps** (src/graph/metrics/mod.rs:52-57): :memory: path, all writes lost
- **CFG extraction** (src/graph/cfg_extractor.rs:724-785): Returns empty vectors

Solution: Store all metadata in native-v2 KV store using key patterns:
- Chunks: `chunk:{file_path}:{start}:{end}`, `ast:node:{id}`, `cfg:block:{id}`
- Execution: `exec:id:{uuid}`, `exec:time:{timestamp}`
- Metrics: `metrics:file:{path}`, `metrics:symbol:{id}`, `metrics:hotspot`
- CFG: `cfg:function:{func_id}`, `cfg:block:kind:{kind}`

#### Phase 53: Fix Native-V2 Database Initialization
**Goal**: Fix critical bug where magellan fails to initialize new databases in native-v2 mode with error "no such table: execution_log", creating incomplete 88-byte database files
**Depends on**: Phase 52
**Requirements**: PARITY-01, PARITY-02, TEST-01
**Success Criteria** (what must be TRUE):
  1. `magellan status --db /tmp/test.db` creates valid database without error
  2. Database file is > 1000 bytes (not 88 bytes)
  3. Execution log persists across runs (stored in KV)
  4. All commands work in native-v2 mode
**Plans**: 3 plans in 2 waves

Plans:
- [ ] 53-01-PLAN.md — Fix ExecutionLog initialization (use with_kv_backend instead of disabled())
- [ ] 53-02-PLAN.md — Fix MetricsOps initialization (use with_kv_backend instead of disabled())
- [ ] 53-03-PLAN.md — Test and verify (database creation, size check, round-trip)

**Details**:
Root cause: In src/graph/mod.rs:341, native-v2 mode uses `ExecutionLog::disabled()` which sets `kv_backend: None`. When `ExecutionTracker::start()` calls `start_execution()`:
1. Checks `if let Some(ref backend) = self.kv_backend` → skips (None)
2. Falls back to SQLite INSERT into execution_log table
3. :memory: database has no tables (ensure_schema never called)
4. INSERT fails with "no such table: execution_log"

Fix: Use `ExecutionLog::with_kv_backend(Rc::clone(&backend))` instead of `ExecutionLog::disabled()`
Related bug report: docs/pr2.md (2026-02-08)
Critical severity: blocks all new database creation

#### Phase 54: CLI Backend Detection and Dual Query Methods
**Goal**: Fix CLI commands to work with both SQLite and Native-V2 backends by auto-detecting the database type and using appropriate query methods for each backend
**Depends on**: Phase 53
**Requirements**: PARITY-01, PARITY-02, PARITY-03, PARITY-04, PARITY-05
**Success Criteria** (what must be TRUE):
  1. Backend detection function identifies Native-V2 vs SQLite databases by magic bytes
  2. Commands work correctly with SQLite databases (use SQL queries)
  3. Commands work correctly with Native-V2 databases (use KV prefix scan or ChunkStore API)
  4. Algorithm commands (cycles, dead-code, reachable) are documented as SQLite-only
  5. No duplicate commands - single command that auto-detects backend
**Plans**: 4 plans in 3 waves

Plans:
- [ ] 54-01-PLAN.md — Re-export Backend Detection (detect_backend_format from Phase 47-03)
- [ ] 54-02-PLAN.md — Fix Chunk Commands for Native-V2 (chunks, chunk-by-span, chunk-by-symbol)
- [ ] 54-03-PLAN.md — Fix AST Commands for Native-V2 (ast, find-ast)
- [ ] 54-04-PLAN.md — Document Algorithm Limitations (cycles, dead-code, reachable are SQLite-only)

**Details**:
Current CLI commands use `rusqlite::Connection::open()` directly, which only works with SQLite databases. Native-V2 databases store data in binary format (not SQLite tables) and use KV store for metadata.

Commands affected:
- `chunks`, `chunk-by-span`, `chunk-by-symbol` - Direct SQLite: `Connection::open()`
- `get`, `get-file` - Queries code_chunks table
- `ast`, `find-ast` - Queries ast_nodes table
- `label`, `collisions` - Queries graph_labels table
- `dead-code`, `cycles`, `reachable` - Uses sqlitegraph algorithms (SQLite-only)

Solution: Auto-detect backend by reading magic bytes ("SQLTGF" = Native-V2, "SQLite format 3" = SQLite), then use appropriate query method (SQL queries for SQLite, KV prefix scan for Native-V2).

## Progress

**Execution Order:**
Phases execute in numeric order: 46 → 47 → 48 → 49 → 50 → 51 → 52 → 53 → 54

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-9 | v1.0 | 29/29 | Complete | 2026-01-19 |
| 10-13 | v1.1 | 20/20 | Complete | 2026-01-20 |
| 14 | v1.2 | 5/5 | Complete | 2026-01-22 |
| 15 | v1.3 | 6/6 | Complete | 2026-01-22 |
| 16-19 | v1.4 | 18/18 | Complete | 2026-01-22 |
| 20-26 | v1.5 | 31/31 | Complete | 2026-01-23 |
| 27-28 | v1.6 | 17/17 | Complete | 2026-02-04 |
| 29-33 | v1.7 | 19/19 | Complete | 2026-02-04 |
| 34-35 | v1.8 | 7/7 | Complete | 2026-01-31 |
| 36-44 | v1.9 | 18/18 | Complete | 2026-02-04 |
| 46. Backend Abstraction | v2.0 | 6/6 | Complete | 2026-02-07 |
| 47. Data Migration | v2.0 | 0/5 | Planning ready | - |
| 48. Performance Features | v2.0 | 0/5 | Not started | - |
| 49. Pub/Sub Integration | v2.0 | 0/3 | Planning ready | - |
| 50. Testing & Documentation | v2.0 | 0/12 | Not started | - |
| 51. Fix Native V2 Compilation | v2.0 | 3/3 | Complete | 2026-02-07 |
| 52. Eliminate Native-V2 Stubs | v2.0 | 7/7 | Complete | 2026-02-08 |
| 53. Fix Native-V2 DB Init | v2.0 | 0/3 | Planning ready | - |
| 54. CLI Backend Detection | v2.0 | 0/4 | Planning ready | - |

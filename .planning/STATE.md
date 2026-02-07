# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-06)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** Phase 49 - Pub/Sub Integration (COMPLETE)

## Current Position

Phase: 49 of 50 (Pub/Sub Integration)
Plan: 3 of 3 in current phase
Status: Phase complete
Last activity: 2026-02-07 — Completed 49-03-PLAN.md (Pub/sub shutdown and CLI integration)

Progress: [██████████████████░] 89.0% (186/209 total plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 170 (v1.0 through v1.9)
- Average duration: ~12 min
- Total execution time: ~34 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1-9 (v1.0) | 29 | ~7h | ~15 min |
| 10-13 (v1.1) | 20 | ~5h | ~15 min |
| 14 (v1.2) | 5 | ~1h | ~12 min |
| 15 (v1.3) | 6 | ~1.5h | ~15 min |
| 16-19 (v1.4) | 18 | ~3h | ~10 min |
| 20-26 (v1.5) | 31 | ~5h | ~10 min |
| 27-28 (v1.6) | 17 | ~2h | ~7 min |
| 29-33 (v1.7) | 19 | ~2.5h | ~8 min |
| 34-35 (v1.8) | 7 | ~1h | ~9 min |
| 36-44 (v1.9) | 18 | ~1.5h | ~5 min |

**Recent Trend:**
- Last 5 plans: ~5-8 min each (v1.9 phases focused on modular features)
- Trend: Fast (focused infrastructure with minimal changes)

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

**From Phase 48-02 (KV Index Population and Invalidation):**
- Implemented populate_symbol_index() to create sym:fqn:{fqn} → SymbolId mappings during indexing
- Implemented lookup_symbol_by_fqn() for O(1) symbol lookup by fully-qualified name
- Implemented invalidate_file_index() to delete KV entries before reindex/deletion
- Added sym:fqn_of:{id} reverse index for efficient invalidation (no graph queries needed)
- Integrated KV population into index_file() after symbol insertion (same WAL transaction)
- Integrated KV invalidation into delete_file_facts() before symbol deletion
- Function signatures use &dyn GraphBackend (consistent with codebase patterns)
- Use anyhow::Result instead of Box<dyn std::error::Error> for error handling
- Lazy cleanup strategy: sym:fqn:* entries overwritten on reindex, not deleted individually

**From Phase 48-04 (Performance Benchmark Suite):**
- Created benchmark harness with test graph generation functions (harness.rs)
- Implemented B1 neighbor expansion benchmark (100 nodes x 100 neighbors)
- Implemented B2 reachability traversal benchmark (depth-10 BFS)
- Implemented B3 symbol lookup benchmark (SQL vs KV comparison)
- Added __backend_for_benchmarks() to CodeGraph for direct backend access in benchmarks
- Used Rc<dyn GraphBackend> return type to match internal storage (not Arc)
- Hardcoded test symbol names in B3 benchmark instead of using find_by_symbol_id (not public API)
- Configured Criterion harness in Cargo.toml with harness = false
- Baseline metrics established: B1 (3.4µs), B2 (26µs), B3 (71ns per lookup)

**From Phase 48-03 (Clustered Adjacency Feature Flag):**
- Added native-v2-perf feature flag to Cargo.toml propagating v2_experimental to sqlitegraph
- Clustered adjacency automatically enabled when v2_experimental feature is present in sqlitegraph
- Documented algorithms.rs limitation: uses SqliteGraphBackend directly, requires SQLite backend
- Tiered feature structure: native-v2 (base) → native-v2-perf (experimental with clustering)
- Added comprehensive feature flag documentation to lib.rs for users
- Feature gate enables A/B benchmarking in Phase 48-04

**From Phase 48-01 (KV Index Module Infrastructure):**
- Created src/kv/ module with encoding, keys, and public API (feature-gated to native-v2)
- Used flat_map pattern for encode_symbol_ids (not map) to avoid intermediate allocations
- Key format: namespace:value pattern (e.g., "sym:fqn:{fqn}") for readability and prefix scans
- All key functions return Vec<u8> to match KvStore API requirements (avoid String conversion)
- Public API stubs defined now but implementation deferred to 48-02 (clean separation of infrastructure vs logic)
- Added criterion 0.5 dev-dependency for benchmark suite (48-04 preparation)

**From Phase 49-02 (FileSystemWatcher Pub/Sub Integration):**
- Integrated pub/sub components into FileSystemWatcher struct (feature-gated to native-v2)
- Created with_pubsub() constructor with graceful degradation on subscription failure
- Added recv_batch_merging() method for combined filesystem + pub/sub event reception
- Prioritize filesystem events over pub/sub events in recv_batch_merging()
- Added CodeGraph::__backend_for_watcher() to expose backend for pub/sub subscription
- Use Box<PubSubEventReceiver> for size erasure in struct field
- Thread-safe backend uses Arc<dyn GraphBackend + Send + Sync>

**From Phase 49-03 (Pub/Sub Shutdown and CLI Integration):**
- Implemented Drop for PubSubEventReceiver using ManuallyDrop pattern
- Added shutdown() method to consume receiver and join thread cleanly
- Implemented Drop for FileSystemWatcher to clean up pub/sub receiver
- Added shutdown() method to FileSystemWatcher for explicit cleanup
- Integrated pub/sub into watch pipeline (native-v2 feature only)
- Created watcher_loop_with_native_backend for separate backend connection
- Created integration tests for pub/sub lifecycle
- Commits: 3fa5ae8, f843489, 668e0a8, 57d4680

**From Phase 49-01 (PubSubEventReceiver Module):**
- Created PubSubEventReceiver module for Native V2 backend event subscription
- Subscribe to all graph mutation events via SubscriptionFilter::all()
- Extract file_path from NodeChanged events using GraphBackend::get_node with SnapshotId
- Use Arc<dyn GraphBackend + Send + Sync> for thread-safe backend sharing (Rc is not Send)
- Skip EdgeChanged events (edge_id cannot be decoded via GraphBackend trait - no get_edge method)
- Skip KVChanged events (cannot extract file path from key_hash)
- Send file paths via mpsc::channel to main thread (FileNodeCache is not thread-safe)
- Event loop uses 100ms timeout for responsive shutdown checking
- Module is feature-gated to native-v2 (src/watcher/pubsub_receiver.rs)

**From Phase 46-03 (Conditional Backend Selection):**
- Added #[cfg(feature = "native-v2")] for NativeGraphBackend initialization
- Added #[cfg(not(feature = "native-v2"))] for SqliteGraphBackend initialization
- Wrapped SQLite-specific PRAGMA configuration in #[cfg(not(feature = "native-v2"))]
- Both backend paths produce Rc<dyn GraphBackend> for use with Ops modules
- Use NativeGraphBackend constructors directly instead of open_graph() factory (Box<dyn GraphBackend> cannot wrap in Rc)
- Native backend mirrors SqliteGraph::open() behavior: open if exists, create if not

**From Phase 46-02b (Ops Backend Conversion):**
- Upgraded to sqlitegraph 1.5.0 which adds delete_entity() and entity_ids() directly to GraphBackend trait
- Changed ReferenceOps and CallOps to use Rc<dyn GraphBackend> instead of Rc<SqliteGraphBackend>
- Removed .graph() accessor pattern in favor of direct trait method calls (self.backend.delete_entity())
- Commented out SQLite-specific label functionality (not available on trait object)
- Known limitation: algorithms.rs module uses concrete SqliteGraph type, requires future work

**From Phase 46-01 (Feature Flag Configuration):**
- Disable sqlitegraph default features to prevent dual-backend compilation
- Propagate native-v2 feature flag from magellan to sqlitegraph dependency
- Compile-time backend selection ensures zero runtime overhead for unused backend

**Key decisions from previous milestones:**
- [v1.7] RefCell → Mutex migration in FileSystemWatcher for thread-safe concurrent access
- [v1.7] Lock ordering hierarchy: dirty_paths → graph locks → wakeup channel
- [v1.5] Use BLAKE3 for SymbolId (128-bit, 32 hex chars) for collision resistance
- [v1.5] Split Canonical FQN (identity) vs Display FQN (human-readable)
- [v1.3] Thread-local parser pool for performance (7 per-language parsers)
- [v1.3] SQLite PRAGMAs: WAL mode, synchronous=NORMAL, 64MB cache

### Pending Todos

None yet.

### Blockers/Concerns

**From v2.0 Research:**
- Type signature changes (Rc<SqliteGraphBackend> → Rc<dyn GraphBackend>) affect all modules - foundational work must compile first
- Data format incompatibility between SQLite and Native V2 - explicit migration command required
- Side tables (chunks, metrics, execution_log, ast_nodes, cfg_blocks) use rusqlite directly - may need dual-connection handling
- Real-world performance validation needed - 10x claims from sqlitegraph benchmarks need Magellan-specific verification
- KV index maintenance strategy - populate during indexing vs lazy loading decision pending
- **algorithms.rs compatibility**: The algo::* functions in sqlitegraph require concrete SqliteGraph type, not available through trait object. Needs architectural solution (trait extension or conditional compilation)

**Research flags for planning:**
- Phase 48: KV index design strategy needs performance testing for optimal index keys
- Phase 49: Pub/Sub event filtering semantics may need research for optimal subscription filters

## Session Continuity

Last session: 2026-02-07
Stopped at: Completed 49-03-PLAN.md (Pub/sub shutdown and CLI integration)
Resume file: None
Blockers:
- algorithms.rs module uses concrete SqliteGraph type - requires conditional compilation to work with Native backend
- 305 tests fail with native-v2 feature due to algorithms.rs limitation (verified in 46-05)
- Pre-existing test failures: migration_tests expects schema v5 (actual is v7), parser_tests trait parsing issues

**From Phase 49-02 (FileSystemWatcher Pub/Sub Integration):**
- Integrated pub/sub components into FileSystemWatcher struct
- Created with_pubsub() constructor with graceful degradation
- Added recv_batch_merging() method for combined event reception
- Added CodeGraph::__backend_for_watcher() for backend access
- Fixed type mismatch (Arc<dyn GraphBackend> → Arc<dyn GraphBackend + Send + Sync>)
- Fixed conditional field initialization (cannot use cfg in struct construction)
- Commits: c88e2d4, e9a84ee, b0a33a0, 7d6ebba, 35dcbec

**From Phase 49-01 (PubSubEventReceiver Module):**
- Created PubSubEventReceiver module (253 lines)
- Implemented event loop with timeout-based shutdown checking
- Extracted file_path from NodeChanged events
- Fixed thread safety issue (Rc → Arc for Send)
- Fixed duplicate cfg attribute (removed from pubsub_receiver.rs)
- Fixed signal handling loop in watch_cmd.rs (clippy never_loop)
- Fixed snapshot_id type mismatch (SnapshotId wrapper)
- Commit: abc264e

**From Phase 48-02 (KV Index Population and Invalidation):**
- Implemented populate_symbol_index(), lookup_symbol_by_fqn(), invalidate_file_index()
- Added sym_fqn_of_key() to keys.rs for reverse FQN lookup during invalidation
- Integrated KV population into index_file() after symbol insertion
- Integrated KV invalidation into delete_file_facts() before symbol deletion
- All 16 KV tests pass (5 encoding + 8 keys + 2 module + 1 sym_fqn_of)
- Commits: 501d14a (KV functions), 62fdc5c (integration)

**From Phase 48-01 (KV Index Module Infrastructure):**
- Created src/kv/ module with encoding.rs, keys.rs, mod.rs (feature-gated to native-v2)
- Implemented Vec<i64> <-> Vec<u8> encoding using flat_map + to_le_bytes pattern
- Added key construction helpers for all index patterns (sym:fqn:, sym:id:, file:path:, file:sym:, sym:rev:)
- Defined public API stubs (populate_symbol_index, invalidate_file_index, lookup_symbol_by_fqn)
- Added criterion 0.5 dev-dependency for benchmark suite
- All 15 tests pass (5 encoding + 8 keys + 2 module)

**From Phase 46-05 (Backend Test Verification):**
- SQLite backend: 820 tests pass, 17 fail (96.7% pass rate)
- Native V2 backend: 532 tests pass, 305 fail (62.7% pass rate)
- All BACKEND-01 through BACKEND-05 requirements verified as satisfied
- algorithms.rs module identified as main Native V2 blocker (uses SqliteGraph concrete type)
- Pre-existing test failures documented: migration_tests expects schema v5 (actual is v7), parser_tests trait parsing issues

**From Phase 46-04 (Backend Compilation Verification):**
- Both SQLite and Native V2 backends compile successfully
- Build environment note: RUSTC_WRAPPER must be unset if sccache is not available

## v2.0 Milestone Summary

**Milestone Goal:** Migrate from SQLiteGraph's SQLite backend to Native V2 backend for 10x traversal performance, O(1) symbol lookups, and pub/sub events.

**Phases:** 46-50 (29 plans planned)
- Phase 46: Backend Abstraction Foundation (4 plans) - Type signature changes, feature flag propagation
- Phase 47: Data Migration & Compatibility (5 plans) - Snapshot export/import, backend detection, migration CLI
- Phase 48: Native V2 Performance Features (5 plans) - KV store indexing, clustered adjacency, benchmarks
- Phase 49: Pub/Sub Integration (3 plans) - Event subscription, cache invalidation, cleanup
- Phase 50: Testing & Documentation (12 plans) - Feature parity, CI matrix, documentation updates

**Total v2.0 Requirements:** 30
- Backend Infrastructure: 5 (BACKEND-01 to BACKEND-05)
- Data Migration: 5 (MIGRATE-01 to MIGRATE-05)
- Performance Features: 5 (PERF-01 to PERF-05)
- Feature Parity: 5 (PARITY-01 to PARITY-05)
- Testing: 5 (TEST-01 to TEST-05)
- Documentation: 5 (DOCS-01 to DOCS-05)

**Coverage:** All 30 v2.0 requirements mapped to phases 46-50.

**Next Step:** Execute 48-05-PLAN.md (Run A/B comparison benchmarks) to quantify clustered adjacency performance improvement.

## Session Continuity

Last session: 2026-02-06
Stopped at: Completed 48-04-PLAN.md (Performance benchmark suite)
Resume file: None
Blockers:
- algorithms.rs module uses concrete SqliteGraph type - requires conditional compilation to work with Native backend
- 305 tests fail with native-v2 feature due to algorithms.rs limitation (verified in 46-05)
- Pre-existing test failures: migration_tests expects schema v5 (actual is v7), parser_tests trait parsing issues

**From Phase 48-04 (Performance Benchmark Suite):**
- Created benchmark harness with setup_test_graph(), setup_large_graph(), setup_high_fanout_graph()
- Implemented B1 (neighbor expansion), B2 (reachability), B3 (symbol lookup) benchmarks
- Added __backend_for_benchmarks() to CodeGraph for direct backend access
- Baseline metrics: B1 (3.4µs), B2 (26µs), B3 (71ns per lookup)
- Commits: 32fa712 (harness), 9f2bc7a (benchmarks)

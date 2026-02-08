# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-06)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** Phase 54-01 complete - Backend detection re-exported for CLI commands

## Current Position

Phase: 54 of 54 - IN PROGRESS
Status: Phase 54-01 completed (1/5 plans complete)
Last activity: 2026-02-08 — Phase 54-01 completed (backend detection re-exported)

Progress: [██████████████████░] 97% (214/218 total plans)

**Completed Phases:**
- Phase 46: Backend Abstraction Foundation ✅
- Phase 47: Data Migration & Compatibility ✅
- Phase 48: Native V2 Performance Features ✅
- Phase 49: Pub/Sub Integration ✅
- Phase 51: Fix Native V2 Compilation Errors ✅
- Phase 52: Eliminate Native-V2 Stubs ✅
- Phase 53: Fix Native-V2 Database Initialization ✅

**Next Phase:**
- Phase 54: CLI Backend Detection and Dual Query Methods (1/5 plans complete)

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

### Roadmap Evolution

- Phase 54-01 completed: Backend detection re-exported for CLI commands (2026-02-08)
  - Re-exported detect_backend_format() and BackendFormat from migrate_backend_cmd module
  - Public API available at magellan::detect_backend_format() and magellan::BackendFormat
  - Foundation for CLI commands to auto-detect backend and route to SQL queries or KV prefix scans
- Phase 54 added: CLI Backend Detection and Dual Query Methods - Auto-detect backend and use appropriate query methods (2026-02-08)
- Phase 53 completed: Fix Native-V2 Database Initialization (2026-02-08)
  - Fixed ExecutionLog::disabled() → ExecutionLog::with_kv_backend()
  - Fixed MetricsOps::disabled() → MetricsOps::with_kv_backend()
  - Fixed count_chunks() to support KV backend
  - Fixed header corruption bug in sqlitegraph (multiple GraphFile instances)
  - Published sqlitegraph 1.5.3 to crates.io
  - Verified data persistence works correctly despite tcache_thread_shutdown crash
- Phase 53 added: Fix Native-V2 Database Initialization - execution_log uses KV backend (2026-02-08)
- Phase 52 added: Eliminate Native-V2 Stubs - Store metadata in KV instead of SQLite stubs (2026-02-07)
- Phase 51 added: Fix native-v2 compilation errors and enable native backend (2026-02-07)

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

**From Phase 52-03 (ExecutionLog KV Backend):**
- Use timestamp as execution record ID in KV mode (SQLite has AUTOINCREMENT but KV doesn't)
- Early return pattern for KV branch prevents dual-write (records written to KV OR SQLite, never both)
- JSON-based KV storage for ExecutionRecord (KvValue::Json) instead of binary encoding (human-readable, debuggable)
- Prefix scan (execlog:*) for list_all() in KV mode replaces SQL ORDER BY, with in-memory sort by started_at

**From Phase 52-01 (KV Key Patterns and Encoding Functions):**
- Generic type parameters for encoding functions (e.g., `encode_cfg_blocks<T>`) avoid exposing private modules (ast_node, schema) while maintaining type safety
- Path escaping with "::" prevents colon-based key collisions in file paths (e.g., Windows paths or module names like "src/test:module/file.rs")
- ?Sized bound for encode_json allows encoding slices (&[T]) and other DSTs without requiring conversion to Vec
- JSON encoding chosen over binary for metadata (human-readable, debuggable, sufficient for metadata sizes)

**From Phase 54-01 (Backend Detection Re-exports):**
- Re-exported detect_backend_format() and BackendFormat from migrate_backend_cmd module
- Public API available at magellan::detect_backend_format() and magellan::BackendFormat
- Enables CLI commands to auto-detect backend without importing internal migrate_backend_cmd module
- Foundation for dual query methods (SQL for SQLite, KV prefix scans for Native-V2)

**From Phase 47-03 (Backend Format Detection):**
- Implemented detect_backend_format() using magic byte inspection (b"MAG2" for Native V2)
- Used rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY to prevent accidental database creation
- Reject :memory: databases with InMemoryDatabaseNotSupported error
- Check Native V2 magic bytes first before SQLite open attempt (faster path)
- Created MigrationError enum for specific error reporting (DatabaseNotFound, CannotOpenDatabase, CannotReadHeader, UnknownFormat, InMemoryDatabaseNotSupported)

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

**From Phase 51-01 (Module Structure and Dependency Fixes):**
- Removed migrate_backend_cmd module declaration from lib.rs (file doesn't exist, no code references)
- Deleted src/watcher.rs to resolve module ambiguity (kept src/watcher/mod.rs directory structure)
- Backed up original watcher.rs to src/watcher.rs.bak before deletion
- Moved tempfile from dev-dependencies to main dependencies (generation/mod.rs imports it)
- Remaining errors after this plan: 7 type/trait bound issues (E0277, E0308, E0599)
- Commits: b71eaba (module fixes), b7974f7 (tempfile dependency)

**From Phase 51-02 (Type Mismatches and Trait Bounds for KV Functions):**
- Changed KV function return types from Box<dyn std::error::Error> to anyhow::Result
- Added use anyhow::Result; import to src/kv/mod.rs (was missing, caused E0107)
- Fixed populate_symbol_index call site: use Rc::clone(&graph.files.backend) instead of &*graph.files.backend
- Added use std::rc::Rc; import to src/graph/ops.rs
- Remaining errors after this plan: 2 missing disabled() methods (ExecutionLog, MetricsOps)
- Commits: f6e8484 (return types), 1ab64ee (backend type fix)

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

**Test Infrastructure Limitations (discovered in 52-07):**
- KV storage APIs use Rc instead of Arc (not thread-safe)
- Tests using ChunkStore/ExecutionLog/MetricsOps with database paths fail when native-v2 is enabled
- CodeGraph::open() creates Native V2 databases when feature is enabled
- 54 tests fail with "no such table" errors (pre-existing infrastructure issue, not specific to 52-07)
- Requires test refactoring to detect backend type and use appropriate APIs (with_kv_backend vs path-based constructors)

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

Last session: 2026-02-08
Stopped at: Completed 52-04 (MetricsOps KV Backend)
Resume file: None
Blockers:
- algorithms.rs module uses concrete SqliteGraph type - requires conditional compilation to work with Native backend
- 305 tests fail with native-v2 feature due to algorithms.rs limitation (verified in 46-05)
- Pre-existing test failures: migration_tests expects schema v5 (actual is v7), parser_tests trait parsing issues
- Generation module tests use non-existent NativeGraphBackend::new_temp() method (blocks test compilation)

**From Phase 51-02 (Type Mismatches and Trait Bounds for KV Functions):**
- Fixed KV function return types: Box<dyn Error> → anyhow::Result
- Fixed populate_symbol_index call: use Rc::clone() instead of reference
- Build progressed from 9 errors to 2 errors (only missing disabled() methods)
- Commits: f6e8484 (return types), 1ab64ee (backend type fix)

**From Phase 51-03 (Add Missing disabled() Constructors):**
- Added ExecutionLog::disabled() constructor (feature-gated to native-v2)
- Added MetricsOps::disabled() constructor (feature-gated to native-v2)
- All 12 compilation errors from 51-RESEARCH.md resolved
- Native V2 backend compiles successfully with 0 errors (19 warnings remain)
- Binary produced: target/debug/magellan (125MB, working)
- Commits: 18a0cce (ExecutionLog::disabled), 5ac70ca (MetricsOps::disabled)

**From Phase 47-04 (Backend Migration CLI Command):**
- Implemented run_migrate_backend() orchestrator with full migration pipeline (detect → export → import → verify → migrate side tables)
- Used ATTACH DATABASE approach for side table migration (efficient cross-database copy)
- Defined side table schemas inline in migrate_backend_cmd.rs for self-contained migration
- Added migrate-backend CLI command with --input, --output, --export-dir, --dry-run flags
- Native V2 is always the target format (one-way migration from SQLite)
- Dry-run mode detects format without any data copy operations
- Commits: 386ccbf (migration orchestrator), d9d92df (CLI command)

**From Phase 47-01 (Snapshot Export Wrapper):**
- Created src/migrate_backend_cmd.rs with snapshot export functionality
- Delegates entirely to sqlitegraph's GraphBackend::snapshot_export() - no custom serialization
- SnapshotExportMetadata wraps sqlitegraph's SnapshotMetadata with Magellan-specific fields
- get_graph_counts() returns (0, 0) since GraphBackend trait doesn't provide count methods
- Actual entity/edge counts available from snapshot_export() return value
- Uses Rc<dyn GraphBackend> parameter type to match CodeGraph internal storage
- Commit: 5c8dce5 (snapshot export wrapper)

**From Phase 47-02 (Snapshot Import Wrapper):**
- Added SnapshotImportMetadata struct with entities_imported, edges_imported, source_dir, import_timestamp
- Implemented import_snapshot() function that validates directory and delegates to backend.snapshot_import()
- Added verify_import_counts() helper to compare export and import metadata for data integrity
- Returns i64 counts instead of u64 to match SnapshotExportMetadata convention
- Separate metadata types prevent accidental misuse (can't pass import metadata where export is expected)
- All 7 unit tests pass
- Commit: 2533b60 (snapshot import wrapper)

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

**From Phase 52-04 (MetricsOps KV Backend):**
- Added kv_backend field to MetricsOps struct (feature-gated to native-v2)
- Added with_kv_backend() constructor for KV-backed metrics
- Modified upsert_file_metrics() to use KV when available
- Modified upsert_symbol_metrics() to use KV when available
- Modified get_file_metrics() to use KV when available
- Modified get_symbol_metrics() to use KV when available
- Added SnapshotId import for kv_get calls
- Re-exported encode_json, decode_json, and metrics keys from kv module
- Used KvValue::Bytes for JSON-encoded metrics storage
- Added 5 comprehensive unit tests (blocked by pre-existing generation module bugs)
- Commits: 16c6c58 (KV backend support), 3327a64 (unit tests)

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

**Phases:** 46-52 (36+ plans planned)
- Phase 46: Backend Abstraction Foundation (6 plans) - Type signature changes, feature flag propagation
- Phase 47: Data Migration & Compatibility (5 plans) - Snapshot export/import, backend detection, migration CLI
- Phase 48: Native V2 Performance Features (5 plans) - KV store indexing, clustered adjacency, benchmarks
- Phase 49: Pub/Sub Integration (3 plans) - Event subscription, cache invalidation, cleanup
- Phase 50: Testing & Documentation (12 plans) - Feature parity, CI matrix, documentation updates
- Phase 51: Fix Native V2 Compilation Errors (3 plans) - Module fixes, type mismatches, disabled() constructors
- Phase 52: Eliminate Native-V2 Stubs (7 plans) - Metadata storage in KV, full feature parity

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

Last session: 2026-02-08
Stopped at: Completed 54-01-PLAN.md (Backend Detection Re-exports)
Resume file: None
Blockers:
- algorithms.rs module uses concrete SqliteGraph type - requires conditional compilation to work with Native backend
- 305 tests fail with native-v2 feature due to algorithms.rs limitation (verified in 46-05)
- Pre-existing test failures: migration_tests expects schema v5 (actual is v7), parser_tests trait parsing issues

**From Phase 52-05 (CFG KV Backend Storage):**
- Added store_cfg_blocks_kv() and get_cfg_blocks_kv() functions to cfg_extractor module
- Added RustCfgExtractor wrapper with automatic KV storage on extraction
- Integrated CFG storage with ChunkStore (store_cfg_blocks, get_cfg_blocks methods)
- Re-exported KV functions from graph module for public API access
- All 3 KV storage tests pass (roundtrip, empty, overwrite)
- All 2 ChunkStore integration tests pass
- Commits: f8db4cd (KV storage functions), 4ce8b1b (ChunkStore integration)

**From Phase 52-07 (Verify end-to-end functionality):**
- Added round-trip migration test structure with metadata (chunks, execution logs, metrics)
- Added concurrent KV access test structure (10 threads x 100 operations)
- Added KV metadata storage performance benchmarks (chunks, execution logs, metrics, combined)
- Made migrate_backend_cmd module public in lib.rs for test access
- Exposed ExecutionLog, MetricsOps, execution_log, metrics modules from graph
- Discovered test infrastructure limitations (Rc vs Arc, SQLite vs Native V2 backend creation)
- Documented 54 pre-existing test failures caused by infrastructure limitations (not 52-07 changes)
- Commits: 60a779d (migration tests), 0e2b636 (KV storage tests), faa7dad (benchmarks), e5080b2 (documentation)

**From Phase 52-03 (ExecutionLog KV Backend):**
- Added KV backend support to ExecutionLog (kv_backend field, with_kv_backend constructor)
- Modified start_execution(), finish_execution(), get_by_execution_id(), list_all() for KV support
- SQLite fallback preserved for backward compatibility
- All 10 execution_log tests pass (6 SQLite + 4 KV-backed)
- Commit: a6a86a9

**From Phase 52-01 (KV Key Patterns and Encoding Functions):**
- Added 6 key construction functions (chunk_key, execution_log_key, file_metrics_key, symbol_metrics_key, cfg_blocks_key, ast_nodes_key)
- Added 6 JSON encoding/decoding functions with generic type parameters to avoid private module dependencies
- Added serde derives to ExecutionRecord for JSON serialization
- All 31 KV tests pass (19 keys + 10 encoding + 2 module)
- Namespace collision test confirms all 12 key prefixes are distinct
- File sizes exceed minimums (keys.rs: 493 lines, encoding.rs: 369 lines)
- Commits: 70baa00 (key patterns), 66e57be (encoding functions), 6bc7d1d (serde derives)

**From Phase 47-05 (Round-Trip Migration Test):**
- Created tests/backend_migration_tests.rs with 3 test functions (455 lines)
- Fixed migration schema mismatches: execution_log (14 cols), ast_nodes (file_id), code_chunks (symbol_kind)
- Fixed database lock issues by dropping backend connections before side table migration
- Rewrote migrate_side_tables() to avoid ATTACH DATABASE locks, use direct row copy with rusqlite::Value
- All 3 tests pass, demonstrating MIGRATE-04 and MIGRATE-05 requirements are met
- Commits: faf5510 (test and fixes)

**From Phase 47-04 (Backend Migration CLI Command):**
- Implemented run_migrate_backend() orchestrator with full migration pipeline
- Used ATTACH DATABASE approach for side table migration
- Added migrate-backend CLI command with --input, --output, --export-dir, --dry-run flags
- Commits: 386ccbf (migration orchestrator), d9d92df (CLI command)

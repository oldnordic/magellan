# PROJECT: Magellan

## What This Is

Magellan is a deterministic codebase mapping CLI for local developers. It watches source trees, extracts AST-level facts (symbols, references, and call relationships) across 7 languages, and persists them into a searchable SQLite-backed graph database with contract-grade JSON outputs and stable IDs.

## Core Value

Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

## Current Milestone: v2.2 Code Quality & Cross-File Relations

**Goal:** Fix cross-file reference indexing, re-enable caller/callee tracking, improve code quality (reduce unwrap() calls, split main.rs), and remove SQLite-specific code from generic traits.

**Target features:**
- Cross-file reference indexing enables inter-file relationship tracking
- Caller/callee tracking re-enabled in query/find commands
- Code quality: split main.rs (2874 lines → modules), reduce unwrap() panic points
- Backend abstraction: remove SQLite-specific labels from GraphBackend trait
- AST node storage integrated with KV backend (native-v2)
- Fragile areas fixed: validation.rs, cfg_extractor.rs, ingest/pool.rs

**Version:** v2.1 Backend Parity Completion (shipped 2026-02-08)

**Recently Shipped:**
- v2.1: Backend Parity Completion — All CLI query commands work on Native-V2 backend, ChunkStore KV support, comprehensive test suite
- v2.0: Native V2 Backend Migration — KV store, WAL transactions, clustered adjacency, pub/sub events
- v1.7: Concurrency & Thread Safety — RefCell → Arc<Mutex> migration, lock ordering, timeout-based shutdown

**Version:** v1.7 Concurrency & Thread Safety (shipped 2026-01-24)

**Recently Shipped:**
- v1.7: Concurrency & Thread Safety — RefCell → Arc<Mutex<T>> migration, lock ordering documentation, timeout-based shutdown, error propagation improvements, comprehensive verification testing
- v1.6: Quality & Bugfix — partially complete (CSV export fixes pending Phase 28)

**v1.7 Deliverables:**
- RefCell → Arc<Mutex<T>> migration in FileSystemWatcher for thread-safe concurrent access
- Lock ordering hierarchy documented and enforced (dirty_paths → wakeup send)
- Thread shutdown timeout (5 seconds) prevents indefinite hangs
- Error propagation for chunk storage (no silent failures)
- Safe bounds checking helper prevents panics on malformed AST nodes
- Parser cleanup function releases tree-sitter C resources
- 29 verification tests (2,371 lines) — TSAN, stress, performance, shutdown

**v1.6 Status:** Quality & Bugfix milestone (CSV export, compiler warnings) — Phase 27 not started, Phase 28 pending

## Requirements

### Validated (Previously Shipped)

- ✓ Watch directories for file create/modify/delete and process events deterministically — v1.0
- ✓ Extract AST-level symbol facts (functions/classes/methods/enums/modules) for 7 languages — v1.0
- ✓ Extract reference facts and call graph edges (caller → callee) across indexed files — v1.0
- ✓ Persist graph data to SQLite via sqlitegraph and support query-style CLI access — v1.0
- ✓ Export graph data for downstream tooling (JSON/JSONL/DOT/CSV/SCIP) — v1.0
- ✓ Continue running on unreadable/invalid files (per-file errors don't kill the watcher) — v1.0
- ✓ Clean shutdown on SIGINT/SIGTERM — v1.0
- ✓ CLI outputs as structured JSON with explicit schemas (schema_version) — v1.0
- ✓ Stable identifiers in outputs (execution_id, match_id, span_id, symbol_id) — v1.0
- ✓ Span-aware outputs (byte offsets + line/col) with deterministic ordering — v1.0
- ✓ Validation hooks (checksums + pre/post verification) and execution logging — v1.0
- ✓ FQN-based symbol lookup eliminates name collisions — v1.1
- ✓ Path traversal validation prevents CVE-2025-68705 class vulnerabilities — v1.1
- ✓ Row-count assertions verify delete operation completeness — v1.1
- ✓ SCIP export verified by round-trip tests — v1.1
- ✓ Security documentation (database placement, path protection) — v1.1
- ✓ Unified JSON schema output (StandardSpan + rich span extensions) — v1.2
- ✓ Code deduplication (common module with shared utilities) — v1.3
- ✓ Thread-local parser pooling (7 language parsers) — v1.3
- ✓ SQLite performance tuning (WAL mode, cache_size, synchronous) — v1.3
- ✓ Parallel file scanning (rayon par_iter) — v1.3
- ✓ LRU cache for graph queries — v1.3
- ✓ Streaming JSON export for large graphs — v1.3
- ✓ Path normalization across all entry points (PATH-01) — v1.4
- ✓ Result propagation in index_references (ERR-01) — v1.4
- ✓ Byte slice bounds checking (BOUND-01) — v1.4
- ✓ File-scoped counting in reconcile (COUNT-01) — v1.4
- ✓ DeleteResult verification (DEL-01) — v1.4
- ✓ Thread-safe ChunkStore with Arc<Mutex> (THREAD-01) — v1.4
- ✓ Parser warmup error propagation (POOL-01) — v1.4
- ✓ expect() with clear invariant messages (UNWRAP-01) — v1.4
- ✓ PRAGMA connection scoped cleanup (CLEAN-01) — v1.4
- ✓ Watcher shutdown signal (WATCH-01) — v1.4
- ✓ --version/-V flags (CLI-01) — v1.4
- ✓ --output flag per-command (CLI-02) — v1.4
- ✓ Position conventions documented (DOC-01) — v1.4
- ✓ Fixed misleading comments (DOC-02) — v1.4
- ✓ Cleaned up unused variables (LEAK-01) — v1.4
- ✓ :memory: database limitations documented (LEAK-02) — v1.4
- ✓ RefCell usage documented (REFCELL-01) — v1.4
- ✓ Clear :memory: error messages (CONTEXT-01) — v1.4
- ✓ BLAKE3-based SymbolId with 32-character hex output (v1.5)
- ✓ Canonical FQN vs Display FQN split for unambiguous identity (v1.5)
- ✓ All 7 language parsers emit canonical and display FQN (v1.5)
- ✓ Graph-based ambiguity modeling using alias_of edges (v1.5)
- ✓ CLI UX enhancements (--symbol-id, --ambiguous, --first) (v1.5)
- ✓ Export format versioning (2.0.0) (v1.5)
- ✓ Database migration command with backup/rollback (v1.5)
- ✓ FileSystemWatcher uses thread-safe synchronization (RefCell → Arc<Mutex>) — v1.7
- ✓ Lock ordering prevents deadlock in PipelineSharedState — v1.7
- ✓ Thread join errors are logged and handled gracefully — v1.7
- ✓ Chunk storage errors are propagated (not silently ignored) — v1.7
- ✓ Cache invalidation happens after file mutations — v1.7
- ✓ String slice operations use safe bounds checking — v1.7
- ✓ Parser cleanup function exists and is called during shutdown — v1.7
- ✓ Duplicate parser APIs are consolidated — v1.7
- ✓ Single-threaded constraints documented for caches — v1.7
- ✓ Concurrency tests pass (TSAN blocked by toolchain, manual verification passed) — v1.7

### Active (v2.2 Code Quality & Cross-File Relations)

**Cross-File Relations:**
- [ ] Cross-file reference indexing creates inter-file relationships
- [ ] Caller/callee tracking works across file boundaries
- [ ] Query commands show complete call relationships

**Code Quality:**
- [ ] main.rs split into focused modules (<300 LOC each)
- [ ] unwrap() calls replaced with proper Result handling
- [ ] Fragile areas (validation.rs, cfg_extractor.rs, ingest/pool.rs) hardened

**Backend Abstraction:**
- [ ] SQLite-specific labels removed from GraphBackend trait
- [ ] AST node storage integrated with KV backend
- [ ] Backend-agnostic interfaces throughout

### Out of Scope (Carry Forward)

- Semantic analysis or type checking — explicitly not a goal
- LSP server or editor language features — CLI-only v1
- Async runtimes or background thread pools — keep deterministic + simple
- Configuration files — prefer CLI flags only
- Web APIs / network services — local tool only
- Automatic database cleanup — user controls DB lifecycle
- Multi-root workspaces (multiple roots in one run/DB) — out of scope for v1
- LSIF export — deprecated in favor of SCIP

## Context

**Current State (v1.7 shipped):**
- ~30,000 lines of Rust
- 33 phases, 132 plans completed over ~6 days
- Tech stack: Rust 2021, tree-sitter, sqlitegraph v1.0.0, SCIP 0.6.1, BLAKE3 1.5
- Git stats for v1.7: 14 files changed, 3,028 insertions, 117 deletions

**Features Shipped (v1.7):**
- RefCell → Arc<Mutex<T>> migration in FileSystemWatcher
- Lock ordering hierarchy documented and enforced (dirty_paths → wakeup send)
- Thread join panic handling with downcast_ref for both &str and String
- 5-second timeout-based watcher thread shutdown
- Chunk storage error propagation via ? operator
- Cache invalidation at 5 mutation points
- safe_slice() and safe_str_slice() helper functions (35 usages across 8 parser modules)
- Parser warmup with error reporting
- cleanup_parsers() function (documented no-op for tree-sitter auto-cleanup)
- Deprecated extract_symbols instance method
- Superseded methods removed (walk_tree, extract_symbol)
- Thread safety documentation (files.rs, cache.rs, pool.rs, indexer.rs)
- 29 verification tests: TSAN (6), stress (6), performance (5), shutdown (12)
- Full details: `.planning/milestones/v1.7-ROADMAP.md`

**Primary users:** local developers running Magellan against their own repositories during development and refactoring.

## Constraints

- **Interface**: CLI commands are the primary interface — keep flags explicit and stable
- **DB location**: User chooses DB path via `--db <FILE>` — no hidden defaults
- **Correctness**: Prioritize correctness and determinism over micro-optimizations
- **Determinism**: Deterministic ordering in outputs and scans (sorted paths/results)
- **Span fidelity**: Outputs must include byte offsets and line/col where applicable
- **Languages**: Rust, Python, Java, JavaScript, TypeScript, C, C++
- **No config files**: CLI flags only; no `.env` or config-driven behavior

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| CLI-first tool for local developers | Keeps scope tight; enables scripting + integration | ✓ Good |
| Use tree-sitter for AST fact extraction | Cross-language parsing with deterministic syntax trees | ✓ Good |
| Persist facts in SQLite-backed graph (sqlitegraph) | Portable, inspectable, queryable local store | ✓ Good |
| `--db` flag required for DB path selection | No hidden state; supports repeatable runs | ✓ Good |
| Watch does "scan initial + then watch" | v1 must produce a complete baseline before incremental updates | ✓ Good |
| Structured JSON output with stable IDs + span-aware fields | Enables deterministic downstream tooling and safe automation | ✓ Good |
| Deterministic ordering everywhere (sorted outputs) | Diff-friendly, reproducible automation | ✓ Good |
| Validation hooks + execution logging with `execution_id` | Verifiability + audit trail for runs and refactors | ✓ Good |
| SCIP export for interoperability | Sourcegraph standard; LSIF deprecated | ✓ Good |
| v1.4 focused on bug fixes | Issues found during testing need resolution before new features | ✓ Good |
| v1.5 Symbol Identity | BLAKE3-based SymbolId fixes FQN collisions with explicit ambiguity handling | ✓ Good |
| v1.7 Concurrency & Thread Safety | RefCell → Mutex migration eliminates data races; lock ordering prevents deadlocks | ✓ Good |
| Milestone-based development | Small, focused milestones enable shipping value incrementally | ✓ Good |
| Use Arc<Mutex<T>> with .unwrap() | Maintains RefCell's panic-on-poison behavior for thread safety | ✓ Good |
| Lock ordering hierarchy: dirty_paths → wakeup | Prevents deadlocks by never sending while holding other locks | ✓ Good |
| Thread shutdown timeout (5 seconds) | Prevents indefinite hangs; logs error and continues | ✓ Good |
| Centralized safe_slice() helper | Reduces code duplication; prevents panics on malformed AST nodes | ✓ Good |

---
*Last updated: 2026-02-08 for v2.2 Code Quality & Cross-File Relations milestone*

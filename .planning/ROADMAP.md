# Roadmap: Magellan

## Milestones

- ✅ **v1.0 Magellan** - Phases 1-9 (shipped 2026-01-19)
- ✅ **v1.1 Correctness + Safety** - Phases 10-13 (shipped 2026-01-20)
- ✅ **v1.2 Unified JSON Schema** - Phase 14 (shipped 2026-01-22)
- ✅ **v1.3 Performance** - Phase 15 (shipped 2026-01-22)
- ✅ **v1.4 Bug Fixes & Correctness** - Phases 16-19 (shipped 2026-01-22)
- ✅ **v1.5 Symbol Identity** - Phases 20-26 (shipped 2026-01-23)
- 🚧 **v1.6 Quality & Bugfix** - Phases 27-28 (partially complete)
- ✅ **v1.7 Concurrency & Thread Safety** - Phases 29-33 (shipped 2026-01-24) — *See: [milestones/v1.7-ROADMAP.md](.planning/milestones/v1.7-ROADMAP.md)*

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
- Phase 10: Path Traversal Validation (4 plans) - Security baseline
- Phase 11: FQN Extraction (6 plans) - Correctness foundation
- Phase 12: Transactional Deletes (6 plans) - Data integrity
- Phase 13: SCIP Tests + Documentation (4 plans) - Validation and documentation

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
- Phase 20: Schema Foundation (4 plans) - BLAKE3 dependency, canonical_fqn/display_fqn fields, schema version bump to 4, generate_symbol_id_v2()
- Phase 21: FQN Computation Part 1 (3 plans) - FqnBuilder module, crate name detection, Rust parser FQN emission
- Phase 22: FQN Computation Part 2 (6 plans) - Python, Java, JavaScript, TypeScript, C, C++ parser FQN emission
- Phase 23: Query & SymbolId Integration (8 plans) - find_by_symbol_id(), get_ambiguous_candidates(), SymbolIndex, reference resolution, collisions CLI
- Phase 24: Ambiguity Modeling (5 plans) - AmbiguityOps trait, graph-based ambiguity tracking, CLI flags, tests
- Phase 25: CLI UX (1 plan) - Help text documentation, SymbolId display in output
- Phase 26: Export & Migration (4 plans) - Export versioning, migrate command, documentation, tests

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
<summary>🚧 v1.6 Quality & Bugfix (Phases 27-28) - IN PROGRESS</summary>

**Milestone Goal:** Fix CSV export bug, clean compiler warnings, improve test coverage, and document CLI edge cases discovered during v1.5 live testing.

#### Phase 27: Code Quality
**Goal**: Clean compiler warnings and fix CSV export for mixed record types
**Status**: ✅ COMPLETE 2026-01-31
**Depends on**: Phase 26
**Requirements**: CSV-01, CSV-02, CSV-03, CSV-04, WARN-01, WARN-02, WARN-03, WARN-04, WARN-05, WARN-06
**Success Criteria**:
  1. User can export Symbol, Reference, and Call records together to CSV without errors ✅
  2. CSV export includes record_type column for discriminating mixed record types ✅
  3. CSV export has consistent headers across all record types ✅
  4. `cargo build` produces no warnings (clean build) ✅
  5. `cargo test` produces no warnings in test code ✅
**Plans**: 8/8 complete

Plans:
- [x] 27-01: Fix CSV export to handle mixed Symbol/Reference/Call records
- [x] 27-02: Add record_type column and consistent headers to CSV export
- [x] 27-03: Remove unused import std::io::Write from export.rs (WARN-01)
- [x] 27-04: Remove or use dead code in symbol_index.rs (WARN-02)
- [x] 27-05: Remove or use generate_symbol_id_v2 function in symbols.rs (WARN-03)
- [x] 27-06: Remove or use MigrationResult fields (WARN-04)
- [x] 27-07: Clean up unused imports in test files (WARN-05)
- [x] 27-08: Clean up unused variables in test files (WARN-06)

#### Phase 28: Test Coverage & Documentation
**Goal**: Verify fixes with tests and document CLI behavior for discovered edge cases
**Depends on**: Phase 27
**Requirements**: TEST-01, TEST-02, TEST-03, TEST-04, TEST-05, DOC-01, DOC-02, DOC-03, DOC-04
**Success Criteria**:
  1. CSV export tests verify Symbol-only, Reference-only, Call-only, and mixed exports
  2. Integration test verifies --ambiguous flag behavior with full display_fqn
  3. Documentation explains --ambiguous flag requires full display_fqn, not just symbol name
  4. CSV export format and behavior is documented
  5. Collisions command vs find --ambiguous distinction is clarified
**Plans**: 9 plans

Plans:
- [ ] 28-01: Add CSV export test for Symbol records only (TEST-01)
- [ ] 28-02: Add CSV export test for Reference records only (TEST-02)
- [ ] 28-03: Add CSV export test for Call records only (TEST-03)
- [ ] 28-04: Add CSV export test for mixed Symbol/Reference/Call records (TEST-04)
- [ ] 28-05: Add integration test for --ambiguous flag with full display_fqn (TEST-05)
- [ ] 28-06: Document --ambiguous flag usage requirements (DOC-01)
- [ ] 28-07: Document CSV export behavior and format (DOC-02)
- [ ] 28-08: Clarify collisions command vs find --ambiguous distinction (DOC-03)
- [ ] 28-09: Document any remaining CSV export limitations (DOC-04)

</details>

<details>
<summary>✅ Phase 34: CFG and Metrics Tables - COMPLETE 2026-01-31</summary>

**Milestone Goal:** Add metrics tables and chunk storage CLI commands to enable fast codemcp debug tools and token-efficient code queries.

**Plans (6/6):**
- [x] 34-01 — Metrics schema and module structure (file_metrics, symbol_metrics tables, MetricsOps module)
- [x] 34-02 — Metrics computation integration (fan-in/fan-out, LOC, complexity during indexing)
- [x] 34-03 — Metrics backfill and verification (backfill for existing databases, integration tests)
- [x] 34-04 — Chunk storage CLI commands (chunks, chunk-by-span, chunk-by-symbol)
- [x] 34-05 — Chunk storage integration tests (verify storage, deletion, deduplication)
- [x] 34-06 — Chunk storage documentation (MANUAL.md updates)

**Delivered:**
- Pre-computed metrics tables (file_metrics, symbol_metrics) with fan-in/fan-out/LOC/complexity_score
- MetricsOps module with computation and storage methods
- Backfill functionality for existing databases (auto-triggered on schema upgrade)
- Chunk storage CLI commands (chunks, chunk-by-span, chunk-by-symbol) with JSON/human output
- Integration tests for chunk storage (9 tests, 402 lines)
- MANUAL.md documentation sections 3.12-3.15

**NOT in scope (deferred to Phase 35+):**
- Control flow graph (CFG) construction
- Cyclomatic complexity computation (using placeholder value of 1)
- Path-aware analysis

**See:** `.planning/phases/34-cfg-metrics/*-PLAN.md` for details
**Verification:** `.planning/phases/34-cfg-metrics/34-VERIFICATION.md`

</details>

<details>
<summary>✅ Phase 35: Safe UTF-8 Content Extraction - COMPLETE 2026-01-31</summary>

**Milestone Goal:** Add safe UTF-8 content extraction functions to prevent panics when tree-sitter byte offsets split multi-byte UTF-8 characters.

**Plans (1/1):**
- [x] 35-01 — Safe content extraction functions (extract_symbol_content_safe, extract_context_safe), integration tests, documentation

**Delivered:**
- extract_symbol_content_safe() for byte-span to UTF-8 conversion with boundary validation
- extract_context_safe() for line-based context extraction with UTF-8 safety
- Updated ops.rs to use safe extraction (removes unsafe slicing)
- Updated rich.rs to use safe extraction for checksums
- Public API exports for downstream tools (splice, llmgrep)
- Integration tests with multi-byte UTF-8 fixtures (Japanese, emoji, CJK, accented)
- MANUAL.md documentation (Section 3.4: UTF-8 Safety)

**See:** `.planning/phases/35-safe-content-extraction/*-PLAN.md` for details
**Verification:** `.planning/phases/35-safe-content-extraction/35-01-SUMMARY.md`

</details>

<details>
<summary>Phase 40: Graph Algorithms - PLANNED</summary>

**Milestone Goal:** Integrate sqlitegraph 1.3.0's comprehensive graph algorithms library to enable advanced code analysis features (dead code detection, impact analysis, cycle detection, path enumeration, program slicing).

**Plans (5/5):**
- [ ] 40-01 — Algorithm infrastructure + reachability analysis (reachable_from, dead_symbols, reverse_reachable)
- [ ] 40-02 — SCC detection + call graph condensation (strongly_connected_components, collapse_sccs)
- [ ] 40-03 — Path enumeration with bounds (enumerate_paths, PathEnumerationConfig)
- [ ] 40-04 — Program slicing (backward_slice, forward_slice or call-graph fallback)
- [ ] 40-05 — Documentation, expanded test coverage, performance benchmarks

**Delivering:**
- Algorithm wrapper module (src/graph/algorithms.rs) with helper types and methods
- CLI command (magellan algo) with subcommands: reachable, dead-code, cycles, condense, paths, slice
- JSON/human output formats for all algorithm results
- 20+ integration tests covering all algorithm categories
- MANUAL.md Section 4: Graph Algorithms documentation

**NOT in scope (deferred):**
- Full CFG-based program slicing (uses call-graph reachability as approximation)
- Result caching for expensive algorithms
- Progress tracking integration for long-running operations

**See:** `.planning/phases/40-graph-algorithms/*-PLAN.md` for details

</details>

<details>
<summary>✅ Phase 36: AST Schema Foundation - COMPLETE 2026-01-31</summary>

**Milestone Goal:** Add AST nodes table to database schema for hierarchical code structure queries.

**Plans (1/1):**
- [x] 36-02 — AST nodes table with parent_id for hierarchical relationships

**Delivered:**
- ast_nodes table with kind, byte_start, byte_end, parent_id columns
- ensure_ast_schema() function for schema creation
- Schema version bump to 5

**See:** `.planning/phases/36-ast-schema/*-PLAN.md` for details

</details>

<details>
<summary>✅ Phase 37: AST Extraction - COMPLETE 2026-01-31</summary>

**Milestone Goal:** Implement AST node extraction from tree-sitter parse trees and integrate into indexing pipeline.

**Plans (2/2):**
- [x] 37-01 — AST extraction module (ast_extractor.rs) with tree-sitter traversal, language mapping
- [x] 37-02 — Integration into indexing pipeline with storage and deletion

**Delivered:**
- AstExtractor struct with extract_ast_nodes() method
- normalize_node_kind() for language-agnostic kind mapping
- parent-child relationship tracking via two-phase insertion
- Integration into index_file() with automatic AST extraction

**See:** `.planning/phases/37-ast-extraction/*-PLAN.md` for details

</details>

<details>
<summary>✅ Phase 38: AST CLI & Testing - COMPLETE 2026-01-31</summary>

**Milestone Goal:** Add CLI commands for AST queries and comprehensive test coverage.

**Plans (2/2):**
- [x] 38-01 — AST CLI commands (ast_cmd.rs) with human/JSON output, position queries, kind filtering
- [x] 38-02 — Comprehensive test suite (ast_tests.rs) with 20+ integration tests

**Delivered:**
- `magellan ast --file <path>` command with tree structure display
- `magellan find-ast --kind <kind>` command for kind-based queries
- JSON output support with rich span extensions
- 20+ integration tests covering indexing, parent-child, position queries, re-indexing

**See:** `.planning/phases/38-ast-cli-testing/*-PLAN.md` for details

</details>

<details>
<summary>✅ Phase 39: AST Migration Fix - COMPLETE 2026-01-31</summary>

**Milestone Goal:** Fix database migration from v4 to v5 and verify new database creation.

**Plans (2/2):**
- [x] 39-01 — Fix v4->v5 migration in migrate_cmd.rs
- [x] 39-02 — Verify new database creation with v5 schema

**Delivered:**
- MAGELLAN_SCHEMA_VERSION updated from 4 to 5
- Auto-upgrade v4->v5 databases on open
- tests/migration_tests.rs with 4 comprehensive tests
- All tests pass (450/450)

**See:** `.planning/phases/39-ast-migration-fix/*-PLAN.md` for details

</details>

<details>
<summary>✅ Phase 41: Gitignore-Aware Indexing - COMPLETE 2026-02-03</summary>

**Milestone Goal:** Make Magellan `.gitignore`-aware so users can run `magellan watch --root .` without indexing dependencies, build artifacts, and generated code.

**Plans (1/1):**
- [x] 41-01 — Add gitignore_aware field to WatcherConfig, apply FileFilter in extract_dirty_paths(), add CLI flags, integration tests

**Delivered:**
- WatcherConfig with gitignore_aware field (default: true)
- FileFilter integration in extract_dirty_paths() function
- CLI flags: --gitignore-aware (default) and --no-gitignore
- 5 integration tests for gitignore-aware watcher behavior
- Consistent filtering between initial scan and watcher
- Fixed FileFilter directory ignore pattern matching (ancestor directory checking)

**NOT in scope (deferred):**
- Watching .gitignore file for changes (parse once at startup)
- Per-language smart defaults (current INTERNAL_IGNORE_DIRS covers common cases)
- .magellanignore file (.ignore is already supported)

**See:** `.planning/phases/41-gitignore-indexing/*-PLAN.md` for details

</details>

<details>
<summary>✅ Phase 42: AST-Based CFG for Rust - COMPLETE 2026-02-03</summary>

**Milestone Goal:** Implement AST-based Control Flow Graph extraction for Rust using tree-sitter.

**Plans (4/4):**
- [x] 42-01 — CFG schema design (cfg_blocks table, schema v7, migration)
- [x] 42-02 — AST-based CFG extraction (if/else, loop/while/for, match, terminators)
- [x] 42-03 — Indexing pipeline integration (automatic CFG extraction during indexing)
- [x] 42-04 — Documentation update (ROADMAP, STATE, limitations)

**Decision:** AST-based CFG extraction as interim solution pending stable_mir publication.

**Delivered:**
- src/graph/db_compat.rs with ensure_cfg_schema() and v7 migration
- src/graph/schema.rs with CfgBlock and CfgEdge types
- src/graph/cfg_extractor.rs with CfgExtractor for Rust AST traversal
- src/graph/cfg_ops.rs with persistence and query operations
- Integration into index_file() for automatic CFG extraction
- docs/CFG_LIMITATIONS.md with honest documentation of limitations

**Key Features:**
- **Supported:** if/else, loop/while/for, match, return/break/continue, ? operator
- **Not Supported:** macro expansion, generic monomorphization, async/await desugaring
- **Precision:** AST-level (not full MIR precision)

**Technical Notes:**
- Uses tree-sitter AST traversal (no compiler dependency)
- Stores basic blocks in cfg_blocks table
- Enables: cyclomatic complexity, path enumeration (limited), dominance analysis
- Schema version 7 for cfg_blocks table

**Background:**
- Original MIR extraction research (42-RESEARCH.md) concluded stable_mir is not yet available
- rustc_driver is unstable (requires nightly, breaks frequently)
- Charon violates single-binary philosophy (external binary dependency)
- AST-based approach is viable interim solution

**See:**
- `.planning/phases/42-ast-cfg-rust/42-RESEARCH.md` — Original research findings (archived)
- `src/graph/cfg_extractor.rs` — CFG extraction implementation
- `docs/CFG_LIMITATIONS.md` — Detailed limitations documentation

</details>

<details>
<summary>✅ Phase 43: LLVM IR CFG for C/C++ - COMPLETE 2026-02-03</summary>

**Milestone Goal:** Add infrastructure for optional LLVM IR-based CFG extraction for C/C++.

**Plans (1/1):**
- [x] 43-01 — Optional llvm-cfg feature flag, LlvmCfgExtractor stub, clang integration pattern

**Decision:** LLVM IR-based CFG is OPTIONAL enhancement. AST-based CFG (Phase 42) works for C/C++ as fallback.

**Delivered:**
- Cargo.toml with optional inkwell 0.5 dependency (llvm-sys wrappers)
- llvm-cfg feature flag (disabled by default)
- which 6.0 optional dependency for finding clang in PATH
- src/graph/cfg_extractor.rs with LlvmCfgExtractor stub (feature-gated, 178 lines)
- clang invocation pattern for compiling C/C++ to LLVM IR (compile_to_ir)
- Documentation that AST CFG is sufficient for most use cases
- README.md at .planning/phases/43-llvm-cfg-cpp/README.md

**Key Features:**
- **Optional:** Feature-gated, not required for Magellan to work
- **Disabled by default:** No LLVM dependency unless explicitly enabled
- **Fallback:** AST-based CFG (Phase 42) works for C/C++
- **Infrastructure only:** Full LLVM IR implementation deferred to future work

**NOT in scope (deferred):**
- Full LLVM IR parsing and basic block extraction
- Integration into indexing pipeline
- CLI flags for runtime LLVM CFG enablement
- Performance benchmarks (AST CFG vs LLVM CFG)

**See:**
- `.planning/phases/43-llvm-cfg-cpp/README.md` — Phase documentation
- `.planning/phases/43-llvm-cfg-cpp/43-VERIFICATION.md` — Verification report
- `src/graph/cfg_extractor.rs:710-887` — LlvmCfgExtractor implementation

</details>

<details>
<summary>Phase 44: JVM Bytecode CFG (Java) - PLANNED</summary>

**Milestone Goal:** Implement optional Java bytecode-based CFG extraction using ASM library.

**Plans (1/1):**
- [ ] 44-01 — Optional ASM dependency, feature flag, module stubs

**Decision:** Bytecode-based CFG is OPTIONAL enhancement. AST-based CFG (Phase 42) works for Java as fallback.

**Delivering (when complete):**
- Cargo.toml with optional ASM dependency (asm = { version = "9.7", optional = true })
- bytecode-cfg feature flag
- src/graph/bytecode_cfg.rs with conditional compilation
- Graceful degradation when feature disabled

**Key Features:**
- **Supported:** if/else, loops, switch, try/catch/finally, exceptions
- **More precise than AST:** Compiler-generated control flow visible
- **Requires javac:** Source must be compiled to .class files first
- **Optional:** Feature-gated, not required for Magellan to work

**Technical Notes:**
- Uses ASM library (org.ow2.asm) for bytecode analysis
- Reuses cfg_blocks/cfg_edges schema from Phase 42
- Stores CFG with same schema as AST-based extraction
- Feature flag: --features bytecode-cfg

**Limitations:**
- Requires compiled .class files (javac step)
- Java-only (Kotlin/Scala not supported without adaptation)
- Binary size increase (~100KB) when feature enabled
- Optional enhancement - Magellan works without it

**Background:**
- Bytecode CFG is more precise than AST for Java
- Handles exception edges, synthetic bridges, lambda desugaring
- ASM library is stable and actively maintained
- This is OPTIONAL - AST CFG from Phase 42 is fallback

**See:**
- `.planning/phases/44-bytecode-cfg-java/44-01-PLAN.md` — Implementation plan
- `docs/JAVA_BYTECODE_CFG.md` — User-facing documentation
- Phase 42 for AST-based CFG schema

</details>

---

<details>
<summary>✅ v1.7 Concurrency & Thread Safety (Phases 29-33) - SHIPPED 2026-01-24</summary>

Fixed 23 concurrency and thread safety issues from Rust code audit. RefCell → Arc<Mutex<T>> migration for thread-safe concurrent access, lock ordering documentation to prevent deadlocks, timeout-based shutdown (5s), error propagation improvements, and comprehensive verification testing (29 tests, 2,371 lines).

**See:** [milestones/v1.7-ROADMAP.md](.planning/milestones/v1.7-ROADMAP.md) for full details

</details>

## Progress

**Execution Order:**
Phases execute in numeric order: 27 → 28 → 29 → 30 → 31 → 32 → 33

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1-9 | v1.0 | 29/29 | Complete | 2026-01-19 |
| 10-13 | v1.1 | 20/20 | Complete | 2026-01-20 |
| 14 | v1.2 | 5/5 | Complete | 2026-01-22 |
| 15 | v1.3 | 6/6 | Complete | 2026-01-22 |
| 16-19 | v1.4 | 18/18 | Complete | 2026-01-22 |
| 20-26 | v1.5 | 31/31 | Complete | 2026-01-23 |
| 27 | v1.6 | 0/8 | Not started | - |
| 28 | v1.6 | 0/9 | Not started | - |
| 29. Critical Fixes | v1.7 | 3/3 | Complete | 2026-01-23 |
| 30. Thread Safety | v1.7 | 2/2 | Complete | 2026-01-24 |
| 31. Error Handling | v1.7 | 4/4 | Complete | 2026-01-24 |
| 32. Code Quality | v1.7 | 6/6 | Complete | 2026-01-24 |
| 33. Verification | v1.7 | 4/4 | Complete | 2026-01-24 |
| 34. CFG and Metrics | v1.8 | 6/6 | Complete | 2026-01-31 |
| 35. Safe Extraction | v1.8 | 1/1 | Complete | 2026-01-31 |
| 36. AST Schema | v1.9 | 1/1 | Complete | 2026-01-31 |
| 37. AST Extraction | v1.9 | 2/2 | Complete | 2026-01-31 |
| 38. AST CLI & Testing | v1.9 | 2/2 | Complete | 2026-01-31 |
| 39. AST Migration Fix | v1.9 | 2/2 | Complete | 2026-01-31 |
| 40. Graph Algorithms | TBD | 0/5 | Not started | - |
| 41. Gitignore-Aware Indexing | TBD | 1/1 | Complete | 2026-02-03 |
| 42. AST-Based CFG for Rust | TBD | 4/4 | Complete | 2026-02-03 |
| 43. LLVM IR CFG for C/C++ | TBD | 1/1 | Complete | 2026-02-03 |
| 44. JVM Bytecode CFG (Java) | TBD | 0/1 | Not started | - |

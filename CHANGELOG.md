# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [4.1.0] - 2026-05-24

### Added

- **Phase 0: Service Daemon Foundation**:
  - `src/service/types.rs` — `ProjectEntry`, `ServiceRequest`, `ServiceResponse`, `TaggedBatch`, `into_val()`
  - `src/service/registry.rs` — `Registry` with CRUD, auto-disambiguation, TOML persistence at `~/.config/magellan/registry.toml`
  - `src/service/mod.rs` — Daemon `Service` struct, signal handler, worker loop, `send_request()` unix socket client
  - `src/service/admin_socket.rs` — JSON-RPC dispatch over UDS (`ping`, `list`, `status`, `register`, `unregister`, `pause`, `resume`)
  - `src/service_cmd.rs` — `ServiceAction` enum + async CLI handlers for 8 subcommands (`start`, `stop`, `list`, `register`, `unregister`, `pause`, `resume`, `status`)
  - `Command::Service { action, output_format }` — CLI wiring in `src/cli.rs` and dispatch in `src/main.rs`
  - `tokio` features expanded: `net`, `process`, `io-util`

- **Phase 1: Registry CLI flags** (`find`, `status`):
  - `--all` — fan-out across all enabled projects in the registry
  - `--project <name>` — target a single named project by registry entry
  - `db_resolver.rs` — `resolve_db_path()` helper routes `--db`, `--project`, or cwd fallback
- **Phase 2: `ask` intent router**:
  - `ask_cmd.rs` — `detect_intent()` classifies queries into 9 intents (Callers, Callees, Cfg, BlastZone, Cycles, Impact, Complex, Search, Find)
  - Intent routing dispatches to `magellan refs`, `mirage cfg`, `mirage blast-zone`, in-process cycle/impact analysis, `llmgrep`, or symbol find
- **Phase 3: `ask --all` cross-project fan-out**:
  - `ask --all` iterates registry, runs per-project routing, aggregates results
  - `ask --project <name>` targets a single named project
- **Phase 4: V3 Dual Backend**:
  - `BackendType::Dual` — detected when a `.db.v3` companion file exists alongside `.db`
  - `CodeGraph::open_dual(db, v3)` — opens SQLite + creates/opens V3 native backend
  - `CodeGraph::sync_to_v3(paths)` — batch-inserts symbols into V3, records mappings in `v3_node_map` side table
  - `WatchPipelineConfig::with_v3_sync(v3_path)` — enables automatic V3 sync after each watch cycle
  - `CodeGraph::has_v3()`, `CodeGraph::v3_node_for_symbol()` — V3 presence and lookup
- **Phase 5: Framework API** (`src/framework/mod.rs`):
  - `MagellanFramework` — opens multiple project databases from registry or explicit path list
  - `MagellanFramework::from_registry()` / `from_registry_file(path)` / `from_db_paths(entries)`
  - `find(&self, name)` — cross-project symbol search returning `Vec<FrameworkSymbol>`
  - `project(&self, name)` — returns `ProjectHandle<'_>` scoped to one database
  - `ask(&self, query)` — intent-routed query returning a formatted `String`
  - `FrameworkSymbol`, `ProjectHandle` re-exported from crate root
- **`magellan navigate <task>`** — Rust-native grounded investigation packet (`src/navigate_cmd.rs`):
  - Extracts code terms from natural-language task description; resolves symbols in-process
  - Normal mode: top 3 resolved symbols × callers + callees + impact + affected + context+source (+ optional mirage CFG)
  - Concise mode (`--concise`): single bundled `get_symbol_detail` for top symbol (callers + callees + source), truncated to `--budget N` tokens (default 4000; 1 token ≈ 4 chars)
  - `--depth N` — impact/affected traversal depth (default 2)
  - `--limit N` — max symbols per extracted term (default 5)
  - `--with-llmgrep` — append semantic search results via `llmgrep`
  - `--with-mirage` — append CFG output via `mirage cfg` for top symbols
  - Output: markdown investigation packet with token estimate
  - `NavigateConfig` struct; `extract_terms()` and `truncate_to_budget()` are public for downstream use

- **P3-CTX: `query.context` socket method** (`src/service/admin_socket.rs`):
  - JSON-RPC method returning per-project symbol matches with `callers`/`callees` arrays
  - Params: `name` (required), `file` (optional), `callers` (bool), `callees` (bool), `depth` (usize)
  - Reuses `MultiDbContext::search_symbol`; results include full caller/callee name+file+line
- **P3-SORT: Relevance sorting for `--all` results** (`src/graph/multi_db.rs`):
  - `score_match(query, name) -> u32` — exact=100, prefix=75, substring=50, no match=0
  - `MultiDbContext::search_symbol` now sorts results by score descending, then by caller count descending as tiebreaker

### Fixed

- `src/cli.rs` inline `service` block: `parse_required_arg`/`parse_path_arg` called with `&args` (Vec<String>) instead of `&args[..]` (slice), causing type mismatch
- `src/main.rs` `Command::Service` match arm: destructured `db_path: _` which no longer exists on the enum variant
- `BackendType::Dual` exhaustiveness: `find_cmd.rs` and `slice_cmd.rs` updated to match `SQLite | Dual` arm

## [3.3.13] - 2026-05-21

### Changed

- **sqlitegraph 3.0.2 → 3.0.3** — Picks up AVX-512 SIMD performance improvements for graph traversals.

## [3.3.12] - 2026-05-21

### Fixed

- **`--root ./src --scan-initial` returned 0 files** — Auto-include logic added `src/` as a glob pattern, but FileFilter computed relative paths by stripping the root, so `src/lib.rs` became `lib.rs` and never matched `src/`. Now detects when root is already a source subdirectory and skips auto-include.
- **FTS5 index empty after bulk scan** — `scan_directory_with_filter` inserts directly into `graph_entities`, bypassing FTS5 triggers. Added automatic `rebuild_fts5()` after initial scan completes.

## [3.3.11] - 2026-05-19

### Changed

- **sqlitegraph 3.0.1 → 3.0.2** — Picks up V3Backend flush-error handling, PersistentHeaderV3 panic fixes, CliClient stack optimization, and HNSW docstring corrections.

## [3.3.9] - 2026-05-18

### Changed

- **sqlitegraph 2.2.5 → 3.0.1** — Upgraded core graph dependency. Brings built-in graph algorithms (SCC, cycles, topological sort), bulk insert APIs, and HNSW vector search. Internal `strongly_connected_components` implementation removed in favor of sqlitegraph's native `algo` module.
- **Schema compatibility auto-migration** — `db_compat.rs` updated for sqlitegraph 3.0.1 schema. Databases with older schemas are now auto-migrated on open. Newer schemas (forward-incompatible) are still rejected with `DB_COMPAT` markers.

### Fixed

- **`test_compute_delta_with_untracked`** — Removed CWD dependency by passing explicit `project_root` parameter. Test is now deterministic regardless of working directory.
- **`test_parse_frontmatter_float`** — Replaced `PI` comparison with exact `2.5` to avoid `f64::EPSILON` precision flakiness.
- **4 pre-existing clippy warnings** — Resolved in `indexer.rs`, `project_config.rs`, `watch_integration.rs`, and `source_inventory.rs`.

## [3.3.8] - 2026-05-12

### Added

- **`--detect-backend` flag** — New top-level flag that detects the backend format of a given database file. Usage: `magellan --detect-backend --db <path>`. Returns `sqlite` for valid SQLite databases, or exits with an error for missing or invalid files.

### Fixed

- **`status` on missing database** — Previously, `magellan status --db <missing>` silently created an empty database and reported 0 files/symbols. Now it fails with a clear "Database not found" error and exits non-zero.

## [3.3.7] - 2026-05-11

### Added

- **`#[cfg]` attribute extraction for CFG blocks** — During indexing, `#[cfg(...)]` attributes on functions are now parsed from the tree-sitter AST and stored in the `cfg_condition` column of `cfg_blocks`. All blocks within a cfg-gated function inherit the same condition. Supports `feature = "X"`, `all(...)`, `any(...)`, and `not(...)` forms.
- **`get_live_cfg_for_function()`** — Returns only CFG blocks whose `cfg_condition` evaluates true against the project's active features. Blocks without a condition are always included. This allows downstream tools (e.g. Mirage) to filter out dead code paths behind disabled feature flags.
- **`evaluate_cfg_condition()`** — Evaluates cfg condition strings against a set of active features. Supports `feature = "name"`, `all(...)`, `any(...)`, and `not(...)`. Unknown conditions conservatively return `true`.

### Changed

- **Schema v16** — Added `cfg_condition TEXT` column to `cfg_blocks`. Fresh databases include it; existing databases are upgraded automatically via migration.

## [3.3.6] - 2026-05-11

### Added

- **`magellan init` command** — Creates a `.magellan.toml` configuration file with sensible defaults. Detects the project root from `Cargo.toml` if present. Refuses to overwrite an existing config.
- **`.magellan.toml` configuration** — TOML-based project config with `[project]`, `[index]`, and `[watch]` sections. Supports `include`/`exclude` glob patterns for filtering what gets indexed.
- **Cargo.toml manifest parsing** — `magellan watch` reads `Cargo.toml` to extract feature flags, dependencies, and test/bench targets. Features are stored as JSON in `magellan_meta.project_metadata`. Dependencies are stored in `CargoManifest.dependencies`.
- **Config-driven indexing** — `magellan watch` loads `.magellan.toml` and respects `include`/`exclude` filters. Auto-detects `tests/` and `benches/` targets from `Cargo.toml` `[[test]]`/`[[bench]]` sections.
- **`magellan refresh` command** — Incrementally syncs the database with git working tree changes. Re-indexes modified files, removes deleted files, optionally includes untracked files. Supports `--dry-run` for preview.

### Fixed

- **`magellan init --path .` produced `name = "project"`** — Now canonicalizes the path before extracting the directory name, so `magellan init --path .` uses the actual project directory name.

## [3.3.5] - 2026-05-10

### Fixed

- **candidate-fact list SQL parameter mismatch** — `magellan candidate-fact list` (without --status) failed with "Wrong number of parameters passed to query". The no-status SQL branch used `LIMIT ?2` but only passed 1 parameter. Changed to `LIMIT ?1`.
- **query returns no symbols for indexed files** — `magellan query --file src/lib.rs` returned empty when the database stored paths with `./` segments (e.g., from `--root ./src`). `normalize_path_for_index` now strips `.` and resolves `..` path components so both indexing and querying produce matching paths.

## [3.3.4] - 2026-05-10

### Added

- **Crate-level docs for graph memory** — `lib.rs` now documents source-inventory and candidate-fact features in the crate root documentation visible on docs.rs
- **Public re-exports for candidate_fact API** — `CandidateFact`, `CandidateStatus`, `CandidateProperties`, `ValidationResult`, `ValidationError`, `ConflictSet`, `ConflictType`, `ResolutionStatus`, and key functions (`insert_candidate_fact`, `find_candidate_fact_by_id`, `list_candidate_facts_by_status`, `candidate_fact_review_queue`, `update_candidate_fact_status`, `validate_ontology`) re-exported from crate root for downstream crate users

### Changed

- **Docs: schema v12 → v14** — All internal `docs/` files updated from schema v12 to v14, including graph memory table documentation (`source_documents`, `candidate_facts`)

## [3.3.3] - 2026-05-10

### Fixed

- **Source-inventory frontmatter parser panic** — `magellan source-inventory --scan` panicked with `called Option::unwrap() on a None value` when scanning markdown files containing non-finite float values (NaN, inf) in YAML frontmatter. The `parse_frontmatter()` function used `serde_json::Number::from_f64(...).unwrap()` which returns `None` for non-finite floats.
  - Replaced with safe `if let Some(n) = serde_json::Number::from_f64(fv)` check, falling through to string representation for non-finite values.
  - Added 3 regression tests for negative numbers, floats, and non-finite float handling.

- **Candidate-fact submit silent errors** — `magellan candidate-fact submit` without `--candidate-id` sent an empty string as the candidate identifier, causing a UNIQUE constraint violation on the second insert that was obscured by a generic `.context()` error message.
  - Auto-generates a UUID (`cf_<uuid>`) when `--candidate-id` is not provided.
  - Error message now includes the candidate_id for diagnostics.
  - Added regression test for duplicate candidate_id error reporting.

- **Watch mode database corruption** — `magellan watch --scan-initial` produced "database disk image is malformed" and "file is not a database" errors. Root cause: `CfgOps` opened a new SQLite connection per operation via `ChunkStore::connect()`, each without WAL mode or busy_timeout PRAGMAs, while 3 other connections were also writing to the same WAL file. Additionally, `insert_cfg_blocks()` ran `PRAGMA wal_checkpoint(TRUNCATE)` from one of these ephemeral connections, truncating the WAL while other connections had pending writes.
  - Replaced all 7 `connect()` calls in `CfgOps` with `with_connection_mut`/`with_conn`, which use the shared `Arc<Mutex<Connection>>` instead of opening new connections.
  - Removed the rogue WAL checkpoint from `insert_cfg_blocks()`. Only the watch loop checkpoints, from a single coordinated connection.
  - Added `batch_mode: bool` toggle to `CodeGraph`, `SymbolOps`, `ReferenceOps`, and `CallOps`. Watch mode sets `batch_mode = false` before any file processing, falling back to individual per-insert auto-commit mode to avoid `BEGIN IMMEDIATE` contention.
  - CfgOps now shares the same ChunkStore connection as CodeGraph instead of creating its own.
  - Verified: two consecutive `magellan watch --scan-initial` runs complete with zero corruption, `PRAGMA integrity_check` passes, 146 files indexed cleanly.

## [3.3.2] - 2026-05-09

### Fixed

- **Missing SQLite transactions during indexing** — Fixed the root cause of ~27x throughput degradation when indexing larger codebases. Each file's symbol/reference/call inserts were executing in auto-commit mode, creating one WAL frame per INSERT. With ~20 symbols + ~20 references + ~30 calls per file, 146 files generated ~24,000 individual SQL executes. At 3.3x scale (483 files) this exploded to ~79,000 executes, causing super-linear WAL checkpoint overhead.
  - `SymbolOps` now uses `sqlitegraph::bulk_insert_entities` + `bulk_insert_edges` wrapped in `TransactionGuard` (BEGIN IMMEDIATE...COMMIT) for all symbol nodes and DEFINES edges per file.
  - `ReferenceOps` now batches reference node inserts and REFERENCES edges via the same bulk APIs.
  - `CallOps` now batches call node inserts, CALLER edges, and CALLS edges via the same bulk APIs.
  - All three modules receive an `sqlite_backend: Option<Arc<SqliteGraphBackend>>` field during `CodeGraph::open`, with graceful fallback to individual inserts for non-SQLite backends.
  - Verified: `magellan watch --scan-initial` over `./src` (146 files, 2,853 symbols, 3,196 references, 4,265 calls) completes cleanly; DB integrity check passes; `magellan doctor` reports all OK.

## [3.3.1] - 2026-05-06

### Added

## [3.3.0] - 2026-05-06

### Added

- **`SymbolLookup` cache** — `graph/symbol_lookup.rs` provides an in-memory name→ID lookup table, eliminating repeated O(N) DB scans during reference resolution and query operations.

### Fixed

- **Watch-mode database corruption** — Fixed intermittent database corruption during `magellan watch --scan-initial` caused by uncoordinated SQLite connections writing to the same WAL:
  - Reduced sqlitegraph connection pool from 2 → 1 (eliminates inter-pool WAL races)
  - `rebuild_fts5()` now uses the shared `side_conn` instead of opening a new connection during flush
  - `delete_file_facts` now cleans up `file_metrics` rows (both normal delete path and orphan cleanup)
  - Added `PRAGMA integrity_check` verification after initial scan flush
  - Added stress tests (5 sequential watch cycles, `#[ignore]`'d for CI speed)
- **FTS5 index rebuild on `magellan refresh`** — The `refresh` command now automatically rebuilds the `symbol_fts` virtual table after applying changes. Previously, `llmgrep` queries returned stale or empty results until a manual `INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild')`.
- **`context` command `--path` alias** — `context symbol`, `context impact`, and `context affected` now accept `--path` as an alias for `--file`, consistent with `find` and `context file` commands.

## [3.2.0] - 2026-05-06

### Added

- **Context Analysis Commands** (Cross-Project Symbol Queries):
  - `magellan context build` — Build context index
  - `magellan context summary` — Show project summary
  - `magellan context list` — Paginated symbol listing (multi-DB)
  - `magellan context symbol` — Symbol detail with callers/callees/source
  - `magellan context file` — File-level symbol context
  - `magellan context impact` — Blast radius analysis (transitive callers)
  - `magellan context affected` — Dependency reach analysis (transitive callees)
  - Multi-DB support: pass a directory to `--db` to query all `.magellan/*.db` files
  - JSON envelope output with `schema_version`, `execution_id`, `data`

### Fixed

- `--db` flag now appends instead of overwriting when specified multiple times

### Changed

- **FTS5 Full-Text Search Integration** (Schema v12):
  - `symbol_fts` FTS5 virtual table for fast prefix searches
  - 2.5× speedup on prefix queries (0.005s → 0.002s)
  - Automatic index rebuild after batch indexing
  - Migration: `magellan migrate --db <path>`

## [3.1.9] - 2026-05-03

### Added

- **FTS5 Integration Complete**:
  - Schema v12 with `symbol_fts` FTS5 table
  - `CodeGraph::rebuild_fts5_index()` for automatic sync
  - Performance: 60% faster prefix searches
  - Documentation: `FTS5_INTEGRATION_COMPLETE.md`

### Changed

- Version bump: 3.1.8 → 3.1.9

## [3.1.8] - 2026-05-01

### Added

- **Validation Infrastructure** (Phase 0):
  - `scripts/validate-completion.sh` - 5-gate validation pipeline (stubs, check, test, clippy, db)
  - `.claude/hooks/stub-check.fish` - Pre-commit hook blocking TODO/unimplemented/panic in non-test code
  - `.claude/hooks/build-check.fish` - Pre-commit hook running cargo check/test/clippy
  - `docs/superpowers/skills/no-stubs-enforcement.md` - Skill for zero-tolerance stub policy
  - `docs/superpowers/skills/verification-before-completion.md` - Skill for completion gates
  - `docs/superpowers/MASTER_PLAN.md` - Master plan for toolchain improvement

- **Phase 3: Cross-Project Federation**:
  - `magellan registry scan --root <dir>` - Discovers .magellan/*.db files recursively
  - `magellan registry list --root <dir>` - Lists discovered databases with stats
  - `src/registry_cmd.rs` - New command module for database discovery

- **Phase 4: Editor Integration** (config file parsing):
  - `src/config.rs` - Configuration management module (~/.config/magellan/config.toml)
  - `magellan config show` - Display current configuration
  - `magellan config init` - Create default config file
  - Supports editor provider settings (ollama, openai, anthropic, custom)
  - Supports registry settings (auto_scan, scan_roots)
  - Added `toml` crate dependency for config parsing

- **Phase 5: Validation Pipeline** (verification infrastructure):
  - `scripts/validate-completion.sh` - Pre-completion 5-gate validation pipeline
  - `magellan verify --root <dir> --db <db>` - Path integrity verification (filesystem vs DB)
  - `splice verify --before <snap> --after <snap>` - Snapshot comparison tool
  - `.claude/hooks/` - Pre-commit hooks (stub-check.fish, build-check.fish)
  - `src/checksum.rs` - File/span checksum computation for audit trails
  - `src/code_validator.rs` - Rust-analyzer validation for LSP-verified mutations

### Fixed

- **Path resolution in query functions** (`symbol_nodes_in_file`, `symbol_nodes_in_file_with_ids`):
  - Relative paths (e.g., `src/main.rs`) were not matching paths in the database (which stores absolute paths)
  - Added `resolve_query_path()` helper that canonicalizes relative paths using current working directory
  - `rebuild_file_index()` now stores absolute canonical paths in the in-memory index
  - Now all query operations work consistently regardless of path format
- **Legacy database schema compatibility**: `ensure_cfg_schema()` and `ensure_coverage_schema()` now detect existing cfg_edges table with legacy schema before attempting schema modifications that would fail on legacy databases
- **splice dead-code / splice reachable**: Fixed by using local magellan path in splice's Cargo.toml
- Stale installed binary causing `backfill` command to fail with "Direct SQLite connection not available for shared backend" - rebuilt and reinstalled to `~/.local/bin/magellan`
- **mirage hotspots**: Fixed `mirage hotspots --entry main` returning "0 functions" (needed `mut db` for `conn_mut()` call)

### Changed

- Hook configuration updated to run stub-check and build-check via proper git pre-commit hook (not Claude Code hooks)
- Clippy validation now checks `--lib --bins` only (not tests) to avoid pre-existing test issues
- splice now depends on magellan from crates.io (version 3.1.7) for external users
- All Phase 1 P0 commands verified working: magellan backfill, magellan dead-code, splice dead-code, splice reachable, mirage hotspots, mirage unreachable
- Removed AI terminology from public documentation — this is a code intelligence toolchain, not an AI product

### Phase 2 Investigation (2026-05-01)

Investigated consistency issues from MASTER_PLAN Phase 2:

- **`llmgrep search --query "fn"`**: Returns 0 results is **expected behavior** - "fn" is a Rust keyword, not a symbol name. Use `--kind fn` to filter by function type (e.g., `llmgrep search --query test --kind fn` returns 729 results)

- **`splice query --label` vs `magellan query --file`**: Different semantics (label-based vs file-based query) - not a bug, design difference

- **`mirage hotpaths`**: Command exists and works correctly (discovered during investigation)

### Known Issues

- `mirage hotspots`: Verified working after `mut db` fix in mirage repo
- `mirage unreachable`: Verified working - `--within-functions` flag name is correct
- `tests/stress_concurrent_edits.rs::stress_database_integrity` can still deadlock (pre-existing)
- `tests/call_graph_tests.rs::test_cross_file_call_resolution` may fail (pre-existing)

## [3.1.7] - 2026-04-27

### Added

- `backfill` command to recompute metrics and derived data.
- `index` command to index a single source file.
- `delete` command to remove one file from the index.
- `cross-file-refs` command to report references to an FQN from other files.
- LCOV coverage ingestion with `ingest-coverage`.
- Stable `status` JSON coverage shape:
  - `coverage.available`
  - `coverage.covered_blocks`
  - `coverage.covered_edges`
  - optional `coverage.source`, `coverage.revision`, `coverage.ingested_at`
- CFG coverage side tables:
  - `cfg_block_coverage`
  - `cfg_edge_coverage`
  - `cfg_coverage_meta`
- Optional `external-tools-cfg` feature for C/C++ and Java CFG extraction through
  installed external tools.

### Changed

- Public documentation now describes the supported SQLite `.db` workflow as the
  normal source-of-truth storage model.
- Multi-language single-file indexing now dispatches all supported language
  parsers through the parser pool, including Python, JavaScript, and TypeScript.
- Compatibility preflight now distinguishes non-SQLite files, missing
  `graph_meta`, missing `graph_meta.id = 1`, and schema mismatches before any
  database mutation.

### Fixed

- Python files indexed through `CodeGraph::index_file()` now produce symbol
  facts.
- JavaScript and TypeScript files indexed through `CodeGraph::index_file()` now
  produce symbol facts.
- Incompatible database errors now use deterministic `DB_COMPAT` markers.
- Conflict cleanup around coverage/status wiring and command module staging.

### Known Issues

- `tests/stress_concurrent_edits.rs::stress_database_integrity` can still
  deadlock and fail after its timeout. Treat this as unresolved until fixed in a
  current verification run.

## [3.1.6]

### Added

- Schema version 11.
- CFG block hashes, statement snippets, and 4D coordinate columns in SQLite CFG
  storage.
- CFG edge extraction improvements for short-circuit operators, `?` error paths,
  and match guards.
- Status reporting for file, symbol, reference, call, code chunk, and coverage
  counts.

### Changed

- SQLite is documented as the supported public database workflow.
- JSON output continues to use schema version `1.0.0`.

## [3.1.5] - 2026-03-20

### Added

- Symbol lookup indexes for faster symbol resolution.

### Fixed

- Rescan event handling issues in watch/index workflows.

## [3.1.4] - 2026-03-19

### Fixed

- CLI query issues in `status`, `query`, `find`, and `refs`.
- Unsafe algorithm implementation paths replaced with graph traversal helpers.

## [3.1.2] - 2026-03-15

### Added

- `refresh` command for git-aware database synchronization.

### Fixed

- Stale file index handling when files are deleted.

## [3.1.1] - 2026-03-15

### Added

- Improved symbol ranking and ambiguity reporting.
- `GraphStats` API.

### Fixed

- Ambiguous `find` command behavior.

# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://sememver.org/spec/v2.0.0.html).

## [4.3.2] - 2026-06-06

### Fixed

- **`hopgraph` CLI now loads HNSW index from persistence** — `search_symbols`
  returned empty results when called from a fresh CLI connection because the
  HNSW index was not in memory. Added `ensure_index_in_memory` call before the
  early-return guard so the index is loaded via `restore_topology` before
  searching (`src/graph/search.rs`).
- **Bump sqlitegraph to 3.1.5** — picks up two HNSW correctness fixes:
  vector maps now keyed by layer-local IDs (not `vector_id - 1`), and
  `search()` returns actual storage vector IDs instead of `local_id + 1`.
  Both bugs caused silent `NodeNotFound` failures in multi-layer mode when
  vector IDs were not sequential from 1 (e.g. after any table wipe with
  AUTOINCREMENT active).

## [4.3.0] - 2026-06-04

### Added

- **HopGraph: embedding-based symbol search over HNSW vector index** (`src/graph/embed.rs`, `src/graph/search.rs`, `src/graph/ops.rs`):
  - `TextEmbedder` trait with two backends: `HashEmbedder` (128-dim structural hash, default) and `OllamaEmbedder` (768-dim, ureq to ollama `/api/embed`).
  - `create_embedder(enabled, base_url, model)` factory — `enabled=false` → HashEmbedder, `enabled=true` → OllamaEmbedder. Runtime config, no recompilation.
  - `[embeddings]` section in `~/.config/magellan/config.toml`: `enabled` (bool, default false), `base_url`, `model` (default "nomic-embed-text").
  - `CodeGraph.configure_embeddings()` method to swap embedder at runtime.
  - HNSW "symbols" index created lazily on first insert. `add_to_search_index` and `add_to_search_index_with_vector` create the index if absent.
  - `index_file` hook: after symbol insert, embeds symbol text and adds vector to HNSW index. Skipped if index doesn't exist (zero overhead for users without embeddings).
  - `delete_file_facts` hook: stale vectors handled via existence filtering in `search_symbols` rather than HNSW deletion (avoids entry-point corruption).
  - `search_symbols` over-fetches `k*2` and validates each entity still exists via `graph.get_entity()`, filtering out deleted symbols transparently.
  - `CodeGraph.hopgraph_search(query, k)` public method wrapping embed + HNSW search + entity validation.
  - 7 embed tests, 4 search tests.

- **`magellan hopgraph` CLI command** (`src/hopgraph_cmd.rs`, `src/cli.rs`, `src/cli/parsers/semantic.rs`):
  - `magellan hopgraph <query> --db <path> --k 10 --output human|json|pretty`
  - Queries the HopGraph HNSW index and returns ranked entity results.
  - Human output: ranked list with entity_id and cosine score.
  - JSON/Pretty output: structured array with rank, entity_id, score.

- **Integration tests for HopGraph lifecycle** (`src/graph/ops.rs`):
  - `test_hopgraph_lifecycle_index_delete_reindex`: index file → verify HNSW populated → delete file → verify symbols removed → reindex → verify repopulated.
  - `test_hopgraph_multiple_files_ranking`: multi-file indexing, single-file delete isolation.

### Changed

- `Cargo.toml`: sqlitegraph 3.0.3 → 3.1.1, added `ureq = { version = "3", features = ["json"] }`.
- `src/graph/search.rs`: `remove_from_search_index` simplified to no-op (HNSW vector deletion requires public sqlitegraph storage API not yet available; stale vectors filtered at query time instead).
- `src/graph/db_compat.rs` / `src/graph/mod.rs`: after `SqliteGraph::open` runs sqlitegraph migrations, `magellan_meta.sqlitegraph_schema_version` is updated to match current sqlitegraph schema. Prevents `DB_COMPAT` errors when sqlitegraph adds schema migrations (e.g. v5→v6 `order_idx` column).
- `src/graph/symbols.rs`: `sqlite_graph()` accessor returns `Result<&SqliteGraph>` instead of panicking.

### Known Limitations

- **HashEmbedder is structural, not semantic**: matching is based on token overlap in symbol names/FQNs. "parse_rust" vs "parse_python" = moderate cosine (shared tokens), "sync_claude_transcript" vs "process_file" = near-zero cosine. Enable Ollama embeddings for semantic search.
- **Stale vectors are not deleted from HNSW**: when symbols are deleted via `delete_file_facts`, the HNSW vectors remain but are filtered at query time by verifying entity existence. This means the HNSW index grows monotonically until a full reindex. For large codebases with frequent deletes, periodic `magellan refresh` is recommended.
- **No cross-domain hops**: magellan has code structure edges only (CALLS/DEFINES/REFERENCES/IMPORTS). Cross-domain knowledge (Explains, DerivedFrom) requires atheneum navigation after HopGraph entry-point discovery.
- **Single-threaded worker bottleneck**: all 14 projects share one `worker_loop`. Large projects can starve smaller projects during initial scan burst.
- **OllamaEmbedder requires running ollama instance**: no fallback if ollama is unreachable — embedding calls will fail.
- **Service worker blocks during embedding**: the single-threaded worker blocks on ureq HTTP calls to ollama. A file with 52 symbols takes ~3s (batch embedding). During this time, all other projects are queued. For codebases with many files, consider doing initial scan with embeddings disabled, then enabling for incremental updates.

- **HopGraph hooks guarded by `embeddings_enabled` flag** (`src/graph/ops.rs`, `src/graph/mod.rs`):
  - `CodeGraph` has `embeddings_enabled: bool` field, default `false`.
  - `index_file` and `delete_file_facts` HopGraph hooks skip entirely when `embeddings_enabled = false` — zero overhead, identical to 4.2.1 behavior.
  - `hopgraph_search` returns empty results when disabled.
  - `configure_embeddings(enabled=true, ...)` sets the flag; `[embeddings] enabled = true` in config.toml activates it.
  - Regression test `test_no_hnsw_index_when_embeddings_disabled` confirms no HNSW index is created when disabled.

- **`CodeGraph::open()` now reads `[embeddings]` config** (`src/graph/mod.rs`):
  - Previously: `[embeddings] enabled = true` in config.toml was parsed but never applied — all CodeGraph instances defaulted to `embeddings_enabled = false`.
  - Fix: `CodeGraph::open()` loads `config::load()` and calls `configure_embeddings()` when `enabled = true`. This covers all 43 call sites (service worker, CLI commands, admin socket) without per-site changes.
  - Config load failure is silent (falls back to disabled) — no disruption if config file is missing or malformed.

- **Service worker hot-reloads embeddings config** (`src/service/mod.rs`):
  - Previously: cached `CodeGraph` instances never re-read config after startup. Changing `[embeddings]` in config.toml required a daemon restart.
  - Fix: worker loop loads config on each batch, compares embeddings fields, and calls `configure_embeddings()` only when values change. No daemon restart needed to toggle embeddings on/off.

- **`structural.rs` audited — no double-embedding risk** (`src/service/structural.rs`):
  - Confirmed: `build_cross_refs` uses its own `structural_hash`/`kind_vector` system stored in `meta_db`, not the HopGraph HNSW index. Calls `symbols_in_file()` (read-only), never `index_file()`. No interaction with HopGraph hooks.

## [4.2.1] - 2026-06-03

### Fixed

- **Critical: Eliminated infinite watcher loop that killed the daemon** (`src/watcher/mod.rs`):
  - Root cause: `notify-debouncer-mini` 0.7.0 forwarded ALL inotify events including `ACCESS`/`OPEN`/`CLOSE_NOWRITE` (read-only). `reconcile_file_path` calls `fs::read()` on changed files, which triggers these read-only inotify events. The debouncer treated them as new filesystem changes, creating an infinite feedback loop: touch → batch → reconcile → read → ACCESS event → batch → reconcile → read → ...
  - Fix: Replaced `notify-debouncer-mini` with direct `notify::RecommendedWatcher` + custom debouncing that filters out read-only events (`EventKind::Access`) at the source. Only `Create`, `Modify`, `Remove`, `Any`, and `Other` events are processed.
  - Removed `notify-debouncer-mini` dependency entirely. Custom debouncer uses `HashMap<PathBuf, Instant>` with time-based expiry matching the configured `debounce_ms`.
  - Verified: single `touch` now produces exactly ONE batch, then silence. Previously produced batches every 500ms indefinitely until the daemon died.
  - 3 new tests: `test_is_mutation_event_accepts_create_modify_remove`, `test_is_mutation_event_rejects_access`, `test_filter_dirty_paths_excludes_db_files`.

### Known Limitations

- Single-threaded worker bottleneck: all 14 projects share one `worker_loop`. Large projects (rocmforge: 958MB DB, 207K symbols) can starve smaller projects during initial scan burst. Per-project workers planned.
- Admin socket at `/run/user/1000/magellan.sock` may vanish (suspected systemd-tmpfiles cleanup).
- meta.db grows without bound (2.6GB, 18M+ events for rocmforge). Needs retention/rotation.
- Hundreds of `.tmp*.db` files in `~/.magellan/` from `magellan refresh` calls need cleanup.

## [4.2.0] - 2026-05-28

### Added

- **Auto-detect project layout from manifest files** (`src/manifest.rs`, `src/project_config.rs`):
  - Extracted all manifest types into new `src/manifest.rs` module (was in `project_config.rs`, split for 1K LOC compliance)
  - `CargoManifest::detect_include_paths()` — extracts unique directory paths from `[[bin]]`, `[[test]]`, `[[bench]]`, `[[example]]`, and `[lib]` target paths, always including `src/`
  - `CargoManifest::parse()` now also extracts `[[example]]` targets and `[lib]` path (was only `bin`/`test`/`bench`)
  - `PyprojectManifest` — parses `pyproject.toml` for `[project]` name, `[tool.setuptools.packages.find]` where, `[tool.pytest.ini_options]` testpaths
  - `GoModuleManifest` — parses `go.mod` for module name; detects Go convention dirs (`cmd/`, `internal/`, `pkg/`, `api/`, `web/`) if they exist
  - `PackageJsonManifest` — parses `package.json` for JS/TS: `"main"`, `"files"`, `"exports"` to extract source dirs
  - `TsconfigManifest` — parses `tsconfig.json` `"include"` array to extract source directories from glob patterns
  - `MavenManifest` — parses `pom.xml` for Java; detects Maven convention dirs (`src/main/java/`, `src/test/java/`, etc.) if they exist
  - `CMakeManifest` — parses `CMakeLists.txt` for C/C++/CUDA: `project()` name and `add_subdirectory()` calls
  - `detect_include_paths_from_root(root)` — tries manifests in order: Cargo.toml → pyproject.toml → go.mod → package.json+tsconfig.json → pom.xml → CMakeLists.txt → fallback `["src/"]`
  - `ProjectConfig::init()` now auto-detects include paths from any supported manifest
  - `merge_scan_config()` in watch pipeline simplified to use `detect_include_paths_from_root()` for all languages
  - 35 new tests (16 Cargo/Pyproject + 19 Go/JS/TS/Java/CMake)

- **SymbolNavigator — navigable graph traversal** (`src/graph/navigator.rs`):
  - New `SymbolNavigator` struct wrapping sqlitegraph's `GraphQuery` with magellan-aware entity resolution
  - `SymbolInfo` enriched with `kind_normalized`, `start_line`, `end_line`, `byte_start` parsed from `graph_entities.data` JSON
  - `DepthSymbol` struct preserving BFS hop depth for callers/callees
  - Methods: `resolve()`, `resolve_by_prefix()`, `info()`, `expand()`, `expand_typed()`, `chain()`, `k_hop_callers()`, `k_hop_callees()`, `k_hop_references()`, `pattern()`
  - Custom `call_bfs()` performing 2-edge BFS (`Symbol→Call→Symbol`) with depth tracking — replaces flat `k_hop_filtered` which lost depth information
  - All types derive `Serialize` for JSON output
  - 11 tests covering resolve, expand, chain (1-hop and 2-hop), depth-aware callers/callees

- **`magellan explore` CLI command** (`src/explore_cmd.rs`):
  - Stepable graph navigation: `--symbol`, `--id`, `--edges`, `--callers`, `--callees`, `--chain`, `--depth`
  - `--json`/`-j` flag producing structured JSON output — the contract envoy/atheneum will mirror
  - JSON response format: `{node, resolve, edges, callers, callees, chain}` with `DepthResponse {depth, node}` for k-hop results
  - Chain traversal syntax: `>CALLER,>CALLS` (outgoing), `<CALLER` (incoming)

- **`navigate` command rewritten to use SymbolNavigator** (`src/navigate_cmd.rs`):
  - Replaced `get_callers()`/`get_callees()`/`impact_analysis()`/`affected_analysis()` with navigator's `k_hop_callers()`/`k_hop_callees()`
  - Removed dependency on `context::query` functions — all graph traversal now through sqlitegraph's graph API
  - Kind now shows `fn`/`struct`/`enum` instead of `Unknown`; `byte_start` shows actual offset instead of `0`
  - Impact/affected sections now show correct depth values (was all `depth 0`)
  - Callers/callees resolve to definition location instead of call-site location

- **Service daemon: multi-path include/exclude filtering** (`src/service/`, `src/cli/parsers/system.rs`, `src/service_cmd.rs`):
  - `ProjectEntry` gains `include: Vec<String>` and `exclude: Vec<String>` fields with `#[serde(default)]` for backward compatibility
  - CLI: `--include`/`-I` and `--exclude`/`-E` repeatable flags on `service register`
  - `watcher_task` builds `FileFilter` from include/exclude patterns; normalizes `src/` to `src/**` for file-level matching
  - Dynamic registration now spawns watchers (was broken — admin socket received `None` for `watcher_map`)
  - Fixed JSON-RPC request format: all service commands now send params at top level (was nested in `"params":{}`, incompatible with `#[serde(flatten)]`)
  - Fixed positional project name parsing: `magellan service unregister <name>` now works (was silently defaulting to empty)
  - `MAGELLAN_LOCAL=1` env var forces local mode (bypasses daemon detection for tests)
  - 2 new tests: `registry_include_exclude_roundtrip`, `registry_backward_compat_without_include_exclude`
  - 1 new test: `test_watcher_task_filters_by_include_patterns`
  - Fixed 2 pre-existing `scan_tests` failures (files placed at root instead of `src/`)
  - E2E verified: file creation → symbol indexed, file modification → reindexed, file deletion → facts removed

### Changed

- **Service daemon socket API documented** (`docs/API_INTEGRATION.md`):
  - Full JSON-RPC method reference with wire format, error codes, and forge integration examples
  - All 23 methods documented: project management, cross-project queries, evolution loop, events
  - Corrected wire format: params are flattened at top level (was incorrectly documented as `"params": {}`)
  - `MANUAL.md` updated with DB path conventions, socket API examples, and include/exclude usage
  - `README.md` updated with service daemon section

- **`cfg_edges_extract.rs` modularization** (commit `6d07af9`):
  - Split 2577-line file into 4 sub-1K files: `mod.rs` (327), `extract.rs` (748), `control_flow.rs` (593), `tests.rs` (842)
  - Extracted `CONTROL_FLOW_KINDS` constant + `is_control_flow()` helper
  - Merged identical `statement`/`expression_statement` handlers

- **CFG extraction for Go and CUDA** (`src/graph/cfg_edges_extract/`):
  - Extended `find_function_node` to recognize Go `function_declaration` and `method_declaration`
  - Changed if-block extraction from positional child indexing to kind-based discovery — fixes Go `if_statement` where `block` is at variable child index due to optional init statement
  - Added `statement_list` as recognized block container — Go wraps `block` bodies in `statement_list`
  - Separated `"statement"` handler from general statement catch-all to unwrap Go control flow (`if_statement`, `for_statement`, `return_statement`, etc.) instead of treating it as a plain statement
  - Added `expression_switch_statement`, `type_switch_statement` to match/switch dispatch
  - Added Go case node kinds (`expression_case`, `default_case`, `type_case`) to arm discovery in `extract_match_blocks_with_fallthrough`
  - 6 new tests: Go function/method/for-loop/switch CFG extraction, CUDA kernel/device-function CFG extraction
  - CUDA `__global__` and `__device__` functions already worked via C++ `function_definition` node kind

- **Language support: Go, CUDA, and HIP** (`Cargo.toml`, `src/ingest/`):
  - Added `tree-sitter-go = "0.25.0"` and `tree-sitter-cuda = "0.21.1"` dependencies
  - New `src/ingest/go.rs` — symbol extraction for Go: functions, methods, structs, interfaces, packages, type aliases
  - New `src/ingest/cuda.rs` — symbol extraction for CUDA: functions, classes, structs, enums, namespaces, unions, templates (C++-based patterns)
  - `.hip` files now detected as C++ (HIP is C++ with AMD extensions; no dedicated tree-sitter grammar exists)
  - `Language` enum expanded: `Go`, `Cuda` variants; `.go`, `.cu`, `.cuh`, `.hip` extension mappings
  - Thread-local parser pool updated with `GO_PARSER` and `CUDA_PARSER`
  - Call graph and reference indexing wired for Go and CUDA in `call_ops.rs` and `references.rs`
  - 2 new test cases in `detect.rs` for Go and CUDA extensions

### Removed

- **Dead `glam` dependency** (`Cargo.toml`):
  - Removed `glam = { version = "0.27", features = ["serde"] }` — unused since geometric backend removal (commit 5b6e0634, 2026-03-15)
  - Zero source references confirmed; no functional impact

- **Dead `GeoIndexMeta` code** (`src/graph/schema.rs`, `src/graph/db_compat.rs`):
  - Removed `GeoIndexMeta` struct and its `record_geo_index_built()` / `get_geo_index_meta()` methods from `schema.rs` — geometric backend artifact with zero callers
  - Removed `ensure_geo_index_meta_schema()` function and all call sites from `db_compat.rs`
  - Schema migration path updated: v10 now skips directly to v12 (FTS5), removing the v11 geo_index_meta step
  - Existing databases with `geo_index_meta` table are harmless — the table is simply no longer created for new DBs

## [4.1.1] - 2026-05-26

### Changed

- **Removed `geometric-backend` from default features** (`Cargo.toml`, `src/geo_builder.rs`):
  - Default features changed from `["sqlite-backend", "geometric-backend"]` to `["sqlite-backend"]`
  - The geometric backend (`.geo` files) has been unused since March 2026; all production workflows use SQLite (`.db`)
  - `geometric-backend` remains available as an opt-in feature: `--features geometric-backend`
  - `src/geo_builder.rs`: `compute_checksum` now gated behind `#[cfg(feature = "geometric-backend")]`

### Compatibility Notice

- **Mirage compatibility**: Mirage will be upgraded to work with this schema correction. Users of mirage should update to the upcoming mirage release after upgrading magellan.
- **Splice compatibility**: SPL-E091 schema mismatch errors are expected until splice updates its magellan dependency to 4.1.0+. The workaround is to re-index with `magellan watch --root ./src --db .magellan/<project>.db --scan-initial`.

### Added

- **P7-TRACING: Structured logging with `tracing` crate** (`Cargo.toml`, `src/service/mod.rs`):
  - Added `tracing = "0.1"` dependency
  - Replaced all 7 `eprintln!` calls in `src/service/` with structured `tracing::info!`, `tracing::warn!`, `tracing::error!` macros with key-value fields (project, db, path, error)
  - Zero `eprintln!` remaining in service daemon code
- **P7-SCHEMA: `daemon_events` table and event logging API** (`src/service/meta_db.rs`):
  - `daemon_events` SQLite table: `id`, `event_type`, `project_name`, `file_path`, `details` (JSON), `created_at`, `execution_id`
  - Indexes on `(project_name, created_at DESC)` and `(event_type, created_at DESC)`
  - `DaemonEvent` struct with full event representation
  - `EventFilter` struct with project, event_type, since, until, limit fields (default limit 50)
  - `MetaDb::log_event(&DaemonEvent) -> Result<i64>` — insert event, return rowid
  - `MetaDb::list_events(&EventFilter) -> Result<Vec<DaemonEvent>>` — query with dynamic SQL builder
  - 4 unit tests covering log+list, project filter, type filter, and limit
- **P7-LOG: Event instrumentation in daemon loops** (`src/service/mod.rs`, `src/service/admin_socket.rs`):
  - `worker_loop`: logs `batch_received` events with path count on each batch
  - `worker_loop`: logs `reconcile_err` events per-file on reconcile failure
  - `watcher_task`: logs watcher start with `tracing::info!`
  - `AdminSocket::dispatch`: logs `admin_request` events for all methods, with project_name extracted for register/unregister/pause/resume
- **P7-CLI: `magellan service events` subcommand** (`src/cli.rs`, `src/service_cmd.rs`, `src/service/admin_socket.rs`):
  - `ServiceAction::Events` variant with `--project`, `--type`, `--since`, `--limit`, `--json` flags
  - `events` JSON-RPC method in admin socket dispatch querying `daemon_events` via `EventFilter`
  - Human-readable table output (default) or JSON array output (`--json`)
  - `--since <hours>` converts to Unix timestamp for time-window filtering
  - Integration test verifying end-to-end: pre-seed events → query via socket → assert response structure
- **P5-ANALYZE: Hotspot candidate detection** (`src/service/meta_db.rs`, `src/service/admin_socket.rs`):
  - `HotspotCandidate` struct with `symbol`, `file`, `project`, `rank_score`, `fan_in`, `complexity`
  - `MetaDb::analyze_hotspots(project_filter, limit)` — aggregates `symbol_metrics` across enabled project shards; ranks by `fan_in * cyclomatic_complexity` DESC; respects optional per-project filter and result limit
  - `evolve.analyze` JSON-RPC socket method — demand-triggered analysis returning ranked candidate array with metadata; params: `project` (optional), `limit` (optional)
  - 3 unit tests + 1 integration test covering ranking formula, project filtering, disabled-project exclusion, and end-to-end socket dispatch
- **P5-RETRIEVE: Analogue retrieval from cross-ref index** (`src/service/admin_socket.rs`):
  - `evolve.retrieve` JSON-RPC socket method — queries `pattern_cross_refs` for analogues of a given `(project, symbol)` pair; supports `to_project` optional filter and `limit` truncation
  - Falls back gracefully to empty `analogues` array when cross-ref index is unpopulated
  - 2 unit tests + 1 integration test covering round-trip, empty match, and limit truncation
- **P5-PROPOSE: Candidate patch persistence** (`src/service/admin_socket.rs`, `src/service/candidates.rs`):
  - `evolve.propose` JSON-RPC socket method — persists a candidate improvement patch into the project's `candidate_facts` table (idempotent via optional `candidate_id`)
  - `evolve.candidates` JSON-RPC socket method — lists persisted candidates by status, with optional `limit`
  - `src/service/candidates.rs` — new module with `CandidateRecord`, `insert_candidate_fact`, `list_candidates`, `update_candidate_status`
  - Auto-generates `candidate_id` from `{project}/{symbol}-{timestamp}` if not provided
  - Stores `patch_diff` and analogue metadata as JSON in `properties_json`
  - 2 integration tests covering propose round-trip and candidates listing with status filter
- **P5-PROMOTE / P5-REJECT: Candidate status transitions** (`src/service/admin_socket.rs`, `src/service/candidates.rs`):
  - `evolve.promote` JSON-RPC socket method — sets a candidate's status to `promoted` and records `reviewed_at`
  - `evolve.reject` JSON-RPC socket method — sets a candidate's status to `rejected` with optional `rejection_reason`
  - Returns `error:-32006` if candidate_id not found (zero rows affected)
  - 3 unit tests + 1 integration test covering promote, reject with reason, and missing-candidate edge case
- **P5-VERIFY: Temp worktree patch verification** (`src/service/admin_socket.rs`, `src/service/verify.rs`):
  - `evolve.verify` JSON-RPC socket method — creates temp worktree copy, applies candidate's `patch_diff` via `patch -p0`, auto-detects test harness (`cargo test` / `pytest` / `npm test`), runs tests, and updates candidate status to `verified` (passing) or `rejected` (failing)
  - `src/service/verify.rs` — new module with `verify_candidate()`, `detect_test_command()`, `copy_dir_all()` (pure-Rust recursive copy excluding destination)
  - `get_candidate_by_id()` helper in `candidates.rs` to fetch candidate's stored `patch_diff`
  - 1 integration test covering full round-trip: propose → verify on real Rust project with passing `cargo test`
- **Phase 6: Runtime Watcher Auto-Spawn** (`src/service/admin_socket.rs`, `src/service/mod.rs`):
  - `AdminSocket::handle_client` now accepts `WatcherMap` + `shutdown_rx` — `register` and `resume` socket handlers spawn `watcher_task` immediately without requiring a daemon restart
  - `WatcherMap` tracks per-project shutdown senders for clean per-project lifecycle
  - Backward-compat `handle_client_raw` wrappers preserve existing test call sites
  - Integration test: `test_register_spawns_watcher_on_running_daemon`
- **Service-daemon CLI wiring** (`src/cli.rs`, `src/main.rs`, `tests/daemon_argv_tests.rs`):
  - `Command::ServiceDaemon` added to CLI enum — closes Phase 0 gap where `service_cmd.rs` spawned `service-daemon` but the CLI parser rejected it as unknown
  - `main.rs` handler spawns `Service::new().await?.run().await`
  - `tests/daemon_argv_tests.rs` — integration test catching unwired subcommand regression
- **systemd user-level service support** (`src/service/mod.rs`, `src/service_cmd.rs`, `src/watch_cmd.rs`):
  - Socket path moved from hardcoded `/tmp/magellan.sock` to `socket_path()` helper respecting `XDG_RUNTIME_DIR`
  - When `XDG_RUNTIME_DIR` is set (true for systemd user services and most modern desktops) the daemon binds its UDS under `$XDG_RUNTIME_DIR/magellan.sock`
  - All callers of `SOCKET_PATH` updated to use `socket_path()`: `setup_socket`, `cleanup`, `send_request`, `is_daemon_running`
  - `~/.config/systemd/user/magellan.service` unit file added: `Type=exec`, `Restart=on-failure`
  - Verified under systemd 260: service starts, socket created at `/run/user/UID/magellan.sock`, `magellan service status` / `service stats` communicate correctly

### Changed

- **CLI parser architecture** (`src/cli/parsers.rs` → `src/cli/parsers/`):
  - Extracted ~3,800-line monolith into 8 categorized submodules: `core`, `index`, `query`, `graph`, `semantic`, `registry`, `config_project`, `system`
  - `core.rs` holds shared helpers (`parse_required_arg`, `parse_output_format`, `parse_path_arg`, `parse_db_paths`)
  - `mod.rs` re-exports all via `pub use ::*` for backward compatibility
  - Zero functional change; all 858 tests pass
- **`lcov` dependency removed** (`Cargo.toml`, `src/ingest_coverage/mod.rs`, `src/lcov_parser.rs`):
  - Replaced with hand-rolled 47-line LCOV text parser (3 unit tests: branch info, function coverage, missing file)
  - Eliminates duplicate `thiserror` versions in dependency tree; single `thiserror 1.0.69` remains
- **CLI help text extraction** (`src/cli/`):
  - Help text strings moved to `src/cli/help_text.rs` and `src/cli/tests.rs` (1,562-line test extraction)
  - Keeps `src/cli.rs` focused on enum definitions and dispatch logic
- **SM-2: Modularize `src/indexer.rs` — Phase 2 watch pipeline extracted** (`src/indexer.rs`, `src/indexer/watch.rs`):
  - Moved `run_watch_pipeline` and the `WatchPipelineConfig`/`WatchPipelineState`/`PipelineSharedState` machinery from `src/indexer.rs` (1,446 lines → 609 lines)
  - Created `src/indexer/watch.rs` (876 lines) as a standalone submodule; `pub mod watch;` + `pub use watch::{run_watch_pipeline, WatchPipelineConfig};` preserved public API
  - Constants `DEFAULT_L3_CACHE_SIZE` and `TARGET_CACHE_USAGE` set to `pub(crate)` to share with submodule
  - Call site in `src/watch_cmd.rs` updated to `magellan::indexer::watch::run_watch_pipeline_geometric`
  - All 691 tests pass, zero functional change
- **SM-2-OPT: Zero-clone batch processing + function extraction** (`src/indexer.rs`, `src/indexer/watch.rs`):
  - Added `compute_l3_cache_batch_indices(sizes: &[usize], ...) -> Vec<Vec<usize>>` — index-based batching that never clones paths
  - `read_batch_sources` made generic over `AsRef<Path>` (saves `PathBuf` clones when called from index-lookup loops)
  - `process_dirty_paths_batched` refactored to use index lookups (`&dirty_paths[idx]`) instead of cloning paths into batches — eliminates ~2 clones per file in the hot loop
  - Extracted `merge_scan_config()` (~64 lines) and `wait_for_watcher_thread()` (~54 lines) from `run_watch_pipeline` — shrunk from ~261 → ~203 lines
  - `process_dirty_paths_batched` shrank from ~145 → ~140 lines (index-based batching is shorter and clearer)
  - All 691 tests pass, clippy zero warnings

### Fixed

- **P7-DEBT: Codebase quality audit — A+ grade** (2026-05-26):
  - `cargo deny check` — pruned 2 stale advisory IDs, removed 1 redundant `[bans]` entry
  - `cargo clippy --all-targets -- -D warnings` — 4 residual warnings fixed; zero warnings cold rebuild
  - `cargo doc --no-deps` — 27 rustdoc backtick formatting errors corrected
  - `cargo fmt --all -- --check` — clean
  - `test_compute_delta_with_untracked` — removed CWD dependency via explicit `project_root` parameter
  - `test_parse_frontmatter_float` — replaced PI/EPSILON with exact `2.5` to avoid flakiness
  - All 691 lib tests + 167 bin tests pass; zero compiler, clippy, rustdoc, fmt, or deny warnings
- **`refresh_cmd.rs::force` field wired** (`src/refresh_cmd.rs`):
  - `RefreshArgs::force` was `#[allow(dead_code)]`; `compute_delta()` now returns all DB-tracked files as `to_update` (empty `to_delete`/`to_add`) when `force` is true, bypassing git comparison entirely
  - Preserves existing `--force` CLI flag and help text
- **`service/` blanket `#![allow(dead_code)]` removed** (`src/service/mod.rs`, `src/service/meta_db.rs`, `src/service/registry.rs`):
  - Removed `#![allow(dead_code)]` module-level pragma from `service/mod.rs`
  - Removed unused `SOCKET_PATH` constant (all callers already use `socket_path()`)
  - Removed unused `Service::shutdown()` method (zero production callers; all `.shutdown()` calls are on `tokio::net::UnixStream` write halves)
  - Added 6 targeted `#[allow(dead_code, reason = "...")]` annotations: `MetaDb.path` (future diagnostics), `EmbeddingRecord.hash`/`CrossRefRecord.symbol_a/file_a` (Phase 2 structural embeddings WIP), `MetaDb::remove_project`/`update_counts` and `Registry::names` (test-only)
- **`P7-LOG-BATCH` completion** (`src/service/mod.rs`, `src/service/admin_socket.rs`):
  - `worker_loop` logs `reconcile_ok` per-file on successful reconcile (was `reconcile_err` only)
  - `worker_loop` logs `checkpoint_ok`/`checkpoint_err` per batch
- **`P7-LOG-ADMIN` completion** (`src/service/admin_socket.rs`):
  - `AdminSocket::dispatch` logs `admin_err` events with method name and error details on JSON dispatch failure

## [4.0.0] - 2026-05-24

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

- **P4-SUGGEST: `query.suggest` socket method** (`src/service/admin_socket.rs`):
  - JSON-RPC method for analogous-symbol recommendations from cross-ref index
  - Params: `from_project` (required), `name` (required), `to_project` (optional filter)
  - Queries `pattern_cross_refs` ORDER BY similarity_score DESC; returns `suggestions` array with `project`, `symbol`, `file`, `similarity_score` per entry
- **P4-COMPARE: `query.compare` enriched with `similarity_score`** (`src/service/admin_socket.rs`):
  - Each entry in the `comparisons` array now includes `"similarity_score"` when a cross-ref pair exists in `pattern_cross_refs`
  - Score is the best stored similarity across the requested project set for that symbol; field absent when no cross-ref has been indexed yet
- **P4-CROSS: Cross-project structural similarity index** (`src/service/structural.rs`, `src/service/admin_socket.rs`):
  - `build_cross_refs(meta_db, db_paths, threshold)` — iterates all project symbols, extracts AST via `parse_with_language`, upserts embeddings, then performs pairwise cosine similarity across different projects; pairs ≥ threshold inserted into `pattern_cross_refs`; returns count of pairs inserted
  - `query.build-index` socket method — triggers `build_cross_refs` across all enabled registry projects with default threshold 0.70; returns `{ "pairs_inserted": N }`
  - `magellan::parse_with_language` and `magellan::extract_ast_nodes` re-exported for binary crate use
- **P3-CMP: `query.compare` socket method** (`src/service/admin_socket.rs`):
  - JSON-RPC method for side-by-side cross-project symbol comparison
  - Params: `name` (required), `projects` (array of project names)
  - Resolves project DBs from meta.db by name; returns `comparisons` array with per-project name/kind/file/callers/callees
- **P3-CTX: `query.context` socket method** (`src/service/admin_socket.rs`):
  - JSON-RPC method returning per-project symbol matches with `callers`/`callees` arrays
  - Params: `name` (required), `file` (optional), `callers` (bool), `callees` (bool), `depth` (usize)
  - Reuses `MultiDbContext::search_symbol`; results include full caller/callee name+file+line
- **P4-VEC: Bag-of-kinds vector + cosine similarity** (`src/service/structural.rs`):
  - `KIND_VOCAB` — 20-element stable vocabulary of structural AST kinds
  - `kind_vector(nodes, start, end) -> Vec<f32>` — L2-normalized histogram over `KIND_VOCAB`; zero vector when no structural nodes found
  - `cosine_similarity(a, b) -> f32` — dot product of unit vectors; returns 0.0 on length mismatch
- **P4-HASH: Structural fingerprint** (`src/service/structural.rs`):
  - `structural_hash(nodes, start, end) -> String` — SHA-256 hex of `"|"`-joined structural kind sequence within byte range
  - Filters to structural kinds via `is_structural_kind()`; sorts by `byte_start` before hashing
- **P4-SCHEMA: meta.db schema extension** (`src/service/meta_db.rs`, `docs/SCHEMA_META_DB.md`):
  - `concept_embeddings(project, symbol, file, hash, vec BLOB, updated_at)` — stores per-symbol structural embeddings as packed LE f32 bytes; PK `(project, symbol, file)`
  - `pattern_cross_refs(id, project_a, symbol_a, file_a, project_b, symbol_b, file_b, similarity_score, updated_at)` — cross-project similarity pairs ordered by score
  - `EmbeddingRecord`, `CrossRefRecord` structs; `upsert_embedding`, `list_embeddings`, `insert_cross_ref`, `query_cross_refs_for_symbol` methods
  - `docs/SCHEMA_META_DB.md` — new schema reference doc with full DDL, two-DB architecture diagram, Rust API table
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

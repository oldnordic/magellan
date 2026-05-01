# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.1.8] - 2026-05-01

### Added

- **LLM Enforcement Infrastructure** (Phase 0):
  - `scripts/validate-completion.sh` - 5-gate validation pipeline (stubs, check, test, clippy, db)
  - `.claude/hooks/stub-check.fish` - Pre-commit hook blocking TODO/unimplemented/panic in non-test code
  - `.claude/hooks/build-check.fish` - Pre-commit hook running cargo check/test/clippy
  - `docs/superpowers/skills/no-stubs-enforcement.md` - Skill for zero-tolerance stub policy
  - `docs/superpowers/skills/verification-before-completion.md` - Skill for completion gates
  - `docs/superpowers/MASTER_PLAN.md` - Master plan for toolchain improvement

### Fixed

- **Path resolution in query functions** (`symbol_nodes_in_file`, `symbol_nodes_in_file_with_ids`):
  - Relative paths (e.g., `src/main.rs`) were not matching paths in the database (which stores absolute paths)
  - Added `resolve_query_path()` helper that canonicalizes relative paths using current working directory
  - Now all query operations work consistently regardless of path format
- **splice dead-code / splice reachable**: Fixed by using local magellan path in splice's Cargo.toml
- Stale installed binary causing `backfill` command to fail with "Direct SQLite connection not available for shared backend" - rebuilt and reinstalled to `~/.local/bin/magellan`
- **mirage hotspots**: Fixed `mirage hotspots --entry main` returning "0 functions" (needed `mut db` for `conn_mut()` call)

### Changed

- Hook configuration updated to run stub-check and build-check via proper git pre-commit hook (not Claude Code hooks)
- Clippy validation now checks `--lib --bins` only (not tests) to avoid pre-existing test issues
- splice dependency now uses local magellan path for development
- All Phase 1 P0 commands verified working: magellan backfill, magellan dead-code, splice dead-code, splice reachable, mirage hotspots, mirage unreachable

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

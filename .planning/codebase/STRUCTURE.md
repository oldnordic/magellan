# Codebase Structure

**Analysis Date:** 2026-01-19

## Directory Layout

```
magellan/
├── src/                    # Library source code
│   ├── graph/             # Graph persistence layer
│   ├── ingest/            # Language-specific parsers
│   ├── generation/        # Code chunk storage
│   ├── main.rs            # CLI entry point
│   ├── lib.rs             # Library exports
│   ├── indexer.rs         # Watcher event coordinator
│   ├── references.rs      # Reference/call extraction
│   ├── watcher.rs         # Filesystem watcher
│   ├── verify.rs          # Database verification
│   └── *_cmd.rs           # Per-command handlers
├── tests/                 # Integration tests
├── docs/                  # Documentation
├── .planning/             # Planning/state documents
├── Cargo.toml             # Package manifest
└── README.md              # Project overview
```

## Directory Purposes

**src/:**
- Purpose: Main library source code for magellan crate
- Contains: Core modules, CLI implementation, graph persistence, parsing logic
- Key files: `main.rs`, `lib.rs`, `graph/mod.rs`, `ingest/mod.rs`

**src/graph/:**
- Purpose: Graph database operations using sqlitegraph
- Contains: `CodeGraph` facade, node/edge schemas, query operations, CRUD handlers
- Key files: `mod.rs`, `schema.rs`, `ops.rs`, `query.rs`, `files.rs`, `symbols.rs`, `references.rs`, `calls.rs`, `scan.rs`, `count.rs`, `export.rs`, `freshness.rs`, `db_compat.rs`

**src/ingest/:**
- Purpose: Multi-language symbol extraction via tree-sitter
- Contains: Language-specific parsers, language detection, symbol definitions
- Key files: `mod.rs`, `detect.rs`, `python.rs`, `java.rs`, `javascript.rs`, `typescript.rs`, `c.rs`, `cpp.rs`

**src/generation/:**
- Purpose: Store and retrieve source code chunks
- Contains: `ChunkStore`, `CodeChunk` schema
- Key files: `mod.rs`, `schema.rs`

**tests/:**
- Purpose: Integration tests (full database, filesystem operations)
- Contains: CLI tests, parser tests, indexer tests, verification tests
- Key files: `cli_smoke_tests.rs`, `parser_tests.rs`, `watcher_tests.rs`, `graph_persist.rs`, `verify_tests.rs`, `call_graph_tests.rs`, `multi_language_integration_tests.rs`

**docs/:**
- Purpose: Project documentation
- Contains: Architecture docs, development workflow, SQL schema references
- Key files: `SQLITEGRAPH_ARCHITECTURE.md`, `DEVELOPMENT_WORKFLOW.md`

**.planning/:**
- Purpose: State management, architectural decisions, planning documents
- Contains: Roadmap, phase plans, codebase analysis (this file)
- Generated: Yes (by GSD agents)
- Committed: Yes

## Key File Locations

**Entry Points:**
- `src/main.rs`: CLI entry point with command parsing and routing
- `src/lib.rs`: Library exports for use as dependency

**Configuration:**
- `Cargo.toml`: Package metadata, dependencies, features, binary definition

**Core Logic:**
- `src/graph/mod.rs`: `CodeGraph` struct - main API for database operations
- `src/graph/schema.rs`: Node payload definitions (FileNode, SymbolNode, etc.)
- `src/graph/ops.rs`: File indexing and deletion operations
- `src/graph/query.rs`: Symbol and reference queries
- `src/ingest/mod.rs`: `SymbolFact`, `SymbolKind`, `Parser` trait definitions

**Command Handlers:**
- `src/query_cmd.rs`: Query symbols in a file
- `src/find_cmd.rs`: Find symbols by name
- `src/refs_cmd.rs`: Show calls for a symbol
- `src/get_cmd.rs`: Get source code for a symbol
- `src/watch_cmd.rs`: Watch directory and index changes
- `src/verify_cmd.rs`: Verify database vs filesystem

**Testing:**
- `tests/`: Integration tests with real databases and filesystem operations

## Naming Conventions

**Files:**
- Modules: `snake_case.rs` (e.g., `query_cmd.rs`, `call_ops.rs`)
- Tests: `descriptive_name_tests.rs` (e.g., `parser_tests.rs`, `verify_tests.rs`)

**Directories:**
- Lowercase with underscores: `src/graph/`, `src/ingest/`

**Functions:**
- snake_case: `symbols_in_file()`, `reconcile_file_path()`, `delete_file_facts()`

**Structs:**
- PascalCase: `CodeGraph`, `SymbolFact`, `FileNode`, `ReferenceExtractor`

**Constants:**
- SCREAMING_SNAKE_CASE: `STALE_THRESHOLD_SECS`, `MAGELLAN_SCHEMA_VERSION`

## Where to Add New Code

**New Feature:**
- Primary code: `src/graph/` for database operations, new module within `graph/` if related to persistence
- Tests: `tests/` with naming convention `<feature>_tests.rs`

**New Command (CLI):**
- Implementation: `src/<command>_cmd.rs` (follows existing pattern like `query_cmd.rs`)
- Registration: Add command variant to `Command` enum in `src/main.rs`
- Routing: Add match arm in `main()` function in `src/main.rs`

**New Language Parser:**
- Implementation: `src/ingest/<language>.rs` (e.g., `src/ingest/go.rs`)
- Registration: Add `pub mod <language>;` to `src/ingest/mod.rs`
- Language enum: Add variant to `Language` enum in `src/ingest/detect.rs`
- Detection: Add extension mapping in `detect_language()` function
- Indexing: Add match arm in `index_file()` in `src/graph/ops.rs`

**New Component/Module:**
- Implementation: `src/<module>.rs` or `src/<module>/mod.rs` if multi-file
- Export: Add `pub mod <module>;` to `src/lib.rs`

**Utilities:**
- Shared helpers: `src/utils.rs` (create if needed) or within appropriate module

## Special Directories

**target/:**
- Purpose: Rust build artifacts, compiled binaries, dependencies
- Generated: Yes
- Committed: No (gitignored)

**.git/:**
- Purpose: Git repository metadata
- Generated: Yes
- Committed: N/A

**.fastembed_cache/:**
- Purpose: Cached embedding models (not currently used by core codebase)
- Generated: Yes
- Committed: No

**docs/archive/:**
- Purpose: Historical documentation
- Generated: No
- Committed: Yes

**.codemcp/:**
- Purpose: CodeMCP tool state
- Generated: Yes
- Committed: No

---

*Structure analysis: 2026-01-19*

# Codebase Structure

**Analysis Date:** 2026-02-08

## Directory Layout

```
magellan/
├── src/                          # Main source code
│   ├── .codemcp/                 # CodeMCP-specific files (backups)
│   ├── diagnostics/              # Error reporting and monitoring
│   ├── generation/               # Code chunk storage
│   ├── graph/                    # Graph database and algorithms
│   │   ├── export/               # Data export functionality
│   │   │   └── scip.rs          # SCIP format exporter
│   │   ├── metrics/             # Metrics computation
│   │   │   ├── backfill.rs      # Historical metrics
│   │   │   ├── compute.rs       # Metric calculations
│   │   │   ├── query.rs         # Metric queries
│   │   │   └── schema.rs        # Metrics schema
│   │   └── *.rs                 # Core graph modules
│   ├── ingest/                   # Language parsing
│   │   ├── c.rs                 # C language parser
│   │   ├── cpp.rs               # C++ parser
│   │   ├── detect.rs            # Language detection
│   │   ├── java.rs              # Java parser
│   │   ├── javascript.rs       # JavaScript parser
│   │   ├── pool.rs              # Parser pooling
│   │   ├── python.rs            # Python parser
│   │   ├── typescript.rs        # TypeScript parser
│   │   └── mod.rs               # Ingestion entry point
│   ├── kv/                       # KV index (native-v2 only)
│   ├── output/                   # Output formatting
│   ├── watcher/                  # File system monitoring
│   ├── *_cmd.rs                  # CLI command modules
│   ├── ast_cmd.rs                # AST inspection command
│   ├── collisions_cmd.rs         # Symbol collision detection
│   ├── common.rs                 # Shared utilities
│   ├── dead_code_cmd.rs          # Dead code analysis
│   ├── error_codes.rs            # Error code definitions
│   ├── export_cmd.rs             # Data export command
│   ├── files_cmd.rs              # File operations command
│   ├── find_cmd.rs               # Symbol finding
│   ├── get_cmd.rs                # Data retrieval
│   ├── indexer.rs               # Indexing pipeline
│   ├── lib.rs                    # Library entry point
│   ├── migrate_backend_cmd.rs    # Backend migration
│   ├── migrate_cmd.rs            # Schema migration
│   ├── path_enumeration_cmd.rs   # Path enumeration
│   ├── query_cmd.rs              # Graph queries
│   ├── reachable_cmd.rs          # Reachability analysis
│   ├── references.rs             # Reference handling
│   ├── refs_cmd.rs               # Reference queries
│   ├── slice_cmd.rs              # Program slicing
│   ├── validation.rs             # Input validation
│   ├── verify_cmd.rs             # Verification command
│   ├── verify.rs                 # Verification logic
│   ├── watch_cmd.rs              # Watch command
│   └── main.rs                   # CLI entry point
├── tests/                        # Integration tests
├── benches/                      # Performance benchmarks
├── scripts/                      # Utility scripts
├── docs/                        # Documentation
├── .planning/                   # Planning documents
├── target/                       # Build artifacts
├── Cargo.toml                   # Project configuration
├── Cargo.lock                   # Dependency versions
└── README.md                    # Project documentation
```

## Directory Purposes

**`src/` - Main Source Code:**
- Purpose: All application logic
- Contains: CLI commands, parsers, graph operations, utilities
- Key files: `main.rs` (entry), `lib.rs` (API)

**`src/graph/` - Graph Database Core:**
- Purpose: Graph persistence and algorithms
- Contains: Symbol/Reference/Call operations, CFG extraction, graph algorithms
- Key files: `mod.rs` (main interface), `symbols.rs`, `references.rs`, `algorithms.rs`

**`src/ingest/` - Language Parsing:**
- Purpose: Multi-language source code analysis
- Contains: Language-specific parsers, symbol extraction, FQN computation
- Key files: `mod.rs` (common logic), language-specific modules

**`src/watcher/` - File System Monitoring:**
- Purpose: Real-time file change detection
- Contains: Event handling, debouncing, pub/sub support
- Key files: `mod.rs` (main watcher), `pubsub_receiver.rs` (native-v2)

**`src/generation/` - Code Storage:**
- Purpose: Code fragment persistence
- Contains: Chunk storage, retrieval, schema management
- Key files: `mod.rs` (ChunkStore), `schema.rs`

**`src/kv/` - Key-Value Indexing:**
- Purpose: Fast symbol lookups (native-v2 only)
- Contains: KV store interfaces, encoding, key patterns
- Key files: `mod.rs`, `encoding.rs`, `keys.rs`

**`src/output/` - Output Formatting:**
- Purpose: Result presentation and export
- Contains: JSON output, rich formatting, command responses
- Key files: `mod.rs`, `rich.rs`, `command.rs`

**`src/diagnostics/` - Error Reporting:**
- Purpose: Error collection and monitoring
- Contains: Diagnostic types, collection, reporting
- Key files: `mod.rs`, `watch_diagnostics.rs`

**`tests/` - Integration Tests:**
- Purpose: End-to-end testing
- Contains: Backend migration tests, integration scenarios
- Key files: `backend_migration_tests.rs`

## Key File Locations

**Entry Points:**
- `src/main.rs`: CLI application entry point
- `src/lib.rs`: Library API entry point

**Configuration:**
- `Cargo.toml`: Project dependencies and features
- `build.rs`: Build-time configuration (if present)

**Core Logic:**
- `src/graph/mod.rs`: Main graph database interface
- `src/ingest/mod.rs`: Symbol extraction logic
- `src/indexer.rs`: Indexing pipeline coordination
- `src/watcher/mod.rs`: File watching logic

**CLI Commands:**
- `src/find_cmd.rs`: Symbol finding command
- `src/refs_cmd.rs`: Reference querying
- `src/export_cmd.rs`: Data export
- `src/watch_cmd.rs`: File watching command

**Utilities:**
- `src/common.rs`: Shared utilities
- `src/validation.rs`: Input validation
- `src/verify.rs`: Graph verification

## Naming Conventions

**Files:**
- Commands: `<action>_cmd.rs` (e.g., `find_cmd.rs`)
- Core modules: `<name>.rs` (e.g., `graph.rs`, `ingest.rs`)
- Tests: `test_name.rs` or `*_tests.rs`
- Benchmarks: `bench_name.rs`

**Functions:**
- Public API: `snake_case` (e.g., `index_file`, `symbols_in_file`)
- Private helpers: `snake_case` (e.g., `walk_tree_with_scope`)
- Types: `PascalCase` (e.g., `SymbolFact`, `CodeGraph`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `STALE_THRESHOLD_SECS`)

**Directories:**
- Lowercase with underscores (e.g., `ingest`, `graph`, `watcher`)

## Where to Add New Code

**New CLI Command:**
- Primary code: `src/<action>_cmd.rs`
- Tests: `tests/` (integration test)
- Documentation: Update `README.md` with new command

**New Language Support:**
- Parser: `src/ingest/<lang>.rs` (e.g., `go.rs`)
- Grammar: Add to `Cargo.toml` dependencies
- Tests: In parser module or dedicated test file

**New Graph Algorithm:**
- Implementation: `src/graph/algorithms.rs` or new module
- Tests: `src/graph/tests.rs`
- Export support: `src/graph/export/` if needed

**New Storage Feature:**
- Core logic: `src/generation/` or `src/kv/` (for native-v2)
- Schema: Corresponding `schema.rs` file
- Migration: `src/migrate_cmd.rs` and related

**New Output Format:**
- Implementation: `src/output/` (e.g., `json.rs`, `csv.rs`)
- Command integration: `src/export_cmd.rs`

## Special Directories

**`src/.codemcp/`:**
- Purpose: CodeMCP backup files
- Generated: Yes (by CodeMCP tools)
- Committed: No (should be in .gitignore)

**`target/`:**
- Purpose: Build artifacts
- Generated: Yes
- Committed: No

**`tests/`:**
- Purpose: Integration tests
- Generated: No
- Committed: Yes

**`benches/`:**
- Purpose: Performance benchmarks
- Generated: No (results are)
- Committed: Yes (benchmark code)

---

*Structure analysis: 2026-02-08*
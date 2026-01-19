# Codebase Structure

**Analysis Date:** 2026-01-19

## Directory Layout

```
magellan/
├── src/                     # Main library source
│   ├── main.rs             # CLI entry point with command parsing
│   ├── lib.rs              # Public API exports
│   ├── graph/              # Graph persistence layer
│   │   ├── mod.rs          # CodeGraph facade
│   │   ├── schema.rs       # Node/edge type definitions
│   │   ├── ops.rs          # Core operations (index, delete, reconcile)
│   │   ├── query.rs        # Query operations
│   │   ├── scan.rs         # Directory scanning
│   │   ├── filter.rs       # File filtering rules
│   │   ├── validation.rs   # Pre/post-run validation
│   │   ├── export/         # Export functionality
│   │   │   ├── mod.rs      # JSON/JSONL/CSV/DOT export
│   │   │   └── scip.rs     # SCIP binary export
│   │   ├── files.rs        # File node operations
│   │   ├── symbols.rs      # Symbol node operations
│   │   ├── references.rs   # Reference node operations
│   │   ├── calls.rs        # Call graph operations
│   │   ├── call_ops.rs     # Call node CRUD
│   │   ├── count.rs        # Entity counting
│   │   ├── freshness.rs    # Freshness checking
│   │   ├── db_compat.rs     # Schema versioning
│   │   └── execution_log.rs # Execution tracking
│   ├── ingest/             # Language parsers
│   │   ├── mod.rs          # Parser trait, SymbolFact types
│   │   ├── detect.rs       # Language detection
│   │   ├── rust.rs         # (in mod.rs)
│   │   ├── python.rs       # Python parser
│   │   ├── c.rs            # C parser
│   │   ├── cpp.rs          # C++ parser
│   │   ├── java.rs         # Java parser
│   │   ├── javascript.rs   # JavaScript parser
│   │   └── typescript.rs   # TypeScript parser
│   ├── indexer.rs          # Watch pipeline coordination
│   ├── watcher.rs          # Filesystem watcher with debouncing
│   ├── generation/         # Code chunk storage
│   │   ├── mod.rs          # ChunkStore operations
│   │   └── schema.rs       # CodeChunk type
│   ├── output/             # CLI output structures
│   │   ├── mod.rs          # Response type exports
│   │   └── command.rs      # Span, SymbolMatch, ReferenceMatch
│   ├── diagnostics/        # Error/warning tracking
│   │   ├── mod.rs          # Diagnostic types
│   │   └── watch_diagnostics.rs
│   ├── verify.rs           # DB vs filesystem verification
│   ├── references.rs       # ReferenceFact, CallFact types
│   ├── *_cmd.rs            # CLI command handlers
│   │   ├── watch_cmd.rs
│   │   ├── export_cmd.rs
│   │   ├── query_cmd.rs
│   │   ├── find_cmd.rs
│   │   ├── refs_cmd.rs
│   │   ├── get_cmd.rs
│   │   ├── files_cmd.rs
│   │   └── verify_cmd.rs
├── tests/                  # Integration tests
│   ├── export_tests.rs
│   ├── freshness_tests.rs
│   ├── graph_persist.rs
│   ├── indexer_tests.rs
│   ├── watcher_tests.rs
│   ├── cli_*_tests.rs
│   └── ...                 # (25+ test files)
├── Cargo.toml              # Package manifest
├── README.md               # User documentation
└── docs/                   # Additional documentation
```

## Directory Purposes

**`src/`**: Main library source code
- Purpose: All application logic organized by layer
- Contains: graph operations, parsers, CLI commands, utilities
- Key files: `src/main.rs` (CLI entry), `src/lib.rs` (public exports)

**`src/graph/`**: Graph persistence layer
- Purpose: Code graph storage using sqlitegraph backend
- Contains: Node/edge operations, queries, export, validation
- Key files: `src/graph/mod.rs` (CodeGraph), `src/graph/schema.rs` (types)

**`src/ingest/`**: Language-specific parsers
- Purpose: Extract symbols from source code using tree-sitter
- Contains: Parser implementations for 7 languages
- Key files: `src/ingest/mod.rs` (Parser trait, types), `src/ingest/detect.rs`

**`src/graph/export/`**: Data export functionality
- Purpose: Serialize graph to external formats
- Contains: JSON, JSONL, CSV, DOT, SCIP exporters
- Key files: `src/graph/export/mod.rs`, `src/graph/export/scip.rs`

**`tests/`**: Integration tests
- Purpose: End-to-end testing of core functionality
- Contains: 25+ test files covering all major features
- Key files: `tests/graph_persist.rs`, `tests/watcher_tests.rs`, `tests/cli_export_tests.rs`

**`docs/`**: Documentation
- Purpose: Architecture, plans, research notes
- Contains: IMPLEMENTATION_PLAN_V2.md, SQLITEGRAPH_ARCHITECTURE.md

**`.planning/`**: Project planning
- Purpose: Roadmap, phase plans, research summaries
- Contains: phases/, research/ directories with markdown plans

## Key File Locations

**Entry Points:**
- `src/main.rs`: CLI entry point, command parsing and dispatch

**Configuration:**
- `Cargo.toml`: Dependencies, version, feature flags

**Core Logic:**
- `src/graph/mod.rs`: CodeGraph facade (open, index_file, scan_directory, query methods)
- `src/graph/ops.rs`: Core graph operations (index, delete, reconcile)
- `src/graph/query.rs`: Symbol/reference queries
- `src/graph/schema.rs`: FileNode, SymbolNode, ReferenceNode, CallNode definitions

**Testing:**
- `tests/graph_persist.rs`: Graph persistence tests
- `tests/watcher_tests.rs`: Watch mode tests
- `tests/cli_export_tests.rs`: Export command tests

## Naming Conventions

**Files:**
- `mod.rs`: Module exports in each directory
- `{module}_cmd.rs`: CLI command handlers (e.g., `watch_cmd.rs`)
- `{type}.rs`: Single-type modules (e.g., `schema.rs`, `filter.rs`)

**Directories:**
- `src/`: Library source (lowercase, underscore-separated)
- `tests/`: Integration tests
- `docs/`: Documentation

## Where to Add New Code

**New Feature:**
- Primary code: `src/graph/` for graph operations, `src/` for top-level features
- Tests: `tests/{feature}_tests.rs`

**New CLI Command:**
- Implementation: `src/{command}_cmd.rs`
- Registration: Add command enum variant in `src/main.rs`
- Add handler in `src/main.rs::main()` match arm

**New Export Format:**
- Implementation: `src/graph/export/{format}.rs`
- Export enum: Add variant to `ExportFormat` in `src/graph/export/mod.rs`
- Handler: Add branch in `export_graph()`

**New Language Parser:**
- Implementation: `src/ingest/{language}.rs`
- Add module to `src/ingest/mod.rs`
- Add Language variant to `src/ingest/detect.rs`
- Add branch in `src/graph/ops.rs::index_file()`

**Utilities:**
- Shared helpers: `src/{module}.rs` for domain-specific, `src/graph/{module}.rs` for graph-related

## Special Directories

**`target/`**: Generated by Cargo (build artifacts)
- Purpose: Compiled binaries, dependencies
- Generated: Yes
- Committed: No

**`.planning/`**: Project planning and roadmaps
- Purpose: Phase plans, research summaries
- Generated: No
- Committed: Yes

---
*Structure analysis: 2026-01-19*

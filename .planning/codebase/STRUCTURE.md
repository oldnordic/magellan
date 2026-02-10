# Codebase Structure

**Analysis Date:** 2026-02-10

## Directory Layout

```
magellan/
├── src/                    # Source code
│   ├── main.rs            # CLI entry point
│   ├── lib.rs             # Public API exports
│   ├── common.rs          # Language detection, path utilities
│   ├── indexer.rs         # Indexing orchestrator
│   ├── ingest/            # Language parsers
│   ├── graph/             # Database operations
│   ├── generation/        # Code chunk storage
│   ├── watcher/           # Filesystem watcher
│   ├── diagnostics/      # Watch event tracking
│   ├── output/            # Output formatting
│   ├── *cmd.rs            # CLI command implementations
│   └── kv/                # KV store (native-v2 only)
├── tests/                 # Integration tests
├── docs/                  # Documentation
├── .planning/            # Planning documents
├── Cargo.toml            # Project configuration
└── README.md             # Project overview
```

## Directory Purposes

### `src/` - Main Source Code
**Purpose:** All Rust source code for Magellan
- **Contains:** Core functionality, CLI commands, database operations
- **Key files:** `main.rs` (entry), `lib.rs` (API)
- **Structure:** Module-based organization by functionality

### `src/ingest/` - Language Parsers
**Purpose:** Multi-language source code parsing and symbol extraction
- **Contains:** Language-specific parsers, symbol detection, FQN computation
- **Key files:** `mod.rs` (types), `detect.rs` (language detection)
- **Language modules:** `rust.rs`, `c.rs`, `cpp.rs`, `java.rs`, `python.rs`, `javascript.rs`, `typescript.rs`

### `src/graph/` - Database Layer
**Purpose:** SQLiteGraph integration and graph operations
- **Contains:** CodeGraph wrapper, schema definitions, CRUD operations
- **Key files:** `mod.rs` (main type), `schema.rs` (node types), `ops.rs` (operations)
- **Specialized modules:** `query.rs`, `symbols.rs`, `references.rs`, `calls.rs`, `ast_extractor.rs`

### `src/generation/` - Code Storage
**Purpose:** Source code chunk storage and retrieval
- **Contains:** Code deduplication, chunk management
- **Key files:** `mod.rs` (ChunkStore), schema definitions
- **Pattern:** Content-addressable storage with SHA-256 hashing

### `src/watcher/` - Filesystem Monitoring
**Purpose:** Filesystem watching with deterministic event handling
- **Contains:** Event debouncing, batch processing
- **Key files:** `mod.rs` (FileSystemWatcher), `pubsub_receiver.rs` (native-v2)
- **Features:** Gitignore awareness, debounced batching, pub/sub support

### `src/*_cmd.rs` - CLI Commands
**Purpose:** Individual command implementations
- **Contains:** Argument parsing, command logic, output formatting
- **Key files:** `watch_cmd.rs`, `query_cmd.rs`, `find_cmd.rs`, `refs_cmd.rs`, etc.
- **Pattern:** Each command has its own module with run_* function

### `src/kv/` - KV Store (Native V2)
**Purpose:** Key-value storage for native-v2 backend
- **Contains:** In-memory symbol lookups, AST/CFG storage
- **Key files:** `mod.rs` (KV backend implementation)
- **Feature:** Only available with `native-v2` feature flag

## Key File Locations

### Entry Points
- `src/main.rs`: CLI entry point with command dispatch
- `src/lib.rs`: Public API exports and re-exports
- `src/indexer.rs`: Main indexing pipeline orchestrator

### Configuration
- `Cargo.toml`: Dependencies, features, build configuration
- `src/*_cmd.rs`: Command-specific argument parsing

### Core Logic
- `src/graph/mod.rs`: CodeGraph main type and database operations
- `src/ingest/mod.rs`: Symbol types and language detection
- `src/watcher/mod.rs`: Filesystem watcher with debouncing

### Testing
- `tests/`: Integration tests and backend migration tests
- Key files: `backend_integration_tests.rs`, `backend_migration_tests.rs`

## Naming Conventions

### Files
- **Modules:** `snake_case` (e.g., `file_system.rs`)
- **Commands:** `snake_case` with `_cmd` suffix (e.g., `watch_cmd.rs`)
- **Tests:** `snake_case` with `_test.rs` or `_tests.rs` suffix

### Functions
- **Public:** `snake_case` (e.g., `index_file`, `scan_directory`)
- **Private:** `snake_case` with leading underscore (e.g., `_build_fqn`)
- **Commands:** `run_*` pattern (e.g., `run_watch`, `run_query`)

### Types
- **Structs:** `PascalCase` (e.g., `CodeGraph`, `SymbolFact`)
- **Enums:** `PascalCase` (e.g., `SymbolKind`, `Language`)
- **Traits:** `PascalCase` (e.g., `GraphBackend`)

### Variables
- **Local:** `snake_case` (e.g., `file_path`, `symbol_count`)
- **Mutable:** `mut` prefix when needed (Rust convention)
- **Constants:** `SCREAMING_SNAKE_CASE` (e.g., `STALE_THRESHOLD_SECS`)

## Where to Add New Code

### New Feature
- **Primary code:** Create new module in `src/` (e.g., `src/analysis.rs`)
- **CLI command:** Add `src/feature_cmd.rs` and entry in `src/main.rs`
- **Tests:** Add in `tests/` with integration tests

### New Language Support
- **Parser:** Add file in `src/ingest/` (e.g., `src/ingest/go.rs`)
- **Language detection:** Update `src/ingest/detect.rs`
- **Grammar:** Add tree-sitter grammar in separate crate

### New Database Schema
- **Schema:** Update in `src/graph/schema.rs`
- **Operations:** Add in appropriate module (e.g., `src/graph/analysis_ops.rs`)
- **Migration:** Update `src/graph/db_compat.rs`

### New CLI Command
- **Implementation:** Create `src/command_cmd.rs`
- **Entry:** Add match arm in `src/main.rs`
- **Tests:** Add integration test in `tests/`

## Special Directories

### `.planning/`
- **Purpose:** Project planning documents and research
- **Generated by:** GSD planning commands
- **Content:** Architecture docs, phase plans, research findings

### `docs/`
- **Purpose:** Comprehensive documentation
- **Content:** Architecture guides, API docs, migration guides, research
- **Generated docs:** Schema reference, troubleshooting, performance guides

### `tests/`
- **Purpose:** Integration and backend testing
- **Content:** Backend migration tests, integration tests
- **Special:** Tests database migrations and backend compatibility

---

*Structure analysis: 2026-02-10*
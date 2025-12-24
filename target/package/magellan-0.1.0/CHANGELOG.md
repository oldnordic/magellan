# Changelog

All notable changes to Magellan will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Graceful Shutdown** - Signal handling for SIGINT/SIGTERM
  - Magellan now responds cleanly to termination signals
  - Prints "SHUTDOWN" message before exiting
  - Uses `signal-hook` crate for cross-platform signal handling
  - Changed event loop from blocking to polling with 100ms sleep interval

- **Error Reporting** - Deterministic error handling
  - File read errors now log "ERROR {path} {reason}" and continue processing
  - Index errors log "ERROR {path} {reason}" and continue
  - Process no longer crashes on permission errors or malformed files
  - No retries, no backoff, no panics - deterministic behavior

- **Status Snapshot** - `--status` flag for database inspection
  - `magellan watch --status` prints counts and exits immediately
  - Shows total files, symbols, and references in database
  - Useful for monitoring indexing progress
  - New CodeGraph API methods: `count_files()`, `count_symbols()`, `count_references()`

### Changed
- **Event Loop Architecture** - Migrated from blocking to polling
  - Changed from `recv_event()` (blocking) to `try_recv_event()` (non-blocking)
  - Enables periodic shutdown flag checking
  - 100ms sleep interval balances responsiveness with CPU usage
  - Required for graceful signal handling

### Fixed
- **Database File Indexing** - Fixed bug where magellan indexed its own database
  - Added hardcoded filter to skip non-.rs files
  - Prevents indexing of `.db` and `.db-journal` files
  - Eliminates infinite indexing loops

### Technical
- **Dependencies**
  - Added `signal-hook = "0.3"` for Unix signal handling

- **Test Coverage**
  - `tests/signal_tests.rs` (81 LOC) - Verifies graceful shutdown
  - `tests/error_tests.rs` (86 LOC) - Verifies error handling
  - `tests/status_tests.rs` (58 LOC) - Verifies status reporting
  - All 37 tests passing across 12 test suites

- **File Size Limits**
  - `src/main.rs`: 236 LOC (within 300 LOC limit)
  - All test files under 250 LOC limit
  - Modular graph structure maintained

## [0.1.0] - 2025-12-24

### Added
- **Core Magellan Binary** - Runnable codebase mapping tool
  - `magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>]`
  - Watches directories for .rs file changes using notify crate
  - Extracts AST-level facts: functions, structs, enums, traits, modules, impls
  - Extracts symbol references: function calls and type references
  - Persists facts to sqlitegraph database
  - Real-time indexing on file Create/Modify/Delete events

- **Tree-sitter Parsing** - Rust source code analysis
  - Uses tree-sitter-rust grammar (v0.21)
  - Pure function extraction: `extract_symbols(path, source) → Vec<SymbolFact>`
  - Extracts name, kind, and byte spans for all symbols
  - Handles syntax errors gracefully without crashing

- **Reference Extraction** - Symbol cross-referencing
  - Extracts all identifier and scoped_identifier references
  - Matches references by name (acceptable collision rate)
  - Excludes references within symbol's own defining span
  - Creates REFERENCE nodes with REFERENCES edges in graph

- **Graph Persistence** - Sqlitegraph integration
  - File nodes with SHA-256 content hashing
  - Symbol nodes with byte spans
  - Reference nodes with source locations
  - Idempotent re-indexing with automatic cleanup
  - In-memory file index for fast lookups

- **Modular Architecture**
  - `src/graph/mod.rs` (249 LOC) - Public CodeGraph API
  - `src/graph/schema.rs` (29 LOC) - Node labels and edge types
  - `src/graph/files.rs` (161 LOC) - File operations
  - `src/graph/symbols.rs` (107 LOC) - Symbol operations
  - `src/graph/references.rs` (138 LOC) - Reference operations
  - All modules under 300 LOC limit

### Performance
- SHA-256 hashing for content change detection
- In-memory HashMap for O(1) file lookups
- Efficient tree-sitter parsing with AST queries
- Debounced file events (500ms default)

### Limitations
- No initial full scan (requires filesystem events to trigger indexing)
- Only processes .rs files (hardcoded filter)
- No async runtimes or background thread pools
- No config files
- No semantic analysis (AST-level only)
- No LSP features or language integrations

### Testing
- 37 tests across 12 test suites
- Integration tests for all core functionality
- Process spawning tests for binary validation
- Error injection tests for robustness verification
- Full test suite runs in <5 seconds

---

## Feature Freeze Notice

**Magellan is feature frozen as of Phase 6.2.**

Magellan is a **dumb, deterministic codebase mapping tool**. It does NOT:
- Perform semantic analysis
- Build LSP servers or language features
- Use async runtimes or background thread pools
- Use config files
- Perform initial full scans
- Handle non-.rs files
- Provide web APIs or network services

Magellan DOES:
- Watch directories for .rs file changes
- Extract AST-level facts (functions, structs, enums, traits, modules, impls)
- Extract symbol references (calls and type references)
- Persist facts to sqlitegraph database
- Index files on Create/Modify events
- Delete files on Delete events
- Handle permission errors gracefully
- Respond to SIGINT/SIGTERM for clean shutdown
- Report status via --status flag

No new features are planned beyond Phase 6.2.

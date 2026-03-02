# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.1] - 2026-03-02

### Added
- **C/C++ CFG support:** CFG extraction now works for C/C++ files
  - Supports .c, .h, .cpp, .hpp, .cc, .cxx file extensions
  - Uses function_definition node kind for C/C++ (vs function_item for Rust)
  - Iterative tree walk avoids Rust lifetime issues

### Fixed
- Mirage CFG analysis now works with C/C++ codebases
- cfg_blocks table now populated for C/C++ functions

## [3.0.0] - 2026-03-02

### Major Release: Async Indexing, Cross-Repo LSIF, and LLM Context API

This release adds to Magellan a full-featured code intelligence platform with async I/O, cross-repository navigation, and LLM-optimized context queries.

Magellan v3.0.0 is part of the Code Intelligence ecosystem, working alongside:
- **LLMGrep** — Semantic code search
- **Mirage** — CFG-based path analysis
- **Splice** — Safe refactoring with span safety


### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`

#### Async Watcher (Phase 1)
- **Async file I/O** using tokio runtime
  - Non-blocking file reads for improved performance
  - Parallel file processing with configurable concurrency
  - Backpressure handling via bounded channels (100 batch limit)
- **New modules:**
  - `src/watcher/async_watcher.rs` - Async file watcher
  - `src/indexer/async_io.rs` - Async file read utilities
- **New method:** `CodeGraph::scan_directory_async()`

#### Cross-Repository Navigation (Phase 2)
- **LSIF export/import** for cross-repo symbol resolution
  - `magellan export --format lsif --output file.lsif`
  - `magellan import-lsif --db code.db --input package.lsif`
- **LSIF 0.6.0 schema** support
  - Package, Document, Symbol, Range vertices
  - Contains, Item, TextDocument, Moniker edges
- **Auto package detection** from Cargo.toml
- **New module:** `src/lsif/` (export, import, schema)

#### LSP CLI Enrichment (Phase 3)
- **LSP tool integration** via CLI (like Splice)
  - rust-analyzer, jdtls, clangd support
  - Automatic tool detection in PATH
- **Enrich command:** `magellan enrich --db code.db`
  - Extracts type signatures from LSP tools
  - Stores enriched data for LLM context
- **New module:** `src/lsp/` (analyzer, enrich)

#### LLM Context Query Interface (Phase 3)
- **Summarized, paginated context** for LLMs
  - Multi-level summaries (Project, File, Symbol)
  - Token-efficient output (~50-500 tokens per query)
- **Context commands:**
  - `magellan context build` - Build context index
  - `magellan context summary` - Project overview (~50 tokens)
  - `magellan context list` - Paginated symbol listing
  - `magellan context symbol --name X` - Symbol detail with callers/callees
  - `magellan context file --path X` - File-level context
  - `magellan context-server --port 8080` - HTTP API server
- **Pagination support** with cursor-based navigation
- **New module:** `src/context/` (build, query, server)

#### CLI Improvements
- **`--json` flag** for all commands (shorthand for `--output json`)
- **Progress bars** for scan operations (indicatif)
  - Now shows current filename: `Scanning: src/main.rs`
  - Shows percentage and ETA: `[=====> ] 23/143 (16%) ETA: 2s`
- **Configurable watcher timeout** via `MAGELLAN_WATCH_TIMEOUT_MS`
- **Better error messages** with suggestions
  - `magellan context symbol --name main` now shows similar symbols
- **Doctor command** for diagnostics
  - `magellan doctor --db code.db` - Check database health
  - `magellan doctor --db code.db --fix` - Auto-fix issues
- **Web UI server** (experimental, `--features web-ui`, limited by CodeGraph Send+Sync)
  - `magellan web-ui --db code.db --port 8080`
  - Built with axum (like codemcp)

#### CI/CD Infrastructure
- **GitHub Actions workflows**
  - `.github/workflows/ci.yml` - Tests on Linux/macOS/Windows
  - `.github/workflows/release.yml` - Auto-release on tag push
- **Automated testing**
  - Clippy with `-D warnings`
  - Formatting checks
  - TSAN tests
  - Integration tests with llmgrep and mirage
- **Auto-release** on git tag push
  - Creates GitHub release with binaries
  - Publishes to crates.io

### Changed

- **Watcher timeout:** Default increased from 2s to 5s
- **Version bump:** 2.6.0 → 3.0.0 (breaking changes)

### Dependencies

- Added `tokio = "1"` with rt-multi-thread, fs, sync, time, macros
- Added `tokio-stream = "0.1"`
- Added `async-channel = "2"`
- Added `which = "6"` for LSP tool detection
- Added `indicatif = "0.17"` for progress bars
- Added `axum = "0.7"` (optional, web-ui feature)
- Added `tower = "0.4"` (optional, web-ui feature)
- Added `tower-http = "0.5"` (optional, web-ui feature)

### Breaking Changes

- Async runtime required (tokio)
- New context index file (`.context.json`) alongside database
- CLI: New `context` subcommand namespace
- New `doctor`, `enrich`, `web-ui` commands

### Fixed

- Schema alignment verified across magellan, llmgrep, and mirage
- No table or column drift detected
- All tools build and work with updated schema

## [Unreleased]

## [3.0.1] - 2026-03-02

### Added
- **C/C++ CFG support:** CFG extraction now works for C/C++ files
  - Supports .c, .h, .cpp, .hpp, .cc, .cxx file extensions
  - Uses function_definition node kind for C/C++ (vs function_item for Rust)
  - Iterative tree walk avoids Rust lifetime issues

### Fixed
- Mirage CFG analysis now works with C/C++ codebases
- cfg_blocks table now populated for C/C++ functions

## [2.6.0] - 2026-03-01

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- **Progress bar for scan operations:** Visual feedback during initial scan
  - Uses `indicatif` crate for terminal progress bars
  - Shows elapsed time, progress bar, file count, and ETA
  - Template: `{spinner} [{elapsed}] [{bar}] {pos}/{len} files ({eta})`

- **`--json` flag for all commands:** Shorthand for `--output json`
  - `magellan status --json` - JSON output for database status
  - `magellan query --json` - JSON output for file symbols
  - `magellan find --json` - JSON output for symbol search
  - `magellan refs --json` - JSON output for references
  - `magellan dead-code --json` - JSON output for dead code analysis
  - `magellan cycles --json` - JSON output for cycle detection
  - Simplifies tooling integration (no need to remember `--output json`)

### Changed
- **Watcher timeout increased:** Default timeout increased from 2s to 5s
  - Prevents premature exit during slow file operations
  - Fixes timeouts on slow filesystems (network drives, WSL2)
  - Configurable via `MAGELLAN_WATCH_TIMEOUT_MS` environment variable
  - Example: `MAGELLAN_WATCH_TIMEOUT_MS=10000 magellan watch ...`

### Dependencies
- Added `indicatif = "0.17"` for progress bar support

## [2.5.1] - 2026-03-01

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- **C language support:** Complete symbol extraction for C source files
  - Functions, structs, enums, and unions
  - Reference and call graph extraction
  - FQN support with canonical and display names
  - Parser pooling for performance

- **Java language support:** Complete symbol extraction for Java source files
  - Classes, interfaces, enums, methods, and packages
  - Reference and call graph extraction
  - Package-aware FQN with proper scope handling
  - Parser pooling for performance

- **llmgrep compatibility:** Added C and Java AST node kind mappings
  - `C_NODE_KINDS` for tree-sitter-c node types
  - `JAVA_NODE_KINDS` for tree-sitter-java node types
  - Updated `get_supported_languages()` to include "c" and "java"
  - Updated `get_node_kinds_for_language()` for C and Java categories

### Fixed
- Schema alignment verified across magellan, llmgrep, and mirage
- No table or field drift between tools
- All three tools work correctly with SQLite backend

## [2.5.0] - 2026-02-27

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- **Find command:** Implemented caller/callee references in JSON output
  - `--with-callers` flag now includes functions that call the found symbol
  - `--with-callees` flag now includes functions that the found symbol calls
  - Results include caller/callee names, file paths, and location (line/column)
  - Data sourced from existing CALLS edges in the graph

- **SideTables trait:** Added batch insert and parent update methods for AST nodes
  - `store_ast_nodes_batch()` method for bulk AST node insertion with transaction support
  - `update_ast_node_parent()` method for resolving placeholder parent references
  - SQLite: Uses UPDATE query for efficient in-place parent updates
  - V3: Uses delete+reinsert approach (required for KV stores)
  - Proper parent-child relationships now established after batch insertion

## [2.4.9] - 2026-02-22

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- **Schema v8:** Added `cfg_hash` column to `cfg_blocks` table
  - Enables granular cache invalidation for tools like Mirage
  - Hash computed from block data (function_id, kind, terminator, spans)
  - Falls back to symbol_id for backward compatibility with v7 databases

## [2.4.8] - 2026-02-21

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- Shortened `--help` output. Research shows users ignore help >25 lines.
  - `--help` / `-h` now shows concise 16-line quick reference
  - `--help-full` / `-H` shows complete 179-line documentation
  - Short help includes most common commands with concrete examples

## [2.4.7] - 2026-02-21

### Fixed
- Fixed reference tracking dropping internal calls. `find_dead_code` was reporting
  false positives because it didn't see calls within the same file. Now tracks
  all calls with optional symbol_id when available.

- Fixed metrics fan-in/fan-out always showing zero. SQL had wrong column names
  (target_id vs to_id, source_id vs from_id). Now counts edges correctly.

- Implemented cyclomatic complexity. Was hardcoded to 1, now computes from CFG
  blocks (branches + 1).

- Refactored parse_args_impl. Was almost 2000 lines, split into 25 separate
  parser functions. Added some helper functions for common arg parsing patterns.
  Complexity dropped from ~130 to ~32. Added a bunch of tests too.

- Added Debug trait to Command enum so tests are easier to write.

- Cleaned up some compiler warnings (unused imports) with cargo fix.

- Documented the unsafe code in algorithms.rs properly with SAFETY comments.

## [2.4.6] - 2026-02-20

### Updated
- Update sqlitegraph from 2.0.5 to 2.0.7 (bug fixes)

## [2.4.5] - 2026-02-16

### Fixed
- V3 backend wasn't persisting nodes properly. Updated to sqlitegraph 2.0.5
  which fixes this. V3 databases now save and reload correctly.

## [2.4.4] - 2026-02-14

### Fixed
- Changed Rc to Arc for thread safety in V3 backend. Fixed double-open issue.

## [2.4.3] - 2026-02-14

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- Symbol lookup by entity ID: `CodeGraph::get_symbol_by_entity_id()`

## [2.4.2] - 2026-02-14

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- V3 KV operations for llmgrep integration (prefix scan, FQN lookup, etc)

## [2.4.1] - 2026-02-14

### Changed
- Rewrote README.md to be more concise

## [2.4.0] - 2026-02-14

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- V3 backend support (native binary format, no SQLite)
- Full backend parity - both SQLite and V3 support all features

### Changed
- Changed backend from Rc to Arc for thread safety

## [2.3.0] - 2026-02-13

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- Graph algorithms: reachability, dead code, cycles, paths, slicing
- Side tables for metrics and execution logging

## [2.2.0] - 2026-02-12

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- AST node extraction and storage
- CFG (control flow graph) support
- Code chunk storage

## [2.1.0] - 2026-02-11

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- Multi-language support: Rust, Python, C, C++, Java, JavaScript, TypeScript
- Call graph extraction (caller -> callee)

## [2.0.0] - 2026-02-10

### Changed
- Complete rewrite on top of sqlitegraph
- New backend architecture

## [1.0.0] - 2026-01-15

### Added

#### Context API Documentation
- **Deterministic JSON contract** for LLM context queries
  - `docs/CONTEXT_API_CONTRACT.md` with full schema specification
  - Project summary, symbol list, symbol detail, file context
  - Pagination with cursor-based navigation
  - Error response format with error codes
  - Version history and backward compatibility

#### Benchmarks
- **Context query latency benchmarks** (`benches/context_bench.rs`)
  - context_summary, context_list, context_symbol, context_file
  - Large codebase tests (100k+ symbols)
  - Run: `cargo bench --bench context_bench`

#### Stress Tests
- **LSIF import stress tests** (`tests/lsif_stress_tests.rs`)
  - Small (100), Medium (10k), Large (100k) symbol imports
  - Invalid file handling, empty file handling
  - All 7 tests passing
  - Run: `cargo test --test lsif_stress_tests`
- Initial release
- Basic symbol extraction for Rust
- File watching with notify

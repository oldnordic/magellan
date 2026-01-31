# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.9.0] - 2026-01-31

### Added
- **AST Node Storage**: Schema v5 adds `ast_nodes` table for hierarchical code structure
  - `ast_nodes` table: id, parent_id, kind, byte_start, byte_end
  - Indexes on `parent_id` and `(byte_start, byte_end)` for efficient queries
  - Parent-child relationships tracked via stack-based traversal
- **AST CLI Commands**: Two new commands for querying AST structure
  - `ast` - Show AST tree for a file with optional position filtering
  - `find-ast` - Find AST nodes by kind across all files
  - Both commands support JSON output via `--output json`
- **AST Extraction Integration**: Automatic AST extraction during file indexing
  - Uses tree-sitter parse trees for all supported languages
  - Extracts structural nodes (functions, blocks, control flow, etc.)
  - Stores parent-child relationships for tree reconstruction
- **Analysis Scripts**: Three new scripts for AST-based code analysis
  - `scripts/ast-query.sh` - Query AST nodes by kind, show counts and tree structure
  - `scripts/complexity.sh` - Calculate cyclomatic complexity using decision points
  - `scripts/nesting.sh` - Find deeply nested code using parent relationships
- **Database Migration**: Automatic v4→v5 migration with `ast_nodes` table creation

### Changed
- Bumped `MAGELLAN_SCHEMA_VERSION` from 4 to 5
- Modified `ensure_magellan_meta()` to auto-upgrade v4 databases on open

### Technical
- All 450+ lib tests pass (20 new AST tests in `src/graph/ast_tests.rs`)
- 4 new migration tests for v4→v5 upgrade path
- JSON output includes `parent_id` for tree reconstruction
- Common AST node kinds documented in MANUAL.md (function_item, if_expression, etc.)

## [1.8.0] - 2026-01-31

### Added
- **Safe UTF-8 content extraction**: New public API functions for converting tree-sitter byte offsets to UTF-8 text without panicking on multi-byte character boundaries
  - `extract_symbol_content_safe()` - Extract symbol source code from byte offsets with UTF-8 safety
  - `extract_context_safe()` - Extract symbol with before/after context lines
  - `safe_str_slice()` - Safe UTF-8 string slicing with bounds checking (exported from lib.rs)
  - Handles ASCII (1 byte), accented Latin (2 bytes), CJK (3 bytes), emoji (4 bytes)
- **Metrics computation during indexing**: File and symbol metrics automatically computed during file indexing
  - `file_metrics` table: fan_in, fan_out, symbol_count, LOC, complexity_score per file
  - `symbol_metrics` table: fan_in, fan_out, LOC, cyclomatic complexity per symbol
  - Metrics computed in `src/graph/metrics.rs` and wired into `index_file()` Step 7
- **Chunk storage commands**: Three new commands for querying stored code chunks
  - `chunks` - List all chunks with optional filters (limit, file pattern, symbol kind)
  - `chunk-by-span` - Get chunk by exact byte range (file, start, end)
  - `chunk-by-symbol` - Get all chunks for a symbol name (global search, optional file filter)
- **Script enhancements**: `magellan-workflow.sh` updated with new commands
  - `hotspots` - Show top N files by complexity score with tabular output
  - `chunks` - Wrapper for chunk listing
  - `chunk-by-symbol` - Get chunks by symbol name
  - `chunk-by-span` - Get chunk by byte range
  - `file-metrics` - Show metrics for a specific file
  - `backfill` - Trigger metrics backfill helper

### Changed
- `src/output/rich.rs` - Added `extract_from_bytes()` helper for safe checksum computation on byte slices
- `src/lib.rs` - Exported `extract_context_safe`, `extract_symbol_content_safe`, `safe_str_slice` as public API

### Technical
- All chunk commands support JSON output via `--output json` or `--output pretty`
- Chunks use SHA-256 content hashing for deduplication detection
- Metrics use BLAKE3 SymbolId for stable symbol identification
- UTF-8 extraction uses `is_char_boundary()` for valid UTF-8 boundary detection
- Integration tests: 14 tests in `tests/safe_extraction_tests.rs` with multi-byte UTF-8 fixtures

## [1.7.0] - 2026-01-24

### Fixed
- Thread safety: Migrated `RefCell<T>` to `Arc<Mutex<T>>` for all concurrent state
  - `FileSystemWatcher::legacy_pending_batch` now uses `Arc<Mutex<Option<WatcherBatch>>>`
  - `FileSystemWatcher::legacy_pending_index` now uses `Arc<Mutex<usize>>`
  - `PipelineSharedState::dirty_paths` now uses `Arc<Mutex<BTreeSet<PathBuf>>>`
- Lock ordering enforced to prevent deadlocks:
  1. Acquire `dirty_paths` lock first
  2. Send wakeup signal while holding lock
  3. Release lock
- Error propagation in watcher shutdown with timeout-based termination
- Added 29 verification tests across: bounds checking, call graphs, CLI export, FQN integration, orphan deletion, delete transactions, ignore rules, path validation, rich spans, symlink handling, watch buffering, and ambiguity resolution

### Changed
- RefCell removed from threading model - documented in MANUAL.md
- TSAN test suite created for thread safety verification

## [1.6.0] - Skipped

Milestone skipped. CSV export fixes deferred to future release.

## [1.5.0] - 2026-01-23

### Added
- **BLAKE3 SymbolId**: Stable 32-character hash identifiers (128 bits) for unambiguous symbol reference
  - `--symbol-id <ID>` flag for find/refs commands to use stable IDs
  - `--ambiguous <NAME>` flag to show all candidates for ambiguous names
  - `symbol_id` field added to all symbol exports
- **Canonical FQN**: Unambiguous symbol identity with file path
  - `canonical_fqn`: Full FQN with file path (e.g., `crate::src/lib.rs::Function name`)
  - `display_fqn`: Human-readable FQN without file path (e.g., `crate::module::name`)
- **collisions command**: List ambiguous symbols that share the same display FQN
  - `--field <FIELD>`: fqn, display_fqn, or canonical_fqn
  - `--limit <N>`: Maximum groups to show
- **migrate command**: Upgrade database schema with automatic backup
  - `--dry-run`: Check version without migrating
  - `--no-backup`: Skip backup creation
- **Export format versions**: Added schema versioning to all export formats
  - JSON: Top-level `version` field
  - JSONL: Version record as first line
  - CSV: Header comment with version
  - SCIP: Metadata includes version
  - DOT: Graphviz format (no version field)
- **CSV export**: New export format for spreadsheet compatibility
- **DOT export**: Graphviz DOT format for visualization

### Changed
- Export schema version bumped to 2.0.0
- Database schema version bumped to 4 (BLAKE3 SymbolId)
- `--first` flag deprecated in favor of `--symbol-id`

### Fixed
- FQN collisions now detectable via `collisions` command
- Symbol identity now stable across re-indexing when position unchanged

## [1.4.0] - 2026-01-22

### Fixed
- Path normalization across all entry points (watcher, scan, indexer) - no more duplicate file entries
- Result propagation in index_references - reference counts now accurate
- Byte slice bounds checking - prevents panic on malformed symbol data
- Symbol counting scoped to current file instead of entire database
- DeleteResult verification before re-indexing
- ChunkStore thread safety - uses Arc<Mutex<Connection>> instead of Rc<RefCell>
- Parser warmup now propagates errors instead of silently ignoring
- Parser pool uses expect() with clear invariant messages
- PRAGMA connection cleanup on error via scoped block
- Watcher shutdown signal for clean termination
- Version reporting via --version/-V flags
- Output formatting flags (--output json/pretty) per-command
- Position conventions documented (1-indexed lines, 0-indexed columns)
- Fixed misleading "lazy initialization" comment
- Cleaned up unused variables and compiler warnings
- :memory: database limitation documented
- RefCell usage documentation for single-threaded design
- :memory: database error messages clarified with workarounds

### Changed
- Database version bumped from 3 to 4 (breaking change - requires re-index)
- All phases from v1.4 focused on bug fixes and correctness improvements

## [1.3.0] - 2026-01-22

### Added
- Thread-local parser pooling (7 parsers per language)
- SQLite performance PRAGMAs (WAL mode, synchronous=NORMAL, 64MB cache)
- Parser warmup function for first-parse latency avoidance
- Parallel file scanning using rayon
- LRU cache for graph query results with FileNodeCache integration
- Streaming JSON export (stream_json, stream_json_minified, stream_ndjson)
- Common module (src/common.rs) with shared utility functions

### Changed
- Code deduplication - extracted duplicated helper functions
- Improved indexing performance through parallel processing

## [1.2.0] - 2026-01-22

### Added
- Unified JSON schema output with StandardSpan-compliant positions
- JsonResponse wrapper with tool and timestamp metadata
- Error codes module with 12 MAG-{CAT}-{NNN} error codes
- Rich span extensions: SpanContext, SpanRelationships, SpanSemantics, SpanChecksums
- CLI flags for context, semantics, and checksums (--with-context, --with-semantics, --with-checksums)
- Integration tests for JSON output

### Changed
- Span struct verified as StandardSpan-compliant
- find/query/refs/get commands support --output flag with json/pretty formats

## [1.1.0] - 2026-01-20

### Added
- FQN (Fully Qualified Name) based symbol lookup to eliminate name collisions
- Path traversal validation at all entry points (watcher, scan, indexing)
- Symlink rejection for paths outside project root
- Row-count assertions for delete operation verification
- Orphan detection tests to verify clean graph state after deletes
- SCIP export format with round-trip test coverage
- Security documentation in README and MANUAL

### Changed
- Symbol map keys changed from simple names to FQNs (e.g., `crate::module::Struct::method`)
- Database version bumped from 2 to 3 (breaking change - requires re-index)
- Call indexing now includes fallback lookup for cross-file method calls

### Fixed
- Cross-file method call resolution regression from FQN changes
- Path traversal vulnerability class (CVE-2025-68705)

### Security
- Added path canonicalization before validation for all file access
- Suspicious pattern detection (3+ `../` patterns, mixed patterns)
- Symlinks outside project root are rejected
- Database placement guidance in documentation

## [0.5.3] - 2026-01-13

### Fixed
- Incoming `refs` now includes cross-file Rust method calls when both files are indexed.
- Call graph indexing now builds symbol facts from persisted symbols so cross-file calls are recorded.

## [0.5.0] - 2026-01-02

### Added
- **Label-based symbol queries** - Fast symbol lookup using automatic labels assigned during indexing
  - Language labels: `rust`, `python`, `javascript`, `typescript`, `c`, `cpp`, `java`
  - Symbol kind labels: `fn`, `method`, `struct`, `class`, `enum`, `interface`, `module`, `union`, `namespace`, `typealias`
  - Multi-label queries with AND semantics (e.g., `--label rust --label fn`)
  - `magellan label --db <FILE> --list` - Show all labels with entity counts
  - `magellan label --db <FILE> --label <LABEL>...` - Query symbols by label(s)
  - `magellan label --db <FILE> --count --label <LABEL>` - Count entities with label
- **`--show-code` flag** for label queries - Display actual source code for each symbol result without re-reading files
- **`magellan get` command** - Retrieve code chunks for a specific symbol using stored data
- **`magellan get-file` command** - Retrieve all code chunks from a file
- **`--help` and `-h` flags** - Global help support for all commands
- **Native-v2 backend support** - Build with `--features native-v2` for improved insert performance

### Changed
- Code chunks are now automatically stored during indexing
- Symbols are automatically labeled with language and kind during indexing
- All 97 tests pass with native-v2 backend enabled

### Technical
- Label query API in `src/graph/query.rs` with raw SQL for entity lookup
- Label integration in `src/graph/symbols.rs` - calls `sqlitegraph::add_label()` during symbol insertion
- Code chunk storage via `ChunkStore` in `src/generation/mod.rs`
- Helper methods: `get_entities_by_label()`, `get_entities_by_labels()`, `get_symbols_by_label()`, `get_all_labels()`, `count_entities_by_label()`
- Uses sqlitegraph 0.2.11 with native-v2 backend bug fix

## [0.4.0] - 2026-01-02

### Added
- **`magellan query --explain` cheat sheet** covering selector syntax, glob usage, and related commands
- **Symbol extent reporting (`--symbol` + `--show-extent`)** that prints byte and line/column ranges plus node IDs
- **Glob previews via `magellan find --list-glob <pattern>`** to generate deterministic symbol sets for batch refactors
- **Normalized kind metadata** persisted as `kind_normalized` on every symbol fact
- **Helpful CLI hints** when queries or finds return no results

### Changed
- Query and find output now include the normalized kind tag (e.g., `[fn]`, `[struct]`)

## [0.3.1] - 2025-12-31

### Fixed
- **Rust impl blocks now extract struct name** - `impl_item` nodes now store the struct name in the `name` field

### Added
- `extract_impl_name()` method to Rust parser for impl name extraction
- 3 new tests for impl name extraction

## [0.3.0] - 2025-12-30

### Added
- **Multi-language reference extraction** - Works for all 7 supported languages
- **Multi-language call graph indexing** - Works for all 7 supported languages
- Language-specific `extract_references()` and `extract_calls()` methods
- Language dispatch in reference and call indexing

### Changed
- Removed Rust-only restriction from call indexing
- Reference extraction now uses proper symbol spans for filtering

### Fixed
- Reference extraction bug where byte offsets were not stored in edge data
- Self-reference filtering bug
- Call graph indexing was only working for Rust - now works for all languages

## [0.2.3] - 2025-12-28

### Added
- `--root` option to `query`, `find`, and `refs` commands for explicit relative path resolution

## [0.2.2] - 2025-12-28

### Fixed
- CLI query commands now accept relative file paths

## [0.2.1] - 2025-12-28

### Changed
- Updated README to reflect multi-language support
- Updated MANUAL.md with current command reference

## [0.2.0] - 2025-12-28

### Added
- **Multi-language Support** - C, C++, Java, JavaScript, TypeScript, Python parsers
- **CLI Query Commands** - query, find, refs, files commands
- Language detection by file extension

### Changed
- SymbolKind enum expanded for all languages

## [0.1.1] - 2025-12-28

### Added
- `magellan status` - Database statistics command
- `magellan verify` - Database freshness checking
- `magellan export` - JSON export command
- `--scan-initial` flag
- Timestamp tracking on File nodes

### Fixed
- Duplicate File node bug on database reopen

## [0.1.0] - 2025-12-24

### Added
- Core Magellan Binary - Rust-only codebase mapping tool
- Tree-sitter parsing for Rust source code
- Reference extraction (function calls, type references)
- Graph persistence via sqlitegraph
- Graceful signal handling (SIGINT/SIGTERM)

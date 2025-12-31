# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **Rust impl blocks now extract struct name** - `impl_item` nodes now store the struct name in the `name` field
  - Previously: `impl_item` nodes stored with `name: None`, making them impossible to query
  - Now: Uses `child_by_field_name("type")` to extract the struct name being implemented
  - Works for both `impl StructName { }` and `impl Trait for StructName { }`
  - Enables codemcp to find all impl blocks when renaming a struct

### Added
- `extract_impl_name()` method to Rust parser for impl name extraction
- 3 new tests: `test_extract_impl_name_inherent`, `test_extract_impl_name_trait_impl`, `test_extract_impl_name_both`

## [0.3.0] - 2025-12-30

### Added
- **Multi-language reference extraction** - Reference extraction now works for all 7 supported languages (Rust, Python, C, C++, Java, JavaScript, TypeScript)
- **Multi-language call graph indexing** - Call graph extraction now works for all 7 supported languages
- Language-specific `extract_references()` methods for Python, C, C++, Java, JavaScript, TypeScript parsers
- Language-specific `extract_calls()` methods for Python, C, C++, Java, JavaScript, TypeScript parsers
- Language dispatch in `src/graph/references.rs` for reference extraction
- Language dispatch in `src/graph/call_ops.rs` for call extraction
- Proper span filtering to exclude self-references in all language parsers

### Changed
- Removed Rust-only restriction from call indexing in `src/graph/ops.rs`
- Reference extraction now uses proper symbol spans for filtering (was using placeholders)
- Call extraction now uses proper symbol information (was using placeholders)
- Rename refactoring (via codemcp) now works for Python, C, C++, Java, JavaScript, TypeScript

### Fixed
- Reference extraction bug where byte offsets were not stored in edge data (codemcp rename fix)
- Self-reference filtering bug where references within defining span were incorrectly counted
- Call graph indexing was only working for Rust - now works for all languages

### Technical
- Each language parser now implements `extract_references()` and `extract_calls()`
- Language dispatch pattern implemented in both reference and call indexing
- All parsers extract symbols first to get proper span information for reference filtering

## [0.2.3] - 2025-12-28

### Added
- `--root` option to `query`, `find`, and `refs` commands for explicit relative path resolution
- Users can now run: `magellan query --db mag.db --root /path/to/project --file src/lib.rs`
- Resolves relative paths against explicit root directory (NO guessing from current directory)
- New test: `test_query_with_relative_path_explicit_root` proving TDD compliance

### Changed
- `--root` is optional: if omitted, relative paths resolve from current working directory
- All CLI query commands now accept both absolute and relative file paths

### Technical
- Refactored `resolve_path()` helper function shared across query, find, and refs commands
- No warnings - clean compilation with proper path resolution

## [0.2.2] - 2025-12-28

### Fixed
- CLI query commands now accept relative file paths (previously required absolute paths)
- `magellan query --file src/lib.rs` now works from within the project directory
- `magellan find --name foo --path src/main.rs` now resolves relative paths correctly
- `magellan refs --name bar --path src/lib.rs` now resolves relative paths correctly

## [0.2.1] - 2025-12-28

### Changed
- Updated README to reflect multi-language support
- Updated README to reflect new CLI query commands
- Updated MANUAL.md with current command reference

## [0.2.0] - 2025-12-28

### Added

**Multi-language Support**
- C parser - .c, .h files (tree-sitter-c)
- C++ parser - .cpp, .cc, .cxx, .hpp files (tree-sitter-cpp)
- Java parser - .java files (tree-sitter-java)
- JavaScript parser - .js, .mjs files (tree-sitter-javascript)
- TypeScript parser - .ts, .tsx files (tree-sitter-typescript)
- Python parser - .py files (tree-sitter-python)
- Language detection by file extension
- Parser dispatcher for language-specific extraction

**CLI Query Commands**
- `magellan query --db <FILE> --file <PATH> [--kind <KIND>]` - List symbols in a file
- `magellan find --db <FILE> --name <NAME> [--path <PATH>]` - Find symbol by name
- `magellan refs --db <FILE> --name <NAME> --path <PATH> [--direction <in|out>]` - Show call references
- `magellan files --db <FILE>` - List all indexed files
- Case-insensitive symbol kind filtering
- Multi-file symbol search
- Incoming/outgoing call graph traversal

**New Modules**
- `src/ingest/c.rs` - C language parser
- `src/ingest/cpp.rs` - C++ language parser
- `src/ingest/java.rs` - Java language parser
- `src/ingest/javascript.rs` - JavaScript language parser
- `src/ingest/typescript.rs` - TypeScript language parser
- `src/ingest/python.rs` - Python language parser
- `src/ingest/detect.rs` - Language detection by file extension
- `src/query_cmd.rs` - Query command implementation
- `src/find_cmd.rs` - Find command implementation
- `src/refs_cmd.rs` - Refs command implementation

**Tests**
- `tests/cli_query_tests.rs` - 15 tests for CLI query commands
- `tests/language_parser_tests.rs` - 159 tests for multi-language parsers
- Total: 174+ tests across 27+ test suites

### Changed

- SymbolKind enum now includes: Function, Method, Class, Interface, Enum, Module, Union, Namespace, TypeAlias, Unknown
- Updated symbol kind mapping for language-agnostic representation
- Ingest module split into language-specific parsers
- All modules remain under 300 LOC limit

### Technical

- Added tree-sitter language grammars: c, cpp, java, javascript, typescript, python
- Language detection based on file extension mapping
- Unified symbol kind extraction across all languages

## [0.1.1] - 2025-12-28

### Added

- `magellan status --db <FILE>` - Database statistics command
- `magellan verify --root <DIR> --db <FILE>` - Database freshness checking
- `magellan export --db <FILE>` - JSON export command
- `--scan-initial` flag - Scan directory on startup
- Timestamp tracking on File nodes (last_indexed_at, last_modified)
- Freshness checking module

### Fixed

- Duplicate File node bug on database reopen

### Changed

- Command structure: separated subcommands instead of flags
- Database Schema: FileNode now includes timestamps

## [0.1.0] - 2025-12-24

### Added

- Core Magellan Binary - Rust-only codebase mapping tool
- Tree-sitter parsing for Rust source code
- Reference extraction (function calls, type references)
- Graph persistence via sqlitegraph
- Graceful signal handling (SIGINT/SIGTERM)
- Error reporting with continued processing

---

**Project Status:** Stable

Magellan is a deterministic codebase mapping tool. It does NOT perform semantic analysis, build LSP servers, use async runtimes, or provide web APIs.

It DOES watch directories for source file changes, extract AST-level facts, and persist to a graph database.

# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Find command:** Implemented caller/callee references in JSON output
  - `--with-callers` flag now includes functions that call the found symbol
  - `--with-callees` flag now includes functions that the found symbol calls
  - Results include caller/callee names, file paths, and location (line/column)
  - Data sourced from existing CALLS edges in the graph

## [2.4.9] - 2026-02-22

### Added
- **Schema v8:** Added `cfg_hash` column to `cfg_blocks` table
  - Enables granular cache invalidation for tools like Mirage
  - Hash computed from block data (function_id, kind, terminator, spans)
  - Falls back to symbol_id for backward compatibility with v7 databases

## [2.4.8] - 2026-02-21

### Added
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
- Symbol lookup by entity ID: `CodeGraph::get_symbol_by_entity_id()`

## [2.4.2] - 2026-02-14

### Added
- V3 KV operations for llmgrep integration (prefix scan, FQN lookup, etc)

## [2.4.1] - 2026-02-14

### Changed
- Rewrote README.md to be more concise

## [2.4.0] - 2026-02-14

### Added
- V3 backend support (native binary format, no SQLite)
- Full backend parity - both SQLite and V3 support all features

### Changed
- Changed backend from Rc to Arc for thread safety

## [2.3.0] - 2026-02-13

### Added
- Graph algorithms: reachability, dead code, cycles, paths, slicing
- Side tables for metrics and execution logging

## [2.2.0] - 2026-02-12

### Added
- AST node extraction and storage
- CFG (control flow graph) support
- Code chunk storage

## [2.1.0] - 2026-02-11

### Added
- Multi-language support: Rust, Python, C, C++, Java, JavaScript, TypeScript
- Call graph extraction (caller -> callee)

## [2.0.0] - 2026-02-10

### Changed
- Complete rewrite on top of sqlitegraph
- New backend architecture

## [1.0.0] - 2026-01-15

### Added
- Initial release
- Basic symbol extraction for Rust
- File watching with notify

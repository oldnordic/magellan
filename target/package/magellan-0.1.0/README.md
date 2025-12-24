# Magellan

**A dumb, deterministic codebase mapping tool for Rust projects.**

Magellan watches your Rust source files, extracts AST-level facts, and builds a searchable graph database of symbols and references. No semantic analysis, no magic—just deterministic, observable data extraction.

## What Magellan Does

- **Watches** directories for `.rs` file changes (Create/Modify/Delete)
- **Extracts** AST-level facts: functions, structs, enums, traits, modules, impl blocks
- **Tracks** symbol references: function calls and type references
- **Persists** everything to a sqlitegraph database
- **Handles** errors gracefully—keeps running even when files are unreadable
- **Shuts down** cleanly on SIGINT/SIGTERM with status reporting

## What Magellan Does NOT Do

Magellan is intentionally limited:
- ❌ No semantic analysis or type checking
- ❌ No LSP server or language features
- ❌ No async runtimes or background thread pools
- ❌ No config files
- ❌ No initial full scans (requires events to trigger)
- ❌ No non-Rust file support
- ❌ No web APIs or network services

## Installation

### From Source

```bash
# Clone the repository
git clone <repository-url>
cd magellan

# Build the binary
cargo build --release

# The binary will be at target/release/magellan
```

### Requirements

- Rust 1.70+ (2021 edition)
- Linux/macOS (signal handling uses Unix signals)
- SQLite 3 (via sqlitegraph dependency)

## Quick Start

```bash
# Start watching a project
magellan watch --root /path/to/rust/project --db /path/to/magellan.db

# In another terminal, check status
magellan watch --root /path/to/rust/project --db /path/to/magellan.db --status

# Magellan will now:
# 1. Watch for .rs file changes
# 2. Extract symbols and references
# 3. Store them in magellan.db
# 4. Log each event: "MODIFY src/lib.rs symbols=5 refs=3"
```

## Usage

### Basic Command

```bash
magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>]
```

**Arguments:**
- `--root <DIR>` - Directory to watch recursively (required)
- `--db <FILE>` - Path to sqlitegraph database (required)
- `--debounce-ms <N>` - Debounce delay in milliseconds (default: 500)
- `--status` - Print counts and exit immediately (optional)

**Examples:**

```bash
# Watch current directory
magellan watch --root . --db ./magellan.db

# Watch with custom debounce
magellan watch --root ./src --db ./cache/magellan.db --debounce-ms 1000

# Check database status
magellan watch --root . --db ./magellan.db --status
# Output:
# files: 42
# symbols: 387
# references: 1241
```

### Output Format

**Normal Operation:**
```
Magellan watching: /path/to/project
Database: /path/to/magellan.db
CREATE src/main.rs symbols=2 refs=0
MODIFY src/lib.rs symbols=5 refs=3
DELETE src/old.rs
ERROR src/permission_denied.rs Permission denied (os error 13)
```

**Shutdown:**
```
SHUTDOWN
```

**Status:**
```
files: 42
symbols: 387
references: 1241
```

## Database Schema

Magellan stores data in a sqlitegraph database with the following structure:

**Nodes:**
- `File` - `{ path: String, hash: String }`
- `Symbol` - `{ name: String, kind: String, byte_start: usize, byte_end: usize }`
- `Reference` - `{ file: String, byte_start: usize, byte_end: usize }`

**Edges:**
- `DEFINES` - File → Symbol (which file defines this symbol)
- `REFERENCES` - Reference → Symbol (what symbol is referenced)

**Symbol Kinds:**
- Function
- Struct
- Enum
- Trait
- Module
- Impl

## Error Handling

Magellan is designed to be resilient:

**Permission Errors:**
```
ERROR /path/to/file.rs Permission denied (os error 13)
```
- Logs the error
- Continues processing other files
- No crash, no retry

**Syntax Errors:**
- Files with invalid Rust syntax are skipped
- No symbols extracted from malformed files
- Watcher continues running

**Missing Files:**
- Files deleted during processing are handled gracefully
- ENOENT errors are silently skipped
- No crashes on race conditions

## Signal Handling

**SIGINT (Ctrl+C) / SIGTERM:**
- Prints "SHUTDOWN"
- Exits cleanly
- Database is properly closed
- No data loss

## Architecture

```
magellan/
├── src/
│   ├── main.rs              # Binary entry point (236 LOC)
│   ├── lib.rs               # Public API exports
│   ├── watcher.rs           # Filesystem event watcher (156 LOC)
│   ├── ingest.rs            # Tree-sitter parser (184 LOC)
│   ├── indexer.rs           # Event coordination (125 LOC)
│   ├── references.rs        # Reference extraction (171 LOC)
│   └── graph/
│       ├── mod.rs           # CodeGraph API (306 LOC)
│       ├── schema.rs        # Node/edge types (29 LOC)
│       ├── files.rs         # File operations (161 LOC)
│       ├── symbols.rs       # Symbol operations (107 LOC)
│       └── references.rs    # Reference operations (138 LOC)
└── tests/
    ├── cli_smoke_tests.rs   # Binary tests (72 LOC)
    ├── signal_tests.rs      # Signal handling tests (81 LOC)
    ├── error_tests.rs       # Error handling tests (86 LOC)
    ├── status_tests.rs      # Status flag tests (58 LOC)
    └── ...
```

## Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test cli_smoke_tests
cargo test --test signal_tests
cargo test --test error_tests
cargo test --test status_tests

# Run with output
cargo test -- --nocapture
```

**Test Coverage:**
- 37 tests across 12 test suites
- Unit tests for parsing, graph operations
- Integration tests for indexer, watcher
- Process tests for binary (signals, errors, status)
- All tests pass in <5 seconds

## Performance

**Indexing Speed:**
- ~1000 lines/sec on modern hardware
- SHA-256 hashing for content change detection
- In-memory HashMap for O(1) file lookups

**Database Size:**
- ~1KB per symbol (including metadata)
- ~500B per reference
- Typical Rust project: 100-1000 symbols, 500-5000 references

**Memory Usage:**
- ~50MB base RSS
- ~10MB per 1000 files indexed
- Grows with project size

## Limitations

**By Design:**
- Only processes `.rs` files
- No initial full scan (waits for events)
- AST-level only (no semantic analysis)
- No cross-crate resolution
- No macro expansion tracking

**Technical:**
- Single-threaded event processing
- No incremental parsing
- No query optimization
- No indexing heuristics

## Contributing

Magellan is **feature frozen**. No new features are planned.

**Bug Reports:**
- Open an issue with reproduction steps
- Include `magellan --status` output
- Attach test case if possible

**Patches:**
- Must pass all existing tests
- Must maintain LOC limits (≤300 per file)
- Must follow existing code style
- No new features without explicit approval

## License

```
Magellan - Dumb, deterministic codebase mapping tool
Copyright (C) 2025  Feanor

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
```

**SPDX-License-Identifier: GPL-3.0-or-later**

## Acknowledgments

Built with:
- [notify](https://github.com/notify-rs/notify) - Filesystem watching
- [tree-sitter](https://tree-sitter.github.io/) - AST parsing
- [tree-sitter-rust](https://github.com/tree-sitter/tree-sitter-rust) - Rust grammar
- [sqlitegraph](https://github.com/synbit/sqlitegraph) - Graph persistence
- [signal-hook](https://github.com/vorner/signal-hook) - Signal handling

## Version

**Current Version:** 0.1.0
**Release Date:** 2025-12-24
**Status:** Feature Frozen

See [CHANGELOG.md](CHANGELOG.md) for version history.

# Technology Stack

**Analysis Date:** 2026-01-19

## Languages

**Primary:**
- Rust 2021 Edition - Core language for the entire codebase

**Secondary:**
- SQL - Embedded queries for sqlitegraph and rusqlite operations
- Tree-sitter grammar definitions - AST parsing for 7 languages (Rust, Python, C, C++, Java, JavaScript, TypeScript)

## Runtime

**Environment:**
- Rust 1.70+ (tested with 1.92.0)
- Linux/macOS (Unix signal handling via signal-hook)

**Package Manager:**
- Cargo 1.92.0
- Lockfile: `Cargo.lock` (present, committed)

## Frameworks

**Core:**
- sqlitegraph 1.0.0 - Graph database persistence layer (nodes, edges, labels)
- tree-sitter 0.22 - Parser framework for multi-language AST extraction
- notify 7.0 - File system watcher for directory monitoring

**Testing:**
- Built-in Rust test framework (`cargo test`)
- tempfile 3.10 - Test file/directory fixtures

**Build/Dev:**
- serde 1.0 - Serialization/deserialization
- serde_json 1.0 - JSON export functionality
- anyhow 1.0 - Error handling
- thiserror 1.0 - Error derive macros

## Key Dependencies

**Critical:**
- sqlitegraph 1.0.0 - Core graph database backend. Provides `SqliteGraphBackend`, `GraphBackend`, `NodeId`, `NodeSpec`. All persistence flows through this.
- tree-sitter 0.22 + language grammars - AST parsing. Used in `src/ingest/*.rs` for symbol extraction from 7 languages
- rusqlite 0.31 - Direct SQLite access for side tables (`code_chunks`, metadata) that sqlitegraph doesn't manage

**Infrastructure:**
- notify 7.0 - File system events in `src/watcher.rs`
- walkdir 2.5 - Directory traversal in `src/graph/scan.rs`
- globset 0.4 - Pattern matching for symbol queries
- sha2 0.10 + hex 0.4 - Content hashing for file fingerprints in `src/graph/files.rs`
- signal-hook 0.3 - Graceful shutdown on SIGINT/SIGTERM in `src/watcher.rs`

## Configuration

**Environment:**
- No environment variables required
- All configuration via CLI arguments (`--root`, `--db`, `--debounce-ms`)
- No config files

**Build:**
- `Cargo.toml` defines single binary: `magellan` at `src/main.rs`
- Optional feature: `native-v2` (enables sqlitegraph's native-v2 backend for 2-3x faster inserts)

**Features:**
- `default = []` - No default features
- `native-v2 = ["sqlitegraph/native-v2"]` - Performance optimization

## Platform Requirements

**Development:**
- Rust 1.70+
- Cargo
- Native build tools (gcc/make)

**Production:**
- Standalone binary (static linking possible)
- SQLite 3 embedded via rusqlite
- No runtime dependencies

---

*Stack analysis: 2026-01-19*

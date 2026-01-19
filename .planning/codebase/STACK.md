# Technology Stack

**Analysis Date:** 2026-01-19

## Languages

**Primary:**
- Rust 2021 Edition - All application code, CLI tool

**Secondary:**
- SQL - SQLite database schema (managed by sqlitegraph)
- Protocol Buffers - SCIP format definitions

## Runtime

**Environment:**
- Linux, macOS, Windows (cross-platform via Rust)

**Package Manager:**
- Cargo - Rust package manager
- Lockfile: `Cargo.lock` (present)

## Frameworks

**Core:**
- sqlitegraph v1.0.0 - Graph database backend for SQLite
- tree-sitter v0.22 - Parser generator for multiple languages

**Testing:**
- Built-in Rust test framework (`#[test]` attribute)
- tempfile v3.10 - Temporary directory test fixtures

**Build/Dev:**
- Cargo - Build system and dependency manager

## Key Dependencies

**Critical:**
- anyhow v1.0 - Error handling (anyhow::Result)
- thiserror v1.0 - Error type derivation
- serde v1.0 - Serialization framework
- serde_json v1.0 - JSON serialization
- rusqlite v0.31 - SQLite bindings

**Infrastructure:**
- notify v8.2.0 - Filesystem watching
- notify-debouncer-mini v0.7.0 - Event debouncing
- walkdir v2.5 - Directory traversal
- sha2 v0.10 - SHA-256 hashing
- hex v0.4 - Hex encoding
- signal-hook v0.3 - Signal handling (Ctrl+C)

**Language Support:**
- tree-sitter-rust v0.21
- tree-sitter-python v0.21
- tree-sitter-c v0.21
- tree-sitter-cpp v0.21
- tree-sitter-java v0.21
- tree-sitter-javascript v0.21
- tree-sitter-typescript v0.21

**Export Formats:**
- csv v1.3 - CSV export
- scip v0.6.1 - SCIP protocol support
- protobuf v3.7 - Protocol buffer encoding for SCIP
- base64 v0.22 - Base64 encoding

**Utilities:**
- globset v0.4 - Glob pattern matching
- ignore v0.4.25 - Gitignore-style filtering

## Configuration

**Environment:**
- Command-line arguments (no .env file support)
- Configuration via CLI flags only

**Build:**
- Cargo.toml defines features and dependencies

## Platform Requirements

**Development:**
- Rust 1.70+ (2021 edition)
- Cargo (comes with Rust)
- SQLite 3 (linked via libsqlite3-sys)

**Production:**
- Standalone binary (static linking preferred)
- No runtime dependencies beyond system SQLite library

---
*Stack analysis: 2026-01-19*

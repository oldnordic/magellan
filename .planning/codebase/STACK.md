# Technology Stack

**Analysis Date:** 2026-02-08

## Languages

**Primary:**
- Rust 2021 edition - Main language for all source code

## Runtime

**Environment:**
- Rust runtime
- Native compilation to binary
- No runtime dependencies at execution time

**Package Manager:**
- Cargo [version from Cargo.lock]
- Lockfile: Cargo.lock (present)

## Frameworks

**Core:**
- CLI tool (no web framework)
- Command-line interface with subcommands

**Testing:**
- Criterion [0.5] - Benchmarking
- Built-in test harness

**Build/Dev:**
- Cargo build system
- Benchmarks in benches/perf_suite.rs

## Key Dependencies

**Critical:**
- sqlitegraph [1.5.3] - Core graph database and algorithms
- tree-sitter [0.22] + language parsers - AST parsing for multiple languages
- anyhow [1.0] - Error handling
- thiserror [1.0] - Custom error types

**Infrastructure:**
- rusqlite [0.31] - SQLite bindings (when not using native-v2)
- notify [8.2.0] + notify-debouncer-mini [0.7.0] - File system watching
- rayon [1.11] - Parallel processing
- serde [1.0] + serde_json [1.0] - Serialization
- tempfile [3.10] - Temporary file handling

**Data Processing:**
- ahash [0.8] - Fast hashing
- uuid [1.0] - UUID generation
- chrono [0.4] - Date/time handling
- sha2 [0.10] + blake3 [1.5] + hex [0.4] - Cryptographic hashing
- base64 [0.22] - Base64 encoding

**Language Support:**
- tree-sitter-rust [0.21]
- tree-sitter-python [0.21]
- tree-sitter-c [0.21]
- tree-sitter-cpp [0.21]
- tree-sitter-java [0.21]
- tree-sitter-javascript [0.21]
- tree-sitter-typescript [0.21]

**Optional Dependencies:**
- inkwell [0.5] (optional) - LLVM IR bindings for C/C++ CFG extraction
- which [6] (optional) - Find clang in PATH
- java_asm [0.1] (optional) - Java bytecode parsing (placeholder)

## Configuration

**Environment:**
- Configuration via command-line arguments
- No environment variables required for basic operation
- Optional features via Cargo feature flags

**Build:**
- Cargo.toml for dependency management
- build.rs for custom build configuration
- Feature flags for optional functionality

## Platform Requirements

**Development:**
- Rust toolchain (nightly/stable)
- LLVM/Clang (optional, for llvm-cfg feature)
- Java development kit (optional, for bytecode-cfg feature)

**Production:**
- Rust runtime (statically linked)
- No external runtime dependencies
- Optional: LLVM/Clang for enhanced C/C++ support

---

*Stack analysis: 2026-02-08*
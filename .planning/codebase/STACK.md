# Technology Stack

**Analysis Date:** 2026-02-10

## Languages

**Primary Language:**
- **Rust** (2024 edition)
- Version: Latest stable (via rust-toolchain.toml if present)
- Purpose: All core functionality, CLI tools

**Supporting Languages:**
- **Shell:** Bash scripts for tooling and CI
- **Markdown:** Documentation
- **SQL:** Database queries (via rusqlite)

## Runtime and Build

**Build System:**
- **Cargo:** Rust package manager and build tool
- Workspace: Single-package project (magellan)

**Compilation:**
- Target: Native binary (`magellan`)
- Features: Conditional compilation via Cargo features
  - `native-v2`: Enable native-v2 backend support
  - `default`: SQLite backend with standard features

**Binary:**
- Name: `magellan`
- Installation: `cargo install magellan`
- Distribution: crates.io

## Frameworks and Libraries

**Core Dependencies:**

| Dependency | Version | Purpose |
|------------|---------|---------|
| `sqlitegraph` | 1.5.5+ | Graph database backend abstraction |
| `tree-sitter` | Latest | Language parsing infrastructure |
| `rusqlite` | Latest | SQLite database access |
| `anyhow` | Latest | Error handling |
| `clap` | Latest | CLI argument parsing |
| `serde` | Latest | Serialization/deserialization |
| `tokio` | Latest | Async runtime (if needed) |

**Language Parsers:**
- tree-sitter-rust
- tree-sitter-c
- tree-sitter-cpp
- tree-sitter-java
- tree-sitter-python
- tree-sitter-javascript
- tree-sitter-typescript
- And others for multi-language support

**CLI Framework:**
- clap: Command-line argument parsing
- Custom command dispatch in `src/main.rs`

## Database

**SQLiteGraph:**
- Backend: SQLite (default) or Native-V2 (feature-flagged)
- Schema: Dynamic with migrations
- Location: User-specified via `--db` argument
- Connection: Shared connections for transactional operations

**Native-V2 Backend (feature flag):**
- KV-based storage for O(1) lookups
- 10-100x performance improvement
- 70%+ smaller database size
- Snapshot/restore capabilities

## Configuration

**Runtime Configuration:**
- Command-line arguments (no config file)
- Environment variables (for CI/testing)
- Database path specified per invocation

**Build Configuration:**
- `Cargo.toml`: Dependencies, features, metadata
- Feature flags for conditional compilation
- Workspace-level dependencies

## Development Tools

**Code Quality:**
- rustfmt: Code formatting
- clippy: Linting and code quality checks
- cargo test: Unit and integration testing

**Documentation:**
- rustdoc: API documentation
- Manual: Comprehensive user guide (MANUAL.md)
- Architecture docs: In `docs/` directory

**CI/CD:**
- GitHub Actions (if .github/workflows/ exists)
- Test matrix across Rust versions
- ThreadSanitizer (TSAN) for concurrency testing

## External Dependencies

**System Requirements:**
- Rust toolchain (stable)
- SQLite (bundled via rusqlite)
- File system access for indexing

**Optional Tools:**
- git: For .gitignore-aware file watching
- tree-sitter language grammars: Auto-discovered

## Cargo Features

**Available Features:**
- `native-v2`: Enable native-v2 backend (KV storage)
- `default`: Standard SQLite backend

**Feature Usage:**
```bash
# Use native-v2 backend
cargo install magellan --features native-v2

# Build with specific features
cargo build --features native-v2
```

---

*Stack analysis: 2026-02-10*

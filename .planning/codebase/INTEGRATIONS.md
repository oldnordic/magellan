# External Integrations

**Analysis Date:** 2026-02-08

## APIs & External Services

**File System:**
- Native file system access via Rust std::fs
- File watching via notify crate
- Path resolution and globbing via globset and ignore

**Code Analysis APIs:**
- Tree-sitter parsers for multiple programming languages
- No external code analysis APIs (all parsing done locally)

## Data Storage

**Databases:**
- SQLite/sqlitegraph - Primary storage for graph data
  - Native V2 backend with WAL transactions (optional feature)
  - Traditional SQLite backend (default)
  - File-based storage (.db files)

**File Storage:**
- Local file system only
- No cloud storage integration
- No distributed file systems

**Caching:**
- In-memory caching via sqlitegraph KV store (native-v2 feature)
- No external caching services

## Authentication & Identity

**Auth Provider:**
- No authentication required (local development tool)
- No external identity providers

**Security:**
- No API keys or secrets required
- All processing happens locally

## Monitoring & Observability

**Error Tracking:**
- Built-in error handling via anyhow
- No external error tracking services
- File system diagnostics via watch_diagnostics.rs

**Logs:**
- Console output only
- No logging infrastructure or external log aggregation
- Optional file logging via export commands

## CI/CD & Deployment

**Hosting:**
- Self-hosted binary distribution
- No cloud hosting dependencies
- GitHub repository: https://github.com/oldnordic/magellan

**CI Pipeline:**
- No external CI integrations
- Local testing via cargo test
- Benchmarking via criterion

## Environment Configuration

**Required env vars:**
- None required for basic operation

**Optional env vars:**
- PATH - Used when finding clang (llvm-cfg feature)
- JAVA_HOME - Used for Java bytecode parsing (future feature)

**Secrets location:**
- No secrets required
- All configuration via command-line arguments

## Webhooks & Callbacks

**Incoming:**
- None (no web server or API endpoints)

**Outgoing:**
- None (no external API calls)

## Language Processing

**Supported Languages:**
- Rust (tree-sitter-rust)
- Python (tree-sitter-python)
- C (tree-sitter-c)
- C++ (tree-sitter-cpp)
- Java (tree-sitter-java)
- JavaScript (tree-sitter-javascript)
- TypeScript (tree-sitter-typescript)

**Processing:**
- All parsing done locally via tree-sitter
- No external language service integrations
- No LLM or AI-based code analysis

## Export Formats

**Code Export:**
- JSON output for all command results
- SCIP protocol support for interoperability
- No external export destinations

**Graph Export:**
- Custom graph formats
- No standard graph database integrations

---

*Integration audit: 2026-02-08*
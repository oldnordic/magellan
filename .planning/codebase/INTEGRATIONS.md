# External Integrations

**Analysis Date:** 2026-01-19

## APIs & External Services

**None:** Magellan is a standalone CLI tool with no external API calls or cloud services.

**MCP (Model Context Protocol):**
- Configuration present in `.mcp.json` for local codemcp server
- This is a local development tool integration, not an external service
- Location: `/home/feanor/Projects/magellan/.mcp.json`

## Data Storage

**Databases:**
- SQLite (via sqlitegraph 1.0.0)
  - Connection: User-provided path via `--db <FILE>` CLI argument
  - Client/ORM: sqlitegraph (`SqliteGraphBackend`, `GraphBackend`) for graph operations
  - Direct access: rusqlite 0.31 for side tables (`code_chunks`, metadata tables)

**Database Schema Components:**
- `graph_meta` - sqlitegraph internal table
- `graph_nodes` - sqlitegraph node storage
- `graph_edges` - sqlitegraph edge storage
- `graph_labels` - sqlitegraph label storage
- `code_chunks` - Magellan side table for storing source code fragments
- `magellan_meta` - Magellan metadata including schema version

**File Storage:**
- Local filesystem only
- No cloud storage integration
- All file paths are local/absolute paths

**Caching:**
- `.fastembed_cache/` - FastEmbed cache directory (excluded from git)
- Used only for embedding-related features if enabled

## Authentication & Identity

**Auth Provider:**
- None (no authentication required)
- Local filesystem access only
- No network services

## Monitoring & Observability

**Error Tracking:**
- None (local CLI tool)

**Logs:**
- stdout/stderr for CLI output
- No structured logging framework
- Error reporting via anyhow::Error

## CI/CD & Deployment

**Hosting:**
- GitHub repository: https://github.com/feanor/magellan
- No CI/CD configuration detected

**CI Pipeline:**
- None detected (no `.github/workflows/`, `.gitlab-ci.yml`, etc.)

## Environment Configuration

**Required env vars:**
- None (CLI is fully configured via command-line arguments)

**Optional env vars:**
- `RUST_LOG` - For debug logging when running as MCP server (see `.mcp.json`)
- `ANTHROPIC_AUTH_TOKEN` - Only for codemcp MCP integration
- `ANTHROPIC_BASE_URL` - Only for codemcp MCP integration

**Secrets location:**
- `.mcp.json` (local only, gitignored)
- No secrets required for core magellan functionality

## Webhooks & Callbacks

**Incoming:**
- None (no HTTP server)

**Outgoing:**
- None (no HTTP client)

## Language Parsers (Tree-sitter Grammars)

**Supported Languages:**
- Rust - `tree-sitter-rust 0.21` (used in `src/ingest/`, `src/references.rs`)
- Python - `tree-sitter-python 0.21` (used in `src/ingest/python.rs`)
- C - `tree-sitter-c 0.21` (used in `src/ingest/c.rs`)
- C++ - `tree-sitter-cpp 0.21` (used in `src/ingest/cpp.rs`)
- Java - `tree-sitter-java 0.21` (used in `src/ingest/java.rs`)
- JavaScript - `tree-sitter-javascript 0.21` (used in `src/ingest/javascript.rs`)
- TypeScript - `tree-sitter-typescript 0.21` (used in `src/ingest/typescript.rs`)

**Parser Implementation Pattern:**
Each language has a parser module in `src/ingest/` that:
1. Creates a `tree_sitter::Parser`
2. Sets the language grammar (e.g., `tree_sitter_rust::language()`)
3. Walks the AST to extract `SymbolFact` structs
4. Returns facts for persistence in the graph

---

*Integration audit: 2026-01-19*

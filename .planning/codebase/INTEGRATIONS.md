# External Integrations

**Analysis Date:** 2026-01-19

## APIs & External Services

**None - local-only tool:**
- Magellan does not make network calls or use external APIs
- All processing is local to the machine

## Data Storage

**Databases:**
- SQLite (via sqlitegraph crate)
- Connection: Local file path passed via `--db` flag
- Client: rusqlite v0.31, sqlitegraph v1.0.0
- Schema: Managed by sqlitegraph, extended by Magellan side-tables

**File Storage:**
- Local filesystem only
- Source files: Read from user-specified `--root` directory
- Output: stdout or file path via `--output` flag

**Caching:**
- No external caching
- In-memory file index: `HashMap<String, i64>` in `FileOps`

## Authentication & Identity

**Auth Provider:**
- None (local-only tool)

**Implementation:**
- No authentication mechanisms
- No user accounts or permissions

## Monitoring & Observability

**Error Tracking:**
- None (errors printed to stderr)

**Logs:**
- stdout: User-facing results (JSON or human-readable)
- stderr: Errors, warnings, diagnostic information
- No structured logging framework

## CI/CD & Deployment

**Hosting:**
- crates.io (Rust package registry)
- GitHub releases (for binaries)

**CI Pipeline:**
- None defined in repository (no .github/workflows/ detected)

## Environment Configuration

**Required env vars:**
- None (all configuration via CLI arguments)

**Secrets location:**
- N/A (no secrets used)

## Webhooks & Callbacks

**Incoming:**
- None

**Outgoing:**
- None

## File Format Integrations

**SCIP (Source Code Intelligence Protocol):**
- SDK/Client: scip v0.6.1 crate
- Purpose: Export graph data to SCIP binary format
- Implementation: `src/graph/export/scip.rs`
- Output: Binary protobuf file (.scip)

**JSON/JSONL:**
- Purpose: Structured data export for LLM consumption
- Implementation: serde_json

**CSV:**
- Purpose: Tabular data export
- Implementation: csv crate

**DOT (Graphviz):**
- Purpose: Call graph visualization
- Implementation: Custom formatter in `src/graph/export.rs`

## Language Parsers

**tree-sitter Integration:**
- tree-sitter v0.22: Parser framework
- Language grammars: tree-sitter-rust, tree-sitter-python, tree-sitter-c, tree-sitter-cpp, tree-sitter-java, tree-sitter-javascript, tree-sitter-typescript
- Purpose: Parse source code to extract symbols and references

---
*Integration audit: 2026-01-19*

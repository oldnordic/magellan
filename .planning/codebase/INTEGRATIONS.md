# External Integrations

**Analysis Date:** 2026-02-10

## External APIs

**None - Magellan operates entirely offline:**
- No network calls to external services
- All processing is local
- No API keys or authentication required

## Databases

**SQLite:**
- Purpose: Primary data storage for code graph
- Access: Via rusqlite library
- Schema: Managed by SQLiteGraph
- Location: User-specified file path

**Native-V2 KV Store (feature flag):**
- Purpose: High-performance key-value storage
- Access: Via SQLiteGraph backend abstraction
- Schema: Internal KV format
- Performance: 10-100x faster than SQL for lookups

## File System

**Watching:**
- notify: Filesystem event notifications
- debounce: Batch processing of file changes
- .gitignore awareness: Filters version-controlled files

**Access Patterns:**
- Read-only: Source code files
- Read-write: Database files
- Temp: Temporary files during processing

## Authentication

**None:**
- Magellan does not authenticate users
- No login/session management
- No external auth providers (OAuth, SSO, etc.)
- Local tool with file-based permissions

## Webhooks

**None:**
- Magellan does not send webhooks
- No HTTP server component
- No event publishing to external systems

## MCP Integration

**SQLiteGraph MCP Server:**
- Purpose: Expose code graph to MCP clients
- Tools: Symbol lookup, reference finding, graph traversal
- Database: Uses `codegraph.db` created by Magellan

**Usage Pattern:**
```bash
# Start Magellan watcher
magellan watch --root . --db .codemcp/codegraph.db

# MCP clients query the database
mcp.tools.find_symbol("main")
mcp.tools.get_references("function_name")
```

## Language Server Integration

**Potential (not implemented):**
- LSP protocol support for editor integration
- Symbol navigation and code intelligence
- Hover documentation

## Build Tool Integration

**Git Integration:**
- .gitignore awareness in file watcher
- File change detection for incremental indexing
- Repository root detection

**Cargo Integration:**
- For Rust projects: understands Cargo structure
- Can index crate dependencies
- Supports workspace detection

## IDE Integration

**Via CLI:**
- Editors can invoke `magellan` commands
- Output parsing for symbol information
- File path references for navigation

## Testing Infrastructure

**ThreadSanitizer (TSAN):**
- Runtime: Thread safety analysis
- CI: Automatic data race detection
- Test files: `tests/tsan_thread_safety_tests.rs`

## Dependency Graph

**External Crates:**
```
magellan
├── sqlitegraph (graph database abstraction)
├── tree-sitter (parsing infrastructure)
│   └── tree-sitter-* (language grammars)
├── rusqlite (SQLite bindings)
├── clap (CLI parsing)
├── anyhow (error handling)
├── serde (serialization)
├── tokio (async runtime, optional)
└── notify (filesystem watching)
```

## Network Usage

**Outbound:**
- None (fully offline operation)

**Inbound:**
- None (no server component)

## File Format Support

**Source Code:**
- Rust: .rs files
- C/C++: .c, .cpp, .h, .hpp files
- Java: .java files
- Python: .py files
- JavaScript/TypeScript: .js, .ts, .tsx files
- And more via tree-sitter grammars

**Data:**
- SQLite: .db database files
- JSON: Export/import functionality
- SCIP: (if supported) Code Intelligence Protocol

---

*Integration analysis: 2026-02-10*

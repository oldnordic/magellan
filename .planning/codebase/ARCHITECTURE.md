# Architecture

**Analysis Date:** 2026-01-19

## Pattern Overview

**Overall:** Layered graph database with filesystem watching and multi-language AST parsing

**Key Characteristics:**
- Graph-based persistence using sqlitegraph (nodes + edges in SQLite)
- Multi-language symbol extraction via tree-sitter parsers
- Deterministic, idempotent indexing operations
- File system watching with debounced event processing
- Language-agnostic symbol representation with normalized kinds

## Layers

**CLI Layer:**
- Purpose: Command-line interface and argument parsing
- Location: `src/main.rs`
- Contains: Command definitions, argument parsing, command routing
- Depends on: Library layer (`magellan` crate)
- Used by: End users via CLI

**Command Modules:**
- Purpose: Per-command implementation handlers
- Location: `src/*_cmd.rs` (e.g., `src/query_cmd.rs`, `src/find_cmd.rs`, `src/refs_cmd.rs`, `src/get_cmd.rs`, `src/watch_cmd.rs`, `src/verify_cmd.rs`)
- Contains: Command-specific logic that bridges CLI and library
- Depends on: `graph` module for database operations
- Used by: `main.rs` command router

**Graph Persistence Layer:**
- Purpose: Core graph database operations and schema management
- Location: `src/graph/mod.rs`, `src/graph/*.rs`
- Contains: `CodeGraph` struct, node/edge definitions, query operations, CRUD operations
- Depends on: `sqlitegraph` crate for graph storage, `ingest` for parsing, `generation` for code chunks, `references` for call graph
- Used by: All commands that read/write indexed data

**Ingestion Layer:**
- Purpose: Parse source code and extract symbol facts
- Location: `src/ingest/mod.rs`, `src/ingest/*.rs`
- Contains: Language-specific parsers (`c.rs`, `cpp.rs`, `java.rs`, `javascript.rs`, `python.rs`, `typescript.rs`), language detection, symbol kind definitions
- Depends on: `tree-sitter` and language grammars
- Used by: Graph layer during file indexing

**Reference Extraction Layer:**
- Purpose: Extract references and calls between symbols
- Location: `src/references.rs`
- Contains: `ReferenceExtractor`, `CallExtractor`, fact structures
- Depends on: `tree-sitter` for AST traversal, `ingest` for symbol definitions
- Used by: Graph layer for cross-file reference indexing

**Code Generation/Storage Layer:**
- Purpose: Store and retrieve source code chunks
- Location: `src/generation/mod.rs`, `src/generation/schema.rs`
- Contains: `ChunkStore`, `CodeChunk` schema, chunk storage operations
- Depends on: `rusqlite` for direct database access (bypasses sqlitegraph)
- Used by: Graph layer for symbol code retrieval

**Watcher Layer:**
- Purpose: Monitor filesystem for changes
- Location: `src/watcher.rs`
- Contains: `FileSystemWatcher`, `FileEvent` types, debouncing logic
- Depends on: `notify` crate for filesystem events
- Used by: `indexer.rs` for real-time indexing

**Indexer Layer:**
- Purpose: Coordinate between watcher and graph updates
- Location: `src/indexer.rs`
- Contains: Event handlers, indexing loop orchestration
- Depends on: `watcher` for events, `graph` for persistence
- Used by: `watch_cmd.rs` for watch mode

**Verification Layer:**
- Purpose: Compare database state vs filesystem
- Location: `src/verify.rs`
- Contains: `verify_graph`, `VerifyReport`
- Depends on: `graph` for database queries, `walkdir` for filesystem scanning
- Used by: `verify_cmd.rs`

## Data Flow

**Initial Indexing (watch command with --scan-initial):**

1. CLI parses `watch` command with `--scan-initial` flag
2. `CodeGraph::open()` creates/opens database at specified path
3. `CodeGraph::scan_directory()` walks filesystem recursively
4. For each source file:
   - `reconcile_file_path()` computes SHA-256 hash
   - `index_file()` detects language, extracts symbols via tree-sitter
   - Symbols inserted as Symbol nodes with DEFINES edges from File node
   - Code chunks stored in separate `code_chunks` table
   - References extracted and linked via REFERENCES edges
   - Calls extracted and linked via CALLS edges
5. Progress reported via callback (current/total file counts)

**File Change Event Processing:**

1. `FileSystemWatcher` thread receives `notify::Event`
2. Event converted to `FileEvent` (Create/Modify/Delete), filtered to exclude DB files
3. `run_indexer()` or `run_indexer_n()` receives event via channel
4. `handle_event()` delegates to `CodeGraph`:
   - Create/Modify: `delete_file()` then `index_file()` with new contents
   - Delete: `delete_file()` to remove all derived data
5. Database updated atomically per event

**Query Flow (e.g., `magellan query --db file.db --file src/main.rs`):**

1. CLI parses `query` command
2. `query_cmd::run_query()` opens `CodeGraph`
3. `symbols_in_file()` queries File node by path
4. Neighbors retrieved via DEFINES edges (outgoing from File)
5. Symbol nodes deserialized and returned as `SymbolFact` vector
6. Results printed to stdout

**Call Graph Traversal:**

1. `calls_from_symbol()` finds caller's Symbol node
2. Query outgoing CALLS edges from caller
3. Call nodes deserialized to `CallFact` vector
4. Reverse direction uses incoming CALLS edges

## Key Abstractions

**CodeGraph:**
- Purpose: Single entry point for all graph database operations
- Examples: `src/graph/mod.rs:40-422`
- Pattern: Facade over sqlitegraph with domain-specific methods. Internally delegates to specialized sub-modules (`files`, `symbols`, `references`, `calls`).

**SymbolFact:**
- Purpose: Language-agnostic representation of a code symbol
- Examples: `src/ingest/mod.rs:69-90`
- Pattern: Pure data structure with byte/line/column extents, kind classification, and optional name. No behavior methods.

**FileNode/SymbolNode/ReferenceNode/CallNode:**
- Purpose: Node payloads persisted in sqlitegraph
- Examples: `src/graph/schema.rs:10-59`
- Pattern: Serde-serializable structs stored as JSON in sqlitegraph's node data field.

**Parser Trait (Per-Language):**
- Purpose: Extract symbols from source code
- Examples: `src/ingest/python.rs`, `src/ingest/java.rs`, etc.
- Pattern: Each language has a `*Parser` struct with `extract_symbols(file_path, source) -> Vec<SymbolFact>`. Stateless, pure function.

**Reconciliation:**
- Purpose: Determine if file needs reindexing based on content hash
- Examples: `src/graph/ops.rs:reconcile_file_path`
- Pattern: Compare stored hash vs computed hash. Return `ReconcileOutcome` (Deleted/Unchanged/Reindexed).

## Entry Points

**Binary Entry Point:**
- Location: `src/main.rs:792-923`
- Triggers: User runs `magellan <command>` executable
- Responsibilities: Argument parsing, command routing, error handling

**Library Entry Points:**
- Location: `src/lib.rs`
- Triggers: External code links `magellan` as dependency
- Responsibilities: Re-exports public API (CodeGraph, types, functions)

**Watch Mode Entry Point:**
- Location: `src/watch_cmd.rs` → `src/indexer.rs`
- Triggers: User runs `magellan watch --root <dir> --db <file>`
- Responsibilities: Creates watcher, opens graph, runs event loop

**Database Open Sequence:**
- Location: `src/graph/mod.rs:68-110`
- Triggers: `CodeGraph::open(&db_path)`
- Responsibilities:
  1. Compatibility preflight (`db_compat::preflight_sqlitegraph_compat`)
  2. sqlitegraph initialization with schema migrations
  3. Magellan metadata table creation (`db_compat::ensure_magellan_meta`)
  4. ChunkStore schema initialization

## Error Handling

**Strategy:** Result propagation with anyhow::Result

**Patterns:**
- Functions return `anyhow::Result<T>` for error propagation
- Context added via `.map_err(|e| anyhow::anyhow!("Context: {}", e))`
- CLI layer prints errors and exits with non-zero status
- No error swallowing: all errors propagate to caller

**Validation:**
- File existence checked before indexing
- Hash comparison prevents unnecessary re-indexing
- Language detection returns `None` for unknown extensions (not an error)

## Cross-Cutting Concerns

**Logging:** None (no structured logging framework). Errors printed to stderr via `eprintln!`.

**Validation:**
- File path validation via `std::path::Path`
- Hash verification before re-indexing
- Language detection via extension table (no content inference)

**Authentication:** Not applicable (local filesystem tool).

**Concurrency:**
- Watcher runs in dedicated thread (`std::thread::spawn`)
- Event channel uses `std::sync::mpsc`
- Graph operations are single-threaded (synchronous)
- Database access protected by SQLite's internal locking

**Determinism:**
- File iteration sorted alphabetically
- Symbol output sorted by line/column/byte offset
- Entity IDs sorted before batch deletions

---

*Architecture analysis: 2026-01-19*

# Architecture

**Analysis Date:** 2026-02-10

## Pattern Overview

**Overall:** Deterministic, event-driven codebase indexing system with SQLiteGraph persistence

**Key Characteristics:**
- Event-driven architecture with filesystem watcher
- Deterministic and idempotent operations
- Multi-language support via tree-sitter parsers
- SQLiteGraph backend with dual backend support (SQLite and Native V2)
- CLI tool with extensible command structure

## Layers

### Ingest Layer
- **Purpose:** Multi-language source code parsing and symbol extraction
- **Location:** `src/ingest/`
- **Contains:** Language-specific parsers, symbol detection, FQN computation
- **Depends on:** Tree-sitter grammars, language detection
- **Used by:** Indexer pipeline

### Graph Layer
- **Purpose:** Database operations and graph persistence
- **Location:** `src/graph/`
- **Contains:** CodeGraph wrapper, CRUD operations, query interfaces
- **Depends on:** SQLiteGraph backend, schema management
- **Used by:** Indexer, CLI commands

### Indexing Layer
- **Purpose:** Filesystem observation and indexing orchestration
- **Location:** `src/indexer.rs`, `src/watcher/`
- **Contains:** Filesystem watcher, event debouncing, reconciliation logic
- **Depends on:** Graph layer, filesystem events
- **Used by:** Watch command

### CLI Layer
- **Purpose:** Command-line interface and user interaction
- **Location:** `src/main.rs`, `src/*_cmd.rs`
- **Contains:** Command parsing, argument handling, output formatting
- **Depends on:** All other layers
- **Used by:** End users

### Storage Layer
- **Purpose:** Data persistence and caching
- **Location:** `src/generation/`, src/graph modules
- **Contains:** Code chunk storage, file node cache, metrics
- **Depends on:** SQLiteGraph backend
- **Used by:** Graph layer

## Data Flow

### Indexing Pipeline:

1. **Filesystem Watcher** (src/watcher/)
   - Monitors file changes with debounced batch events
   - Emits deterministic WatcherBatch with sorted paths

2. **Event Handler** (src/indexer.rs)
   - Receives file events
   - Calls reconcile_file_path for deterministic updates

3. **Reconciliation** (src/graph/ops.rs)
   - Computes SHA-256 hash of file contents
   - Upsert File node or delete if removed
   - Clears existing symbols/references for the file

4. **Ingestion** (src/ingest/)
   - Detects language from file extension
   - Parses symbols using tree-sitter
   - Builds fully-qualified names with scope tracking

5. **Persistence** (src/graph/)
   - Inserts Symbol nodes with DEFINES edges
   - Extracts and stores references
   - Builds call graph edges
   - Stores code chunks for retrieval

### Query Pipeline:

1. **CLI Command** (src/main.rs, src/*_cmd.rs)
   - Parses arguments and validates
   - Creates CodeGraph instance

2. **Graph Query** (src/graph/query.rs, calls.rs, etc.)
   - Executes symbol/reference/call queries
   - Uses file node cache for performance
   - Returns structured results

3. **Output Formatting** (src/output/)
   - Formats results as JSON or human-readable
   - Includes context, code snippets, or semantic info

## Key Abstractions

### CodeGraph
- **Purpose:** Main database wrapper providing deterministic operations
- **Examples:** `src/graph/mod.rs:132-829`
- **Pattern:** Facade pattern with specialized operation modules

### SymbolFact
- **Purpose:** Language-agnostic symbol representation
- **Examples:** `src/ingest/mod.rs:24-54`
- **Pattern:** Enum-based type system with language mapping

### WatcherBatch
- **Purpose:** Deterministic batch of file events
- **Examples:** `src/watcher/mod.rs:57-80`
- **Pattern:** Value object with sorted paths for deterministic behavior

### ChunkStore
- **Purpose:** Source code storage with deduplication
- **Examples:** `src/generation/mod.rs`
- **Pattern:** Repository pattern with content hashing

## Entry Points

### Main CLI Entry
- **Location:** `src/main.rs:42-562`
- **Purpose:** Command parsing and dispatch
- **Triggers:** Subcommand execution
- **Responsibilities:** Error handling, output formatting

### Watch Command
- **Location:** `src/watch_cmd.rs`, `src/indexer.rs`
- **Purpose:** Continuous indexing with filesystem watching
- **Triggers:** Filesystem event processing
- **Responsibilities:** Event debouncing, batch processing

### Index Command (via indexer)
- **Location:** `src/indexer.rs:75-100`
- **Purpose:** One-time directory scanning
- **Triggers:** Directory traversal and file processing
- **Responsibilities:** Progress reporting, deterministic ordering

## Error Handling

**Strategy:** Result-based error propagation with contextual information

**Patterns:**
- `anyhow::Result` for error propagation
- Custom error types in modules
- Graceful degradation for non-critical failures
- Transaction rollback on database errors

## Cross-Cutting Concerns

**Logging:** Structured logging with execution tracking
**Validation:** Graph invariants and filesystem consistency checks
**Caching:** File node cache with LRU eviction
**Concurrency:** Single-threaded indexing, thread-safe queries

---

*Architecture analysis: 2026-02-10*
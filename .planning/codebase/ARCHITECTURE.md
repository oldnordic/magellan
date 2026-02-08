# Architecture

**Analysis Date:** 2026-02-08

## Pattern Overview

**Overall:** Multi-language codebase analysis tool with graph-based persistence

**Key Characteristics:**
- Deterministic code indexing using tree-sitter parsers
- SQLite-backed graph database with optional native V2 backend
- Multi-language support (Rust, Python, Java, C/C++, JavaScript/TypeScript)
- Real-time file watching with debounced batch processing
- Modular design with clear separation of concerns

## Layers

**CLI Layer (`src/` command modules):**
- Purpose: Command-line interface and user interaction
- Location: `src/*.rs` (main.rs, *_cmd.rs files)
- Contains: CLI argument parsing, command dispatch, output formatting
- Depends on: Core library (`src/lib.rs`)
- Used by: End users, shell scripts

**Core Library (`src/lib.rs`):**
- Purpose: Public API and module organization
- Location: `src/lib.rs`
- Contains: Module exports, common utilities, public type definitions
- Depends on: All sub-modules
- Used by: CLI commands, external integration

**Ingest Layer (`src/ingest/`):**
- Purpose: Language-specific source code parsing
- Location: `src/ingest/`
- Contains: Language parsers, symbol extraction, FQN computation
- Depends on: tree-sitter grammars, common utilities
- Used by: Graph indexing, file scanning

**Graph Layer (`src/graph/`):**
- Purpose: Graph database operations and algorithms
- Location: `src/graph/`
- Contains: Symbol/Reference/Call operations, CFG extraction, graph algorithms
- Depends on: sqlitegraph backend, metrics, validation
- Used by: Indexer, query commands, analysis tools

**Storage Layer (`src/generation/`, `src/kv/`):**
- Purpose: Code chunk storage and KV indexing
- Location: `src/generation/`, `src/kv/` (native-v2 only)
- Contains: Code chunk persistence, KV indexes for fast lookups
- Depends on: SQLite backend, KV store
- Used by: Graph operations, symbol resolution

**Watcher Layer (`src/watcher/`):**
- Purpose: Real-time file system monitoring
- Location: `src/watcher/`
- Contains: File event handling, debouncing, pub/sub (native-v2)
- Depends on: notify library, indexer pipeline
- Used by: Continuous indexing, live development

**Diagnostics Layer (`src/diagnostics/`):**
- Purpose: Error reporting and monitoring
- Location: `src/diagnostics/`
- Contains: Diagnostic collection, error tracking, metrics
- Depends on: Graph operations, execution logging
- Used by: All components for error reporting

## Data Flow

**File Processing Flow:**

1. **File Discovery** (`src/watcher/` or manual scan)
   - Monitor file system changes or scan directory
   - Generate file events with metadata

2. **Ingestion** (`src/ingest/`)
   - Detect language from file extension
   - Parse with appropriate tree-sitter grammar
   - Extract symbol facts with scope tracking
   - Compute FQNs and canonical names

3. **Graph Storage** (`src/graph/`)
   - Create/update File node with content hash
   - Store Symbol nodes with DEFINES edges
   - Extract and store Reference nodes with REFERENCES edges
   - Store Call nodes with CALLS edges

4. **Indexing** (`src/graph/symbol_index.rs`, `src/kv/`)
   - Build symbol name-to-ID mapping
   - Create file-to-symbol reverse index
   - Generate reference/call indexes for fast queries

5. **Analysis** (`src/graph/algorithms.rs`, etc.)
   - Run graph algorithms (cycles, dead code, etc.)
   - Compute metrics and statistics
   - Generate reports and exports

**Real-time Updates:**
- Watcher detects file changes
- Debounce events into batches
- Reconcile file state (update/delete)
- Re-index affected files
- Invalidate caches as needed

## Key Abstractions

**CodeGraph:**
- Purpose: Main graph database interface
- Location: `src/graph/mod.rs::CodeGraph`
- Provides: File, Symbol, Reference, Call CRUD operations
- Pattern: Facade pattern over multiple operation modules

**SymbolFact:**
- Purpose: Language-agnostic symbol representation
- Location: `src/ingest/mod.rs::SymbolFact`
- Contains: File path, kind, name, spans, FQNs
- Pattern: Data transfer object with serialization

**ScopeStack:**
- Purpose: Track nested symbol scopes during parsing
- Location: `src/ingest/mod.rs::ScopeStack`
- Contains: Stack of scope names, language-specific separators
- Pattern: Stack-based scope tracking

**ChunkStore:**
- Purpose: Code fragment storage for efficient retrieval
- Location: `src/generation/mod.rs::ChunkStore`
- Contains: Code chunks with byte spans, file mapping
- Pattern: Content-addressable storage

**FileSystemWatcher:**
- Purpose: Real-time file monitoring
- Location: `src/watcher/mod.rs::FileSystemWatcher`
- Contains: Event debouncing, batch processing, pub/sub support
- Pattern: Observer pattern with debouncing

## Entry Points

**Main CLI:**
- Location: `src/main.rs`
- Triggers: Command parsing and dispatch
- Responsibilities: Parse arguments, call appropriate command handlers

**Index Pipeline:**
- Location: `src/indexer.rs::run_indexer`
- Triggers: Batch file processing
- Responsibilities: Coordinate ingestion, graph storage, indexing

**Watch Pipeline:**
- Location: `src/indexer.rs::run_watch_pipeline`
- Triggers: File system events
- Responsibilities: Coordinate watcher, indexer, debouncing

**Individual Commands:**
- Location: `src/*_cmd.rs` files
- Triggers: Specific CLI subcommands
- Responsibilities: Command-specific logic and output

## Error Handling

**Strategy:** Layered error handling with context preservation

**Patterns:**
- anyhow::Result for error propagation
- Custom error types in modules
- Diagnostic collection for non-fatal issues
- Graceful degradation for missing features

**Validation:**
- Graph invariant checking
- File system path validation
- Symbol name normalization
- Cross-reference consistency

## Cross-Cutting Concerns

**Logging:**
- Execution logging via `ExecutionLog`
- Diagnostic collection for monitoring
- Performance metrics collection

**Concurrency:**
- Thread-safe design with Arc<Mutex<T>>
- Global lock ordering to prevent deadlocks
- Shared state for pipelines

**Caching:**
- File node cache for frequently accessed files
- KV indexes for O(1) symbol lookups (native-v2)
- Cache invalidation on file changes

---

*Architecture analysis: 2026-02-08*
# Architecture

**Analysis Date:** 2026-01-19

## Pattern Overview

**Overall:** Layered graph-based indexing system with local-first persistence

**Key Characteristics:**
- Deterministic: Same inputs produce identical graph state (sorted paths, hash-based identity)
- Local-first: Single-file SQLite database, no network dependencies
- Incremental: Watch mode with debounced batch updates
- Multi-language: tree-sitter based parsing for 7 languages

## Layers

**CLI Layer (`src/main.rs`):**
- Purpose: Command-line interface and argument parsing
- Location: `src/main.rs`
- Contains: Command definitions, argument parsing, execution tracking
- Depends on: Graph layer, indexer, watcher
- Used by: End users via CLI

**Graph Persistence Layer (`src/graph/`):**
- Purpose: Code graph storage using sqlitegraph backend
- Location: `src/graph/mod.rs`
- Contains: FileOps, SymbolOps, ReferenceOps, CallOps, ChunkStore, ExecutionLog
- Depends on: sqlitegraph crate, rusqlite
- Used by: All commands that query or modify the graph

**Language Ingestion Layer (`src/ingest/`):**
- Purpose: Parse source code and extract symbols/references/calls
- Location: `src/ingest/mod.rs`, `src/ingest/{rust,python,c,cpp,java,javascript,typescript}.rs`
- Contains: Parser trait, language-specific parsers, Language detection
- Depends on: tree-sitter grammars
- Used by: Graph indexing operations

**Indexer/Watcher Layer (`src/indexer.rs`, `src/watcher.rs`):**
- Purpose: Coordinate file watching and incremental updates
- Location: `src/indexer.rs`, `src/watcher.rs`
- Contains: FileSystemWatcher, WatchPipeline, reconcile operations
- Depends on: notify, notify-debouncer-mini
- Used by: Watch command

**Export Layer (`src/graph/export/`):**
- Purpose: Serialize graph data to external formats
- Location: `src/graph/export.rs`, `src/graph/export/scip.rs`
- Contains: JSON, JSONL, CSV, DOT, SCIP exporters
- Depends on: serde, csv, scip, protobuf
- Used by: Export command

**Output Layer (`src/output/`):**
- Purpose: Structured JSON responses for CLI commands
- Location: `src/output/mod.rs`, `src/output/command.rs`
- Contains: Response types, execution ID generation
- Depends on: serde_json
- Used by: All CLI commands

**Diagnostics Layer (`src/diagnostics/`):**
- Purpose: Track skipped files, errors, and warnings
- Location: `src/diagnostics/mod.rs`, `src/diagnostics/watch_diagnostics.rs`
- Contains: WatchDiagnostic, DiagnosticStage, SkipReason
- Depends on: None (pure data types)
- Used by: Scan, watch operations

## Data Flow

**Initial Scan Flow:**

1. User invokes `magellan watch --root <DIR> --db <FILE>`
2. `src/watch_cmd.rs` creates WatchPipelineConfig with root/db paths
3. `src/indexer.rs::run_watch_pipeline()`:
   - Starts FileSystemWatcher thread (begins buffering events immediately)
   - Calls `graph.scan_directory()` for baseline
   - Drains any events that arrived during scan
   - Enters main watch loop
4. `src/graph/scan.rs::scan_directory()`:
   - Walks directory tree with walkdir
   - Applies FileFilter rules (ignores, gitignore, include/exclude)
   - For each file: calls `graph.index_file()` and `graph.index_references()`
5. `src/graph/ops.rs::index_file()`:
   - Computes SHA-256 hash of source
   - Deletes old symbols/edges for file (if exists)
   - Detects language via `src/ingest/detect.rs`
   - Parses symbols with language-specific parser
   - Inserts Symbol nodes and DEFINES edges
   - Stores code chunks in ChunkStore
   - Indexes calls

**Watch Mode Flow:**

1. FileSystemWatcher detects file changes via notify crate
2. notify-debouncer-mini coalesces events within debounce window (default 500ms)
3. WatcherBatch emitted with sorted, deduplicated paths
4. Main thread receives batch via `recv_batch_timeout()`
5. For each path: `graph.reconcile_file_path()`:
   - If file deleted: calls `delete_file_facts()`
   - If file exists and hash unchanged: skip (ReconcileOutcome::Unchanged)
   - If file exists and hash changed: delete old data, re-index
6. Output logged: MODIFY/DELETE/UNCHANGED with counts

**Query Flow (e.g., `magellan query --db <DB> --file <PATH>`):**

1. `src/query_cmd.rs::run_query()` parses arguments
2. Opens CodeGraph at db_path
3. Calls `graph.symbols_in_file()` or `graph.symbol_nodes_in_file_with_ids()`
4. `src/graph/query.rs` queries sqlitegraph:
   - Finds File node by path
   - Gets outgoing DEFINES edges to Symbol nodes
   - Returns sorted results (by line, column, byte offset)
5. Output formatted as human-readable or JSON

**Export Flow:**

1. `src/export_cmd.rs::run_export()` parses format and filters
2. Calls appropriate export function:
   - JSON/JSONL: `export_graph()` from `src/graph/export.rs`
   - CSV: `export_csv()`
   - DOT: `export_dot()`
   - SCIP: `export_scip()` from `src/graph/export/scip.rs`
3. Export functions iterate all graph entities via `backend.entity_ids()`
4. Results written to stdout or file

## Key Abstractions

**CodeGraph:**
- Purpose: Central graph database wrapper
- Examples: `src/graph/mod.rs` (CodeGraph struct)
- Pattern: Builder-like initialization with `open()`, then method calls

**Node Types (via sqlitegraph):**
- FileNode: `{path, hash, last_indexed_at, last_modified}`
- SymbolNode: `{symbol_id, name, kind, kind_normalized, byte_start, byte_end, start_line, start_col, end_line, end_col}`
- ReferenceNode: `{file, byte_start, byte_end, start_line, start_col, end_line, end_col}`
- CallNode: `{file, caller, callee, caller_symbol_id, callee_symbol_id, byte_start, byte_end, start_line, start_col, end_line, end_col}`

**Edge Types:**
- DEFINES: File -> Symbol (file defines a symbol)
- REFERENCES: Reference -> Symbol (reference points to symbol)
- CALLER: Symbol -> Call (caller symbol invokes call)
- CALLS: Call -> Symbol (call targets callee symbol)

**Stable IDs:**
- span_id: Byte-range based identifier for code spans
- symbol_id: SHA-256 hash of `language:fqn:span_id` for cross-run symbol identity
- execution_id: UUID for tracking individual command executions

## Entry Points

**`src/main.rs::main()`:**
- Location: `src/main.rs:1126`
- Triggers: CLI invocation
- Responsibilities: Parse args, dispatch to command handlers, return exit code

**Command Handlers:**
- `run_watch()`: `src/watch_cmd.rs` - Watch and index directory
- `run_export()`: `src/export_cmd.rs` - Export graph data
- `run_query()`: `src/query_cmd.rs` - Query symbols in file
- `run_find()`: `src/find_cmd.rs` - Find symbol by name
- `run_refs()`: `src/refs_cmd.rs` - Show callers/callees
- `run_status()`: `src/main.rs` - Show database statistics
- `run_verify()`: `src/verify_cmd.rs` - Verify DB vs filesystem

## Error Handling

**Strategy:** anyhow::Result with context preservation

**Patterns:**
- Functions return `Result<T>` from anyhow crate
- Errors are propagated with `?` operator
- Context added via `.map_err(|e| anyhow::anyhow!("context: {}", e))`
- Watch/index operations continue on individual file errors (collected as diagnostics)

**Validation:**
- Pre-run validation: `src/graph/validation.rs::pre_run_validate()`
- Post-run validation: `src/graph/validation.rs::validate_graph()`
- Checks: orphan references, orphan calls, missing paths

## Cross-Cutting Concerns

**Logging:** stderr eprintln! for errors/diagnostics, stdout for results

**Validation:**
- Pre-run: Check root/db paths exist
- Post-run: Check for orphan nodes (references/calls with missing targets)

**Authentication:** Not applicable (local-only tool)

**Determinism:**
- Sorted file processing (BTreeSet for paths)
- Hash-based content identity (SHA-256)
- Stable symbol IDs from language+fqn+span
- Deterministic sorting in all query outputs

---
*Architecture analysis: 2026-01-19*

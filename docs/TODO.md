# Magellan TODO

## Project Status

**Current Version:** 0.3.0
**Last Updated:** 2025-12-30
**Current State:** Multi-language support complete (7 languages)
**Feature Status:** Stable - See CHANGELOG.md for version history

**Module Layout (v0.3.0):**
```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Public API
├── watcher.rs           # Filesystem watcher
├── indexer.rs           # Event coordination
├── references.rs        # Reference/Call fact types
├── verify.rs            # Database verification logic
├── ingest/
│   ├── mod.rs           # Parser dispatcher & Rust parser
│   ├── detect.rs        # Language detection (7 languages)
│   ├── c.rs             # C parser
│   ├── cpp.rs           # C++ parser
│   ├── java.rs          # Java parser
│   ├── javascript.rs    # JavaScript parser
│   ├── typescript.rs    # TypeScript parser
│   └── python.rs        # Python parser
├── query_cmd.rs         # Query command
├── find_cmd.rs          # Find command
├── refs_cmd.rs          # Refs command
├── verify_cmd.rs        # Verify CLI handler
├── watch_cmd.rs         # Watch CLI handler
└── graph/
    ├── mod.rs           # CodeGraph API
    ├── schema.rs        # Node/edge types
    ├── files.rs         # File operations
    ├── symbols.rs       # Symbol operations
    ├── references.rs    # Reference node operations
    ├── calls.rs         # Call edge operations
    ├── call_ops.rs      # Call node operations
    ├── ops.rs           # Graph indexing operations
    ├── query.rs         # Query operations
    ├── count.rs         # Count operations
    ├── export.rs        # JSON export
    ├── scan.rs          # Scanning operations
    ├── freshness.rs     # Freshness checking
    └── tests.rs         # Graph tests
```

---

## Task Breakdown

### Upcoming Query & Metadata Work (codemcp blockers)
- ✅ `--explain-query` CLI help now ships via `magellan query --explain` (`src/query_cmd.rs`). The flag prints selector syntax, related commands, and usage examples per `docs/CLI_SEARCH_PLAN.md` §8. Agents have deterministic guidance instead of guessing.
- ✅ Normalized `kind` metadata is persisted through `SymbolFact.kind_normalized`, `graph::schema::SymbolNode.kind_normalized`, and JSON export payloads. CLI output (query/find) now prints both the display kind and the canonical key so codemcp can drop manual `kind` parameters.
- ✅ Symbol extent query is exposed through `CodeGraph::symbol_extents` and `magellan query --symbol <name> --show-extent`, returning byte/line spans plus node IDs for `read_symbol`.
- ✅ Glob pattern search is available via `magellan find --list-glob <pattern>` backed by `globset`, returning deterministic node IDs for bulk rename planning.
- ✅ Error hints are emitted whenever `magellan query`/`magellan find` return empty results, nudging agents toward `--explain` or the glob helper instead of silent failures.
- 🔜 Next: thread the normalized-kind + extent data through the upcoming MCP `read_symbol`/`bulk_rename` endpoints (docs/CLI_SEARCH_PLAN.md §9) and add regression tests covering JSON export consumers once codemcp integrates the new schema.

### Task 1: Project Setup
**Status:** ✅ Complete
**Description:** Initialize Rust project with dependencies
**Deliverables:**
- ✅ `Cargo.toml` with dependencies: notify, anyhow, thiserror, serde, tempfile, tree-sitter, tree-sitter-rust
- ✅ Basic `src/` directory structure
**Verification:**
- [x] `cargo check` passes
- [x] `cargo test` runs

---

### Task 2: Filesystem Watching
**Status:** ✅ Complete
**Description:** Implement file watcher with debouncing
**Deliverables:**
- ✅ `src/watcher.rs` (156 LOC, within 300 LOC limit)
- ✅ Watches configured directory recursively
- ✅ Filters directories, emits file events only
- ✅ Emits: path, event_type (Create/Modify/Delete), timestamp
**Verification:**
- [x] Unit test: create file triggers Create event
- [x] Unit test: modify file triggers Modify event
- [x] Unit test: delete file triggers Delete event
- [x] Unit test: rapid changes produce events
- [x] Integration test: watch nested directory, verify events
**Design Decision:**
- Uses `notify::recommended_watcher` with callback
- Watcher thread blocks forever to stay alive
- Events sent via mpsc channel
- Tests use polling helper with timeout for reliability

---

### Task 3: Tree-sitter Parsing
**Status:** ✅ Complete
**Description:** Parse Rust source files and extract symbols
**Deliverables:**
- ✅ `src/ingest.rs` (184 LOC, within 300 LOC limit)
- ✅ Uses tree-sitter Rust grammar (tree-sitter-rust v0.21)
- ✅ Extracts: functions, structs, enums, traits, modules, impl blocks
- ✅ Records: name (if any), kind, byte_start, byte_end, file_path
**Verification:**
- [x] Test: empty file → no symbols
- [x] Test: syntax error → graceful handling (no crash)
- [x] Test: simple function → name and span extracted
- [x] Test: struct → name and span extracted
- [x] Test: enum → name and span extracted
- [x] Test: trait → name and span extracted
- [x] Test: module → name and span extracted
- [x] Test: impl block → detected
- [x] Test: multiple symbols → all extracted
- [x] Test: nested modules → flat extraction (all symbols)
- [x] Test: byte spans → within source bounds
- [x] Test: pure function → same input produces same output
**Design Decision:**
- Pure function: `extract_symbols(path, source) → Vec<SymbolFact>`
- No filesystem access in parser
- No semantic analysis
- Flat symbol structure (no hierarchy)
- Rust grammar nodes used:
  - `function_item` → SymbolKind::Function
  - `struct_item` → SymbolKind::Struct
  - `enum_item` → SymbolKind::Enum
  - `trait_item` → SymbolKind::Trait
  - `impl_item` → SymbolKind::Unknown (impl blocks have no name)
  - `mod_item` → SymbolKind::Module
  - `identifier` / `type_identifier` → name extraction

---

### Task 4: Reference Extraction
**Status:** ✅ Complete
**Description:** Extract symbol references from AST
**Deliverables:**
- ✅ `src/references.rs` (171 LOC) - reference extraction module
- ✅ `ReferenceFact` struct with file_path, referenced_symbol, byte_start, byte_end
- ✅ `ReferenceExtractor` with `extract_references()` method
- ✅ `Parser::extract_references()` extension method for convenience
- ✅ Walk AST to find `identifier` and `scoped_identifier` nodes
- ✅ Match by name only (collisions acceptable)
- ✅ Exclude references within symbol's own defining span
**Implementation:**
- Uses tree-sitter nodes: `identifier`, `scoped_identifier`
- For `scoped_identifier` (e.g., `a::foo()`), extracts final component `foo`
- Does NOT recurse into `scoped_identifier` children (prevents duplicate extraction)
- Checks reference position: must start AFTER symbol's defining span ends
**Graph Integration (src/graph.rs):**
- ✅ Added `ReferenceNode {file, byte_start, byte_end}` payload
- ✅ `index_references(path, source)` → usize (number of references indexed)
- ✅ `references_to_symbol(symbol_id)` → Vec<ReferenceFact>
- ✅ Creates "Reference" nodes with "REFERENCES" edges to Symbol nodes
- ⚠️  **LOC VIOLATION:** src/graph.rs is 523 LOC (exceeds 300 LOC limit)
**Verification:**
- [x] Test: function call → reference extracted ✅
- [x] Test: exclude defining span → zero references inside own span ✅
- [x] Test: scoped identifier → reference extracted ✅
- [x] Test: persist round-trip → references persisted and queryable ✅
**Test Results:**
- All 4 reference tests passing
- All 32 total tests passing (28 previous + 4 new)
- `cargo check` passes

---

### Task 4.1: Graph Modularization
**Status:** ✅ Complete
**Description:** Refactor monolithic src/graph.rs (523 LOC) into modular structure
**Deliverables:**
- ✅ src/graph/mod.rs (249 LOC) - public CodeGraph API
- ✅ src/graph/schema.rs (29 LOC) - labels, edge types, helper structs
- ✅ src/graph/files.rs (161 LOC) - file node operations, hashing, file_index
- ✅ src/graph/symbols.rs (107 LOC) - symbol node operations, DEFINES edges
- ✅ src/graph/references.rs (138 LOC) - reference node operations, REFERENCES edges, queries
- ✅ Each file ≤ 300 LOC
- ✅ NO logic changes, NO signature changes
- ✅ Uses Rc<SqliteGraphBackend> for shared backend across modules
- ✅ Clean visibility: only CodeGraph is pub, FileOps/SymbolOps/ReferenceOps are crate-private
- ✅ Zero compiler warnings (unused imports cleaned)
**Verification:**
- [x] cargo test passes (32/32 tests pass)
- [x] cargo check passes (no warnings)
- [x] All existing tests unchanged and passing
**Implementation Notes:**
- Used Rc<SqliteGraphBackend> to share backend across FileOps, SymbolOps, ReferenceOps
- Added GraphBackend trait import to all module files for trait methods
- Deleted old src/graph.rs (524 LOC) and src/schema.rs (63 LOC)
- src/lib.rs updated to use new graph module structure
- Module visibility: submodules are crate-private, only CodeGraph and query methods are public

---

### Task 5: Sqlitegraph Schema
**Status:** ✅ Complete
**Description:** Define and initialize graph schema
**Deliverables:**
- ✅ `src/schema.rs` (63 LOC) - defines constants for node labels and edge types
- ✅ Node labels: `File`, `Symbol`
- ✅ Edge types: `DEFINES`, `REFERENCES` (reserved for Phase 4)
- ✅ `src/graph.rs` (381 LOC) with persistence operations:
  - ✅ `open(db_path)` → CodeGraph
  - ✅ `index_file(path, source)` → usize (number of symbols indexed)
  - ✅ `delete_file(path)` → ()
  - ✅ `symbols_in_file(path)` → Vec<SymbolFact>
**Implementation Details:**
- Uses opaque JSON payloads for FileNode {path, hash} and SymbolNode {name, kind, byte_start, byte_end}
- SqliteGraphBackend (concrete type) for full API access including delete operations
- In-memory HashMap<String, NodeId> index for fast file lookups
- SHA-256 hashing for content change detection
- Idempotent re-index: delete old symbols, insert new ones
**Verification:**
- [x] Test: round_trip_symbols_in_file → index + query works
- [x] Test: idempotent_reindex → re-index with same content works
- [x] Test: idempotent_reindex → re-index with different content updates symbols
- [x] Test: delete_file_cleanup → file + symbols removed
- [x] Test: multiple_files_independent → files don't interfere
- [x] Test: symbol_fact_persistence → all fields preserved correctly
**Key Discoveries:**
- sqlitegraph uses opaque serde_json::Value payloads (NOT per-property access)
- SqliteGraphBackend has `entity_ids()` public method (NOT `all_entity_ids()` which is private)
- Must import GraphBackend trait to use its methods on SqliteGraphBackend
- NeighborQuery has `edge_type` field (NOT `edge_filter`) and no `node_filter` field

---

### Task 6: Update-on-Change Logic
**Status:** ✅ Complete
**Description:** Re-ingest files when they change
**Deliverables:**
- ✅ `src/indexer.rs` (125 LOC) - coordinator with run_indexer and run_indexer_n
- ✅ run_indexer() - blocking service mode for production
- ✅ run_indexer_n() - bounded mode for testing (processes max_events events then returns)
- ✅ handle_event() - private helper for Create/Modify/Delete mapping
- ✅ Graceful ENOENT handling: skips unreadable files (deterministic, no crashes)
**Test Implementation:**
- ✅ All 4 tests use run_indexer_n with bounded event counts
- ✅ Tests use threaded file operations to avoid blocking
- ✅ **CREATE events are not content-stable; tests assert eventual correctness via MODIFY**
**Verification:**
- [x] Integration test: modify file → old data deleted ✅
- [x] Integration test: modify file → new data persisted ✅
- [x] Integration test: delete file → all data removed ✅
- [x] Integration test: create file → eventual indexing via MODIFY ✅
- [x] Integration test: sequential events → correct final state ✅
**Key Design Decision:**
CREATE events fire at file operation start, not after content is written. Tests verify that after a content-stable MODIFY event, Magellan indexes correctly. This matches real-world behavior where CREATE may race with write completion.

---

### Task 6.1: CLI/Binary MVP Runner
**Status:** ✅ Complete
**Description:** Implement runnable binary that watches root dir and keeps sqlitegraph index up to date
**Deliverables:**
- ✅ `Cargo.toml` - added [[bin]] section for magellan binary
- ✅ `src/main.rs` (162 LOC, within 200 LOC limit)
  - Command: `magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>]`
  - Uses std::env::args for parsing (no external deps)
  - Default debounce-ms: 500 (from WatcherConfig::default())
  - Opens CodeGraph at --db path
  - Starts FileSystemWatcher on --root
  - Loops forever processing events
  - Filters to .rs files only (skips .db, .db-journal)
  - Logging: "{event_type} {path} symbols={n} refs={m}" for Create/Modify, "DELETE {path}" for Delete
- ✅ `src/watcher.rs` - Added `std::fmt::Display` impl for EventType
- ✅ `tests/cli_smoke_tests.rs` (72 LOC, within 250 LOC limit)
  - Spawns magellan binary process
  - Creates/modify .rs file in watched dir
  - Verifies stdout output contains expected log lines
  - Opens CodeGraph and asserts symbols indexed
**Verification:**
- [x] Test: watch command → binary runs and indexes file ✅
- [x] Test: stdout → contains MODIFY event with symbol count ✅
- [x] Test: database → symbols persisted and queryable ✅
- [x] Test: references → references indexed and queryable ✅
- [x] `cargo test --test cli_smoke_tests -- --nocapture` passes
- [x] `cargo test -- --nocapture` passes (all 33 tests: 5 watcher + 12 parser + 5 graph + 4 reference + 4 indexer + 1 CLI + 2 schema)
- [x] `cargo check` passes
**Test Output:**
```
running 1 test
test test_watch_command_indexes_file_on_create ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.61s
```
**Limitations:**
- No initial full scan (requires filesystem events to trigger indexing)
- Only processes .rs files (hardcoded filter)
- No async runtimes, no background thread pools beyond watcher thread
- No config files
- Binary must be killed with SIGTERM
**Implementation Notes:**
- Uses minimal std::env::args parsing (no clap dependency)
- Filters files by .rs extension to avoid indexing database files
- Path fallback for CARGO_BIN_EXE_magellan env var (constructs path from test exe)
- Follows indexer.rs pattern: read file, delete_file, index_file, index_references
- One small sleep (100ms) for process startup before file operations

---

### Task 6.2: Operational Hardening
**Status:** ✅ Complete
**Description:** Make magellan robust in production use with graceful shutdown, error handling, and status reporting
**Deliverables:**

#### Task 6.2.1 - Graceful Shutdown ✅
- ✅ Added `signal-hook = "0.3"` dependency to Cargo.toml
- ✅ `src/main.rs` (236 LOC, within 300 LOC limit)
  - Signal handling for SIGINT and SIGTERM using signal-hook::iterator::Signals
  - Uses Arc<AtomicBool> for shutdown flag shared with signal handler thread
  - Changed from blocking `recv_event()` to `try_recv_event()` with 100ms sleep loop
  - Prints "SHUTDOWN" on signal receipt before exiting
- ✅ `tests/signal_tests.rs` (81 LOC, within 250 LOC limit)
  - Spawns magellan binary, sends SIGTERM via kill command
  - Verifies process exits within 2 second timeout
  - Asserts stdout contains "SHUTDOWN"
**Verification:**
- [x] Test: SIGTERM → process prints SHUTDOWN and exits cleanly ✅

#### Task 6.2.2 - Deterministic Error Reporting ✅
- ✅ `src/main.rs` - Enhanced error handling in event loop
  - File read errors: log "ERROR {path} {error}" and continue
  - Index errors: log "ERROR {path} {error}" and continue
  - No retries, no backoff, no panic on errors
- ✅ `tests/error_tests.rs` (86 LOC, within 250 LOC limit)
  - Creates good.rs and bad.rs files
  - Makes bad.rs unreadable with chmod 000
  - Modifies good.rs to trigger event
  - Verifies ERROR line printed for bad.rs
  - Verifies MODIFY line printed for good.rs
  - Verifies good.rs was indexed despite bad.rs error
**Verification:**
- [x] Test: unreadable file → ERROR logged, process continues ✅
- [x] Test: subsequent files indexed after error ✅

#### Task 6.2.3 - Status Snapshot ✅
- ✅ `src/main.rs` - Added `--status` flag
  - Parse `--status` flag in parse_args()
  - Added `run_status()` function that opens graph and counts entities
  - Added public methods to CodeGraph:
    - `count_files()` → Result<usize>
    - `count_symbols()` → Result<usize>
    - `count_references()` → Result<usize>
  - Output format: "files: {n}\nsymbols: {n}\nreferences: {n}"
  - Exits immediately after printing (no watching)
- ✅ `tests/status_tests.rs` (58 LOC, within 250 LOC limit)
  - Indexes file with 2 functions and 1 reference
  - Runs `magellan watch --status`
  - Asserts output contains correct counts
**Verification:**
- [x] Test: --status flag → prints correct counts and exits ✅
- [x] Test: counts match actual graph state ✅

**Overall Phase 6.2 Verification:**
- [x] cargo test --test cli_smoke_tests -- --nocapture ✅
- [x] cargo test --test signal_tests -- --nocapture ✅
- [x] cargo test --test error_tests -- --nocapture ✅
- [x] cargo test --test status_tests -- --nocapture ✅
- [x] cargo test -- --nocapture ✅ (all 37 tests pass)
- [x] cargo check passes ✅
- [x] All files under 300 LOC limit ✅

**Test Results:**
```
running 37 tests (5 watcher + 12 parser + 5 graph + 4 reference + 4 indexer + 1 CLI + 2 schema + 1 signal + 1 error + 1 status + 1 graph_schema)
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Implementation Notes:**
- Signal handling uses signal-hook crate (allowed per task requirements)
- Changed event loop from blocking `recv_event()` to polling `try_recv_event()` to check shutdown flag
- 100ms sleep in event loop balances responsiveness with CPU usage
- Error handling uses `match` instead of `?` to avoid early returns
- Status counting uses backend.entity_ids() and filters by node.kind
- All new code follows existing patterns and conventions

---

### Task 7: Query API
**Status:** Pending
**Description:** Implement read queries for graph data
**Deliverables:**
- Extend `src/graph.rs` with query operations:
  - `symbols_in_file(path) → Vec<Symbol>`
  - `references_of(symbol_name) → Vec<Symbol>`
  - `impacted_files(symbol_name) → Vec<File>`
  - `symbol_definition(symbol_name) → Option<(File, Symbol)>`
**Verification:**
- [ ] Test: symbols_in_file → returns all symbols in file
- [ ] Test: references_of → returns all symbols referencing target
- [ ] Test: impacted_files → returns all files containing references
- [ ] Test: symbol_definition → returns file location of symbol

---

### Task 8: Smoke Test (End-to-End)
**Status:** Pending
**Description:** Prove round-trip: file → symbol → reference → impacted file
**Deliverables:**
- `tests/smoke.rs`
- Integration test:
  1. Create temp directory with sample Rust files
  2. Run Magellan watcher
  3. Modify file to add function call
  4. Query: symbols_in_file → new function found
  5. Query: references_of → reference extracted
  6. Query: impacted_files → both files linked
**Verification:**
- [ ] All assertions pass
- [ ] No data loss
- [ ] Deterministic: same input → same output

---

## Completion Criteria

**Phase 0 complete when:**
- [x] CONTRACT.md exists and is frozen
- [x] TODO.md exists with all tasks defined
- [x] No Rust code written yet (waiting approval)

**Phase 1 complete when:**
- [x] Task 1 (Project Setup) complete
- [x] Task 2 (Filesystem Watching) complete
- [x] `cargo test` passes (6/6 tests pass)
- [x] `cargo check` passes (no warnings except unused config param)
- [x] No scope creep (only notify crate used, no sqlitegraph/tree-sitter yet)

**Phase 3 complete when:**
- [x] Task 5 (Sqlitegraph Schema) complete
- [x] `cargo test` passes (28/28 tests pass: 4 unit + 12 parser + 5 watcher + 5 graph + 2 schema)
- [x] `cargo check` passes
- [x] Opaque JSON payloads used (no per-property access)
- [x] Idempotent re-index verified
- [x] All files under 300 LOC limit

**Project complete when:**
- [ ] All 8 tasks completed
- [x] `cargo test` passes (28/28 tests pass so far: 4 unit + 12 parser + 5 watcher + 5 graph + 2 schema)
- [x] `cargo check` passes
- [ ] Smoke test proves end-to-end functionality
- [ ] No scope creep detected

---

## Change Log

**2025-12-23:**

**Phase 0:**
- Created CONTRACT.md
- Created TODO.md with 8 tasks
- Frozen Phase 0 scope

**Phase 1:**
- Created Cargo.toml with dependencies
- Implemented src/watcher.rs (156 LOC)
- Implemented src/lib.rs (public API)
- Created tests/watcher_tests.rs (191 LOC)
- All tests passing:
  - test_file_create_event ✅
  - test_file_modify_event ✅
  - test_file_delete_event ✅
  - test_debounce_rapid_changes ✅
  - test_watch_temp_directory ✅
  - test_event_type_serialization ✅
- `cargo test` result: ok. 6 passed; 0 failed
- `cargo check` result: Finished `dev` profile
- Phase 1 complete

**Phase 2:**
- Added tree-sitter dependencies (tree-sitter 0.22, tree-sitter-rust 0.21)
- Implemented src/ingest.rs (184 LOC)
- Created tests/parser_tests.rs (209 LOC)
- Updated src/lib.rs to export ingest module
- All tests passing (19 total):
  - test_symbol_kind_serialization ✅
  - test_event_type_serialization ✅
  - test_empty_file ✅
  - test_syntax_error_file ✅
  - test_single_function ✅
  - test_struct_definition ✅
  - test_enum_definition ✅
  - test_trait_definition ✅
  - test_module_declaration ✅
  - test_impl_block ✅
  - test_multiple_symbols ✅
  - test_nested_modules ✅
  - test_byte_spans ✅
  - test_pure_function_same_input ✅
- `cargo test` result: ok. 19 passed; 0 failed
- `cargo check` result: Finished `dev` profile
- Phase 2 complete
- Rust grammar nodes documented in TODO.md
- Next: Phase 3 (Sqlitegraph Schema)

**Phase 3:**
- Added sqlitegraph dependencies (path: ../sqlitegraph/sqlitegraph, sha2 0.10, hex 0.4)
- Implemented src/schema.rs (63 LOC) - node labels and edge type constants
- Implemented src/graph.rs (381 LOC):
  - FileNode {path, hash} and SymbolNode {name, kind, byte_start, byte_end} as opaque JSON payloads
  - CodeGraph::open() creates SqliteGraphBackend from SqliteGraph
  - index_file() computes SHA-256, upserts File node, deletes old symbols, inserts new symbols
  - delete_file() removes file node and all symbols (cascade)
  - symbols_in_file() queries DEFINES edges to get all symbols
  - In-memory HashMap<String, NodeId> for fast file lookups
  - rebuild_file_index() scans all entities using backend.entity_ids()
- Created tests/graph_persist.rs (153 LOC) with 5 integration tests
- Updated src/lib.rs to export graph module and CodeGraph
- All tests passing (28 total):
  - Previous 19 tests ✅
  - test_round_trip_symbols_in_file ✅
  - test_idempotent_reindex ✅
  - test_delete_file_cleanup ✅
  - test_multiple_files_independent ✅
  - test_symbol_fact_persistence ✅
  - test_schema_constants ✅
  - test_hash_computation ✅
- `cargo test` result: ok. 28 passed; 0 failed
- `cargo check` result: Finished `dev` profile
- Phase 3 complete
- Key sqlitegraph constraints documented:
  - Opaque serde_json::Value payloads (no per-property access)
  - SqliteGraphBackend.entity_ids() is public (all_entity_ids is private)
  - Must import GraphBackend trait to use its methods
  - NeighborQuery has edge_type field, no node_filter field
- Next: Phase 4 (Reference Extraction)

**Phase 4:**
- Created src/references.rs (171 LOC):
  - ReferenceFact struct with file_path, referenced_symbol, byte_start, byte_end
  - ReferenceExtractor with extract_references() method
  - Parser::extract_references() extension for convenience
  - Uses tree-sitter nodes: identifier, scoped_identifier
  - For scoped_identifier (e.g., a::foo()), extracts final component
  - Does NOT recurse into scoped_identifier children (prevents duplicate extraction)
  - Checks reference position: must be AFTER symbol's defining span
- Extended src/graph.rs (now 523 LOC - exceeds 300 LOC limit, needs refactoring):
  - Added ReferenceNode {file, byte_start, byte_end} payload
  - index_references() parses symbols, extracts references, inserts Reference nodes, creates REFERENCES edges
  - references_to_symbol() queries incoming REFERENCES edges
  - Follows Phase 3 patterns exactly for persistence
- Created tests/references_tests.rs (153 LOC) with 4 integration tests:
  - test_extract_reference_to_function ✅
  - test_exclude_references_within_defining_span ✅
  - test_persist_and_query_references ✅
  - test_scoped_identifier_reference ✅
- Updated src/lib.rs to export references module and ReferenceFact
- All tests passing (32 total):
  - Previous 28 tests ✅
  - All 4 reference tests ✅
- `cargo test --test references_tests` result: ok. 4 passed; 0 failed
- `cargo test` result: ok. 32 passed; 0 failed
- `cargo check` result: Finished `dev` profile
- Phase 4 complete
- ⚠️  TECHNICAL DEBT: src/graph.rs exceeds 300 LOC limit (523 LOC)
- Tree-sitter node kinds used: identifier, scoped_identifier
- Next: Phase 5 (Update-on-Change Logic) or refactoring src/graph.rs to meet LOC limit

**Phase 4.1:**
- Refactored monolithic src/graph.rs (523 LOC) into modular structure
- src/graph/mod.rs (249 LOC) - public CodeGraph API
- src/graph/schema.rs (29 LOC) - labels, edge types, helper structs
- src/graph/files.rs (161 LOC) - file node operations
- src/graph/symbols.rs (107 LOC) - symbol node operations
- src/graph/references.rs (138 LOC) - reference node operations
- All files ≤ 300 LOC
- Phase 4.1 complete

**Phase 5:**
- Implemented src/indexer.rs (125 LOC) - coordinator with run_indexer and run_indexer_n
- Tests use run_indexer_n with bounded event counts for reliability
- Graceful ENOENT handling: skips unreadable files
- Phase 5 complete

**Phase 6.1:**
- Implemented src/main.rs (162 LOC) - runnable magellan binary
- Command: `magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>]`
- Uses std::env::args for parsing (no clap)
- Filters to .rs files only to avoid indexing database files
- Logging: "{event_type} {path} symbols={n} refs={m}"
- tests/cli_smoke_tests.rs (72 LOC) - spawns binary and verifies output
- Phase 6.1 complete

**Phase 6.2:**
**Task 6.2.1 - Graceful Shutdown:**
- Added signal-hook = "0.3" dependency
- Signal handling for SIGINT/SIGTERM using signal-hook::iterator::Signals
- Arc<AtomicBool> for shutdown flag
- Changed from blocking recv_event() to polling try_recv_event() with 100ms sleep
- Prints "SHUTDOWN" before exit
- tests/signal_tests.rs (81 LOC) - sends SIGTERM and verifies SHUTDOWN

**Task 6.2.2 - Deterministic Error Reporting:**
- Enhanced error handling in event loop
- File read errors: log "ERROR {path} {error}" and continue
- Index errors: log "ERROR {path} {error}" and continue
- No retries, no backoff, no panic
- tests/error_tests.rs (86 LOC) - creates unreadable file, verifies ERROR logged

**Task 6.2.3 - Status Snapshot:**
- Added --status flag to parse_args()
- run_status() function opens graph and counts entities
- Added CodeGraph methods: count_files(), count_symbols(), count_references()
- Output: "files: {n}\nsymbols: {n}\nreferences: {n}"
- tests/status_tests.rs (58 LOC) - runs --status and verifies counts

**Phase 6.2 Complete:**
- All 37 tests passing (5 watcher + 12 parser + 5 graph + 4 reference + 4 indexer + 1 CLI + 2 schema + 1 signal + 1 error + 1 status + 1 graph_schema)
- cargo check passes
- All files under 300 LOC limit

**Current State (v0.3.0):**
- Magellan is a dumb, deterministic codebase mapping tool
- Multi-language support: Rust, Python, C, C++, Java, JavaScript, TypeScript
- Magellan does NOT:
  - Perform semantic analysis
  - Build LSP servers or language features
  - Use async runtimes
  - Use background thread pools
  - Use config files
  - Provide web APIs or network services
  - Cross-file symbol resolution (planned for future)
- Magellan DOES:
  - Watch directories for source file changes (7 languages)
  - Extract AST-level facts (functions, classes, methods, enums, modules, etc.)
  - Extract symbol references (calls and type references)
  - Build call graphs for all 7 languages
  - Persist facts to sqlitegraph database
  - Index files on Create/Modify events
  - Delete files on Delete events
  - Handle permission errors gracefully
  - Respond to SIGINT/SIGTERM for clean shutdown
  - Report status via `status` command

---

### Database Freshness Safeguards (2025-12-28)
**Status:** ✅ Complete
**Description:** Add timestamp tracking and verification to detect stale databases
**Deliverables:**

#### Task 1: Timestamp Tracking ✅
- ✅ Extended FileNode schema with `last_indexed_at: i64` and `last_modified: i64`
- ✅ src/graph/files.rs - Added `now()`, `get_file_mtime()` helpers
- ✅ Modified `find_or_create_file_node()` to capture timestamps
- ✅ tests/timestamp_tests.rs (4 tests) - TDD approach
**Verification:**
- [x] test_file_node_includes_timestamps ✅
- [x] test_timestamps_update_on_reindex ✅
- [x] test_last_modified_captured_from_filesystem ✅
- [x] test_file_node_json_serialization ✅

#### Task 2: magellan verify Command ✅
- ✅ src/verify.rs (175 LOC) - verify_graph() function
- ✅ VerifyReport {missing, new, modified, stale}
- ✅ src/main.rs - Added Verify command variant
- ✅ src/verify_cmd.rs (51 LOC) - CLI handler
- ✅ tests/verify_tests.rs (5 tests) - TDD approach
- ✅ Added `all_file_nodes()` public method to CodeGraph
**Verification:**
- [x] test_verify_clean_database ✅
- [x] test_verify_detects_deleted_files ✅
- [x] test_verify_detects_new_files ✅
- [x] test_verify_detects_modified_files ✅
- [x] test_verify_detects_stale_files ✅
**Test Results:**
```
running 5 tests
test result: ok. 5 passed; 0 failed
```

#### Task 3: Pre-Query Staleness Warning ✅
- ✅ src/graph/freshness.rs (151 LOC) - freshness checking module
- ✅ FreshnessStatus struct with `is_stale()`, `minutes_since_index()`, `warning_message()`
- ✅ `check_freshness(graph: &CodeGraph) -> Result<FreshnessStatus>`
- ✅ STALE_THRESHOLD_SECS constant (300 seconds = 5 minutes)
- ✅ Added `all_file_nodes_readonly()` for read-only access
- ✅ tests/freshness_tests.rs (5 tests) - TDD approach
- ✅ Re-exported via src/graph/mod.rs
**Verification:**
- [x] test_fresh_database_no_warning ✅
- [x] test_stale_database_produces_warning ✅
- [x] test_empty_database_no_warning ✅
- [x] test_warning_includes_time_difference ✅
- [x] test_freshness_threshold_constant ✅
**Test Results:**
```
running 5 tests
test result: ok. 5 passed; 0 failed
```

**Final Verification:**
- [x] All 80 tests pass (7 freshness module + 5 freshness_tests + 68 existing)
- [x] cargo check passes (zero warnings in new code)
- [x] All files under 300 LOC:
  - freshness.rs: 151 LOC
  - files.rs: 240 LOC
  - mod.rs: 286 LOC
  - verify.rs: 175 LOC
  - verify_cmd.rs: 51 LOC
- [x] Binary built and installed to `/home/feanor/.local/bin/magellan`

**Files Modified:**
- src/graph/schema.rs - Added timestamp fields to FileNode
- src/graph/files.rs - Timestamp capture helpers, read-only API
- src/graph/mod.rs - Added freshness module, re-exports
- src/verify.rs - NEW (175 LOC)
- src/verify_cmd.rs - NEW (51 LOC)
- src/main.rs - Added Verify command
- src/lib.rs - Exported verify module
- tests/timestamp_tests.rs - NEW (166 LOC)
- tests/verify_tests.rs - NEW (158 LOC)
- tests/freshness_tests.rs - NEW (160 LOC)
- docs/DATABASE_FRESHNESS_PLAN.md - NEW (396 LOC)

**Plan:** docs/DATABASE_FRESHNESS_PLAN.md - Complete design document

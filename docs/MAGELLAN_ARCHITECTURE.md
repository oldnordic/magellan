# Magellan Architecture & Integration Guide

**Status:** Reference Document
**Purpose:** Design and integration guide for Magellan
**Date:** 2026-04-21
**Magellan Version:** 3.1.6+

---

## Overview

Magellan is a **deterministic, idempotent codebase mapping tool** that:

1. **Observes files** - Detects source code changes
2. **Extracts symbols** - Uses tree-sitter parsers for multi-language support
3. **Persists facts** - Stores nodes and edges in a graph database
4. **Enables queries** - Provides symbol/reference/call graph lookups

**Key Principle:** Magellan is "dumb" - it records facts, doesn't interpret behavior.

---

## Three Backend Architecture

Magellan supports **three distinct storage backends**:

### Backend Comparison

| Backend | Feature | File Ext | Status | Performance |
|---------|---------|----------|--------|-------------|
| **SQLite** (default) | SQL-based storage | `.db` | ✅ Complete | Baseline |
| **Native V3** | Binary KV store | `.v3` | ✅ Complete | 10-20x faster |
| **Geometric** | 4D spatiotemporal indexing | `.geo` | ✅ Complete | Experimental |

**Important:** Database files are **not** compatible between backends.

### SQLite Backend (Default)

**Status:** ✅ Feature-complete, stable, production-ready

Uses SQL queries on relational tables via sqlitegraph:

```
graph_entities ──┐
graph_edges ─────┼──► SQLite database (.db)
graph_labels ────┤
code_chunks ─────┤
ast_nodes ───────┘
```

### Native V3 Backend

**Status:** ✅ Complete, recommended for production

- High-performance native binary format
- KV-based storage (no SQLite dependency)
- Unlimited capacity
- B+Tree clustered adjacency storage

Build with:
```bash
cargo build --release --features native-v3
```

### Geometric Backend

**Status:** 4D spatiotemporal features complete

The geometric backend uses **GeoGraphDB** for 4D spatiotemporal indexing of CFG blocks:

#### 4D Spatial Mapping

| Coordinate | CFG Property | Meaning |
|------------|--------------|---------|
| X | Dominator Depth | How deep in dominator tree |
| Y | Loop Nesting | How many loops deep |
| Z | Branch Distance | Shortest path from entry to branch |
| T | Git Temporal | Commit timestamp for evolution tracking |

**Performance:** O(log n) path queries instead of O(2^n) enumeration

#### 4D Coordinate Storage

CFG blocks are stored with 4D coordinates in both SQLite and geometric backends:

```rust
pub struct CfgBlock {
    pub id: u64,
    pub kind: BlockKind,
    pub coord_x: i32,  // Dominator depth
    pub coord_y: i32,  // Loop nesting
    pub coord_z: i32,  // Branch distance
    pub coord_t: i64,  // Git timestamp
    pub source_location: String,
}
```

**What's implemented:**
- Dominator depth computation via Lengauer-Tarjan algorithm
- Loop nesting detection via Tarjan's SCC algorithm
- Branch distance from entry block
- Git temporal coordinate for commit-aware queries
- Range queries: `--depth-range-x 0-5 --depth-range-y 0-3 --depth-range-z 0-10`

Build with:
```bash
# 4D spatial features work
cargo build --release --features geometric-backend

# Standalone CLI
magellan-geometric create --db code.geo
magellan-geometric index --root . --db code.geo
```

---

## Backend Capability Model (v3.1.6+)

### Capability Detection

Magellan 3.1.6+ introduces a **runtime capability model** that enables:

1. **Backend-aware help/usage messaging** - Commands show what they support
2. **Command validation** - Early error when backend lacks required capability
3. **Build feature detection** - `--backends` flag shows compiled-in backends
4. **Operational status reporting** - `status` shows backend type

### Capability Structure

Located in `src/capabilities.rs`:

```rust
pub enum BackendType {
    SQLite,      // .db files (default)
    Geometric,   // .geo files (requires geometric-backend feature)
    NativeV3,    // .v3 files (requires native-v3 feature)
}

pub struct BackendCapabilities {
    // Core capabilities
    pub supports_symbol_queries: bool,
    pub supports_call_graph: bool,
    pub supports_cfg_analysis: bool,
    pub supports_chunks: bool,
    pub supports_cycles: bool,
    pub supports_paths: bool,
    pub supports_slice: bool,
    pub supports_vacuum_maintenance: bool,
    pub supports_dead_code: bool,
    pub supports_reachability: bool,
    pub supports_export: bool,
    pub supports_ast: bool,
    pub supports_labels: bool,
    // ... metadata fields
}
```

### Backend Detection by File Extension

```rust
BackendType::from_extension(Some("db"))   // => Some(SQLite)
BackendType::from_extension(Some("geo"))  // => Some(Geometric) if built, None otherwise
BackendType::from_extension(Some("v3"))   // => Some(NativeV3)
BackendType::from_extension(Some("xyz"))  // => Some(SQLite) - default fallback
BackendType::from_extension(None)         // => Some(SQLite) - default fallback
```

**Important:** Unknown extensions default to SQLite. This ensures tool compatibility
even when users specify arbitrary filenames.

### Capability Matrix

| Capability | SQLite | Geometric | Native V3 |
|------------|--------|-----------|-----------|
| Symbol queries | ✅ | ✅ | ✅ |
| Call graph | ✅ | ✅ | ✅ |
| CFG analysis | ✅ | ✅ | ✅ |
| Chunks | ✅ | ✅ | ✅ |
| Cycles | ✅ | ✅ | ✅ |
| Paths (enumeration) | ❌ | ✅ | ❌ |
| Slice | ✅ | ✅ | ✅ |
| AST queries | ✅ | ❌ | ✅ |
| Labels | ✅ | ❌ | ✅ |
| Vacuum/maintenance | ✅ | ✅ | ✅ |
| Dead code | ✅ | ✅ | ✅ |
| Reachability | ✅ | ✅ | ✅ |
| Export | ✅ | ✅ | ✅ |

### Command Routing

Commands are validated against backend capabilities before execution:

```rust
// src/capabilities.rs
pub fn validate_command(
    command: &str,
    backend_caps: &BackendCapabilities,
) -> Result<(), CommandValidationError> {
    // Check if command is supported by this backend
    // Return error if capability missing
}
```

**Example validation:**
- `ast` command → requires `supports_ast` → fails on Geometric
- `paths` command → requires `supports_paths` → fails on SQLite/Native V3
- `label` command → requires `supports_labels` → fails on Geometric
- `find` command → requires `supports_symbol_queries` → works on all backends

### Checking Compiled Backends

```bash
# Show available backends
magellan --backends

# Output example:
# Backend      | Ext | Built | Feature         | Capabilities
# -------------|-----|-------|-----------------|------------------
# SQLite       | db  | Yes   | sqlite-backend  | symbol queries, call graph, CFG analysis...
# Geometric    | geo | Yes   | geometric-backend| symbol queries, call graph, CFG analysis, path enumeration...
# Native V3    | v3  | No    | native-v3       | Not built (requires --features native-v3)

# Version shows compiled backends
magellan --version
# magellan 3.1.6+ (abc123 2026-04-21) rustc 1.75.0 backends: sqlite,geometric
```

---

## Parser Pool Architecture

Magellan uses a **thread-local parser pool** to eliminate per-file parser allocation overhead:

### Thread-Local Parser Reuse

Located in `src/ingest/pool.rs`:

```rust
pub fn with_parser<F, R>(lang: Language, f: F) -> R
where
    F: FnOnce(&mut Parser) -> R,
{
    // Get or create thread-local parser for this language
    let parser = PARSER_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        pool.get_or_create(lang)
    });
    f(parser)
}

pub fn with_parser_opt<F, R>(lang: Option<Language>, f: F) -> R
where
    F: FnOnce(Option<&mut Parser>) -> R,
{
    match lang {
        Some(l) => with_parser(l, |p| f(Some(p))),
        None => f(None),
    }
}
```

### Supported Languages

| Language | Parser | File Extensions |
|----------|--------|-----------------|
| Rust | tree-sitter-rust | `.rs` |
| Python | tree-sitter-python | `.py` |
| C | tree-sitter-c | `.c`, `.h` |
| C++ | tree-sitter-cpp | `.cpp`, `.cc`, `.hpp` |
| Java | tree-sitter-java | `.java` |
| JavaScript | tree-sitter-javascript | `.js`, `.jsx` |
| TypeScript | tree-sitter-typescript | `.ts`, `.tsx` |

### Lazy Initialization

Parsers are created **on first use per thread**, not at startup:

```rust
thread_local! {
    static PARSER_POOL: RefCell<ParserPool> = RefCell::new(ParserPool::new());
}

impl ParserPool {
    fn get_or_create(&mut self, lang: Language) -> &mut Parser {
        self.parsers.entry(lang).or_insert_with(|| {
            let mut parser = Parser::new();
            parser.set_language(lang.into()).unwrap();
            parser
        })
    }
}
```

**Benefits:**
- No parser allocation per file (major bottleneck eliminated)
- Thread-safe: each thread has its own parser set
- Supports up to 7 languages simultaneously per thread
- Zero-cost after first use in a thread

---

## Parse-Once Optimization

Magellan parses each source file **exactly once** during indexing, sharing the `tree_sitter::Tree` across all extraction phases:

### Single-Parse Pipeline

```rust
pub fn index_file(
    graph: &mut CodeGraph,
    path: &Path,
    content: &str,
    tree: &Tree,           // <-- Parsed once, shared
    lang: Language,
) -> Result<Vec<SymbolNode>> {
    let root = tree.root_node();
    
    // Phase 1: Extract AST nodes (uses same tree)
    let ast_nodes = extract_ast_nodes(&root);
    
    // Phase 2: Extract imports (uses same tree)
    let imports = extract_imports(&root, lang);
    
    // Phase 3: Extract symbols (uses same tree)
    let symbols = extract_symbols(&root, path, content);
    
    // Phase 4: Extract CFG (uses same tree)
    for func in &symbols {
        if func.kind == SymbolKind::Function {
            index_cfg_with_4d_coordinates_from_node(
                graph, func, &root, content, path,
            )?;
        }
    }
    
    Ok(symbols)
}
```

### What Still Uses the Parser Pool

Some operations still require language-specific parser wrappers:

| Operation | Uses Pool | Reason |
|-----------|-----------|--------|
| `call_ops` | Yes | Needs language-specific call extraction |
| `references` | Yes | Needs language-specific reference patterns |
| `index_file` | No | Parse-once optimization |
| `index_cfg` | No | Reuses tree from `index_file` |

**Performance Impact:** 2-3x faster indexing on large codebases due to eliminated redundant parsing.

---

## CFG Architecture

### Old vs New

| Aspect | Old (`cfg_extractor.rs`) | New (`cfg_edges_extract.rs`) |
|--------|--------------------------|------------------------------|
| Blocks | Simple list | Typed blocks with metadata |
| Edges | None | Typed edges with labels |
| Coordinates | None | 4D (X, Y, Z, T) |
| Storage | SQLite only | SQLite + geometric |

### Edge Types

```rust
pub enum CfgEdgeType {
    Fallthrough,      // Sequential execution
    ConditionalTrue,  // if/match branch taken
    ConditionalFalse, // if/match branch not taken
    Jump,             // break/continue/goto
    BackEdge,         // Loop back edge
    Call,             // Function call
    Return,           // Function return
}
```

### 4D Coordinates

Each CFG block has 4D coordinates computed during extraction:

| Coordinate | Algorithm | Meaning |
|------------|-----------|---------|
| X | Lengauer-Tarjan | Dominator tree depth |
| Y | Tarjan SCC | Loop nesting depth |
| Z | BFS from entry | Shortest path to this block |
| T | Git log | Commit timestamp |

### Delegation Pattern

`cfg_ops.rs` delegates the old `index_cfg_for_function` to the new `index_cfg_with_4d_coordinates_from_node`:

```rust
// src/graph/cfg_ops.rs
pub fn index_cfg_for_function(
    graph: &mut CodeGraph,
    func: &SymbolNode,
    root: &Node,
    content: &str,
    path: &Path,
) -> Result<()> {
    // Delegates to new implementation
    index_cfg_with_4d_coordinates_from_node(
        graph, func, root, content, path
    )
}
```

---

## File Hashing

Magellan uses **xxHash64** for fast content-based change detection:

### Hash Format

| Algorithm | Hash Length | Speed |
|-----------|-------------|-------|
| xxHash64 | 16-char hex | ~10 GB/s |
| SHA-256 (old) | 64-char hex | ~200 MB/s |

### Usage in `files.rs`

```rust
use xxhash_rust::xxh3::xxh3_64;

pub fn compute_file_hash(content: &str) -> String {
    let hash = xxh3_64(content.as_bytes());
    format!("{:016x}", hash)
}
```

The hash is stored in `graph_entities` (FileNode) and used by `reconcile_file_path()` to detect changes without re-reading unchanged files.

---

## Re-Index Semantics (v3.1.6+)

### Reconcile Behavior

The `reconcile_file_path()` method in both `CodeGraph` (SQLite) and `GeometricBackend`
provides **deterministic, idempotent re-indexing**:

### SQLite Reconcile (`src/graph/ops.rs`)

```rust
pub fn reconcile_file_path(
    graph: &mut CodeGraph,
    path: &Path,
    path_key: &str,
) -> Result<ReconcileOutcome>
```

**Algorithm:**

1. **Check file existence:**
   - If file does NOT exist on filesystem → delete all facts, return `Deleted`

2. **Compute content hash:**
   - Read file contents
   - Compute xxHash64 hash (16-char hex)

3. **Check for changes:**
   - Compare with stored hash in `graph_entities` (FileNode)
   - If hash matches → return `Unchanged` (no-op)

4. **Delete old data:**
   - Delete all symbols via DEFINES edges from file
   - Delete all references for those symbols
   - Delete all calls for those symbols
   - Delete all AST nodes for the file
   - Delete all CFG blocks for the file
   - Delete all code chunks for the file

5. **Re-index:**
   - Run `index_file()` to extract symbols (parse-once)
   - Run `index_references()` to extract references
   - Return `Reindexed { symbols, references, calls }`

**Key Invariants:**

- **Symbol counts are stable** across re-index cycles (no inflation)
- **File hash is the source of truth** for freshness
- **Delete-then-insert ensures** no stale data accumulates
- **No partial updates** - transaction either fully commits or fully rolls back

### Geometric Reconcile (`src/graph/geo_index.rs`)

```rust
pub fn reconcile_file_path(
    backend: &mut GeometricBackend,
    path: &Path,
) -> Result<GeoReconcileOutcome>
```

**Algorithm:**

1. **Check file existence:**
   - If file does NOT exist → delete symbols, remove from tracking

2. **Compute content hash:**
   - Read file, compute xxHash64 hash

3. **Check for changes:**
   - Compare with stored hash
   - If unchanged → return `Unchanged`

4. **Delete old data:**
   - Remove symbols from in-memory index
   - Remove function_ids from CFG tracking (marks CFG blocks as stale)
   - Stale CFG blocks are excluded from next save (garbage collection)

5. **Re-index:**
   - Extract symbols via tree-sitter (parse-once)
   - Extract CFG blocks with 4D coordinates if applicable
   - Insert into in-memory structures
   - Return `Reindexed`

**Key Difference from SQLite:**

- CFG blocks are **not immediately deleted** from storage
- They become **stale** and excluded from `cfg_function_ids` tracking
- Vacuum operation physically removes stale blocks

### Re-Index Idempotence

Both backends guarantee idempotence:

```rust
// Running reconcile multiple times on unchanged file produces same result
assert!(matches!(reconcile_file_path(path), Ok(ReconcileOutcome::Unchanged)));
assert!(matches!(reconcile_file_path(path), Ok(ReconcileOutcome::Unchanged)));
assert!(matches!(reconcile_file_path(path), Ok(ReconcileOutcome::Unchanged)));
```

### Churn Test Validation

See `tests/churn_harness_test.rs` for validation that:
- Symbol counts remain constant across 5 re-index cycles
- File counts remain constant
- Database size stabilizes after initial WAL creation
- VACUUM reclaims space after deletion

---

## Vacuum and Maintenance (v3.1.6+)

### SQLite VACUUM

SQLite backend supports standard SQLite VACUUM via `rusqlite`:

```bash
# Not directly exposed via CLI (future enhancement)
# Use sqlite3 directly:
sqlite3 code.db "VACUUM;"
```

**Effects:**
- Rebuilds database file, reclaiming free space
- Resets auto-increment sequences
- Defragments indexes
- Requires ~2x temporary disk space during operation

### Geometric CFG Vacuum

Geometric backend provides `vacuum_cfg()` method in `GeometricBackend`:

```rust
pub fn vacuum_cfg(&self) -> Result<VacuumResult>
```

**Algorithm:**

1. **Get tracked function IDs** - Source of truth for live CFG
2. **Count before state:**
   - Live blocks (blocks for tracked functions)
   - Total blocks (includes stale)
   - Live edges (edges between live blocks)
   - Total edges (includes stale)
3. **Build fresh CFG section:**
   - Iterate tracked function IDs
   - Copy only live blocks to new data structure
   - Copy only live edges
4. **Write to storage:**
   - Open existing `.geo` file
   - Replace CFG section with new data
   - Flush changes
5. **Calculate reclaimed:**
   - `blocks_reclaimed = total_blocks_before - live_blocks_before`
   - `edges_reclaimed = total_edges_before - live_edges_before`
   - `bytes_reclaimed = file_size_before - file_size_after`

**VacuumResult:**

```rust
pub struct VacuumResult {
    pub live_blocks_before: usize,
    pub total_blocks_before: usize,
    pub blocks_reclaimed: usize,
    pub live_edges_before: usize,
    pub total_edges_before: usize,
    pub edges_reclaimed: usize,
    pub bytes_reclaimed: u64,
}
```

**When to Vacuum:**

After multiple re-index cycles on the same files, stale CFG data
accumulates in the in-memory CfgStore. Calling `vacuum_cfg()` rebuilds
the persisted CFG section with only live data.

**Important:** Vacuum does NOT affect symbols or call graph - only CFG
blocks and edges. Stale symbols are handled differently (see Re-Index
Semantics).

---

## Database Schema

### Core Tables (SQLite Backend)

#### `cfg_blocks` - Control Flow Graph Blocks

```sql
CREATE TABLE cfg_blocks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id     INTEGER NOT NULL,
    block_kind      TEXT NOT NULL,
    coord_x         INTEGER NOT NULL,
    coord_y         INTEGER NOT NULL,
    coord_z         INTEGER NOT NULL,
    coord_t         INTEGER,
    source_location TEXT,
    FOREIGN KEY (function_id) REFERENCES graph_entities(id)
);
```

**4D Coordinates:**
| Column | Meaning | Computation |
|--------|---------|-------------|
| `coord_x` | Dominator depth | Lengauer-Tarjan algorithm |
| `coord_y` | Loop nesting | Tarjan SCC detection |
| `coord_z` | Branch distance | BFS from entry block |
| `coord_t` | Git timestamp | `git log -1 --format=%ct` |

#### `cfg_edges` - CFG Block Connections

```sql
CREATE TABLE cfg_edges (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    from_block  INTEGER NOT NULL,
    to_block    INTEGER NOT NULL,
    edge_type   TEXT NOT NULL,
    FOREIGN KEY (from_block) REFERENCES cfg_blocks(id),
    FOREIGN KEY (to_block) REFERENCES cfg_blocks(id)
);
```

**Edge Types:** `Fallthrough`, `ConditionalTrue`, `ConditionalFalse`, `Jump`, `BackEdge`, `Call`, `Return`

#### `graph_entities` - Symbol Nodes

```sql
CREATE TABLE graph_entities (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    kind      TEXT NOT NULL,
    name      TEXT NOT NULL,
    file_path TEXT,
    data      TEXT NOT NULL
);
```

**data payload (SymbolNode JSON):**
```json
{
    "symbol_id": "28e17e99cb937643",
    "fqn": "tests::test_default_config",
    "canonical_fqn": "codemcp::/path/to/file.rs::Function test_default_config",
    "display_fqn": "codemcp::tests::test_default_config",
    "name": "test_default_config",
    "kind": "Function",
    "kind_normalized": "fn",
    "byte_start": 19452,
    "byte_end": 20580,
    "start_line": 627,
    "end_line": 659,
    "start_col": 4,
    "end_col": 5
}
```

#### `graph_edges` - Relationships

```sql
CREATE TABLE graph_edges (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id   INTEGER NOT NULL,
    to_id     INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    data      TEXT NOT NULL
);
```

**Edge Types:**
| edge_type | Meaning |
|-----------|---------|
| REFERENCES | Symbol references another symbol |
| CALLS | Function calls another function |
| DEFINES | File defines a symbol |
| CALLER | Reverse of CALLS |

#### `code_chunks` - Source Code Storage

```sql
CREATE TABLE code_chunks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path     TEXT NOT NULL,
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL,
    content       TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    symbol_name   TEXT,
    symbol_kind   TEXT,
    created_at    INTEGER NOT NULL
);
```

#### `ast_nodes` - AST Node Storage

```sql
CREATE TABLE ast_nodes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id  INTEGER,
    kind       TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end   INTEGER NOT NULL
);
```

---

## CLI Interface

### Command Structure

```bash
magellan <command> [arguments]
```

### Commands

| Command | Purpose | Key Arguments |
|---------|---------|---------------|
| `watch` | Index + watch for changes | `--root`, `--db`, `--debounce-ms` |
| `export` | Export graph data | `--db`, `--format`, `--output` |
| `status` | Database statistics | `--db` |
| `query` | List symbols in file | `--db`, `--file`, `--kind` |
| `find` | Find symbol by name | `--db`, `--name` |
| `refs` | Show callers/callees | `--db`, `--name`, `--direction` |
| `context` | LLM Context API | `--db`, `summary`/`list`/`symbol`/`file` |
| `enrich` | LSP type enrichment | `--db` |
| `files` | List all files | `--db` |
| `doctor` | Self-diagnostics | `--db`, `--fix` |

### Context API (v3.0.0+)

```bash
# Project overview (~50 tokens)
magellan context summary --db code.db --json

# Paginated symbol list
magellan context list --db code.db --kind fn --page 1 --page-size 50 --json

# Symbol detail with call graph
magellan context symbol --db code.db --name main --callers --callees --json

# File-level context
magellan context file --db code.db --path src/main.rs --json
```

### Export Formats

```bash
# JSON export
magellan export --db code.db --format json > graph.json

# LSIF export (for cross-repo navigation)
magellan export --db code.db --format lsif --output project.lsif

# Import external LSIF
magellan import-lsif --db code.db --input dependency.lsif
```

---

## Code Structure

```
magellan/src/
├── main.rs              # CLI entry point
├── lib.rs               # Public API exports
├── common.rs            # Language detection
├── indexer.rs           # Indexing orchestrator
├── cli.rs               # Argument parsing
├── geometric_cmd.rs     # Geometric backend CLI
├── ingest/              # Language parsers
│   ├── mod.rs
│   ├── pool.rs           # Thread-local parser pool
│   ├── detect.rs
│   ├── rust.rs
│   ├── c.rs
│   ├── cpp.rs
│   ├── java.rs
│   ├── python.rs
│   ├── javascript.rs
│   └── typescript.rs
├── graph/               # Database operations
│   ├── mod.rs           # CodeGraph main type
│   ├── schema.rs        # Node payload types
│   ├── ops.rs           # CRUD operations
│   ├── query.rs         # Symbol queries
│   ├── algorithms.rs    # Graph algorithms
│   ├── geometric_backend.rs  # 4D spatiotemporal backend
│   ├── cfg_extractor.rs # Legacy CFG (blocks only)
│   ├── cfg_edges_extract.rs # New CFG (blocks + edges + 4D)
│   ├── cfg_ops.rs       # CFG delegation layer
│   ├── ast_*.rs         # AST modules
│   └── metrics/         # Code metrics
├── context/             # LLM Context API
├── lsp/                 # LSP enrichment
├── lsif/                # LSIF export/import
├── generation/          # Code chunk storage
├── output/              # Output formatting
├── watcher/             # Async file watcher
└── diagnostics/         # Watch event tracking
```

---

## Architecture Flow

```
┌──────────────────────────────────────────────────────────────┐
│                    Indexing Pipeline                        │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐ │
│  │  File   │───▶│ Ingest  │───▶│ Extract │───▶│ Persist │ │
│  │ System  │    │ Parser │    │ Symbols │    │   DB    │ │
│  └─────────┘    └─────────┘    └─────────┘    └─────────┘ │
│                     │                              │        │
│                     ▼                              ▼        │
│              ┌─────────┐                    ┌─────────┐    │
│              │ Detect  │                    │ CodeGraph│    │
│              │Language│                    │ Wrapper │    │
│              └─────────┘                    └─────────┘    │
│                                                     │       │
└─────────────────────────────────────────────────────────────┘
```

---

## Position Conventions

Magellan uses **tree-sitter position conventions**:

| Type | Convention | Example |
|------|------------|---------|
| Lines | 1-indexed | Line 1 is first line |
| Columns | 0-indexed | Column 0 is first character |
| Bytes | 0-indexed | Byte 0 is first byte |

---

## Integration Points

### Same Database Principle

**Extend `codegraph.db`** with new tables (NOT separate database):

**Why same DB?**
- Single source of truth
- JOIN queries work natively
- Atomic updates (symbol + path in ONE transaction)
- Better performance

### Linking via symbol_id

Use the **stable symbol_id** from Magellan's SymbolNode:

```rust
pub struct SymbolNode {
    pub symbol_id: Option<String>,  // xxHash64 of language:fqn:span_id
    // ...
}
```

---

## Design Recommendations

### Schema Extension

Add to Magellan's migration system in `src/graph/db_compat.rs`:

```rust
pub const CUSTOM_SCHEMA_VERSION: i32 = 1;

pub fn ensure_custom_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS custom_table (...)",
        [],
    )?;
    Ok(())
}
```

### Incremental Computation

When Magellan indexes a file:

```rust
pub fn on_file_indexed(codegraph: &CodeGraph, path: &str) -> Result<()> {
    // 1. Get all functions in the file
    let functions = codegraph.symbols_in_file_with_kind(path, Some(SymbolKind::Function))?;

    // 2. For each function, build CFG
    for func in functions {
        let cfg = build_cfg(&func)?;
        codegraph.store_cfg(func.id, cfg)?;
    }

    Ok(())
}
```

---

## Backend Selection Guide

### When to use SQLite Backend

- Compatibility with existing tools
- Debugging (human-readable SQL)
- Small to medium codebases
- Development/testing

### When to use Native V3 Backend

- Production deployments
- Large codebases (10M+ LOC)
- High query throughput
- When performance matters

### When to use Geometric Backend

- CFG analysis with 4D coordinates
- When O(log n) path queries are needed
- Temporal evolution tracking (git-aware)
- Production-ready for spatial/temporal queries

---

## Summary

### Database Decision

**Three backends, choose based on use case:**

| Use Case | Backend | File Extension |
|----------|---------|----------------|
| Default/Compatibility | SQLite | `.db` |
| Production Performance | Native V3 | `.v3` |
| 4D Spatiotemporal Analysis | Geometric | `.geo` |

### Building with Different Backends

```bash
# SQLite (default)
cargo build --release

# Native V3 (recommended for production)
cargo build --release --features native-v3

# Geometric (4D spatiotemporal)
cargo build --release --features geometric-backend
```

### Integration Pattern

```
Magellan (structure) + Analysis Tool (behavior) = Complete Code Intelligence
         ↓                        ↓
    WHERE things are        HOW code behaves
```

---

## References

- Magellan Source: `/home/feanor/Projects/magellan/`
- Magellan CLI: `src/main.rs`
- Graph Schema: `src/graph/schema.rs`
- CodeGraph API: `src/graph/mod.rs`
- Geometric Backend: `src/graph/geometric_backend.rs`
- Geometric Docs: `docs/GEOGRAPHDB_*.md`

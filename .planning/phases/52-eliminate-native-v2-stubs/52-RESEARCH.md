# Phase 52: Eliminate Native-V2 Stubs - Research

**Researched:** 2026-02-08
**Domain:** KV Storage, Native Backend Migration, Schema Design
**Confidence:** HIGH

## Summary

Phase 52 requires replacing 4 SQLite stub implementations with proper KV store storage in native-v2 mode. The stubs currently use temporary SQLite databases or `:memory:` paths that lose data on restart. The solution involves leveraging sqlitegraph's existing KV store infrastructure (already used for symbol indexing in Phase 48) and designing new key patterns for each data type.

**Primary recommendation:** Use the existing KV store infrastructure (`kv_set`, `kv_get`, `kv_delete` methods on `GraphBackend`) with JSON serialization for complex types, following the patterns established in `src/kv/mod.rs` for symbol indexing. Implement new key patterns in `src/kv/keys.rs` and encoding functions in `src/kv/encoding.rs` for each stubbed component.

## User Constraints

No user decisions from `/gsd:discuss-phase` yet. All areas are open for recommendation.

## Standard Stack

### Core

| Component | Version | Purpose | Why Standard |
|-----------|---------|---------|--------------|
| **sqlitegraph** | 1.5.1 | Graph backend with KV store | Provides `GraphBackend::kv_set/get/delete` methods and `KvValue` enum (Bytes, String, Integer, Float, Boolean, JSON) |
| **serde_json** | existing | JSON serialization for complex types | Standard Rust serialization, already in dependencies |
| **rusqlite** | 0.31 | SQLite for migration source | Existing dependency, needed for reading side tables during migration |

### Supporting

| Component | Version | Purpose | When to Use |
|-----------|---------|---------|-------------|
| **tempfile** | existing | Testing KV implementations | Create test databases for unit tests |
| **sha2** | existing | Content hashing | Already used in `CodeChunk::compute_hash()` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| JSON serialization | MessagePack/bincode | MessagePack more compact but adds dependency; JSON is human-readable and already available |
| KV storage | Sidecar SQLite tables | Sidecar tables would require additional file management; KV keeps everything in one database file |

**Installation:** No new dependencies required. All necessary crates already in `Cargo.toml`.

## Architecture Patterns

### Recommended Project Structure

```
src/kv/
├── mod.rs              # Existing: symbol indexing functions
├── keys.rs             # UPDATE: add new key patterns for chunks, metrics, etc.
├── encoding.rs         # UPDATE: add encode/decode for new types
└── metadata.rs         # NEW: optional module for metadata structures

src/generation/
├── mod.rs              # UPDATE: replace ChunkStore::in_memory() with KV-backed impl
└── schema.rs           # KEEP: CodeChunk struct already serializable

src/graph/
├── execution_log.rs    # UPDATE: replace ExecutionLog::disabled() with KV-backed impl
├── metrics/
│   └── mod.rs          # UPDATE: replace MetricsOps::disabled() with KV-backed impl
└── cfg_extractor.rs    # UPDATE: replace stub CFG functions with KV-backed storage

src/migrate_backend_cmd.rs  # UPDATE: add side table migration to KV
```

### Pattern 1: KV Key Namespace Design

**What:** Organize keys by namespace prefix to prevent collisions and enable efficient prefix scans.

**When to use:** For all new KV key patterns.

**Example:**
```rust
// From src/kv/keys.rs - existing pattern
pub fn sym_fqn_key(fqn: &str) -> Vec<u8> {
    format!("sym:fqn:{}", fqn).into_bytes()
}

// Recommended new patterns for Phase 52
pub fn chunk_key(file_path: &str, byte_start: usize, byte_end: usize) -> Vec<u8> {
    format!("chunk:{}:{}:{}", file_path, byte_start, byte_end).into_bytes()
}

pub fn execution_log_key(execution_id: &str) -> Vec<u8> {
    format!("execlog:{}", execution_id).into_bytes()
}

pub fn file_metrics_key(file_path: &str) -> Vec<u8> {
    format!("metrics:file:{}", file_path).into_bytes()
}

pub fn symbol_metrics_key(symbol_id: i64) -> Vec<u8> {
    format!("metrics:symbol:{}", symbol_id).into_bytes()
}

pub fn cfg_blocks_key(function_id: i64) -> Vec<u8> {
    format!("cfg:func:{}", function_id).into_bytes()
}

pub fn ast_nodes_key(file_id: u64) -> Vec<u8> {
    format!("ast:file:{}", file_id).into_bytes()
}
```

**Source:** Verified from `/home/feanor/Projects/magellan/src/kv/keys.rs:35-37`

### Pattern 2: JSON Serialization for Complex Types

**What:** Use `serde_json::to_value()` for complex structs, store as `KvValue::Json`.

**When to use:** For structs with multiple fields (CodeChunk, ExecutionRecord, FileMetrics, etc.).

**Example:**
```rust
use serde_json;
use sqlitegraph::backend::KvValue;
use crate::generation::CodeChunk;

// Store a CodeChunk
fn store_chunk_kv(backend: &dyn GraphBackend, chunk: &CodeChunk) -> Result<()> {
    let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
    let json_value = serde_json::to_value(chunk)?;
    backend.kv_set(key, KvValue::Json(json_value), None)?;
    Ok(())
}

// Retrieve a CodeChunk
fn get_chunk_kv(backend: &dyn GraphBackend, file_path: &str, start: usize, end: usize) -> Result<Option<CodeChunk>> {
    let key = chunk_key(file_path, start, end);
    let snapshot = SnapshotId::current();

    match backend.kv_get(snapshot, &key)? {
        Some(KvValue::Json(value)) => {
            let chunk: CodeChunk = serde_json::from_value(value)?;
            Ok(Some(chunk))
        }
        Some(_) => Err(anyhow::anyhow!("Unexpected KV value type")),
        None => Ok(None),
    }
}
```

**Source:** `KvValue::Json` variant exists in sqlitegraph 1.5.1 (verified from source)

### Pattern 3: Bulk Operations with Prefix Scans

**What:** Use key prefix patterns to retrieve all items for a file or function.

**When to use:** For "get all chunks for file", "get all metrics", etc.

**Example:**
```rust
// Get all chunks for a file using prefix scan (if GraphBackend supports it)
// Alternative: store a list index under a separate key
fn get_chunks_for_file(backend: &dyn GraphBackend, file_path: &str) -> Result<Vec<CodeChunk>> {
    // Approach 1: If kv_scan_prefix is available
    // let prefix = format!("chunk:{}:", file_path).into_bytes();
    // let items = backend.kv_scan_prefix(SnapshotId::current(), &prefix)?;

    // Approach 2: Use a secondary index (more reliable)
    let index_key = format!("chunk:index:{}", file_path).into_bytes();
    let snapshot = SnapshotId::current();

    match backend.kv_get(snapshot, &index_key)? {
        Some(KvValue::Bytes(encoded)) => {
            // Decode list of (start, end) tuples
            let mut chunks = Vec::new();
            // ... decode and fetch each chunk
            Ok(chunks)
        }
        _ => Ok(Vec::new()),
    }
}
```

### Anti-Patterns to Avoid

- **Using separate SQLite files:** Don't create temporary databases like the current stub does. It breaks ACID properties and complicates file management.
- **Binary serialization for complex structs:** Don't use bincode or custom binary formats. JSON is debuggable and human-readable.
- **Ignoring migration:** Don't implement KV storage without updating `migrate_backend_cmd.rs` to migrate existing SQLite data.
- **Tight coupling to backend type:** Don't assume `backend` is always `NativeGraphBackend`. Use the `GraphBackend` trait methods.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| KV storage | Custom key-value file format | sqlitegraph's KV store | ACID transactions, crash recovery, single file |
| Serialization | Manual byte encoding | serde_json + KvValue::Json | Handles complex types, error handling |
| Encoding for lists | Custom length-prefixed encoding | Extend `encoding.rs` patterns | Consistent with existing codebase |
| Migration logic | Manual SQL dumping | Extend `migrate_side_tables()` | Already handles row-by-row copy |

**Key insight:** The KV store is already integrated with the WAL transaction system. All writes participate in the same transaction as graph updates, ensuring consistency.

## Common Pitfalls

### Pitfall 1: Ignoring Existing Data During Migration

**What goes wrong:** After implementing KV storage, existing SQLite databases lose their side table data.

**Why it happens:** `migrate_side_tables()` only copies tables to the target database schema, not to KV storage.

**How to avoid:** Extend `migrate_side_tables()` to read from SQLite tables and write to KV store before dropping old tables.

**Warning signs:** Migration succeeds but `magellan get` returns no data, metrics queries return empty results.

### Pitfall 2: Inconsistent Key Encoding

**What goes wrong:** Keys stored with one encoding can't be retrieved with another (e.g., trailing newlines, different separator characters).

**Why it happens:** Multiple developers implement key patterns independently without centralized functions.

**How to avoid:** Always use centralized key construction functions in `src/kv/keys.rs`. Never inline key format strings.

**Warning signs:** "Key not found" errors for data that was just stored, tests fail intermittently.

### Pitfall 3: Forgetting Index Updates

**What goes wrong:** Secondary indexes (like `chunk:index:{file_path}` lists) become stale when data is added/removed.

**Why it happens:** Only the primary KV entry is updated, not the index.

**How to avoid:** Wrap all KV writes in functions that update both primary and index entries atomically.

**Warning signs:** Queries return stale data, count operations return wrong values.

### Pitfall 4: Breaking Snapshot Isolation

**What goes wrong:** Reads see partially written data during concurrent operations.

**Why it happens:** Not passing `SnapshotId::current()` to `kv_get()` calls.

**How to avoid:** Always pass a snapshot ID to `kv_get()` to ensure consistent reads.

**Warning signs:** Tests fail under concurrent load, data inconsistency during watch mode.

## Code Examples

### Example 1: ChunkStore KV Implementation

```rust
// From src/generation/mod.rs - replacing ChunkStore::in_memory()

#[cfg(feature = "native-v2")]
impl ChunkStore {
    /// Create a KV-backed ChunkStore for native-v2 mode
    pub fn with_kv(backend: Rc<dyn GraphBackend>) -> Self {
        Self {
            conn_source: ChunkStoreConnection::Kv(backend),
        }
    }

    /// Store a code chunk using KV store
    pub fn store_chunk_kv(&self, chunk: &CodeChunk) -> Result<i64> {
        if let ChunkStoreConnection::Kv(ref backend) = self.conn_source {
            use crate::kv::keys::chunk_key;
            use sqlitegraph::backend::KvValue;

            let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
            let json_value = serde_json::to_value(chunk)?;
            backend.kv_set(key, KvValue::Json(json_value), None)?;

            // Update file index
            let index_key = format!("chunk:index:{}", chunk.file_path).into_bytes();
            // ... append to existing index

            Ok(0) // KV doesn't assign IDs, use 0 or compute hash-based ID
        }
    }
}
```

### Example 2: ExecutionLog KV Implementation

```rust
// From src/graph/execution_log.rs - replacing ExecutionLog::disabled()

#[cfg(feature = "native-v2")]
impl ExecutionLog {
    /// Create a KV-backed ExecutionLog
    pub fn with_kv(backend: Rc<dyn GraphBackend>) -> Self {
        Self {
            db_path: PathBuf::new(), // Not used in KV mode
            backend: Some(backend),
        }
    }

    pub fn start_execution_kv(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64> {
        if let Some(ref backend) = self.backend {
            use crate::kv::keys::execution_log_key;
            use sqlitegraph::backend::KvValue;

            let record = ExecutionRecord {
                id: 0,
                execution_id: execution_id.to_string(),
                tool_version: tool_version.to_string(),
                args: serde_json::to_string(args)?,
                root: root.map(|s| s.to_string()),
                db_path: db_path.to_string(),
                started_at: now(),
                finished_at: None,
                duration_ms: None,
                outcome: "running".to_string(),
                error_message: None,
                files_indexed: 0,
                symbols_indexed: 0,
                references_indexed: 0,
            };

            let key = execution_log_key(execution_id);
            let json_value = serde_json::to_value(&record)?;
            backend.kv_set(key, KvValue::Json(json_value), None)?;

            Ok(0)
        }
    }
}
```

### Example 3: Migration with KV Support

```rust
// From src/migrate_backend_cmd.rs - extending migrate_side_tables()

pub fn migrate_side_tables_to_kv(
    source_db: &Path,
    target_backend: &Rc<dyn GraphBackend>,
) -> Result<bool> {
    use rusqlite::Connection;

    let source_conn = Connection::open(source_db)?;

    // Migrate code_chunks to KV
    let mut stmt = source_conn.prepare("SELECT * FROM code_chunks")?;
    let rows = stmt.query_map([], |row| {
        Ok(CodeChunk {
            id: Some(row.get(0)?),
            file_path: row.get(1)?,
            byte_start: row.get(2)?,
            byte_end: row.get(3)?,
            content: row.get(4)?,
            content_hash: row.get(5)?,
            symbol_name: row.get(6)?,
            symbol_kind: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;

    for row in rows {
        let chunk = row?;
        let key = chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end);
        let json_value = serde_json::to_value(&chunk)?;
        target_backend.kv_set(key, KvValue::Json(json_value), None)?;
    }

    // Similar for execution_log, file_metrics, symbol_metrics, cfg_blocks, ast_nodes

    Ok(true)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SQLite tables for all data | KV store for metadata (native-v2) | Phase 48 (symbol indexing) | O(1) lookups for symbols |
| Temporary SQLite for stubs | **NEEDS IMPLEMENTATION** | Phase 52 | Full feature parity in native-v2 |

**Deprecated/outdated:**
- `ChunkStore::in_memory()`: Creates temp SQLite DB that gets deleted
- `ExecutionLog::disabled()`: Uses `:memory:` path, all writes lost
- `MetricsOps::disabled()`: Uses `:memory:` path, all writes lost
- CFG stub returning empty vectors: No data persistence

## Open Questions

1. **Should we keep SQLite tables alongside KV during transition?**
   - What we know: Migration currently copies SQLite tables to target schema
   - What's unclear: Whether to maintain dual-write (SQLite + KV) during deprecation period
   - Recommendation: Implement KV-only for native-v2 mode, use migration to convert existing data

2. **How to handle bulk operations efficiently?**
   - What we know: Individual `kv_set()` calls participate in WAL transaction
   - What's unclear: Whether batch/bulk API exists in sqlitegraph
   - Recommendation: Use individual calls within single transaction (acceptable performance)

3. **Secondary index maintenance strategy?**
   - What we know: Need "get all chunks for file" queries
   - What's unclear: Whether to use prefix scans or separate index keys
   - Recommendation: Use separate index keys (e.g., `chunk:index:{file_path}`) storing encoded Vec<(start, end)>

4. **ID assignment for KV-stored entities?**
   - What we know: SQLite assigns auto-increment IDs, KV does not
   - What's unclear: Whether to use hash-based IDs or sequence numbers
   - Recommendation: Use hash-based IDs (content_hash for chunks, execution_id for logs) to avoid global state

## Exact Schema of Each Stubbed Component

### 1. ChunkStore Schema

**SQLite schema** (from `src/generation/mod.rs:94-108`):
```sql
CREATE TABLE code_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    symbol_name TEXT,
    symbol_kind TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(file_path, byte_start, byte_end)
);
```

**Rust type** (`src/generation/schema.rs:13-41`):
```rust
pub struct CodeChunk {
    pub id: Option<i64>,
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub content: String,
    pub content_hash: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub created_at: i64,
}
```

**Indexes:**
- `idx_chunks_file_path` on (file_path)
- `idx_chunks_symbol_name` on (symbol_name)
- `idx_chunks_content_hash` on (content_hash)

**Recommended KV key patterns:**
- `chunk:{file_path}:{start}:{end}` → CodeChunk (JSON)
- `chunk:index:{file_path}` → Vec<(start, end)> (encoded bytes)
- `chunk:symbol:{symbol_name}` → Vec<file_path:start:end> (encoded bytes)

### 2. ExecutionLog Schema

**SQLite schema** (from `src/graph/execution_log.rs:62-78`):
```sql
CREATE TABLE execution_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_id TEXT NOT NULL UNIQUE,
    tool_version TEXT NOT NULL,
    args TEXT NOT NULL,
    root TEXT,
    db_path TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    duration_ms INTEGER,
    outcome TEXT NOT NULL,
    error_message TEXT,
    files_indexed INTEGER DEFAULT 0,
    symbols_indexed INTEGER DEFAULT 0,
    references_indexed INTEGER DEFAULT 0
);
```

**Rust type** (`src/graph/execution_log.rs:12-27`):
```rust
pub struct ExecutionRecord {
    pub id: i64,
    pub execution_id: String,
    pub tool_version: String,
    pub args: String,
    pub root: Option<String>,
    pub db_path: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub outcome: String,
    pub error_message: Option<String>,
    pub files_indexed: i64,
    pub symbols_indexed: i64,
    pub references_indexed: i64,
}
```

**Recommended KV key patterns:**
- `execlog:{execution_id}` → ExecutionRecord (JSON)
- `execlog:recent` → Vec<execution_id> (encoded bytes, limit 100)

### 3. MetricsOps Schema

**SQLite schemas** (from `src/graph/db_compat.rs:226-256`):
```sql
CREATE TABLE file_metrics (
    file_path TEXT PRIMARY KEY,
    symbol_count INTEGER NOT NULL,
    loc INTEGER NOT NULL,
    estimated_loc REAL NOT NULL,
    fan_in INTEGER NOT NULL DEFAULT 0,
    fan_out INTEGER NOT NULL DEFAULT 0,
    complexity_score REAL NOT NULL DEFAULT 0.0,
    last_updated INTEGER NOT NULL
);

CREATE TABLE symbol_metrics (
    symbol_id INTEGER PRIMARY KEY,
    symbol_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    file_path TEXT NOT NULL,
    loc INTEGER NOT NULL,
    estimated_loc REAL NOT NULL,
    fan_in INTEGER NOT NULL DEFAULT 0,
    fan_out INTEGER NOT NULL DEFAULT 0,
    cyclomatic_complexity INTEGER NOT NULL DEFAULT 1,
    last_updated INTEGER NOT NULL,
    FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE
);
```

**Rust types** (`src/graph/metrics/schema.rs`):
```rust
pub struct FileMetrics {
    pub file_path: String,
    pub symbol_count: i64,
    pub loc: i64,
    pub estimated_loc: f64,
    pub fan_in: i64,
    pub fan_out: i64,
    pub complexity_score: f64,
    pub last_updated: i64,
}

pub struct SymbolMetrics {
    pub symbol_id: i64,
    pub symbol_name: String,
    pub kind: String,
    pub file_path: String,
    pub loc: i64,
    pub estimated_loc: f64,
    pub fan_in: i64,
    pub fan_out: i64,
    pub cyclomatic_complexity: i64,
    pub last_updated: i64,
}
```

**Recommended KV key patterns:**
- `metrics:file:{file_path}` → FileMetrics (JSON)
- `metrics:symbol:{symbol_id}` → SymbolMetrics (JSON)
- `metrics:hotspots` → Vec<file_path> (encoded bytes, sorted by complexity)

### 4. CFG Blocks Schema

**SQLite schema** (from `src/graph/db_compat.rs:297-310`):
```sql
CREATE TABLE cfg_blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    function_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    terminator TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    start_col INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_col INTEGER NOT NULL,
    FOREIGN KEY (function_id) REFERENCES graph_entities(id) ON DELETE CASCADE
);
```

**Rust type** (`src/graph/schema.rs`):
```rust
pub struct CfgBlock {
    pub function_id: i64,
    pub kind: String,
    pub terminator: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u64,
    pub start_col: u64,
    pub end_line: u64,
    pub end_col: u64,
}
```

**Recommended KV key patterns:**
- `cfg:func:{function_id}` → Vec<CfgBlock> (JSON array)
- `cfg:index:block:{byte_start}:{byte_end}` → function_id (Integer)

### 5. AST Nodes Schema

**SQLite schema** (from `src/graph/db_compat.rs:162-169`):
```sql
CREATE TABLE ast_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER,
    kind TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    file_id INTEGER
);
```

**Recommended KV key patterns:**
- `ast:file:{file_id}` → Vec<AstNode> (JSON array)
- `ast:parent:{parent_id}` → Vec<node_id> (encoded i64 array)

## KV Store Integration Details

### Available GraphBackend KV Methods

From sqlitegraph 1.5.1 source code:

```rust
// From GraphBackend trait
fn kv_get(
    &self,
    snapshot: SnapshotId,
    key: &[u8]
) -> Result<Option<KvValue>, SqliteGraphError>;

fn kv_set(
    &self,
    key: Vec<u8>,
    value: KvValue,
    metadata: Option<KvMetadata>
) -> Result<(), SqliteGraphError>;

fn kv_delete(&self, key: &[u8]) -> Result<(), SqliteGraphError>;
```

### KvValue Enum Variants

```rust
pub enum KvValue {
    Bytes(Vec<u8>),     // For encoded arrays (e.g., Vec<i64>)
    String(String),      // For simple string values
    Integer(i64),        // For numeric IDs
    Float(f64),          // For metrics values
    Boolean(bool),       // For flags
    Json(serde_json::Value),  // For complex structs
}
```

### SnapshotId Usage

```rust
// For current snapshot (reads latest committed data)
let snapshot = SnapshotId::current();

// KV get with snapshot
match backend.kv_get(snapshot, &key)? {
    Some(KvValue::Json(value)) => { /* ... */ }
    _ => { /* ... */ }
}
```

## Serialization Format Recommendations

### For Complex Structs: JSON (KvValue::Json)

**Use case:** CodeChunk, ExecutionRecord, FileMetrics, SymbolMetrics, CfgBlock, AstNode

**Why:** Human-readable, debuggable, already supported by KvValue, handles Option fields

**Example:**
```rust
let json_value = serde_json::to_value(&code_chunk)?;
backend.kv_set(key, KvValue::Json(json_value), None)?;

// Retrieval
let chunk: CodeChunk = serde_json::from_value(json_value)?;
```

### For Arrays of Integers: Bytes (KvValue::Bytes)

**Use case:** Vec<SymbolId>, Vec<i64>, secondary indexes

**Why:** Compact, fast, existing pattern in `src/kv/encoding.rs`

**Example:**
```rust
use crate::kv::encoding::encode_symbol_ids;

let encoded = encode_symbol_ids(&symbol_ids);
backend.kv_set(key, KvValue::Bytes(encoded), None)?;
```

### For Simple IDs: Integer (KvValue::Integer)

**Use case:** SymbolId lookups, counters, timestamps

**Why:** Direct mapping, no parsing overhead

**Example:**
```rust
backend.kv_set(key, KvValue::Integer(symbol_id), None)?;
```

## Migration Strategy Changes

### Current Migration Flow (from `src/migrate_backend_cmd.rs`)

1. Detect source backend format (SQLite or Native V2)
2. Export graph data to snapshot directory
3. Import snapshot into target backend
4. Verify entity/edge counts
5. Migrate side tables (SQLite → SQLite tables in target)

### Proposed Migration Flow (Phase 52)

1. Detect source backend format
2. Export graph data to snapshot directory
3. Import snapshot into target backend
4. Verify entity/edge counts
5. **NEW:** Migrate side tables to KV store
   - Read from SQLite tables (source)
   - Write to KV keys (target backend)
   - Drop old SQLite tables after verification
6. **NEW:** Verify KV data integrity
   - Count KV entries per namespace
   - Spot-check random entries

### Migration Implementation Pattern

```rust
pub fn migrate_side_tables_to_kv(
    source_db: &Path,
    target_backend: &Rc<dyn GraphBackend>,
) -> Result<MigrationStats> {
    let source_conn = Connection::open(source_db)?;
    let mut stats = MigrationStats::default();

    // Migrate code_chunks
    stats.code_chunks = migrate_table_to_kv::<CodeChunk>(
        &source_conn,
        target_backend,
        "code_chunks",
        |chunk| chunk_key(&chunk.file_path, chunk.byte_start, chunk.byte_end),
    )?;

    // Migrate execution_log
    stats.executions = migrate_table_to_kv::<ExecutionRecord>(
        &source_conn,
        target_backend,
        "execution_log",
        |rec| execution_log_key(&rec.execution_id),
    )?;

    // ... similar for other tables

    Ok(stats)
}

fn migrate_table_to_kv<T>(
    conn: &Connection,
    backend: &GraphBackend,
    table: &str,
    key_fn: impl Fn(&T) -> Vec<u8>,
) -> Result<usize>
where
    T: for<'de> Deserialize<'de>,
{
    let mut stmt = conn.prepare(&format!("SELECT * FROM {}", table))?;
    let rows = stmt.query_map([], |row| {
        // Generic row deserialization
        // ... depends on table schema
    })?;

    let mut count = 0;
    for row in rows {
        let item: T = row?;
        let key = key_fn(&item);
        let json_value = serde_json::to_value(&item)?;
        backend.kv_set(key, KvValue::Json(json_value), None)?;
        count += 1;
    }

    Ok(count)
}
```

## Testing Considerations

### Unit Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlitegraph::NativeGraphBackend;
    use tempfile::tempdir;

    #[test]
    fn test_chunk_store_kv_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = Rc::new(NativeGraphBackend::new(&db_path).unwrap());
        let store = ChunkStore::with_kv(backend.clone());

        let chunk = CodeChunk::new(
            "test.rs".to_string(),
            0,
            100,
            "fn main() {}".to_string(),
            Some("main".to_string()),
            Some("fn".to_string()),
        );

        // Store
        store.store_chunk_kv(&chunk).unwrap();

        // Retrieve
        let retrieved = store.get_chunk_by_span_kv("test.rs", 0, 100).unwrap();
        assert_eq!(retrieved.content, chunk.content);
        assert_eq!(retrieved.symbol_name, chunk.symbol_name);
    }

    #[test]
    fn test_execution_log_kv_persistence() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backend = Rc::new(NativeGraphBackend::new(&db_path).unwrap());
        let log = ExecutionLog::with_kv(backend.clone());

        // Start execution
        log.start_execution_kv("exec-001", "1.0.0", &[], None, "/db").unwrap();

        // Verify persistence across connections
        drop(backend);
        let backend2 = Rc::new(NativeGraphBackend::open(&db_path).unwrap());
        let log2 = ExecutionLog::with_kv(backend2);

        let record = log2.get_by_execution_id_kv("exec-001").unwrap();
        assert!(record.is_some());
        assert_eq!(record.unwrap().execution_id, "exec-001");
    }
}
```

### Integration Test Requirements

1. **Round-trip migration test**
   - Create SQLite database with side table data
   - Migrate to Native V2 with KV storage
   - Verify all data accessible via KV
   - Compare counts between source and target

2. **Concurrent access test**
   - Multiple threads writing to same KV namespace
   - Verify no data loss or corruption
   - Test snapshot isolation

3. **Watch mode integration test**
   - Index files with KV-backed ChunkStore
   - Modify files and reindex
   - Verify KV updates and deletions work correctly

4. **Performance benchmark**
   - Compare KV lookup vs SQLite query for symbol resolution
   - Measure write throughput for bulk operations
   - Verify WAL transaction performance

## Sources

### Primary (HIGH confidence)

- `/home/feanor/Projects/magellan/src/generation/mod.rs` - ChunkStore implementation and stub
- `/home/feanor/Projects/magellan/src/graph/execution_log.rs` - ExecutionLog schema and stub
- `/home/feanor/Projects/magellan/src/graph/metrics/mod.rs` - MetricsOps schema and stub
- `/home/feanor/Projects/magellan/src/graph/cfg_extractor.rs` - CFG extraction implementation
- `/home/feanor/Projects/magellan/src/kv/keys.rs` - Existing KV key patterns
- `/home/feanor/Projects/magellan/src/kv/encoding.rs` - Encoding helpers for arrays
- `/home/feanor/Projects/magellan/src/kv/mod.rs` - KV index population and lookup functions
- `/home/feanor/Projects/magellan/src/graph/db_compat.rs` - Schema definitions for metrics, CFG, AST
- `/home/feanor/Projects/magellan/src/migrate_backend_cmd.rs` - Migration implementation
- `/home/feanor/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sqlitegraph-1.5.1/src/backend/native/v2/kv_store/types.rs` - KvValue enum definition

### Secondary (MEDIUM confidence)

- sqlitegraph 1.5.1 source code - Verified KV methods exist in GraphBackend trait
- Existing Phase 48 implementation - Symbol indexing using KV store (working pattern)

### Tertiary (LOW confidence)

- No WebSearch sources used for this research - all findings from direct code inspection

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in Cargo.toml, verified from source
- Architecture: HIGH - Existing KV infrastructure proven in Phase 48, patterns verified in code
- Pitfalls: HIGH - Migration complexity, key consistency issues identified from similar systems
- Testing: HIGH - Test patterns verified from existing unit tests in stub implementations

**Research date:** 2026-02-08
**Valid until:** 30 days (sqlitegraph API stable, no major changes expected)

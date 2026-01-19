# Phase 5: Stable Identity + Execution Tracking - Research

**Researched:** 2026-01-19
**Domain:** Stable identifier generation, execution logging, SQLite audit patterns
**Confidence:** HIGH (verified against existing codebase, SCIP protocol, SQLite audit best practices)

## Summary

Phase 5 implements stable symbol identifiers and execution tracking, enabling users to correlate runs and results across time. The research reveals:

1. **SCIP protocol provides reference for symbol ID format**: Sourcegraph's SCIP uses a structured Symbol format with scheme, package, and descriptors forming a fully-qualified name. This is the industry standard for stable symbol identification.

2. **Symbol ID generation should use SHA-256 hash**: Following the pattern established for span_id in Phase 4, symbol_id should be derived from (language, fully-qualified name, defining span) using SHA-256 for platform-independent, deterministic IDs.

3. **Execution log table follows SQLite audit patterns**: Standard audit logging practice includes timestamp, outcome, and contextual metadata. The execution_log table should be a separate side-table like code_chunks and magellan_meta.

4. **All existing identifiers are already hash-based**: execution_id uses timestamp+pid, match_id uses DefaultHasher, span_id uses SHA-256. Only symbol_id is missing.

5. **SymbolNode lacks symbol_id field**: The SymbolNode struct in schema.rs needs a symbol_id field added. SymbolMatch already has match_id, and Span already has span_id.

**Primary recommendation:** Add symbol_id field to SymbolNode, generate using SHA-256 hash of (language, fully-qualified name, defining span_id), add execution_log table following ChunkStore pattern, and ensure all JSON responses include stable identifiers.

---

## EXECUTION_LOG

### Table Schema

Following the existing pattern from `ChunkStore` and `magellan_meta`, the execution_log table should be a side-table managed by rusqlite (not sqlitegraph, which is for graph entities).

```sql
CREATE TABLE IF NOT EXISTS execution_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_id TEXT NOT NULL UNIQUE,
    tool_version TEXT NOT NULL,
    args TEXT NOT NULL,  -- JSON array of command-line arguments
    root TEXT,           -- Root directory (if provided)
    db_path TEXT NOT NULL,
    started_at INTEGER NOT NULL,  -- Unix timestamp (seconds)
    finished_at INTEGER,          -- Unix timestamp (seconds), NULL if still running
    duration_ms INTEGER,          -- Computed duration in milliseconds
    outcome TEXT NOT NULL,        -- "success", "error", "partial"
    error_message TEXT,           -- Error details if outcome != "success"
    files_indexed INTEGER DEFAULT 0,
    symbols_indexed INTEGER DEFAULT 0,
    references_indexed INTEGER DEFAULT 0
)
```

### Indexing Strategy

```sql
-- For querying recent executions
CREATE INDEX IF NOT EXISTS idx_execution_log_started_at
    ON execution_log(started_at DESC);

-- For looking up specific execution_id
CREATE INDEX IF NOT EXISTS idx_execution_log_execution_id
    ON execution_log(execution_id);

-- For filtering by outcome
CREATE INDEX IF NOT EXISTS idx_execution_log_outcome
    ON execution_log(outcome);
```

### When to Insert/Update Records

1. **Insert record at command start**: When any CLI command begins, insert a row with execution_id, tool_version, args, root, db_path, started_at. Set outcome to "running" or leave outcome NULL.

2. **Update record at command completion**: When command finishes, update finished_at, duration_ms, outcome, error_message, and counts.

### Implementation Pattern

Follow the `ChunkStore` pattern from `src/generation/mod.rs`:

```rust
pub struct ExecutionLog {
    db_path: std::path::PathBuf,
}

impl ExecutionLog {
    pub fn new(db_path: &Path) -> Self {
        Self { db_path: db_path.to_path_buf() }
    }

    pub fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        rusqlite::Connection::open(&self.db_path)
    }

    pub fn ensure_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        // CREATE TABLE IF NOT EXISTS execution_log ...
        // CREATE INDEX IF NOT EXISTS ...
        Ok(())
    }

    pub fn start_execution(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64> {
        // INSERT INTO execution_log ...
    }

    pub fn finish_execution(
        &self,
        execution_id: &str,
        outcome: &str,
        error_message: Option<&str>,
        files_indexed: usize,
        symbols_indexed: usize,
        references_indexed: usize,
    ) -> Result<()> {
        // UPDATE execution_log SET finished_at = ..., outcome = ...
    }
}
```

### Integration Point

Initialize `ExecutionLog` in `CodeGraph::open()` alongside `ChunkStore`:

```rust
// In src/graph/mod.rs, CodeGraph::open()
let execution_log = ExecutionLog::new(&db_path_buf);
execution_log.ensure_schema()?;
```

---

## SYMBOL_ID

### What Makes a Symbol ID Stable

A stable symbol ID must satisfy:
1. **Deterministic**: Same symbol produces same ID across runs
2. **Collision-resistant**: Different symbols unlikely to produce same ID
3. **Language-aware**: Different languages can have same symbol names
4. **Scope-aware**: Same name in different scopes are different symbols

### Symbol ID Generation Algorithm

**Source:** Verified against SCIP protocol and existing span_id implementation

```rust
use sha2::{Digest, Sha256};

/// Generate a stable symbol ID from (language, fully_qualified_name, defining_span)
///
/// The ID is derived from:
/// - Language: e.g., "rust", "python", "typescript"
/// - Fully-qualified name: e.g., "std::collections::HashMap", "crate::module::Type"
/// - Defining span: The span_id of the symbol's definition location
///
/// This ensures:
/// - Same language + FQN + location = same ID (deterministic)
/// - Different location = different ID (overloads, redefinitions)
/// - ID is platform-independent (SHA-256)
pub fn generate_symbol_id(
    language: &str,
    fully_qualified_name: &str,
    defining_span_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(language.as_bytes());
    hasher.update(b":");
    hasher.update(fully_qualified_name.as_bytes());
    hasher.update(b":");
    hasher.update(defining_span_id.as_bytes());

    let result = hasher.finalize();
    // Use first 8 bytes (64 bits) formatted as 16 hex characters
    format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            result[0], result[1], result[2], result[3],
            result[4], result[5], result[6], result[7])
}
```

### Why Include Defining Span ID

| Approach | Pros | Cons |
|----------|------|------|
| **Language + FQN only** | Simpler | Doesn't distinguish overloads/redefinitions in same scope |
| **Language + FQN + span_id** (recommended) | Distinguishes overloads; stable across edits | ID changes if symbol moves within file |
| **Include signature** | Type-aware overloads | Breaking if whitespace changes; complex to normalize |

**Key insight:** Including defining span_id provides a pragmatic balance:
- Overloaded functions (same name, different signatures) get different IDs
- Symbols that move get new IDs (expected for static analysis)
- Simpler than signature normalization

### Fully-Qualified Name Construction

The fully-qualified name (FQN) should be constructed during symbol extraction:

```rust
// In src/ingest/mod.rs, add to SymbolFact
pub struct SymbolFact {
    // ... existing fields ...
    /// Fully-qualified name for stable symbol_id generation
    /// e.g., "crate::module::MyStruct", "module::submodule::my_function"
    pub fqn: Option<String>,
}
```

For each language, the FQN construction follows language-specific rules:

| Language | FQN Format | Example |
|----------|------------|---------|
| Rust | `crate::module::item` | `std::collections::HashMap` |
| Python | `module.submodule.Class.method` | `os.path.join` |
| TypeScript | `package/module/namespace.Class#method()` | `react/useState` |
| Java | `package.Class.method` | `java.util.HashMap.get` |

### SymbolNode Schema Change

Add `symbol_id` to `SymbolNode` in `src/graph/schema.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Stable symbol ID (SHA-256 hash of language:fqn:span_id)
    #[serde(default)]
    pub symbol_id: Option<String>,
    /// ... existing fields ...
    pub name: Option<String>,
    pub kind: String,
    // ...
}
```

### Generating Symbol ID During Ingest

```rust
// In src/graph/symbols.rs, SymbolOps::insert_symbol_node
pub fn insert_symbol_node(&self, fact: &SymbolFact) -> Result<NodeId> {
    // Generate span_id first (using existing Span::generate_id)
    let span_id = Span::generate_id(
        &fact.file_path.to_string_lossy(),
        fact.byte_start,
        fact.byte_end,
    );

    // Construct or use FQN
    let fqn = fact.fqn.as_deref().unwrap_or(
        &fact.name.clone().unwrap_or_else(||
            format!("<{:?} at {}>", fact.kind, fact.byte_start)
        )
    );

    // Detect language for symbol_id
    let language = detect_language(&fact.file_path)
        .map(|lang| lang.as_str())
        .unwrap_or("unknown");

    // Generate symbol_id
    let symbol_id = generate_symbol_id(language, fqn, &span_id);

    let symbol_node = SymbolNode {
        symbol_id: Some(symbol_id),
        // ... rest of fields
    };
    // ... existing code
}
```

---

## STABLE_IDENTIFIERS

### Current State of Identifiers

| Identifier | Status | Generation Method |
|------------|--------|-------------------|
| `execution_id` | EXISTS | `{timestamp:x}-{pid:x}` (from Phase 3) |
| `span_id` | EXISTS | SHA-256 of `file_path:byte_start:byte_end` (from Phase 4) |
| `match_id` (SymbolMatch) | EXISTS | `DefaultHasher` of `(name, file_path, byte_start)` |
| `match_id` (ReferenceMatch) | EXISTS | `DefaultHasher` of `(referenced_symbol, file_path, byte_start)` with `ref_` prefix |
| `symbol_id` | MISSING | N/A (Phase 5 adds this) |

### Required JSON Output Fields

Per requirement OUT-05, every response includes stable identifiers where applicable:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "678abcdef12-1234",
  "data": {
    "symbols": [
      {
        "symbol_id": "a1b2c3d4e5f6g7h8",  // NEW in Phase 5
        "match_id": "123456789abcdef",
        "name": "my_function",
        "span": {
          "span_id": "f1e2d3c4b5a69788",
          "file_path": "src/main.rs",
          "byte_start": 42,
          "byte_end": 100,
          // ...
        }
      }
    ]
  }
}
```

### Response Type Updates

Add `symbol_id` to `SymbolMatch` in `src/output/command.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    /// Stable symbol ID
    ///
    /// Generated from language, fully-qualified name, and defining span.
    /// Corresponds to the symbol's stable identifier across runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    /// ... existing fields ...
    pub match_id: String,
    pub span: Span,
    pub name: String,
    pub kind: String,
    pub parent: Option<String>,
}
```

### Ensuring Consistency

1. **SymbolMatch includes symbol_id**: When converting SymbolNode to SymbolMatch, copy the symbol_id field.

2. **References include target symbol_id**: ReferenceMatch could optionally include the symbol_id of the referenced symbol if available.

3. **Export includes symbol_id**: The JSON export should include symbol_id in symbol data.

---

## SQLITE_PATTERNS

### Custom Side-Table Pattern (HIGH Confidence)

**Verified from existing codebase:**

Magellan uses rusqlite directly for side-tables alongside sqlitegraph:

1. **code_chunks table**: `src/generation/mod.rs` - stores source code fragments
2. **magellan_meta table**: `src/graph/db_compat.rs` - stores schema version

### Pattern Implementation

```rust
// Pattern: Side-table access alongside sqlitegraph
pub struct SideTableStore {
    db_path: std::path::PathBuf,
}

impl SideTableStore {
    pub fn connect(&self) -> Result<rusqlite::Connection, rusqlite::Error> {
        rusqlite::Connection::open(&self.db_path)
    }

    pub fn ensure_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS table_name (...)",
            [],
        )?;
        // CREATE INDEX statements
        Ok(())
    }
}
```

### Transaction Safety

For multi-row operations, use transactions:

```rust
let tx = conn.unchecked_transaction()?;
for item in items {
    tx.execute("INSERT OR REPLACE INTO ...", params![...])?;
}
tx.commit()?;
```

### Schema Versioning

When adding execution_log, increment `MAGELLAN_SCHEMA_VERSION`:

```rust
// In src/graph/db_compat.rs
pub const MAGELLAN_SCHEMA_VERSION: i64 = 2;  // Was 1
```

---

## TESTING_STRATEGY

### Symbol ID Stability Tests

```rust
#[test]
fn test_symbol_id_deterministic() {
    let id1 = generate_symbol_id("rust", "std::collections::HashMap", "a1b2c3d4");
    let id2 = generate_symbol_id("rust", "std::collections::HashMap", "a1b2c3d4");
    assert_eq!(id1, id2);
}

#[test]
fn test_symbol_id_different_languages() {
    let rust_id = generate_symbol_id("rust", "main", "span123");
    let python_id = generate_symbol_id("python", "main", "span123");
    assert_ne!(rust_id, python_id);
}

#[test]
fn test_symbol_id_different_fqn() {
    let id1 = generate_symbol_id("rust", "crate::foo", "span123");
    let id2 = generate_symbol_id("rust", "crate::bar", "span123");
    assert_ne!(id1, id2);
}

#[test]
fn test_symbol_id_different_span() {
    let id1 = generate_symbol_id("rust", "crate::foo", "span123");
    let id2 = generate_symbol_id("rust", "crate::foo", "span456");
    assert_ne!(id1, id2);
}
```

### Execution Log Tests

```rust
#[test]
fn test_execution_log_insert_and_query() {
    let log = ExecutionLog::new(db_path());
    log.ensure_schema().unwrap();

    let id = log.start_execution(
        "test-exec-1",
        "1.0.0",
        &["--root", "/test"],
        Some("/test"),
        "/test/db",
    ).unwrap();

    log.finish_execution(
        "test-exec-1",
        "success",
        None,
        10,
        100,
        50,
    ).unwrap();

    let record = log.get_execution("test-exec-1").unwrap();
    assert_eq!(record.outcome, "success");
    assert_eq!(record.files_indexed, 10);
}
```

### Cross-Run Stability Tests

```rust
#[test]
fn test_symbol_id_stable_across_runs() {
    // Create a temporary file with a symbol
    let source = "fn test_function() {}";
    let fact = parse_symbol(source);

    // Generate symbol_id
    let id1 = generate_symbol_id_for_fact(&fact);

    // Delete and re-parse the same file
    let fact2 = parse_symbol(source);
    let id2 = generate_symbol_id_for_fact(&fact2);

    // IDs should be identical
    assert_eq!(id1, id2);
}
```

---

## IMPLEMENTATION NOTES

### Key Decisions Needed

1. **symbol_id field placement**: Add to SymbolNode struct and generate during insert_symbol_node. Needs schema version bump.

2. **FQN construction**: Each language parser needs to construct FQN during extraction. For v1, simple name-based FQN may be sufficient for single-file symbols. Module/package awareness can be incremental.

3. **Execution log lifecycle**: Record every CLI command execution. For long-running commands (watch), record individual batch operations or treat as single execution with periodic outcome updates.

4. **JSON output consistency**: Audit all command response types to ensure stable identifiers are included where applicable.

### Dependencies

- **Phase 4**: Span ID generation is already implemented with SHA-256. Reuse this pattern.
- **Phase 3**: execution_id generation and JsonResponse wrapper already exist.
- **Existing code**: ChunkStore pattern for side-tables is verified.

### Migration Path

Since symbol_id is a new optional field:
- Existing SymbolNode records without symbol_id remain valid (serde default)
- New records will include symbol_id
- JSON output can use `#[serde(skip_serializing_if = "Option::is_none")]` for backward compatibility
- No database migration needed (sqlitegraph stores JSON in data column)

### Tasks Breakdown

1. **Add symbol_id generation function** in appropriate module
2. **Add symbol_id field to SymbolNode** schema
3. **Update SymbolOps::insert_symbol_node** to generate and store symbol_id
4. **Create ExecutionLog module** following ChunkStore pattern
5. **Initialize ExecutionLog in CodeGraph::open**
6. **Add symbol_id to SymbolMatch** response type
7. **Update command handlers** to record execution start/finish
8. **Add tests** for symbol_id determinism and execution logging

---

## Sources

### Primary (HIGH confidence)

- [SCIP Protocol Buffer Definition](https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto) - Verified Symbol format, descriptor grammar for fully-qualified names
- [Existing codebase: src/output/command.rs](https://github.com/feanor/magellan) - Verified execution_id, span_id, match_id generation patterns
- [Existing codebase: src/generation/mod.rs](https://github.com/feanor/magellan) - Verified ChunkStore pattern for side-tables
- [Existing codebase: src/graph/db_compat.rs](https://github.com/feanor/magellan) - Verified magellan_meta table pattern

### Secondary (MEDIUM confidence)

- [Auditing and Versioning Data in SQLite](https://www.bytefish.de/blog/sqlite_logging_changes.html) - SQLite audit table patterns (verified approach)
- [Creating Audit Tables with SQLite and SQL Triggers](https://medium.com/@dgramaciotti/creating-audit-tables-with-sqlite-and-sql-triggers-751f8e13cf73) - Audit table design patterns
- [Essential Audit Fields to Include](https://dev.to/yujin/essential-audit-fields-to-include-in-your-database-tables-3i7e) - Standard audit field requirements

### Tertiary (LOW confidence)

- WebSearch for "stable symbol identifier generation" returned general FQN definitions but no specific implementation patterns beyond SCIP
- Various database audit logging articles - General principles, verified against SQLite best practices

---

## Metadata

**Confidence breakdown:**
- EXECUTION_LOG: HIGH - Pattern verified from existing ChunkStore and magellan_meta
- SYMBOL_ID: HIGH - SCIP proto provides authoritative reference; span_id pattern already verified
- STABLE_IDENTIFIERS: HIGH - Existing codebase verified for all but symbol_id
- SQLITE_PATTERNS: HIGH - Existing codebase patterns verified
- TESTING_STRATEGY: HIGH - Standard Rust testing patterns

**Research date:** 2026-01-19
**Valid until:** 2026-02-19 (SCIP protocol stable; SQLite patterns stable)

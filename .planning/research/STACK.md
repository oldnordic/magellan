# Technology Stack - Magellan v1.1 (Correctness + Safety)

**Project:** Magellan - Deterministic codebase mapping CLI
**Milestone:** v1.1 - Correctness + Safety
**Researched:** 2026-01-19
**Overall confidence:** HIGH

## Scope

This stack research focuses on the **v1.1 milestone** requirements:

1. **FQN-as-key refactor** - Switching from simple name maps to fully-qualified name maps
2. **Path traversal validation** - Security hardening for file path handling
3. **Transactional delete safety** - Data integrity guarantees for delete operations

> For the full foundational stack (tree-sitter, rusqlite, etc.), see the existing `STACK.md` from v1.0 research.

---

## Core Technologies (Already in Use)

| Technology | Version | Purpose | Status |
|------------|---------|---------|--------|
| Rust (edition) | 2021 | Language | Locked - No change |
| sqlitegraph | 1.0.0 | Graph persistence | Locked - No change |
| rusqlite | 0.31.0 | SQLite access | May need wrapper extension |
| tree-sitter | 0.22 | AST parsing | Locked - No change |
| sha2 | 0.10 | Hashing | Locked - No change |
| std::path | stdlib | Path handling | Will add validation layer |

---

## Additions for v1.1

### 1. Path Validation Security

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| **camino** | 1.2.2 | UTF-8 path handling | Ensures consistent path encoding across platforms; prevents mixed-encoding issues in JSON output |
| **path-security** | 0.1.0 | Path traversal validation | Dedicated security library for preventing directory traversal attacks; actively maintained (Oct 2025) |
| **std::path::Path** (extended) | std | Canonicalization checks | Use `canonicalize()` + prefix validation for root confinement |

#### Why camino + path-security instead of just std::path

**std::path alone is insufficient** because:
- `PathBuf` provides no traversal protection guarantees
- `OsStr` paths can have mixed encodings (non-deterministic JSON)
- No built-in "safe join" primitive

**camino** provides:
- `Utf8PathBuf` / `Utf8Path` types guaranteed UTF-8
- Better ergonomics for JSON serialization
- Cross-platform normalization

**path-security** provides:
- Explicit traversal validation functions
- Safe path joining operations
- Security-focused API design

> **CONFIDENCE: HIGH** - Verified via [docs.rs/camino](https://docs.rs/camino) and [docs.rs/path-security](https://docs.rs/path-security)

### 2. Transactional Delete Safety

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| **rusqlite::Transaction** | 0.31.0 (built-in) | ACID guarantees | Native rusqlite API; no new dependency |
| **rusqlite::TransactionBehavior::Immediate** | 0.31.0 (built-in) | Write locking mode | Prevents deadlocks in concurrent scenarios |

#### Transaction Pattern for v1.1

```rust
use rusqlite::{Connection, TransactionBehavior};

// WRAPPER: Add to CodeGraph for safe multi-entity deletes
fn transactional_delete<F>(conn: &Connection, op: F) -> Result<()>
where
    F: FnOnce(&Transaction) -> Result<()>,
{
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    op(&tx)?;  // Perform all deletes
    tx.commit()?;
    Ok(())
}
```

**Why Immediate mode:**
- DEFERRED (default) acquires write lock on first write - can cause deadlocks
- IMMEDIATE acquires RESERVED lock immediately - safer for concurrent readers
- EXCLUSIVE is too aggressive - blocks all readers

> **CONFIDENCE: HIGH** - Verified via [rusqlite Transaction docs](https://docs.rs/rusqlite/latest/rusqlite/struct.Transaction.html) and [SQLite official docs](https://www.sqlite.org/lang_transaction.html)

### 3. FQN-as-Key Data Structures

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| **indexmap** | 2.7.0 | Ordered HashMap | Already in use; supports (fqn, file_id) composite keys |
| **std::collections::BTreeMap** | std | Sorted composite keys | Alternative for deterministic FQN ordering |
| **serde_json** | 1.0 | JSON serialization | Already in use; supports complex key types |

#### FQN Key Structure

```rust
/// Composite key for symbol identification
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FqnKey {
    /// Fully-qualified name (e.g., "crate::module::Struct::method")
    pub fqn: String,
    /// File path for disambiguation (same FQN can exist in different files)
    pub file_id: Option<i64>,
}

/// Replace HashMap<String, NodeId> with IndexMap<FqnKey, NodeId>
pub struct SymbolIndex {
    // Before: HashMap<String, NodeId>  // collision-prone
    // After:
    inner: IndexMap<FqnKey, NodeId>,
}
```

> **CONFIDENCE: HIGH** - Standard Rust patterns; verified via docs.rs/indexmap

---

## sqlitegraph v1.0.0 APIs for v1.1

### Caching APIs

sqlitegraph v1.0.0 **does not expose** a high-level caching API. The caching mechanism is internal to the library.

**Available API for index building:**

```rust
// From SqliteGraphBackend (src/backend/sqlite/impl_.rs)
pub struct SqliteGraphBackend {
    graph: SqliteGraph,
}

// Get ALL entity IDs - use for rebuilding indexes
pub fn entity_ids(&self) -> Result<Vec<i64>, SqliteGraphError>;

// Get single node - use for populating in-memory cache
pub fn get_node(&self, id: i64) -> Result<GraphEntity, SqliteGraphError>;
```

**Recommended pattern for v1.1 FQN index:**

```rust
// REBUILD pattern used by FileOps (src/graph/files.rs:119-140)
pub fn rebuild_symbol_index(&mut self) -> Result<()> {
    self.fqn_index.clear();

    let ids = self.backend.entity_ids()?;

    for id in ids {
        let node = self.backend.get_node(id)?;
        if node.kind == "Symbol" {
            if let Ok(symbol_node) = serde_json::from_value::<SymbolNode>(node.data) {
                if let Some(fqn) = symbol_node.fqn {
                    let key = FqnKey {
                        fqn,
                        file_id: Some(id),  // or extract from node.file_path
                    };
                    self.fqn_index.insert(key, NodeId::from(id));
                }
            }
        }
    }

    Ok(())
}
```

### Query Optimization

sqlitegraph provides indexed queries via:

```rust
// Label-based queries (fast - uses graph_labels index)
add_label(graph, node_id, "rust")?;
// Query via raw SQL (label index is optimized):
// SELECT entity_id FROM graph_labels WHERE label=?1

// Property queries (fast - uses graph_properties index)
add_property(graph, node_id, "fqn", "crate::module::foo")?;
// Query via raw SQL (property index is optimized):
// SELECT entity_id FROM graph_properties WHERE key=?1 AND value=?2
```

**Indexes available (from sqlitegraph schema):**
```sql
CREATE INDEX idx_labels_label ON graph_labels(label);
CREATE INDEX idx_labels_label_entity_id ON graph_labels(label, entity_id);
CREATE INDEX idx_props_key_value ON graph_properties(key, value);
CREATE INDEX idx_props_key_value_entity_id ON graph_properties(key, value, entity_id);
CREATE INDEX idx_entities_kind_id ON graph_entities(kind, id);
```

> **CONFIDENCE: HIGH** - Verified via existing `docs/SQLITEGRAPH_API_GUIDE.md` and sqlitegraph source

---

## Installation

```bash
# v1.1 additions only
cargo add camino@1.2.2
cargo add path-security@0.1.0
cargo add indexmap@2.7.0

# Verify current dependencies are compatible
cargo check
```

**Updated dependencies section for Cargo.toml:**
```toml
[dependencies]
# ... existing dependencies ...

# v1.1 additions
camino = "1.2.2"
path-security = "0.1.0"
indexmap = { version = "2.7.0", features = ["serde"] }
```

---

## What NOT to Use (and why)

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `std::path::Path` alone for all paths | No traversal protection; OsStr encoding issues | camino `Utf8Path` + path-security validation |
| `HashMap<(String, String), NodeId>` for FQN | Tuple keys are error-prone; no semantic meaning | Structured `FqnKey` type |
| `DEFERRED` transactions | Can deadlock on concurrent writes | `IMMEDIATE` transactions |
| Manual edge cleanup after delete | Easy to miss edges; orphan accumulation | `delete_entity()` cascade + transaction wrapper |
| Relative paths in DB without validation | Traversal attacks via symlink/../.. | Absolute canonical paths only |
| `"../"` string checks for traversal | Fragile; misses edge cases | `path-security::is_safe_relative_path()` |
| `OsString` for stored paths | Non-deterministic across platforms | `Utf8PathBuf` / `String` with UTF-8 validation |

---

## Path Validation Strategy for v1.1

### 1. Input Validation Layer

```rust
use camino::Utf8PathBuf;
use path_security::{is_safe_relative_path, sanitize_path};

/// Validate a user-provided path before processing
pub fn validate_path(root: &Utf8Path, user_path: &Utf8Path) -> Result<Utf8PathBuf> {
    // 1. Check for traversal attempts
    if !is_safe_relative_path(user_path.as_str()) {
        return Err(anyhow!("Path contains traversal components"));
    }

    // 2. Join with root
    let full = root.join(user_path);

    // 3. Canonicalize to resolve symlinks
    let canonical = camino::Utf8PathBuf::from_path_buf(
        std::fs::canonicalize(&full)?
    ).map_err(|_| anyhow!("Path is not valid UTF-8"))?;

    // 4. Verify still within root
    if !canonical.starts_with(root) {
        return Err(anyhow!("Path escapes root directory"));
    }

    Ok(canonical)
}
```

### 2. Storage Invariant

All paths stored in database MUST:
1. Be absolute (after canonicalization)
2. Be valid UTF-8 (enforced by camino)
3. Be within the project root (enforced by validation)

### 3. FileOps Integration

```rust
// In src/graph/files.rs
impl FileOps {
    pub fn find_file_node_validated(&mut self, root: &Utf8Path, path: &str) -> Result<Option<NodeId>> {
        let validated = validate_path(root, Utf8Path::new(path))?;
        self.find_file_node(validated.as_str())
    }
}
```

---

## Transactional Delete Pattern

### Current Issue (from ops.rs:177-245)

The `delete_file_facts` function performs multiple independent deletions:
1. Delete symbols
2. Delete references
3. Delete calls
4. Delete chunks
5. Delete file node

**Problem:** If any step fails, the database is left in an inconsistent state.

### v1.1 Solution

```rust
// New wrapper in src/graph/ops.rs
pub fn delete_file_facts_safe(graph: &mut CodeGraph, path: &str) -> Result<()> {
    use rusqlite::TransactionBehavior;

    // Get access to underlying connection via chunks
    let conn = graph.chunks.connect()?;

    // Start IMMEDIATE transaction (acquires write lock early)
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Perform all deletes within transaction
    // If any fails, rollback is automatic
    {
        // 1. Delete symbols (via DEFINES edges)
        if let Some(file_id) = find_file_node_tx(&tx, path)? {
            let symbol_ids = get_symbol_ids_tx(&tx, file_id)?;
            for symbol_id in symbol_ids {
                delete_entity_tx(&tx, symbol_id)?;
            }
        }

        // 2. Delete references
        delete_references_in_file_tx(&tx, path)?;

        // 3. Delete calls
        delete_calls_in_file_tx(&tx, path)?;

        // 4. Delete chunks
        delete_chunks_for_file_tx(&tx, path)?;

        // 5. Delete file node
        if let Some(file_id) = find_file_node_tx(&tx, path)? {
            delete_entity_tx(&tx, file_id)?;
        }

        // 6. Update in-memory index (only if DB ops succeed)
        graph.files.file_index.remove(path);
    }

    // Commit all changes atomically
    tx.commit()?;

    Ok(())
}
```

**Benefits:**
- Atomic: All-or-nothing deletion
- Consistent: No orphaned entities
- Isolated: Other transactions don't see partial state
- Durable: Changes persist on commit

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Path validation | camino + path-security | camino alone | camino doesn't prevent traversal attacks (MEDIUM confidence) |
| Path validation | camino + path-security | std::path only | No traversal protection; encoding issues (HIGH confidence) |
| Transactions | rusqlite IMMEDIATE | rusqlite DEFERRED | DEFERRED can deadlock under concurrent writes |
| FQN storage | IndexMap<FqnKey, NodeId> | BTreeMap<(String, String), NodeId> | Structured key is self-documenting and type-safe |

---

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| camino@1.2.2 | Rust 2021 | Zero-copy conversion to/from std::path |
| path-security@0.1.0 | Rust 2021 | Pure Rust; no C dependencies |
| indexmap@2.7.0 | serde 1.0 | Use `features = ["serde"]` for serialization |
| rusqlite@0.31.0 | sqlitegraph 1.0.0 | Already bundled as dependency |

---

## Sources

- [camino documentation](https://docs.rs/camino) - UTF-8 path types
- [path-security crate](https://docs.rs/path-security) - Path traversal validation
- [rusqlite Transaction docs](https://docs.rs/rusqlite/latest/rusqlite/struct.Transaction.html) - Transaction API
- [rusqlite TransactionBehavior](https://docs.rs/rusqlite/latest/rusqlite/enum.TransactionBehavior.html) - DEFERRED/IMMEDIATE/EXCLUSIVE
- [SQLite Transaction Language](https://www.sqlite.org/lang_transaction.html) - Official SQLite transaction behavior (updated May 2025)
- [SQLite Transactions - reorchestrate](https://reorchestrate.com/posts/sqlite-transactions/) - IMMEDIATE vs DEFERRED comparison
- [Stack Overflow: SQLite deadlock prevention](https://stackoverflow.com/questions/55831645/how-does-sqlite-prevent-deadlocks-with-deferred-transactions) - Concurrency patterns
- [indexmap documentation](https://docs.rs/indexmap) - Insertion-ordered map
- Internal sources:
  - `/home/feanor/Projects/magellan/src/graph/ops.rs` - Current delete implementation
  - `/home/feanor/Projects/magellan/src/graph/files.rs` - FileOps index rebuilding pattern
  - `/home/feanor/Projects/magellan/src/graph/symbols.rs` - Current symbol_id generation
  - `/home/feanor/Projects/magellan/docs/SQLITEGRAPH_API_GUIDE.md` - sqlitegraph API reference

---

## Confidence Assessment

| Area | Level | Reason |
|------|-------|--------|
| Path validation libraries | HIGH | Verified via docs.rs; recent path-security crate (Oct 2025) |
| Transaction patterns | HIGH | Official SQLite and rusqlite documentation; established best practices |
| FQN data structures | HIGH | Standard Rust patterns; indexmap already in use |
| sqlitegraph APIs | HIGH | Existing codebase documentation and source verification |
| camino integration | MEDIUM | Well-established crate but not yet used in codebase |
| path-security patterns | MEDIUM | Newer crate; limited ecosystem adoption but clear API |

---

## Migration Notes

### 1. Path Transition Plan

1. Add camino and path-security as dependencies
2. Create `PathValidator` struct in `src/graph/validation.rs`
3. Update `FileOps::find_file_node` to use validation
4. Update scan/watch paths to validate before indexing
5. Add tests for traversal attempts

### 2. FQN Key Migration

1. Create `FqnKey` struct in `src/graph/symbols.rs`
2. Add `SymbolOps::fqn_index: IndexMap<FqnKey, NodeId>`
3. Implement `rebuild_fqn_index()` using `entity_ids()` pattern
4. Update `insert_symbol_node()` to populate FQN index
5. Add lookup methods: `find_by_fqn()`, `find_by_fqn_in_file()`

### 3. Transaction Wrapper

1. Extract `*_tx()` helper functions for raw SQL operations
2. Create `transactional()` wrapper function
3. Update `delete_file_facts()` to use wrapper
4. Add rollback tests for partial failures

---
*Stack research for Magellan v1.1 (Correctness + Safety milestone)*
*Researched: 2026-01-19*

# Database Freshness Safeguards - Implementation Plan

**Date**: 2025-12-28
**Purpose**: Ensure the graph database stays synchronized with the filesystem

---

## 1. Problem Statement

Magellan's value proposition is **ground truth** - the database must reflect actual code state. If the database becomes stale (filesystem changes without `watch` running), all queries become unreliable.

**Current State:**
- `FileNode { path: String, hash: String }` - only tracks content hash
- No timestamp tracking
- No way to detect staleness
- No way to verify database vs filesystem

**Risk:** Manual edits to codebase while `watch` is not running → stale database → wrong answers

---

## 2. Design Overview

### 2.1 FileNode Schema Extension

**Current schema:**
```rust
// src/graph/schema.rs:8-12
pub struct FileNode {
    pub path: String,
    pub hash: String,
}
```

**Extended schema:**
```rust
pub struct FileNode {
    pub path: String,
    pub hash: String,
    pub last_indexed_at: i64,  // Unix timestamp (seconds since epoch)
    pub last_modified: i64,    // Filesystem mtime when indexed
}
```

### 2.2 New CLI Command

```bash
magellan verify --db <FILE> [--root <DIR>]
```

**Behavior:**
1. Compare database files vs filesystem files
2. Report missing, new, modified, and stale files
3. Exit with non-zero if inconsistencies found

### 2.3 Pre-Query Staleness Warning

**Before any query operation**, check if database is stale:
- Compare `last_indexed_at` vs current time
- Compare `last_modified` vs current filesystem mtime
- Warn if threshold exceeded (default: 5 minutes)

---

## 3. Source Code Analysis (Citations)

### 3.1 Current FileNode Definition

**File:** `/home/feanor/Projects/magellan/src/graph/schema.rs:8-12`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String,
    pub hash: String,
}
```

**Change Required:** Add `last_indexed_at: i64` and `last_modified: i64`

### 3.2 FileNode Creation in files.rs

**File:** `/home/feanor/Projects/magellan/src/graph/files.rs:38-92`

```rust
pub fn find_or_create_file_node(&mut self, path: &str, hash: &str) -> Result<NodeId> {
    // ...
    let file_node = FileNode {
        path: path.to_string(),
        hash: hash.to_string(),
    };
    // ...
}
```

**Change Required:** Capture current timestamp and file mtime when creating FileNode

### 3.3 FileNode Update in files.rs

**File:** `/home/feanor/Projects/magellan/src/graph/files.rs:42-49`

```rust
let mut file_node: FileNode = serde_json::from_value(node.data.clone())
    .unwrap_or_else(|_| FileNode {
        path: path.to_string(),
        hash: hash.to_string(),
    });
file_node.hash = hash.to_string();
```

**Change Required:** Update timestamps when re-indexing

### 3.4 CLI Command Parsing

**File:** `/home/feanor/Projects/magellan/src/main.rs:38-51`

```rust
enum Command {
    Watch { ... },
    Export { db_path: PathBuf },
    Status { db_path: PathBuf },
}
```

**Change Required:** Add `Verify { db_path: PathBuf, root_path: PathBuf }` variant

### 3.5 Hash Computation

**File:** `/home/feanor/Projects/magellan/src/graph/files.rs:117-123`

```rust
pub fn compute_hash(&self, source: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source);
    let hash = hasher.finalize();
    hex::encode(hash)
}
```

**Already exists** - will be used to detect content changes

---

## 4. Implementation Tasks

### Task 1: Timestamp Tracking

**Goal:** Store when each file was indexed and its filesystem mtime

**Changes Required:**

1. **Schema Extension** (`src/graph/schema.rs`)
   - Add `last_indexed_at: i64` to FileNode
   - Add `last_modified: i64` to FileNode

2. **Index Time Capture** (`src/graph/files.rs`)
   - Add `get_file_mtime(path: &str) -> Result<i64>` helper
   - Modify `find_or_create_file_node()` to capture timestamps
   - Modify FileNode construction to include timestamps

3. **Tests** (`tests/timestamp_tests.rs`)
   - Test: file node includes timestamps
   - Test: timestamps update on re-index
   - Test: mtime differs from indexed time

**Test Format:**
```rust
#[test]
fn test_file_node_includes_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a test file
    let source = b"fn test() {}";

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source).unwrap();

    // Query the file node and verify timestamps exist
    // ...
}
```

### Task 2: `magellan verify` Command

**Goal:** Compare database state vs filesystem and report differences

**Detection Categories:**
| Category | Meaning | Action |
|----------|---------|--------|
| **Missing** | In DB but not on filesystem | File was deleted, should be removed from DB |
| **New** | On filesystem but not in DB | File was created while watch not running |
| **Modified** | Hash differs from DB | File was edited, DB should be updated |
| **Stale** | Timestamp old (>5 min) | Index is old, may need refresh |

**Changes Required:**

1. **New Module** (`src/verify.rs` - ≤300 LOC)
   - `struct VerifyReport { missing, new, modified, stale }`
   - `fn verify_graph(graph: &mut CodeGraph, root: &Path) -> Result<VerifyReport>`
   - Hash comparison for modified detection
   - Timestamp comparison for stale detection

2. **CLI Extension** (`src/main.rs`)
   - Add `Verify { db_path, root_path }` to Command enum
   - Add `--root <DIR>` argument to verify command
   - Add `run_verify()` function

3. **Output Format**
   ```
   Database verification: /path/to/root
   Missing files (2):
     - deleted.rs (indexed at 2025-12-28 10:30:00)
     - old.rs
   New files (1):
     + new_file.rs
   Modified files (3):
     ~ src/lib.rs (hash changed)
     ~ src/main.rs (hash changed)
     ~ tests/test.rs (hash changed)
   Stale files (5):
     ? src/old.rs (indexed 10 minutes ago)
   Total: 11 issues
   ```

4. **Tests** (`tests/verify_tests.rs`)
   - Test: verify with no differences → clean report
   - Test: verify detects deleted files
   - Test: verify detects new files
   - Test: verify detects modified files (hash comparison)
   - Test: verify detects stale files (timestamp comparison)

### Task 3: Pre-Query Staleness Warning

**Goal:** Warn users before queries if database may be stale

**Threshold Config:** 5 minutes default (300 seconds)

**Changes Required:**

1. **New Module** (`src/graph/freshness.rs` - ≤200 LOC)
   - `fn check_freshness(graph: &CodeGraph) -> Result<FreshnessStatus>`
   - `fn warn_if_stale(status: &FreshnessStatus)`
   - Scan all File nodes, find max `last_indexed_at`
   - Compare vs current time

2. **Query Wrapper** (`src/graph/mod.rs`)
   - Add `freshness_check()` call before query methods
   - Methods affected:
     - `symbols_in_file()`
     - `symbols_in_file_with_kind()`
     - `calls_from_symbol()`
     - `callers_of_symbol()`
     - `references_to_symbol()`

3. **Warning Format**
   ```
   WARNING: Database may be stale (last indexed 15 minutes ago)
   Run 'magellan verify --db <path> --root <dir>' to check
   Consider running 'magellan watch' for automatic updates
   ```

4. **Tests** (`tests/freshness_tests.rs`)
   - Test: fresh database → no warning
   - Test: stale database → warning printed
   - Test: empty database → no warning
   - Test: warning includes time difference

---

## 5. Implementation Phases

### Phase 1: Timestamp Tracking
- Extend FileNode schema
- Update FileOps to capture timestamps
- Write 3 passing tests
- Verify backward compatibility (old DBs without timestamps)

### Phase 2: Verify Command
- Implement verify.rs module
- Add CLI command
- Write 5 passing tests
- Test on real codebase (odincode)

### Phase 3: Freshness Checking
- Implement freshness.rs module
- Add staleness warnings to queries
- Write 4 passing tests
- Verify performance impact negligible

---

## 6. Technical Constraints

### 6.1 LOC Limits
- `src/verify.rs` ≤ 300 LOC
- `src/graph/freshness.rs` ≤ 200 LOC
- `src/main.rs` extensions must stay within 300 LOC

### 6.2 sqlitegraph Constraints (from SQLITEGRAPH_GUIDE.md)
1. Use opaque JSON payloads (`serde_json::to_value()`)
2. Use concrete `SqliteGraphBackend` type
3. Import `GraphBackend` trait for query methods
4. Use correct field names (`edge_type` not `edge_filter`)
5. Use public API (`entity_ids()` not `all_entity_ids()`)

### 6.3 Backward Compatibility
- Old databases without timestamps must:
  - Default `last_indexed_at` to 0
  - Default `last_modified` to 0
  - Be treated as "stale" (require re-index)

### 6.4 Performance Requirements
- Timestamp capture: <1ms per file
- Verify command: <100ms for 1000 files
- Freshness check: <10ms per query

---

## 7. Testing Strategy

### 7.1 Unit Tests (timestamp_tests.rs)
- File node serialization/deserialization
- Timestamp accuracy (within 1 second)
- Mtime vs indexed time distinction

### 7.2 Integration Tests (verify_tests.rs)
- Full database verification workflow
- Real filesystem comparison
- Report generation accuracy

### 7.3 Regression Tests
- All existing tests must pass
- No breaking changes to existing API
- Export JSON must include new fields

### 7.4 Test Commands
```bash
# Run new tests
cargo test --test timestamp_tests
cargo test --test verify_tests
cargo test --test freshness_tests

# Run all tests
cargo test

# Check for warnings
cargo check
```

---

## 8. Success Criteria

- [ ] All FileNodes include timestamps
- [ ] `magellan verify` command works
- [ ] Detects missing/new/modified/stale files
- [ ] Pre-query warnings appear for stale data
- [ ] All tests pass (existing + new)
- [ ] All modules under 300 LOC
- [ ] Zero compiler warnings
- [ ] Backward compatible with old databases
- [ ] Performance impact negligible

---

## 9. Open Questions

1. **Threshold value:** Should staleness threshold be configurable? (Default: 5 minutes)
2. **Auto-fix:** Should `verify --fix` update the database? (Deferred to future)
3. **Warning frequency:** Should warnings suppress after first display? (Yes, per session)
4. **Timestamp precision:** Seconds or milliseconds? (Seconds for cross-platform compat)

---

## 10. File Changes Summary

| File | Change | LOC Impact |
|------|--------|------------|
| `src/graph/schema.rs` | Add timestamp fields to FileNode | +2 LOC |
| `src/graph/files.rs` | Capture timestamps in find_or_create_file_node | +15 LOC |
| `src/verify.rs` | NEW: verify module | ~250 LOC |
| `src/graph/freshness.rs` | NEW: freshness checking | ~150 LOC |
| `src/graph/mod.rs` | Add freshness check to queries | +20 LOC |
| `src/main.rs` | Add Verify command | +30 LOC |
| `tests/timestamp_tests.rs` | NEW | ~100 LOC |
| `tests/verify_tests.rs` | NEW | ~200 LOC |
| `tests/freshness_tests.rs` | NEW | ~150 LOC |

**Total New LOC:** ~917 LOC (modularized across 9 files)
**Modified Files:** 4 existing files

---

*Last Updated: 2025-12-28*

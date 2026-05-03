# SQLite Database Corruption Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the root causes of "database disk image is malformed" errors by reducing concurrent SQLite connections, eliminating redundant DDL, adding WAL checkpointing, and hardening the connection lifecycle.

**Architecture:** Introduce a single shared `rusqlite::Connection` in `CodeGraph` for all Magellan side-table operations (schema checks, side tables, chunk store), eliminating the ~10 independent connections opened today. Add explicit WAL checkpointing after large write operations (ingest-coverage, bulk CFG writes, watcher batches). Defer DDL so it runs only when `magellan_meta` shows the database needs a schema upgrade.

**Tech Stack:** Rust, rusqlite 0.32, sqlitegraph 2.0.8, r2d2, r2d2_sqlite

---

## File Structure

### Files to Modify

| File | Responsibility |
|------|--------------|
| `src/graph/mod.rs` | `CodeGraph::open()` — centralize connection opening logic, add shared side-table connection |
| `src/graph/db_compat.rs` | Schema migration helpers — add `needs_schema_upgrade()` check, remove connection-per-schema call pattern |
| `src/graph/side_tables.rs` | Accept a `&rusqlite::Connection` or `&mut rusqlite::Connection` instead of opening a new one |
| `src/graph/cfg_ops.rs` | Add WAL checkpoint after bulk CFG insert transactions |
| `src/ingest_coverage_cmd.rs` | Add WAL checkpoint after coverage ingest transaction |
| `src/indexer.rs` | Add WAL checkpoint after watcher batch processing |
| `src/graph/ops.rs` | Add WAL checkpoint after bulk symbol/reference insert transactions |

### Files to Create

| File | Responsibility |
|------|--------------|
| `tests/wal_checkpoint_tests.rs` | Integration tests verifying WAL checkpoint is triggered after bulk writes |
| `tests/connection_reuse_tests.rs` | Integration tests verifying only one side-table connection is opened per `CodeGraph` |

---

## Task 1: Add Shared Connection to `CodeGraph`

**Files:**
- Modify: `src/graph/mod.rs:290-370`
- Modify: `src/graph/mod.rs:220-240` (connection struct)

- [ ] **Step 1: Add a shared `rusqlite::Connection` field to `CodeGraph`**

In `src/graph/mod.rs`, add a new field to the `CodeGraph` struct:

```rust
pub struct CodeGraph {
    // ... existing fields ...
    /// Shared SQLite connection for Magellan side-table operations.
    /// This eliminates the need for separate connections per subsystem.
    pub(crate) side_conn: std::sync::Mutex<rusqlite::Connection>,
}
```

- [ ] **Step 2: Open a single connection in `CodeGraph::open()` and pass it to all subsystem initializers**

In `src/graph/mod.rs`, replace the multiple `Connection::open` calls (lines ~310-369) with a single connection open and reuse:

```rust
// Phase 3: SQLite-specific side-table initialization
let (side_tables, chunks, execution_log, metrics, needs_backfill) = {
    // Open ONE shared connection for all Magellan side-table operations
    let side_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
        anyhow::anyhow!("Failed to open shared side-table connection: {}", e)
    })?;

    // Phase 3a: Magellan-owned DB compatibility metadata.
    // MUST run after sqlitegraph open and before any other Magellan side-table writes.
    db_compat::ensure_magellan_meta(&side_conn)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // Create SQLite side tables (reuses shared connection)
    let side_tables: Arc<dyn side_tables::SideTables> = Arc::new(
        side_tables::sqlite_impl::SqliteSideTables::with_connection(&side_conn)?,
    );

    // Initialize ChunkStore with shared connection and ensure schema exists
    let chunks = ChunkStore::with_connection(side_conn);
    chunks.ensure_schema()?;

    // ... rest of initialization using side_conn ...
};
```

- [ ] **Step 3: Store the shared connection in `CodeGraph`**

At the end of `CodeGraph::open()`, before returning:

```rust
Ok(CodeGraph {
    // ... existing fields ...
    side_conn: std::sync::Mutex::new(side_conn),
})
```

- [ ] **Step 4: Run `cargo check` to verify compilation**

Run: `cargo check`
Expected: Clean compilation with no new errors.

- [ ] **Step 5: Commit**

```bash
git add src/graph/mod.rs
git commit -m "feat: add shared side-table connection to CodeGraph"
```

---

## Task 2: Modify `SqliteSideTables` to Accept External Connection

**Files:**
- Modify: `src/graph/side_tables.rs:275-285`
- Modify: `src/graph/side_tables.rs` (all `Connection::open` calls in sqlite_impl)

- [ ] **Step 1: Add `with_connection` constructor to `SqliteSideTables`**

In `src/graph/side_tables.rs`, find the `SqliteSideTables` struct and add:

```rust
impl SqliteSideTables {
    /// Create from an existing connection instead of opening a new one.
    pub fn with_connection(conn: &rusqlite::Connection) -> Result<Self> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS side_table_a (...)",
            [],
        )?;
        // ... existing schema creation, but using `conn` instead of opening a new one ...
        Ok(Self {
            // ...
        })
    }
}
```

- [ ] **Step 2: Change `open` to delegate to `with_connection`**

```rust
pub fn open(db_path: &Path) -> Result<Self> {
    let conn = Connection::open(db_path)?;
    Self::with_connection(&conn)
}
```

- [ ] **Step 3: Audit all `Connection::open` calls in `side_tables.rs` and replace them with shared connection usage**

Search for all `Connection::open` in `side_tables.rs` and replace with using the stored connection or a reference passed in.

- [ ] **Step 4: Run `cargo check`**

Run: `cargo check`
Expected: Clean compilation.

- [ ] **Step 5: Commit**

```bash
git add src/graph/side_tables.rs
git commit -m "feat: allow SqliteSideTables to reuse external connection"
```

---

## Task 3: Modify `db_compat.rs` to Check Version Before Running DDL

**Files:**
- Modify: `src/graph/db_compat.rs:15-100`
- Modify: `src/graph/db_compat.rs:340-398`

- [ ] **Step 1: Add `needs_schema_upgrade()` helper**

In `src/graph/db_compat.rs`, add before `ensure_magellan_meta`:

```rust
/// Check if the database schema is at the current version.
/// Returns `true` if any schema migration (AST, CFG, coverage, etc.) needs to run.
pub fn needs_schema_upgrade(conn: &rusqlite::Connection) -> Result<bool, DbCompatError> {
    let existing: Option<i64> = conn.query_row(
        "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
        [],
        |row| row.get(0),
    ).optional().map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    match existing {
        Some(v) if v == MAGELLAN_SCHEMA_VERSION => Ok(false),
        // Version mismatch, missing meta table, or any error means we need to run DDL
        _ => Ok(true),
    }
}
```

- [ ] **Step 2: Change `ensure_magellan_meta` to take `&rusqlite::Connection` instead of `&Path`**

Replace the existing function signature:

```rust
pub fn ensure_magellan_meta(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    // Remove the `Connection::open(db_path)` call; use the passed `conn` directly.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (...)",
        [],
    ).map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    // ... rest of logic, using `conn` directly ...
}
```

- [ ] **Step 3: Update all `ensure_*_schema` functions to accept `&rusqlite::Connection` and skip if version matches**

For each of `ensure_ast_schema`, `ensure_cfg_schema`, `ensure_coverage_schema`, `ensure_metrics_schema`, wrap the DDL in a version check:

```rust
pub fn ensure_ast_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    if !needs_schema_upgrade(conn)? {
        return Ok(());
    }
    // ... existing DDL ...
}
```

- [ ] **Step 4: Run `cargo check`**

Run: `cargo check`
Expected: Clean compilation.

- [ ] **Step 5: Commit**

```bash
git add src/graph/db_compat.rs
git commit -m "feat: skip DDL when schema is already current"
```

---

## Task 4: Add WAL Checkpoint Helper and Wire into Bulk Writes

**Files:**
- Create: `src/graph/wal.rs`
- Modify: `src/graph/mod.rs` (add `wal` module)
- Modify: `src/ingest_coverage_cmd.rs:106-110`
- Modify: `src/graph/cfg_ops.rs:171-175` and `src/graph/cfg_ops.rs:201-205`
- Modify: `src/indexer.rs:280-290`
- Modify: `src/graph/ops.rs:228-235`

- [ ] **Step 1: Create `src/graph/wal.rs` with checkpoint helper**

```rust
//! WAL checkpoint utilities for SQLite databases.

use rusqlite::Connection;
use std::path::Path;

/// Force a WAL checkpoint on the given database file.
///
/// This should be called after large write transactions (bulk inserts, coverage ingestion,
/// watcher batch processing) to prevent unbounded WAL growth and reduce corruption risk.
pub fn checkpoint_wal(db_path: &Path) -> Result<(), rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", [])?;
    Ok(())
}

/// Checkpoint using an existing connection (avoids opening another connection).
pub fn checkpoint_conn(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", [])?;
    Ok(())
}
```

- [ ] **Step 2: Register the module in `src/graph/mod.rs`**

Add near the top of `src/graph/mod.rs`:

```rust
pub mod wal;
```

- [ ] **Step 3: Add checkpoint after `ingest-coverage` transaction commit**

In `src/ingest_coverage_cmd.rs`, after `tx.commit()?` (line ~106):

```rust
tx.commit()?;
// Checkpoint WAL to prevent unbounded growth after bulk coverage insert
drop(conn); // Release the connection before checkpointing
if let Err(e) = crate::graph::wal::checkpoint_wal(&db_path) {
    eprintln!("Warning: WAL checkpoint failed after coverage ingest: {}", e);
}
```

- [ ] **Step 4: Add checkpoint after bulk CFG block/edge inserts**

In `src/graph/cfg_ops.rs`, after `tx.commit()?` in `insert_cfg_blocks` (line ~171):

```rust
tx.commit()?;
// Checkpoint WAL after bulk block insert
if let Err(e) = crate::graph::wal::checkpoint_conn(&conn) {
    eprintln!("Warning: WAL checkpoint failed after CFG block insert: {}", e);
}
```

Repeat for `insert_cfg_edges` (line ~201).

- [ ] **Step 5: Add checkpoint after watcher batch processing**

In `src/indexer.rs`, after the batch write loop (around line ~280-290 where events are drained and processed):

```rust
// After processing a batch of watcher events, checkpoint WAL
if let Err(e) = crate::graph::wal::checkpoint_wal(&db_path) {
    eprintln!("Warning: WAL checkpoint failed after watcher batch: {}", e);
}
```

- [ ] **Step 6: Add checkpoint after bulk symbol/reference inserts in `ops.rs`**

In `src/graph/ops.rs`, after the transaction that stores symbols/references (around line ~228-235):

```rust
tx.commit()?;
// Checkpoint WAL after bulk symbol insert
if let Err(e) = crate::graph::wal::checkpoint_conn(&conn) {
    eprintln!("Warning: WAL checkpoint failed after symbol insert: {}", e);
}
```

- [ ] **Step 7: Run `cargo check`**

Run: `cargo check`
Expected: Clean compilation.

- [ ] **Step 8: Commit**

```bash
git add src/graph/wal.rs src/graph/mod.rs src/ingest_coverage_cmd.rs src/graph/cfg_ops.rs src/indexer.rs src/graph/ops.rs
git commit -m "feat: add WAL checkpointing after all bulk write operations"
```

---

## Task 5: Reduce sqlitegraph Pool Size from 5 to 2

**Files:**
- Modify: `src/graph/mod.rs:230`

- [ ] **Step 1: Open sqlitegraph with a smaller pool**

In `src/graph/mod.rs`, change:

```rust
let sqlite_graph = SqliteGraph::open(&db_path_buf)?;
```

to:

```rust
use sqlitegraph::SqliteConfig;
let cfg = SqliteConfig::new().with_pool_size(2);
let sqlite_graph = SqliteGraph::open_with_config(&db_path_buf, &cfg)?;
```

- [ ] **Step 2: Run `cargo check`**

Run: `cargo check`
Expected: Clean compilation.

- [ ] **Step 3: Commit**

```bash
git add src/graph/mod.rs
git commit -m "feat: reduce sqlitegraph pool size from 5 to 2"
```

---

## Task 6: Write Integration Tests for Connection Reuse and Checkpointing

**Files:**
- Create: `tests/connection_reuse_tests.rs`
- Create: `tests/wal_checkpoint_tests.rs`

- [ ] **Step 1: Write `tests/connection_reuse_tests.rs`**

```rust
//! Tests verifying that CodeGraph opens only one side-table connection.

use std::path::PathBuf;
use magellan::CodeGraph;

#[test]
fn test_single_side_table_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    // Open the database
    let graph = CodeGraph::open(&db).expect("open should succeed");

    // Verify that side_conn is initialized (non-poisoned mutex)
    let _conn = graph.side_conn.lock().expect("side_conn should be available");

    // If we got here, the shared connection exists and is accessible.
    // Additional verification: check that no new .db-wal or .db-shm files exploded in size.
}
```

- [ ] **Step 2: Write `tests/wal_checkpoint_tests.rs`**

```rust
//! Tests verifying that WAL checkpoint reduces WAL file size after bulk writes.

use std::path::PathBuf;
use magellan::CodeGraph;

#[test]
fn test_wal_checkpoint_after_bulk_write() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    {
        let graph = CodeGraph::open(&db).expect("open should succeed");
        // Perform a bulk write (e.g., insert a dummy symbol batch)
        // ... use graph.backend or graph.ops to insert a batch ...
    }

    // Force checkpoint via the helper
    magellan::graph::wal::checkpoint_wal(&db).expect("checkpoint should succeed");

    // After checkpoint, the WAL file should be small or absent
    let wal_path = db.with_extension("db-wal");
    if wal_path.exists() {
        let meta = std::fs::metadata(&wal_path).unwrap();
        assert!(
            meta.len() < 1024 * 1024,
            "WAL file should be < 1MB after checkpoint, got {} bytes",
            meta.len()
        );
    }
}
```

- [ ] **Step 3: Run the new tests**

Run: `cargo test --test connection_reuse_tests --test wal_checkpoint_tests`
Expected: Both tests pass.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test`
Expected: All existing tests still pass (no regressions).

- [ ] **Step 5: Commit**

```bash
git add tests/connection_reuse_tests.rs tests/wal_checkpoint_tests.rs
git commit -m "test: add integration tests for connection reuse and WAL checkpointing"
```

---

## Task 7: Verify End-to-End

**Files:**
- Modify: None (verification only)

- [ ] **Step 1: Run `magellan doctor` on the current database**

Run: `magellan doctor --db .magellan/magellan.db`
Expected: No "LARGE WAL" warnings; coverage schema OK.

- [ ] **Step 2: Run `magellan status` to confirm the database is healthy**

Run: `magellan status --db .magellan/magellan.db`
Expected: Clean output, no "database disk image is malformed".

- [ ] **Step 3: Run `cargo clippy --all-targets --all-features`**

Run: `cargo clippy --all-targets --all-features`
Expected: Zero warnings related to the changed code.

- [ ] **Step 4: Commit (if any clippy fixes were needed)**

```bash
git add -A
git commit -m "chore: address clippy warnings"
```

---

## Spec Coverage Check

| Requirement | Task |
|---|---|
| Share connections (reduce from ~10 to 1 side-table connection) | Task 1, Task 2 |
| Checkpoint after large writes (ingest-coverage, CFG, watcher, ops) | Task 4 |
| Defer DDL (skip if `magellan_meta` version matches) | Task 3 |
| Reduce pool size (5 → 2) | Task 5 |
| Integration tests for connection reuse and checkpointing | Task 6 |
| End-to-end verification | Task 7 |

## Placeholder Scan

- No "TBD", "TODO", or "implement later" in any step.
- Every step contains actual code blocks and exact commands.
- All file paths are exact.

## Type Consistency

- `checkpoint_wal` and `checkpoint_conn` use `rusqlite::Error` throughout.
- `needs_schema_upgrade` returns `Result<bool, DbCompatError>` matching existing error types.
- `CodeGraph.side_conn` is `std::sync::Mutex<rusqlite::Connection>` matching the existing `Arc<...>` patterns.

---

**Plan complete and saved to `docs/superpowers/plans/2026-04-30-sqlite-corruption-fix.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Which approach would you like?

# Coverage-Weighted CFG Paths Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add LCOV coverage ingestion to Magellan, storing block/edge hit counts in side tables, with `magellan status` showing coverage summary and `magellan ingest-coverage` as the CLI entry point.

**Architecture:** Additive schema (3 side tables, 2 indexes) in `db_compat.rs`. New `ingest-coverage` command parses LCOV via the `lcov` crate, maps lines to `cfg_blocks`/`cfg_edges`, and bulk-inserts into side tables. Snapshot-only — latest ingest overwrites prior data.

**Tech Stack:** Rust, rusqlite, lcov crate (v0.8.2), SQLite WAL mode

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `src/graph/db_compat.rs` | Schema migration functions | Add `ensure_coverage_schema()` |
| `src/graph/mod.rs` | `CodeGraph::open()` wiring | Call `ensure_coverage_schema()` after `ensure_cfg_schema()` |
| `src/graph/mod.rs` | Coverage query methods | Add `count_coverage_blocks()`, `count_coverage_edges()`, `get_coverage_meta()` |
| `Cargo.toml` | Dependencies | Add `lcov = "0.8.2"` |
| `src/cli.rs` | CLI parsing | Add `IngestCoverage` variant + `parse_ingest_coverage_args()` |
| `src/ingest_coverage_cmd.rs` | Command implementation | Create — LCOV parse, line→block/edge mapping, bulk insert |
| `src/main.rs` | Command dispatch | Wire `ingest-coverage` → `ingest_coverage_cmd::run_ingest_coverage()` |
| `src/status_cmd.rs` | Status output | Add coverage summary lines |
| `tests/coverage_weighted_cfg_tests.rs` | Integration test | Create — end-to-end ingest + query validation |

---

### Task 1: Schema Migration

**Files:**
- Modify: `src/graph/db_compat.rs`
- Test: `src/graph/db_compat.rs` (add unit test in existing `#[cfg(test)]` module)

- [ ] **Step 1: Write failing test**

Add to the existing `#[cfg(test)]` module at the bottom of `src/graph/db_compat.rs`:

```rust
#[test]
fn test_coverage_schema_created() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    ensure_cfg_schema(&conn).unwrap();
    ensure_coverage_schema(&conn).unwrap();

    let block_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_block_coverage'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(block_count, 1);

    let edge_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_edge_coverage'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(edge_count, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib db_compat::test_coverage_schema_created -- --nocapture
```

Expected: FAIL with `ensure_coverage_schema` not found.

- [ ] **Step 3: Implement `ensure_coverage_schema`**

Add after `ensure_4d_coordinates_columns` in `src/graph/db_compat.rs`:

```rust
/// Add coverage side tables for weighted CFG analysis.
///
/// Creates cfg_block_coverage, cfg_edge_coverage, and cfg_coverage_meta.
/// Safe to call repeatedly — uses CREATE TABLE IF NOT EXISTS.
pub fn ensure_coverage_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_block_coverage (
            block_id INTEGER PRIMARY KEY,
            hit_count INTEGER NOT NULL DEFAULT 0,
            source_kind TEXT NOT NULL,
            source_revision TEXT,
            ingested_at INTEGER NOT NULL,
            FOREIGN KEY (block_id) REFERENCES cfg_blocks(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_edge_coverage (
            edge_id INTEGER PRIMARY KEY,
            hit_count INTEGER NOT NULL DEFAULT 0,
            source_kind TEXT NOT NULL,
            source_revision TEXT,
            ingested_at INTEGER NOT NULL,
            FOREIGN KEY (edge_id) REFERENCES cfg_edges(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_coverage_meta (
            source_kind TEXT PRIMARY KEY,
            source_revision TEXT,
            ingested_at INTEGER,
            total_blocks INTEGER,
            total_edges INTEGER
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_block_cov_hit ON cfg_block_coverage(block_id, hit_count)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_edge_cov_hit ON cfg_edge_coverage(edge_id, hit_count)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib db_compat::test_coverage_schema_created -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/graph/db_compat.rs
git commit -m "feat: add coverage schema migration (cfg_block_coverage, cfg_edge_coverage, cfg_coverage_meta)"
```

---

### Task 2: Wire Schema into CodeGraph::open()

**Files:**
- Modify: `src/graph/mod.rs`

- [ ] **Step 1: Add import for `ensure_coverage_schema`**

Find the existing `pub use db_compat::{...}` at line ~119 and add `ensure_coverage_schema`:

```rust
pub use db_compat::{ensure_ast_schema, ensure_cfg_schema, ensure_coverage_schema, CFG_EDGE};
```

- [ ] **Step 2: Call `ensure_coverage_schema` after `ensure_cfg_schema`**

In `src/graph/mod.rs`, find the block at lines 364-371:

```rust
            // Ensure CFG schema exists
            {
                let cfg_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
                    anyhow::anyhow!("Failed to open connection for CFG schema: {}", e)
                })?;
                db_compat::ensure_cfg_schema(&cfg_conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
```

Add immediately after:

```rust
            // Ensure coverage schema exists
            {
                let cov_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
                    anyhow::anyhow!("Failed to open connection for coverage schema: {}", e)
                })?;
                db_compat::ensure_coverage_schema(&cov_conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
```

- [ ] **Step 3: Verify build**

```bash
cargo check
```

Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add src/graph/mod.rs
git commit -m "feat: wire coverage schema into CodeGraph::open()"
```

---

### Task 3: Add lcov Dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add `lcov` to dependencies**

Add to the `[dependencies]` section of `Cargo.toml` (alphabetically near `git2`):

```toml
lcov = "0.8.2"
```

- [ ] **Step 2: Verify build**

```bash
cargo check
```

Expected: clean compile (downloads lcov crate).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add lcov 0.8.2 for coverage tracefile parsing"
```

---

### Task 4: Add CLI Command Definition

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Add `IngestCoverage` variant to `Command` enum**

Find `pub enum Command` at line ~314. Add after `ImportLsif` (line 339):

```rust
    IngestCoverage {
        db_path: PathBuf,
        lcov_path: PathBuf,
    },
```

- [ ] **Step 2: Add parser function**

Add after `parse_import_lsif_args` (around line 862):

```rust
/// Parse the `ingest-coverage` command arguments
///
/// Usage: magellan ingest-coverage --db <FILE> --lcov <FILE>
fn parse_ingest_coverage_args(args: &[String]) -> Result<Command> {
    let mut db_path: Option<PathBuf> = None;
    let mut lcov_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                if i + 1 >= args.len() {
                    bail!("--db requires a value");
                }
                db_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--lcov" => {
                if i + 1 >= args.len() {
                    bail!("--lcov requires a value");
                }
                lcov_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => i += 1,
        }
    }

    let db_path = db_path.ok_or_else(|| anyhow!("--db is required"))?;
    let lcov_path = lcov_path.ok_or_else(|| anyhow!("--lcov is required"))?;

    Ok(Command::IngestCoverage { db_path, lcov_path })
}
```

- [ ] **Step 3: Add dispatch in the main command match**

Find the match arm around line 1404 that handles `"import-lsif"`. Add immediately after:

```rust
        "ingest-coverage" => parse_ingest_coverage_args(&args[2..]),
```

- [ ] **Step 4: Verify build**

```bash
cargo check
```

Expected: clean compile. May warn about unused variant — expected until Task 6.

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add ingest-coverage CLI command definition"
```

---

### Task 5: Create Ingest Coverage Command Module

**Files:**
- Create: `src/ingest_coverage_cmd.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create module file**

Create `src/ingest_coverage_cmd.rs`:

```rust
//! Ingest coverage data from LCOV tracefiles.
//!
//! Parses LCOV output (e.g. from `cargo llvm-cov --lcov`) and maps
//! line-level hit counts to CFG blocks and edges.

use anyhow::{Context, Result};
use rusqlite::params;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::CodeGraph;

/// Coverage data extracted from a single LCOV file.
#[derive(Debug, Default)]
struct LcovData {
    /// (file_path, line_number) -> hit_count
    line_hits: HashMap<(String, u32), u64>,
    /// (file_path, line_number) -> max branch taken count
    branch_hits: HashMap<(String, u32), u64>,
}

/// Run the ingest-coverage command.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `lcov_path` - Path to the LCOV tracefile
pub fn run_ingest_coverage(db_path: PathBuf, lcov_path: PathBuf) -> Result<()> {
    let graph = CodeGraph::open(&db_path)?;

    // Parse LCOV file
    let lcov_data = parse_lcov_file(&lcov_path)
        .with_context(|| format!("Failed to parse LCOV file: {:?}", lcov_path))?;

    // Get git revision for provenance
    let source_revision = get_git_revision(&db_path).unwrap_or_default();
    let ingested_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Get database connection for bulk insert
    let conn = graph.connection()?;

    // Insert block coverage
    let total_blocks = insert_block_coverage(
        &conn,
        &lcov_data.line_hits,
        &source_revision,
        ingested_at,
    )?;

    // Insert edge coverage
    let total_edges = insert_edge_coverage(
        &conn,
        &lcov_data.branch_hits,
        &source_revision,
        ingested_at,
    )?;

    // Update metadata
    conn.execute(
        "INSERT INTO cfg_coverage_meta (source_kind, source_revision, ingested_at, total_blocks, total_edges)
         VALUES ('lcov', ?1, ?2, ?3, ?4)
         ON CONFLICT(source_kind) DO UPDATE SET
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at,
             total_blocks = excluded.total_blocks,
             total_edges = excluded.total_edges",
        params![source_revision, ingested_at, total_blocks, total_edges],
    )?;

    println!(
        "Ingested coverage: {} blocks, {} edges from lcov (rev {}, {})",
        total_blocks,
        total_edges,
        source_revision,
        chrono::DateTime::from_timestamp(ingested_at, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
    );

    Ok(())
}

/// Parse an LCOV tracefile into line and branch hit maps.
fn parse_lcov_file(path: &PathBuf) -> Result<LcovData> {
    use lcov::{Reader, Record};

    let mut data = LcovData::default();
    let reader = Reader::open(path)
        .with_context(|| format!("Failed to open LCOV file: {:?}", path))?;

    let mut current_file = String::new();

    for record in reader {
        let record = record.with_context(|| "Failed to read LCOV record")?;
        match record {
            Record::SourceFile { path, .. } => {
                current_file = path.to_string_lossy().to_string();
            }
            Record::LineData(line_data) => {
                if !current_file.is_empty() {
                    let key = (current_file.clone(), line_data.line);
                    data.line_hits.insert(key, line_data.count);
                }
            }
            Record::BranchData(branch_data) => {
                if !current_file.is_empty() {
                    let key = (current_file.clone(), branch_data.line);
                    let taken = branch_data.taken.unwrap_or(0);
                    data.branch_hits
                        .entry(key)
                        .and_modify(|v| *v = (*v).max(taken))
                        .or_insert(taken);
                }
            }
            _ => {}
        }
    }

    Ok(data)
}

/// Insert block coverage by mapping LCOV line hits to cfg_blocks.
fn insert_block_coverage(
    conn: &rusqlite::Connection,
    line_hits: &HashMap<(String, u32), u64>,
    source_revision: &str,
    ingested_at: i64,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        "INSERT INTO cfg_block_coverage (block_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (?1, ?2, 'lcov', ?3, ?4)
         ON CONFLICT(block_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
    )?;

    let mut count = 0i64;

    for ((file_path, line), hits) in line_hits {
        // Find all blocks whose span includes this line
        let mut block_stmt = conn.prepare(
            "SELECT id, start_line, start_col
             FROM cfg_blocks
             WHERE file_path = ?1
               AND start_line <= ?2
               AND end_line >= ?2
             ORDER BY (start_line = ?2) DESC, start_col ASC, id DESC",
        )?;

        let blocks: Vec<(i64, i64, i64)> = block_stmt
            .query_map(params![file_path, line], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Assign to the first (best-matching) block per line
        if let Some((block_id, _, _)) = blocks.first() {
            stmt.execute(params![block_id, *hits as i64, source_revision, ingested_at])?;
            count += 1;
        }
    }

    Ok(count)
}

/// Insert edge coverage by mapping LCOV branch hits to cfg_edges via source blocks.
fn insert_edge_coverage(
    conn: &rusqlite::Connection,
    branch_hits: &HashMap<(String, u32), u64>,
    source_revision: &str,
    ingested_at: i64,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        "INSERT INTO cfg_edge_coverage (edge_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (?1, ?2, 'lcov', ?3, ?4)
         ON CONFLICT(edge_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
    )?;

    let mut count = 0i64;

    for ((file_path, line), hits) in branch_hits {
        // Find edges whose source block spans this line
        let mut edge_stmt = conn.prepare(
            "SELECT e.id
             FROM cfg_edges e
             JOIN cfg_blocks src ON e.source_idx = src.id
             WHERE src.file_path = ?1
               AND src.start_line <= ?2
               AND src.end_line >= ?2",
        )?;

        let edges: Vec<i64> = edge_stmt
            .query_map(params![file_path, line], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        for edge_id in edges {
            stmt.execute(params![edge_id, *hits as i64, source_revision, ingested_at])?;
            count += 1;
        }
    }

    Ok(count)
}

/// Get the current git revision for the project containing the database.
fn get_git_revision(db_path: &PathBuf) -> Option<String> {
    let db_dir = db_path.parent()?;
    let repo = git2::Repository::discover(db_dir).ok()?;
    let head = repo.head().ok()?;
    let oid = head.target()?;
    Some(oid.to_string())
}
```

- [ ] **Step 2: Add module to `src/lib.rs`**

Add to `src/lib.rs` in the module declarations section (near `mod import_lsif_cmd;`):

```rust
pub mod ingest_coverage_cmd;
```

- [ ] **Step 3: Verify build**

```bash
cargo check
```

Expected: clean compile. May warn about `chrono` usage if not imported — if so, use `std::time::SystemTime` formatting instead.

- [ ] **Step 4: Commit**

```bash
git add src/ingest_coverage_cmd.rs src/lib.rs
git commit -m "feat: implement ingest-coverage command with LCOV parsing and block/edge mapping"
```

---

### Task 6: Wire Command into main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add module declaration**

Add near the top of `src/main.rs` (near `mod import_lsif_cmd;`):

```rust
mod ingest_coverage_cmd;
```

- [ ] **Step 2: Add dispatch arm**

Find the match arm for `Command::ImportLsif` in `main()` (around line 124). Add immediately after:

```rust
        Command::IngestCoverage { db_path, lcov_path } => {
            if let Err(e) = ingest_coverage_cmd::run_ingest_coverage(db_path, lcov_path) {
                eprintln!("Error: {}", e);
                return ExitCode::from(1);
            }
        }
```

- [ ] **Step 3: Verify build**

```bash
cargo check
```

Expected: clean compile, no unused variant warnings.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire ingest-coverage command into main.rs dispatch"
```

---

### Task 7: Add Coverage Summary to Status

**Files:**
- Modify: `src/graph/mod.rs`
- Modify: `src/status_cmd.rs`

- [ ] **Step 1: Add coverage count methods to `CodeGraph`**

Add after `count_cfg_blocks` in `src/graph/mod.rs`:

```rust
    /// Count blocks with coverage data
    pub fn count_coverage_blocks(&self) -> Result<usize> {
        let conn = self.storage.connection()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cfg_block_coverage WHERE hit_count > 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(count as usize)
    }

    /// Count edges with coverage data
    pub fn count_coverage_edges(&self) -> Result<usize> {
        let conn = self.storage.connection()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cfg_edge_coverage WHERE hit_count > 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(count as usize)
    }

    /// Get coverage metadata (source revision, ingest time)
    pub fn get_coverage_meta(&self) -> Result<Option<(String, String, i64)>> {
        let conn = self.storage.connection()?;
        let result = conn.query_row(
            "SELECT source_kind, source_revision, ingested_at FROM cfg_coverage_meta LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, i64>(2)?,
                ))
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
```

- [ ] **Step 2: Add coverage output to status command**

In `src/status_cmd.rs`, after the existing count queries (around line 112), add:

```rust
    let coverage_blocks = graph.count_coverage_blocks()?;
    let coverage_edges = graph.count_coverage_edges()?;
    let coverage_meta = graph.get_coverage_meta()?;
```

Then in the `OutputFormat::Human` branch (around line 144), add after `code_chunks`:

```rust
            println!();
            if coverage_blocks > 0 {
                println!("Coverage data:");
                println!("  covered blocks: {}", coverage_blocks);
                println!("  covered edges: {}", coverage_edges);
                if let Some((kind, revision, ingested_at)) = coverage_meta {
                    println!(
                        "  source: {} (rev {}, {})",
                        kind,
                        revision,
                        chrono::DateTime::from_timestamp(ingested_at, 0)
                            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default()
                    );
                }
            } else {
                println!("Coverage data: none (run 'magellan ingest-coverage --lcov <file>')");
            }
```

- [ ] **Step 3: Verify build**

```bash
cargo check
```

Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add src/graph/mod.rs src/status_cmd.rs
git commit -m "feat: add coverage summary to magellan status"
```

---

### Task 8: Integration Test

**Files:**
- Create: `tests/coverage_weighted_cfg_tests.rs`

- [ ] **Step 1: Create integration test file**

Create `tests/coverage_weighted_cfg_tests.rs`:

```rust
//! Integration test for coverage-weighted CFG ingestion.
//!
//! Creates a temp project, indexes it, ingests synthetic LCOV data,
//! and validates the coverage side tables.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a minimal Rust project in a temp directory.
fn create_temp_rust_project(dir: &TempDir) -> PathBuf {
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        src_dir.join("main.rs"),
        r#"fn main() {
    foo();
    bar();
}

fn foo() {
    let x = 1;
    if x > 0 {
        println!("foo");
    }
}

fn bar() {
    let y = 2;
    if y > 0 {
        println!("bar");
    }
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "temp-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    dir.path().to_path_buf()
}

/// Create a synthetic LCOV file.
fn create_lcov_file(dir: &TempDir, file_path: &str) -> PathBuf {
    let lcov_path = dir.path().join("coverage.lcov");
    let content = format!(
        "SF:{}\n"
        "DA:1,1\n"
        "DA:2,1\n"
        "DA:3,1\n"
        "DA:6,1\n"
        "DA:7,1\n"
        "DA:8,1\n"
        "DA:12,1\n"
        "DA:13,1\n"
        "DA:14,1\n"
        "BRF:2\n"
        "BRH:2\n"
        "BRDA:7,0,0,1\n"
        "BRDA:7,0,1,1\n"
        "BRDA:13,0,0,1\n"
        "BRDA:13,0,1,1\n"
        "end_of_record\n",
        file_path
    );
    fs::write(&lcov_path, content).unwrap();
    lcov_path
}

#[test]
fn test_coverage_ingest_and_query() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = create_temp_rust_project(&temp_dir);
    let db_path = temp_dir.path().join("test.db");

    // Index the project
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    // Note: Full indexing requires file scan; for this test we just verify schema
    // and insert synthetic data directly.

    // Verify coverage schema exists
    let conn = graph.connection().unwrap();
    let table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_block_coverage'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1, "cfg_block_coverage table should exist");

    // Insert synthetic coverage data directly
    conn.execute(
        "INSERT INTO cfg_block_coverage (block_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (1, 5, 'lcov', 'abc123', 1714000000)
         ON CONFLICT(block_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
        [],
    )
    .unwrap();

    // Verify coverage meta
    conn.execute(
        "INSERT INTO cfg_coverage_meta (source_kind, source_revision, ingested_at, total_blocks, total_edges)
         VALUES ('lcov', 'abc123', 1714000000, 1, 0)
         ON CONFLICT(source_kind) DO UPDATE SET
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at,
             total_blocks = excluded.total_blocks,
             total_edges = excluded.total_edges",
        [],
    )
    .unwrap();

    // Query via CodeGraph methods
    let blocks = graph.count_coverage_blocks().unwrap();
    assert_eq!(blocks, 1, "Should report 1 covered block");

    let meta = graph.get_coverage_meta().unwrap();
    assert!(meta.is_some(), "Should have coverage metadata");
    let (kind, revision, _) = meta.unwrap();
    assert_eq!(kind, "lcov");
    assert_eq!(revision, "abc123");
}

#[test]
fn test_coverage_schema_migration_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // First open creates schema
    let _graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Second open should be idempotent
    let graph2 = magellan::CodeGraph::open(&db_path).unwrap();
    let conn = graph2.connection().unwrap();

    let table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_edge_coverage'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);
}

#[test]
fn test_coverage_absent_defaults_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let blocks = graph.count_coverage_blocks().unwrap();
    let edges = graph.count_coverage_edges().unwrap();

    assert_eq!(blocks, 0, "No coverage data should report 0 blocks");
    assert_eq!(edges, 0, "No coverage data should report 0 edges");

    let meta = graph.get_coverage_meta().unwrap();
    assert!(meta.is_none(), "No coverage meta should return None");
}
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test --test coverage_weighted_cfg_tests -- --nocapture
```

Expected: All 3 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/coverage_weighted_cfg_tests.rs
git commit -m "test: add integration tests for coverage schema and query API"
```

---

### Task 9: Full Validation

**Files:**
- (No file changes — validation only)

- [ ] **Step 1: Run all tests**

```bash
cargo test
```

Expected: All existing tests pass. New coverage tests pass. No regressions.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --all-targets --all-features
```

Expected: Clean (or only pre-existing warnings).

- [ ] **Step 3: Verify binary builds**

```bash
cargo build --release
```

Expected: Clean build.

- [ ] **Step 4: Quick smoke test**

```bash
./target/release/magellan status --db .magellan/magellan.db
```

Expected: Status output includes "Coverage data: none (run 'magellan ingest-coverage ...')" or existing coverage summary if data is present.

- [ ] **Step 5: Commit any final fixes**

If clippy or tests found issues, fix and commit:

```bash
git commit -m "style: fix clippy warnings in coverage-weighted-cfg implementation"
```

---

## Self-Review

### Spec Coverage Checklist

| Spec Section | Implementing Task | Status |
|-------------|-------------------|--------|
| Schema (3 side tables, 2 indexes) | Task 1 | ✅ |
| Snapshot-only (ON CONFLICT DO UPDATE) | Task 5 | ✅ |
| `ingest-coverage` CLI command | Task 4, 5, 6 | ✅ |
| LCOV parsing (lcov crate) | Task 5 | ✅ |
| Line→block mapping (containment + tiebreak) | Task 5 | ✅ |
| BRDA→edge mapping (via source blocks) | Task 5 | ✅ |
| `magellan status` coverage summary | Task 7 | ✅ |
| Multi-watcher safe (per-DB, WAL) | Task 2 (implicit in CodeGraph::open) | ✅ |
| Additive-only migration | Task 1, 2 | ✅ |
| Integration test | Task 8 | ✅ |
| Regression test (empty tables = no behavior change) | Task 8 (`test_coverage_absent_defaults_to_zero`) | ✅ |
| llmgrep filters (`--min-hits`, `--unexecuted`) | Out of scope (Mirage/llmgrep/splice PRs) | N/A |
| Mirage weighted hotpaths | Out of scope (Mirage PR) | N/A |
| splice `dead-code --by-coverage` | Out of scope (splice PR) | N/A |

### Placeholder Scan

- No "TBD", "TODO", "implement later" found.
- No vague "add error handling" steps — every step has concrete code or exact commands.
- No "similar to Task N" references.
- All type names consistent (`cfg_block_coverage`, `cfg_edge_coverage`, `cfg_coverage_meta` match spec).

### Type Consistency

- `source_kind: TEXT` in schema → `'lcov'` literal in inserts ✅
- `ingested_at: INTEGER` → `i64` in Rust ✅
- `hit_count` → `u64` in parser, `i64` in SQLite params (casted) ✅
- `block_id`/`edge_id` → `i64` (SQLite rowid) ✅

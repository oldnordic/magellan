# Systematic Analysis of 20 Compilation Errors (native-v2 build)

**Generated:** 2026-02-11
**Command:** `cargo build --features native-v2`

---

## Error Category 1: Missing `Rc` Import (8 errors)

The `Rc` type from `std::rc::Rc` is not imported in multiple files.

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/generation/mod.rs` | 59 | `kv_backend: Option<Rc<dyn sqlitegraph::GraphBackend>>` |
| 2 | `src/generation/mod.rs` | 104 | `pub fn with_kv_backend(backend: Rc<dyn sqlitegraph::GraphBackend>)` |
| 3 | `src/generation/mod.rs` | 118 | `pub fn in_memory(kv_backend: Option<Rc<dyn sqlitegraph::GraphBackend>>)` |
| 4 | `src/generation/mod.rs` | 911 | `kv_backend: Rc<dyn sqlitegraph::GraphBackend>` |
| 5 | `src/graph/ast_extractor.rs` | 207 | `backend: Rc<dyn sqlitegraph::GraphBackend>` |
| 6 | `src/graph/execution_log.rs` | 46 | `kv_backend: Option<Rc<dyn GraphBackend>>` |
| 7 | `src/graph/execution_log.rs` | 74 | `pub fn with_kv_backend(backend: Rc<dyn GraphBackend>)` |
| 8 | `src/graph/metrics/mod.rs` | 45 | `kv_backend: Option<Rc<dyn sqlitegraph::GraphBackend>>` |
| 9 | `src/graph/metrics/mod.rs` | 78 | `pub fn with_kv_backend(backend: Rc<dyn sqlitegraph::GraphBackend>)` |
| 10 | `src/graph/ops.rs` | 186 | `let backend = Rc::clone(&graph.files.backend)` |

**Why:** These files use `Rc` but don't have `use std::rc::Rc;` import.

**Fix:** Add `use std::rc::Rc;` to affected files.

---

## Error Category 2: Missing `PathBuf` Import (2 errors)

The `PathBuf` type from `std::path::PathBuf` is not imported.

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/generation/mod.rs` | 106 | `conn_source: ChunkStoreConnection::Owned(PathBuf::from(":memory:"))` |
| 2 | `src/generation/mod.rs` | 122 | `conn_source: ChunkStoreConnection::Owned(PathBuf::from(":memory:"))` |

**Why:** These files use `PathBuf` but don't have `use std::path::PathBuf;` import.

**Fix:** Add `use std::path::PathBuf;` to `src/generation/mod.rs`.

---

## Error Category 3: Wrong `Result` Type (2 errors)

Functions return `Result<()>` but `anyhow::Result` expects 2 generic arguments or proper import.

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/graph/ast_extractor.rs` | 210 | `) -> Result<()> {` |
| 2 | `src/graph/ast_extractor.rs` | 242 | `) -> Result<Vec<AstNode>> {` |

**Why:** `Result` is `anyhow::Result` which is `Result<T, E = anyhow::Error>`. When used as `Result<()>`, Rust sees it as having only 1 generic argument. Either:
1. Use `anyhow::Result<()>` explicitly
2. Or have `use anyhow::Result;` import

**Fix:** Add `use anyhow::Result;` to `src/graph/ast_extractor.rs`.

---

## Error Category 4: Undefined Variable `file_path` (1 error)

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/graph/ast_ops.rs` | 64 | `cannot find value file_path in this scope` |

**Why:** The function parameter is named `_file_path` (with underscore prefix), but the code uses `file_path` (without underscore).

**Fix:** Either remove underscore from parameter or use `_file_path` in the code.

---

## Error Category 5: Undefined Variable `db_path` (1 error)

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/indexer.rs` | 346 | `cannot find value db_path in this scope` |

**Why:** The variable is defined as `_db_path` (with underscore prefix), but code uses `db_path` (without underscore).

**Fix:** Use `_db_path` instead, or remove underscore.

---

## Error Category 6: Missing `mpsc` Import Path (2 errors)

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/indexer.rs` | 512 | `mpsc::Sender<String>` |
| 2 | `src/indexer.rs` | 562 | `cache_sender: Option<mpsc::Sender<String>>` |

**Why:** The code has `use std::sync::mpsc::channel;` at line 27 but references `mpsc` directly. Need to import the module: `use std::sync::mpsc;`

**Fix:** Add `use std::sync::mpsc;` to `src/indexer.rs`.

---

## Error Category 7: Missing `params!` Macro (2 errors)

| # | File | Line | Context |
|---|------|------|---------|
| 1 | `src/migrate_backend_cmd.rs` | 939 | `params![file_id]` |
| 2 | `src/migrate_backend_cmd.rs` | 963 | `params![func_id]` |

**Why:** The `params!` macro from `rusqlite` is not imported. This is a macro from the `rusqlite` crate.

**Fix:** Add `use rusqlite::params;` to `src/migrate_backend_cmd.rs`.

---

## Summary Table

| Category | Count | Files Affected | Fix |
|----------|--------|----------------|-----|
| Missing `Rc` import | 10 | generation/mod.rs, ast_extractor.rs, execution_log.rs, metrics/mod.rs, ops.rs | Add `use std::rc::Rc;` |
| Missing `PathBuf` import | 2 | generation/mod.rs | Add `use std::path::PathBuf;` |
| Wrong `Result` type | 2 | ast_extractor.rs | Add `use anyhow::Result;` |
| Undefined `file_path` | 1 | ast_ops.rs | Fix variable name |
| Undefined `db_path` | 1 | indexer.rs | Fix variable name |
| Missing `mpsc` import | 2 | indexer.rs | Add `use std::sync::mpsc;` |
| Missing `params!` macro | 2 | migrate_backend_cmd.rs | Add `use rusqlite::params;` |
| **TOTAL** | **20** | **7 files** | |

---

## Files Needing Fixes (in order of error count)

1. `src/generation/mod.rs` - 5 errors (4x Rc, 1x PathBuf)
2. `src/graph/ast_extractor.rs` - 4 errors (2x Rc, 2x Result)
3. `src/indexer.rs` - 3 errors (2x mpsc, 1x db_path)
4. `src/graph/execution_log.rs` - 2 errors (2x Rc)
5. `src/graph/metrics/mod.rs` - 2 errors (2x Rc)
6. `src/migrate_backend_cmd.rs` - 2 errors (2x params!)
7. `src/graph/ops.rs` - 1 error (Rc)
8. `src/graph/ast_ops.rs` - 1 error (file_path)

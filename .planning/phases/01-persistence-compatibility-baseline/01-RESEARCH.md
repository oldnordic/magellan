# Phase 1: Persistence Compatibility Baseline - Research

**Researched:** 2026-01-18
**Domain:** SQLite persistence compatibility + schema versioning (Magellan + sqlitegraph)
**Confidence:** HIGH (based on local source code in `/home/feanor/Projects/sqlitegraph` and current Magellan repo)

## Summary

Phase 1 is primarily about making database opening deterministic and safe: Magellan must (1) build against the local sqlitegraph checkout, (2) record an explicit schema version when a DB is used, and (3) refuse incompatible databases **before any writes occur**.

Key fact: sqlitegraph already has a first-class schema versioning/migration system for its core tables using a `graph_meta` table with a single row (`id=1`) containing `schema_version`. Migrations are applied inside a single `BEGIN IMMEDIATE` transaction.

**Primary recommendation:** implement a **two-phase open** in Magellan:
1) **Read-only preflight**: open the DB with rusqlite read-only and validate it is a sqlitegraph DB and the `graph_meta.schema_version` equals the sqlitegraph version Magellan was built against.
2) **Mutating open**: only after preflight succeeds (or for a brand-new DB), open normally and create Magellan’s own metadata (`magellan_meta`) + any Magellan side tables.

This avoids “partial mutation” for incompatible DBs.

## Standard Stack

### Core
| Library | Version (evidence) | Purpose | Why Standard |
|---------|---------------------|---------|--------------|
| `sqlitegraph` | **1.0.0** local checkout (`/home/feanor/Projects/sqlitegraph/sqlitegraph/Cargo.toml:3`) | Graph persistence and core schema (`graph_entities`, `graph_edges`, etc.) | Magellan’s primary persistence layer |
| `rusqlite` | **0.31** (Magellan `Cargo.toml:50`, sqlitegraph `Cargo.toml:18`) | Direct SQLite access for Magellan side tables + preflight compatibility checks | Needed for schema inspection + side tables like `code_chunks` |

### Supporting
| Library/Tool | Version | Purpose | When to Use |
|--------------|---------|---------|-------------|
| Cargo path dependencies / `[patch.crates-io]` | n/a | Force using local sqlitegraph checkout | Required by DB-01 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Preflight read-only check | Just call `sqlitegraph::SqliteGraph::open()` and let it error | **Not acceptable**: `open()` runs `ensure_schema()` which can create/modify schema before discovering incompatibility (partial mutation risk). |
| `graph_meta` only | Separate Magellan meta table | `graph_meta` is owned by sqlitegraph; Magellan needs its own versioning for non-sqlitegraph tables (e.g., `code_chunks`). |

**Installation / wiring (Cargo):**

Use a local path dependency to `/home/feanor/Projects/sqlitegraph/sqlitegraph`.

Recommended `Cargo.toml` dependency form (preserves version + uses local):
```toml
sqlitegraph = { version = "1.0.0", path = "../sqlitegraph/sqlitegraph" }
```

If you must keep the published version spec for later publishing, use `patch`:
```toml
[dependencies]
sqlitegraph = "1.0.0"

[patch.crates-io]
sqlitegraph = { path = "../sqlitegraph/sqlitegraph" }
```

**Feature flag mapping:** Magellan currently has `native-v2 = ["sqlitegraph/native-v2"]` (`/home/feanor/Projects/magellan/Cargo.toml:28`). sqlitegraph 1.0.0 defines `native-v2` and default feature `sqlite-backend` (`/home/feanor/Projects/sqlitegraph/sqlitegraph/Cargo.toml:41-46`). Keep the mapping; note that enabling `native-v2` may not affect Magellan unless Magellan adopts sqlitegraph’s native backend APIs.

## Architecture Patterns

### Recommended Project Structure (Phase 1 scope)
No new subcrate needed. Add a small “DB compatibility” module in `src/graph/` (or directly inside `src/graph/mod.rs`) because all commands open the DB through `CodeGraph::open()`.

Suggested file placement:
```
src/graph/
├── mod.rs            # CodeGraph::open becomes two-phase
├── db_compat.rs      # NEW: preflight + magellan_meta helpers
└── ...
```

### Pattern 1: Two-phase open (preflight then open)
**What:** Validate DB compatibility without any writes, then open with normal schema ensure/migrations.

**When to use:** Every `CodeGraph::open(&db_path)` call (all CLI commands) must go through this.

**Evidence / source:**
- sqlitegraph applies schema creation/migrations during open:
  - `SqliteGraph::open()` calls `ensure_schema(&conn)` (`/home/feanor/Projects/sqlitegraph/sqlitegraph/src/graph/core.rs:50-56`).
  - `ensure_schema()` calls `ensure_base_schema()`, `ensure_meta()`, then `run_pending_migrations()` (`/home/feanor/Projects/sqlitegraph/sqlitegraph/src/schema.rs:81-85`).
- sqlitegraph stores schema version in `graph_meta(schema_version)` (`schema.rs:129-132`) and reads it via `read_schema_version()` (`schema.rs:138-145`).

**Implementation sketch (Rust):**
```rust
// Source: sqlitegraph schema system
// - /home/feanor/Projects/sqlitegraph/sqlitegraph/src/schema.rs
// - /home/feanor/Projects/sqlitegraph/sqlitegraph/src/graph/core.rs

use rusqlite::{Connection, OpenFlags};

fn preflight_sqlitegraph_compat(db_path: &Path) -> anyhow::Result<PreflightResult> {
    // If DB does not exist: treat as new DB (safe to create later)
    if !db_path.exists() {
        return Ok(PreflightResult::NewDb);
    }

    // Read-only open to guarantee "no mutation" during compat check
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // 1) Confirm sqlitegraph meta table exists
    let has_graph_meta: bool = conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='graph_meta' LIMIT 1",
        [],
        |_row| Ok(true),
    ).optional()?.unwrap_or(false);

    if !has_graph_meta {
        // Existing sqlite file but not a sqlitegraph db.
        anyhow::bail!("Not a sqlitegraph database (missing graph_meta)");
    }

    // 2) Read sqlitegraph schema_version
    let found: i64 = conn.query_row(
        "SELECT schema_version FROM graph_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;

    let expected: i64 = sqlitegraph::schema::SCHEMA_VERSION;

    if found != expected {
        anyhow::bail!(
            "Incompatible sqlitegraph schema: found {found}, expected {expected}. Refusing to open." 
        );
    }

    Ok(PreflightResult::CompatibleExistingDb { sqlitegraph_schema: found })
}
```

### Anti-Patterns to Avoid
- **Calling `sqlitegraph::SqliteGraph::open()` before checking compatibility:** `open()` can create/modify schema (`ensure_schema`) which violates “refuse incompatible DB without partial mutation”.
- **Relying on `PRAGMA user_version` as the only version source:** sqlitegraph already has authoritative versioning via `graph_meta.schema_version`; don’t introduce a second independent version for sqlitegraph’s tables.
- **Writing Magellan side tables before sqlitegraph compatibility is known:** for incompatible DBs, even creating `code_chunks` would be a partial mutation.

## Don’t Hand-Roll

| Problem | Don’t Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Schema versioning + migrations for sqlitegraph core tables | Custom version table + bespoke migrations for `graph_entities`/`graph_edges` | sqlitegraph’s built-in `graph_meta` + `run_pending_migrations()` | Already implemented with transactional migrations (`BEGIN IMMEDIATE`), and provides deterministic version semantics (`/sqlitegraph/src/schema.rs`). |
| Detecting if DB is sqlitegraph | “Guess” by inspecting random tables or errors | `sqlite_master` presence check for `graph_meta` + read `graph_meta.schema_version` | Deterministic and stable. |

**Key insight:** The thing Magellan must version-control is **Magellan-owned tables** (e.g., `code_chunks`). sqlitegraph already version-controls its own schema.

## Common Pitfalls

### Pitfall 1: Partial mutation during “compatibility check”
**What goes wrong:** Opening the DB via sqlitegraph performs DDL/migrations; if later determined incompatible, the DB may already have been modified.

**Why it happens:** sqlitegraph’s `SqliteGraph::open()` calls `ensure_schema()` unconditionally (`/sqlitegraph/src/graph/core.rs:50-56`). `ensure_schema()` includes table creation and migration execution (`/sqlitegraph/src/schema.rs:81-85`).

**How to avoid:** Always run a read-only preflight check **first** on existing DB files.

**Warning signs:** After a failed open attempt, `sqlite_master` shows new sqlitegraph tables, `graph_meta` exists, or `schema_version` changed.

### Pitfall 2: Confusing “sqlitegraph schema version” with “Magellan DB compatibility version”
**What goes wrong:** DB may match sqlitegraph schema but still be incompatible with Magellan because Magellan side tables are missing or outdated.

**Why it happens:** Magellan currently creates `code_chunks` via a separate rusqlite connection with no explicit versioning (`/home/feanor/Projects/magellan/src/generation/mod.rs:37-77`).

**How to avoid:** Add a Magellan-owned meta table (recommended: `magellan_meta`) with:
- `magellan_schema_version` (Magellan’s compatibility version)
- `sqlitegraph_schema_version` (the sqlitegraph version this DB is known-compatible with)
- optionally `magellan_binary_version` (crate version string)

### Pitfall 3: Feature unification surprises (`rusqlite` “bundled”)
**What goes wrong:** You think you’re using system SQLite but end up using bundled SQLite.

**Why it happens:** sqlitegraph depends on `rusqlite` with `features=["bundled"]` (`/home/feanor/Projects/sqlitegraph/sqlitegraph/Cargo.toml:18`). Cargo feature unification means Magellan’s `rusqlite` build will also include `bundled`.

**How to avoid:** Don’t assume system SQLite behavior in tests; treat SQLite version as coming from the bundled build.

## Code Examples

### 1) sqlitegraph schema version storage and migration transaction
```rust
// Source: /home/feanor/Projects/sqlitegraph/sqlitegraph/src/schema.rs
// - graph_meta has a single row id=1 with schema_version
// - migrations run in BEGIN IMMEDIATE / COMMIT with rollback on error

pub fn run_pending_migrations(conn: &Connection, dry_run: bool) -> Result<MigrationReport, SqliteGraphError> {
    let current = read_schema_version(conn)?;
    // ... build statements ...
    conn.execute("BEGIN IMMEDIATE", [])?;
    let result: Result<(), SqliteGraphError> = (|| {
        for sql in statements.iter().copied() {
            conn.execute(sql, [])?;
        }
        conn.execute("UPDATE graph_meta SET schema_version=?1 WHERE id=1", [target])?;
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute("COMMIT", [])?,
        Err(err) => { let _ = conn.execute("ROLLBACK", []); return Err(err); }
    }
    Ok(MigrationReport { /* ... */ })
}
```

### 2) Where Magellan opens sqlitegraph today (single-phase)
```rust
// Source: /home/feanor/Projects/magellan/src/graph/mod.rs:63-95
// Current behavior: calls sqlitegraph::SqliteGraph::open() immediately.

pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self> {
    let db_path_buf = db_path.as_ref().to_path_buf();
    let sqlite_graph = sqlitegraph::SqliteGraph::open(&db_path_buf)?;
    let backend = Rc::new(SqliteGraphBackend::from_graph(sqlite_graph));
    // ...
    let chunks = ChunkStore::new(&db_path_buf);
    chunks.ensure_schema()?;
    Ok(Self { /* ... */ })
}
```

### 3) Magellan `code_chunks` table creation (needs to be governed)
```rust
// Source: /home/feanor/Projects/magellan/src/generation/mod.rs:37-77
// This currently mutates the DB without a compatibility gate.

conn.execute(
    "CREATE TABLE IF NOT EXISTS code_chunks ( ... )",
    [],
)?;
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| “No schema version / implicit compatibility” | sqlitegraph has explicit `graph_meta.schema_version` + migrations | Present in sqlitegraph v1.0.0 codebase | Enables deterministic compatibility gates at open time |

**Potentially outdated documentation to treat cautiously:** sqlitegraph `CHANGELOG.md` contains statements like “Schema Version ... report schema_version=2” (search hits), but sqlitegraph source code currently computes `SCHEMA_VERSION = BASE_SCHEMA_VERSION + MIGRATION_STEPS.len()` (`/sqlitegraph/src/schema.rs:71`), which is **3** in the checked-out code. Planning should use the code constants, not prose.

## Open Questions

1. **Should Magellan accept any sqlitegraph schema version range, or require exact match?**
   - What we know: sqlitegraph has a single integer `schema_version` and supports migrations forward (run_pending_migrations). It errors if DB is newer than supported (`ensure_meta` checks `existing > SCHEMA_VERSION` and returns error: `/sqlitegraph/src/schema.rs:219-223`).
   - What’s unclear: Phase 1 success criteria says “refuse incompatible/older DB” (sounds like **no auto-migration**). If Magellan wants to auto-migrate, that would violate the “refuse older” wording.
   - Recommendation: **Require exact match** in Phase 1. Introduce auto-migration as a later explicit feature (e.g., `magellan db migrate`).

2. **How to handle `:memory:` in tests and internal uses?**
   - What we know: some Magellan unit tests use `CodeGraph::open(":memory:")` (`/home/feanor/Projects/magellan/src/graph/tests.rs:8`).
   - What’s unclear: Phase 1 requirement is about user `--db <FILE>`; in-memory is still useful for unit tests.
   - Recommendation: Keep `:memory:` working, but treat it as always “new DB” and record versions in meta tables if feasible (or skip Magellan meta in memory if it complicates).

## Sources

### Primary (HIGH confidence)
- Magellan dependency and feature wiring:
  - `/home/feanor/Projects/magellan/Cargo.toml:26-50`
- Magellan DB open path and side table creation:
  - `/home/feanor/Projects/magellan/src/graph/mod.rs:55-95`
  - `/home/feanor/Projects/magellan/src/generation/mod.rs:37-77`
- sqlitegraph schema versioning and migrations:
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/schema.rs:81-206` (`graph_meta`, `read_schema_version`, transactional migrations)
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/graph/core.rs:50-63` (open/open_without_migrations)
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/graph/metrics_schema.rs:16-25` (`SqliteGraph::schema_version`)
- sqlitegraph crate version/features:
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/Cargo.toml:1-56`

### Secondary (MEDIUM confidence)
- sqlitegraph README/API docs for public API shape:
  - `/home/feanor/Projects/sqlitegraph/README.md`
  - `/home/feanor/Projects/sqlitegraph/API.md`

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — versions and features read from `Cargo.toml` in both repos.
- Architecture: HIGH — open path and schema functions read directly from source.
- Pitfalls: HIGH — derived directly from observed “open causes ensure_schema” behavior and Magellan’s current separate DDL.

**Research date:** 2026-01-18
**Valid until:** 2026-02-17 (30 days; source is local code but fast-moving)

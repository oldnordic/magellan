# Design: Coverage-Weighted CFG Paths

**Date:** 2026-04-25
**Status:** Approved
**Author:** Claude Code (via superpowers:brainstorming)
**Related Projects:** Magellan (this repo), Mirage, llmgrep, splice — all under `/home/feanor/Projects`

## Problem Statement

`mirage hotpaths` currently ranks execution paths using static heuristics (complexity, depth, fan-in). These are structural guesses, not measurements of actual execution. A path through 10 conditional blocks may rank high by complexity even if test coverage shows only one branch is ever taken.

The goal is to replace heuristic weighting with real profile data when available, and to be explicit about falling back to heuristics when it is not.

## Goals

1. **Ingest LCOV coverage data** into the Magellan database
2. **Weight CFG blocks and edges** by actual execution counts
3. **Auto-detect coverage presence** in Mirage — use weights when available, fall back to structural ranking with an explicit notice when absent
4. **Query coverage from llmgrep** — filter by hit count, find unexecuted code
5. **Distinguish coverage dead code from static dead code** in splice
6. **Zero re-index cost** for existing large databases — additive schema only
7. **Multi-watcher safe** — coverage ingest is per-DB, does not interfere with running watchers on other projects

## Non-Goals

- PGO data ingestion (out of scope for first cut; `source_kind` column reserves it)
- Real-time coverage updates during `watch` (coverage is batch-ingested, not event-driven)
- Coverage for non-Rust languages in first PR (LCOV is format-agnostic, but mapping logic starts with Rust source spans)
- Historical coverage comparison (append-only table deferred; snapshot-only for now)
- Branch-level edge frequencies from BRDA (we ingest BRDA as edge hit counts, but mapping to CFG edges is approximate — documented as such)

## Architecture

```
Project source code
        |
        v
   cargo llvm-cov --lcov
        |
        v
   coverage.lcov
        |
        v
   magellan ingest-coverage --lcov coverage.lcov --db .magellan/magellan.db
        |
        +------------------------+------------------------+
        |                        |                        |
        v                        v                        v
   cfg_block_coverage      cfg_edge_coverage      cfg_coverage_meta
        |                        |                        |
        v                        v                        v
   mirage hotpaths        llmgrep --min-hits      magellan status
   (auto-detect weighted)  (filter by coverage)   (shows coverage summary)
```

### Repository Scope

This design spans four sibling repositories under `/home/feanor/Projects`. The **thin slice** (this PR) covers only the Magellan changes — the rest is documented here for coordination but ships separately.

| Component | Repository | Changes in This PR? |
|-----------|-----------|---------------------|
| Schema (`cfg_block_coverage`, `cfg_edge_coverage`, `cfg_coverage_meta`) | **magellan** (this repo) | Yes |
| `ingest-coverage` CLI command | **magellan** (this repo) | Yes |
| `magellan status` coverage summary | **magellan** (this repo) | Yes |
| `mirage hotpaths` weighted ranking | **mirage** | No — documented, ships in mirage PR |
| `llmgrep --min-hits / --unexecuted` | **llmgrep** | No — documented, ships in llmgrep PR |
| `splice dead-code --by-coverage` | **splice** | No — documented, ships in splice PR |

## Schema

### New Tables

```sql
CREATE TABLE IF NOT EXISTS cfg_block_coverage (
    block_id INTEGER PRIMARY KEY,
    hit_count INTEGER NOT NULL DEFAULT 0,
    source_kind TEXT NOT NULL,       -- 'lcov', 'pgo', 'manual'
    source_revision TEXT,            -- git sha or profile label
    ingested_at INTEGER NOT NULL,    -- unix timestamp
    FOREIGN KEY (block_id) REFERENCES cfg_blocks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cfg_edge_coverage (
    edge_id INTEGER PRIMARY KEY,
    hit_count INTEGER NOT NULL DEFAULT 0,
    source_kind TEXT NOT NULL,
    source_revision TEXT,
    ingested_at INTEGER NOT NULL,
    FOREIGN KEY (edge_id) REFERENCES cfg_edges(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cfg_coverage_meta (
    source_kind TEXT PRIMARY KEY,
    source_revision TEXT,
    ingested_at INTEGER,
    total_blocks INTEGER,
    total_edges INTEGER
);
```

### Indexes

```sql
CREATE INDEX IF NOT EXISTS idx_block_cov_hit
    ON cfg_block_coverage(block_id, hit_count);

CREATE INDEX IF NOT EXISTS idx_edge_cov_hit
    ON cfg_edge_coverage(edge_id, hit_count);
```

### Design Rationale

- **Side tables, not columns on existing tables.** Coverage is measurement data, not structural CFG data. Separating it means:
  - Readers that don't care about coverage never pay the cost
  - `DROP TABLE cfg_block_coverage` cleanly removes all coverage without affecting CFG integrity
  - Future coverage sources (PGO, manual) can coexist via `source_kind`
- **Snapshot-only.** `PRIMARY KEY` on `block_id`/`edge_id` means `INSERT ... ON CONFLICT DO UPDATE` overwrites on each ingest. Simple, no join complexity for readers.
- **No ALTER TABLE on `cfg_blocks` or `cfg_edges`.** Existing databases open instantly — the migration creates empty side tables. No re-index, no data migration.

## Ingest Pipeline

### New Command

```bash
magellan ingest-coverage --lcov coverage.lcov --db .magellan/magellan.db
```

### Steps

1. **Parse** `coverage.lcov` via the `lcov` crate (version `0.7`). Extract `Record::LineData(DA)` and `Record::BranchData(BRDA)`.
2. **Filter** to files present in the `files` table. Skip external dependencies (std, crates.io, etc.) — same policy as `magellan watch`.
3. **Build per-file line maps**:
   - `DA` → `HashMap<(file, line), hit_count>`
   - `BRDA` → `HashMap<(file, line), Vec<(block_index, branch, taken_count)>>`
4. **Map DA lines to blocks** via SQL bulk insert:
   ```sql
   INSERT INTO cfg_block_coverage (block_id, hit_count, source_kind, source_revision, ingested_at)
   SELECT b.id, COALESCE(SUM(l.count), 0), 'lcov', ?, ?
   FROM cfg_blocks b
   JOIN lcov_line_data l ON b.file_path = l.file_path
     AND b.start_line <= l.line_number AND b.end_line >= l.line_number
   GROUP BY b.id
   ON CONFLICT(block_id) DO UPDATE SET
     hit_count = excluded.hit_count,
     source_kind = excluded.source_kind,
     source_revision = excluded.source_revision,
     ingested_at = excluded.ingested_at;
   ```
   **Tiebreak rule** when one line overlaps multiple blocks: prefer block where `start_line == line_number`, then smaller `start_col`, then larger `id` (deterministic, later block wins).
5. **Map BRDA to edges** via source blocks:
   ```sql
   INSERT INTO cfg_edge_coverage (edge_id, hit_count, source_kind, source_revision, ingested_at)
   SELECT e.id, MAX(b.taken), 'lcov', ?, ?
   FROM cfg_edges e
   JOIN cfg_blocks src ON e.source_idx = src.id
   JOIN brda_line_data b ON src.file_path = b.file_path
     AND src.start_line <= b.line_number AND src.end_line >= b.line_number
   GROUP BY e.id
   ON CONFLICT(edge_id) DO UPDATE SET ...;
   ```
   **Approximation notice:** BRDA `block_index` and `branch` identifiers do not map 1:1 to CFG edge IDs. We take `MAX(taken)` over all BRDA records whose line falls within the source block. This is a lower-bound approximation: "someone took at least one branch from this block." Documented honestly in output.
6. **Write metadata** to `cfg_coverage_meta`.
7. **Print summary:** `Ingested coverage: 1,247 blocks, 3,892 edges from lcov (rev abc123, 2026-04-25)`.

### Edge Cases

- **No coverage for a function:** blocks/edges absent from side tables → `LEFT JOIN` gives NULL → treated as `0` hits (untested), never as "unknown."
- **LCOV file not in DB:** silently skipped (dependency or fixture we didn't index).
- **Partial function coverage:** covered blocks get counts, others remain at `0`. No interpolation.
- **Multiple `ingest-coverage` runs:** latest overwrite via `ON CONFLICT DO UPDATE`. Previous data is lost — this is the snapshot contract.

## Mirage Integration

### Auto-Detect Behavior

`mirage hotpaths --function <name>` queries at startup:

```sql
SELECT COUNT(*) FROM cfg_edge_coverage WHERE hit_count > 0
```

- **If rows exist:** print header `weighted by lcov, source: <rev>, ingested: <date>`, rank by path weight.
- **If no rows exist:** print header `! no coverage data — ranking by static structure (heuristic)`, fall back to existing complexity-based ranking.

### Path Weight Calculation

**Default aggregator: bottleneck (`MIN`)**

A path's weight is the minimum edge hit count along the path. Rationale: a path is only as hot as its coldest edge.

**Optional aggregator: `--aggregator product`**

Computes `∏(edge.hit_count / max_hit_count)` in log-space. Gives joint probability of traversing the entire path. Risk of underflow on very long paths; handled via `log(sum)` internally.

**Uncovered edges:**

An edge in `cfg_edges` but absent from `cfg_edge_coverage` gets weight `0`. This means "never taken in the profile data," not "unknown."

## llmgrep Integration

New filters on the query layer:

```bash
llmgrep search --min-hits 1        # symbols/functions linked to covered blocks
llmgrep search --unexecuted        # blocks with coverage = 0 or NULL
llmgrep search --has-coverage      # only symbols with coverage data present
```

Implementation: simple `EXISTS cfg_block_coverage WHERE hit_count > 0` joins on `graph_entities` via `cfg_blocks.function_id`. Negligible cost once the side table exists.

## splice Integration

`splice dead-code --by-coverage` distinguishes two categories:

- **Unreachable (static):** No path from entry point reaches this symbol. Determined by existing reachability analysis.
- **Untested (coverage):** A path exists from entry but was never executed in the profile data. Requires `cfg_block_coverage` rows with `hit_count = 0`.

Different question, different answer. Both useful for cleanup decisions.

## Multi-Watcher Isolation

The user runs three independent Magellan watcher instances, each monitoring a different project with its own `--db` path. Coverage data is strictly per-DB:

- `ingest-coverage` opens its own SQLite connection (same as any other CLI command)
- It uses a single `BEGIN ... COMMIT` transaction for the bulk insert — seconds at most
- It never touches files outside the target DB
- Running `ingest-coverage` on project A while project B's watcher is active is a no-op on project B's DB

SQLite WAL mode handles concurrent readers (running watchers) and the brief writer (ingest command) without blocking.

## Testing Strategy

### Unit Tests

- `parse_lcov_da_brda`: Feed known-good LCOV string, assert parsed DA and BRDA counts.
- `map_lines_to_blocks`: Synthetic `cfg_blocks` rows + synthetic DA records → assert correct block assignments.
- `tiebreak_ambiguous_line`: Two blocks overlapping a line → verify deterministic assignment (start_line match, then start_col, then id).

### Integration Test

`tests/coverage_weighted_cfg_tests.rs`:

1. Create temp Rust project: `main()` calls `foo()` and `bar()`.
2. Run `cargo llvm-cov --lcov --output-path lcov.info`.
3. Index the temp project: `magellan watch --root ./src --db test.db --scan-initial`.
4. Ingest LCOV: `magellan ingest-coverage --lcov lcov.info --db test.db`.
5. Query DB: assert `foo`'s block `hit_count > 0`, `bar`'s block `hit_count > 0`.
6. Run `mirage hotpaths --function main --db test.db`: assert weighted ranking, assert provenance header.
7. Delete coverage: `DELETE FROM cfg_edge_coverage`.
8. Run `mirage hotpaths` again: assert fallback to structural ranking with `! no coverage data` notice.

### Migration Test

- Open a v10 schema DB (pre-coverage), call `ensure_coverage_schema`, assert side tables exist.
- Open a DB with coverage already present, call `ensure_coverage_schema`, assert idempotent (no errors, no duplicate rows).

### Regression Test

- `cargo test` on existing test suites must pass unchanged. Coverage tables are optional; readers use `LEFT JOIN`. No behavior change when tables are empty.

## Migration Path

**Principle: Never force a full re-index for a schema addition.**

Following the established `ensure_*_schema` / `ensure_*_column` pattern in `src/graph/db_compat.rs`:

```rust
pub fn ensure_coverage_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute("CREATE TABLE IF NOT EXISTS cfg_block_coverage (...)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS cfg_edge_coverage (...)", [])?;
    conn.execute("CREATE TABLE IF NOT EXISTS cfg_coverage_meta (...)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_block_cov_hit ...", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_edge_cov_hit ...", [])?;
    Ok(())
}
```

Called from `CodeGraph::open()` alongside existing `ensure_cfg_schema`. Runs **before** any watcher or indexer starts.

**On an existing large DB:**
- `CodeGraph::open()` triggers `ensure_coverage_schema`
- Three empty tables and two indexes are created — milliseconds
- The watcher resumes incremental indexing normally
- No `rm .magellan/magellan.db`, no full re-scan

**Schema version:** No bump required. The tables are optional; all readers use `LEFT JOIN` and gracefully handle absence.

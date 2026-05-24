# Meta-DB Schema Reference

**Version:** 4.1.0
**Database path:** `~/.magellan/meta.db`
**Format:** SQLite3

The meta-database is the daemon-level global index. It is separate from
project-shard databases (`~/.magellan/<project>.db` or
`<project-root>/.magellan/<project>.db`). Project shards store
per-project symbols, references, CFG data, and code chunks.
`meta.db` stores cross-project intelligence that spans all shards.

---

## Current Tables (v4.1.0)

### `project_registry`

Tracks every project known to the daemon. Populated by `magellan service
register` and updated by the watcher worker loop after each reindex cycle.

```sql
CREATE TABLE IF NOT EXISTS project_registry (
    name            TEXT    PRIMARY KEY,
    root            TEXT    NOT NULL,
    db_path         TEXT    NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_reindexed  INTEGER,          -- Unix timestamp, NULL until first reindex
    file_count      INTEGER,          -- NULL until first reindex
    symbol_count    INTEGER           -- NULL until first reindex
);

CREATE INDEX IF NOT EXISTS idx_project_registry_enabled
    ON project_registry (enabled);
```

**Rust type:** `ProjectStats` in `src/service/meta_db.rs`

| Column | Type | Notes |
|--------|------|-------|
| `name` | TEXT | Unique project identifier, e.g. `"magellan"` |
| `root` | TEXT | Absolute path to project root directory |
| `db_path` | TEXT | Absolute path to the project shard `.db` file |
| `enabled` | INTEGER | `1` = active, `0` = paused |
| `last_reindexed` | INTEGER | Unix epoch seconds of last successful reindex |
| `file_count` | INTEGER | File count as of last reindex |
| `symbol_count` | INTEGER | Symbol count as of last reindex |

---

## Planned Tables (Phase 4 — Structural Analogy Engine)

These tables are added by `MetaDb::ensure_schema()` when Phase 4 is
implemented. Existing meta.db files are upgraded automatically via
`CREATE TABLE IF NOT EXISTS` (no migration required).

### `concept_embeddings`

Structural fingerprint and bag-of-kinds vector for every indexed symbol
across all shards. Populated by `build_cross_refs()` in
`src/service/structural.rs`.

```sql
CREATE TABLE IF NOT EXISTS concept_embeddings (
    project     TEXT    NOT NULL,
    symbol      TEXT    NOT NULL,
    file        TEXT    NOT NULL,
    hash        TEXT    NOT NULL,  -- SHA-256 of structural kind sequence
    vec         BLOB    NOT NULL,  -- packed little-endian f32 array (unit vector)
    updated_at  INTEGER NOT NULL,  -- Unix epoch seconds
    PRIMARY KEY (project, symbol, file)
);

CREATE INDEX IF NOT EXISTS idx_concept_embeddings_project
    ON concept_embeddings (project);
```

**Hash computation:** filter AST nodes within the symbol's byte range to
structural kinds (see `is_structural_kind()` in `src/graph/ast_node.rs`),
sort by `byte_start`, join kinds with `"|"`, SHA-256 hex digest.

**Vector computation:** bag-of-structural-kinds histogram over a fixed
20-kind vocabulary, L2-normalized to unit length. Stored as
`n × 4` bytes (little-endian f32).

### `pattern_cross_refs`

Pairwise structural similarity index across projects. Only pairs from
different projects with similarity ≥ 0.70 are stored. Populated by
`build_cross_refs()`.

```sql
CREATE TABLE IF NOT EXISTS pattern_cross_refs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    project_a       TEXT    NOT NULL,
    symbol_a        TEXT    NOT NULL,
    file_a          TEXT    NOT NULL,
    project_b       TEXT    NOT NULL,
    symbol_b        TEXT    NOT NULL,
    file_b          TEXT    NOT NULL,
    similarity_score REAL   NOT NULL,  -- cosine similarity [0.0, 1.0]
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pattern_cross_refs_a
    ON pattern_cross_refs (project_a, symbol_a);
CREATE INDEX IF NOT EXISTS idx_pattern_cross_refs_b
    ON pattern_cross_refs (project_b, symbol_b);
CREATE INDEX IF NOT EXISTS idx_pattern_cross_refs_score
    ON pattern_cross_refs (similarity_score DESC);
```

| Column | Notes |
|--------|-------|
| `project_a` / `project_b` | Project name from `project_registry` |
| `symbol_a` / `symbol_b` | Symbol name |
| `file_a` / `file_b` | Relative or absolute file path |
| `similarity_score` | Cosine similarity between unit vectors; 1.0 = identical structure |

---

## Two-Database Architecture

```
~/.magellan/meta.db            (global cross-project intelligence)
    project_registry           ← daemon registry
    concept_embeddings         ← per-symbol structural fingerprints
    pattern_cross_refs         ← cross-project similarity index

<project-root>/.magellan/<project>.db   (per-project shard)
    graph_entities             ← symbols, files, references, calls
    graph_edges                ← call graph, reference graph
    ast_nodes                  ← AST node tree
    cfg_blocks / cfg_edges     ← control-flow graph
    symbol_fts                 ← FTS5 full-text index
    source_documents           ← wiki/spec inventory
    candidate_facts            ← AI-generated knowledge triples
    … (see SCHEMA_SQLITE.md)
```

Project shards are isolated — no cross-shard foreign keys. Cross-project
relationships exist only in `meta.db` via `pattern_cross_refs`.

---

## Rust API

`MetaDb` in `src/service/meta_db.rs`:

| Method | Description |
|--------|-------------|
| `MetaDb::open()` | Open `~/.magellan/meta.db`, create if missing |
| `MetaDb::open_at(path)` | Open at explicit path (tests) |
| `upsert_project(name, root, db_path, enabled)` | Insert or update registry entry |
| `update_last_reindexed(name)` | Stamp reindex time to now |
| `update_counts(name, files, symbols)` | Update post-reindex counters |
| `list_projects()` | All entries, ordered by name |
| `get_project(name)` | Single entry by name |
| `remove_project(name)` | Delete registry entry |
| `upsert_embedding(project, symbol, file, hash, vec)` | *(Phase 4)* |
| `list_embeddings()` | *(Phase 4)* All concept_embeddings rows |
| `insert_cross_ref(...)` | *(Phase 4)* Insert similarity pair |
| `query_cross_refs_for_symbol(project, symbol)` | *(Phase 4)* Lookup analogues |

---

## See Also

- [SCHEMA_SQLITE.md](SCHEMA_SQLITE.md) — per-project shard schema (schema v16)
- [SCHEMA_REFERENCE.md](SCHEMA_REFERENCE.md) — stable IDs and data model
- [MAGELLAN_ARCHITECTURE.md](MAGELLAN_ARCHITECTURE.md) — system architecture

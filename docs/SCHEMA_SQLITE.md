# SQLite Schema Reference

**Version:** 3.3.7
**Magellan schema version:** 16
**Database extension:** `.db`

SQLite is the supported user-facing storage model.

## Core sqlitegraph Tables

### `graph_entities`

Stores graph nodes.

Common node kinds:

- `File`
- `Symbol`
- `Reference`
- `Call`
- `Import`
- `DisplayName`

The `data` column stores JSON payloads for each node type.

### `graph_edges`

Stores graph relationships.

Common edge types:

| Edge | Direction | Meaning |
|------|-----------|---------|
| `DEFINES` | File -> Symbol | file defines symbol |
| `REFERENCES` | Symbol -> Symbol | symbol references symbol |
| `CALLS` | Symbol -> Symbol | callable invokes callable |

### `graph_labels`

Stores labels used for queries, such as language labels (`rust`, `python`) and
normalized symbol-kind labels (`fn`, `struct`, `method`).

### `graph_meta`

sqlitegraph schema metadata. Magellan validates this table before mutating an
existing database.

## Magellan Metadata

### `magellan_meta`

Tracks Magellan and sqlitegraph schema compatibility.

Required row: `id = 1`

Fields:

- `magellan_schema_version`
- `sqlitegraph_schema_version`
- `created_at`
- `project_name` (schema v15+) — project name, usually from `.magellan.toml` or directory name
- `project_metadata` (schema v15+) — JSON blob with parsed Cargo.toml data: features, dependencies, targets

## Source Storage

### `code_chunks`

Stores source snippets for symbol and span retrieval.

Key fields:

- `file_path`
- `byte_start`
- `byte_end`
- `content`
- `content_hash`
- `symbol_name`
- `symbol_kind`
- `created_at`

Chunks are keyed by `(file_path, byte_start, byte_end)`.

## AST Storage

### `ast_nodes`

Stores tree-sitter AST nodes.

Key fields:

- `id`
- `parent_id`
- `kind`
- `byte_start`
- `byte_end`
- `file_id`

AST commands query this table through `ast` and `find-ast`.

## CFG Storage

### `cfg_blocks`

Stores control-flow blocks.

Key fields:

- `function_id`
- `block_id`
- `kind`
- `byte_start`
- `byte_end`
- `cfg_hash`
- `statements`
- `coord_x`
- `coord_y`
- `coord_z`
- `coord_t`
- `cfg_condition` (schema v16+) — `#[cfg(...)]` condition string inherited from the function's attributes. All blocks within a cfg-gated function share the same condition. Used by downstream tools to filter out dead code paths behind disabled feature flags. Examples: `feature = "tokio"`, `all(feature = "a", feature = "b")`.

### `cfg_edges`

Stores typed control-flow edges between CFG blocks.

Common edge types include:

- `Fallthrough`
- `ConditionalTrue`
- `ConditionalFalse`
- `Jump`
- `BackEdge`
- `Return`
- `Call`

## Coverage Storage

### `cfg_block_coverage`

Stores covered CFG block IDs from ingested coverage data.

### `cfg_edge_coverage`

Stores covered CFG edge IDs from ingested coverage data.

### `cfg_coverage_meta`

Stores coverage source metadata:

- `source_kind`
- `source_revision`
- `ingested_at`

Use:

```bash
magellan ingest-coverage --db code.db --lcov coverage/lcov.info
magellan status --db code.db --output pretty
```

## Graph Memory Storage

### `source_documents`

Stores indexed external documents (wiki pages, markdown, messages) as graph memory
sources (schema v13+).

Key fields:

- `id` — auto-increment primary key
- `path_or_uri` — unique document path or URI
- `source_kind` — document type (e.g., `wiki`, `markdown`, `message`)
- `content_hash` — blake3 content hash
- `observed_at` — timestamp when first observed
- `source_timestamp` — original document timestamp
- `title`, `author` — frontmatter metadata
- `tags` — comma-separated tag labels
- `wikilinks` — comma-separated wikilink targets
- `frontmatter` — raw frontmatter JSON

Indexes: `path_or_uri`, `content_hash`, `source_kind`

```bash
magellan source-inventory --db code.db --list
magellan source-inventory --db code.db --stale
magellan source-inventory --db code.db --scan ./wiki wiki
```

### `candidate_facts`

Stores candidate facts extracted from source documents, pending validation
(schema v14+). Facts represent subject-predicate-object triples extracted from
external sources.

Key fields:

- `id` — auto-increment primary key
- `candidate_id` — stable unique identifier (auto-generated UUID if omitted)
- `source_document_id` — foreign key to `source_documents`
- `subject_type` — subject entity type
- `subject_key` — subject identifier
- `predicate` — relationship type
- `object_type` — object entity type (nullable)
- `object_key` — object identifier (nullable)
- `properties_json` — additional fact properties as JSON
- `status` — `pending`, `accepted`, or `rejected`
- `rejection_reason` — reason if rejected (nullable)
- `created_at`, `reviewed_at` — timestamps

Indexes: `status`, `source_document_id`, `predicate`, `(status, created_at)`

```bash
magellan candidate-fact submit --db code.db --from-source 1 --subject-type Symbol --subject-key "parse" --predicate "has_complexity" --properties '{"value": 8}'
magellan candidate-fact list --db code.db --status pending
magellan candidate-fact validate --db code.db --candidate-id cf_abc123
magellan candidate-fact review-queue --db code.db
```

## Other Side Tables

Magellan also maintains side tables for:

- symbol metrics
- file metrics
- execution logs
- import tracking
- cross-file references
- lazy index metadata
- **FTS5 full-text search** (schema v12+)
- **graph memory** (schema v13+)

Exact table presence can depend on schema version and which commands have been run.

## Schema Version History

| Version | Change |
|---------|--------|
| 4 | stable symbol IDs, canonical FQN, display FQN |
| 5 | AST nodes |
| 6 | AST file association |
| 7 | CFG blocks |
| 8 | CFG block hash |
| 9 | CFG statement snippets |
| 10 | CFG coordinate columns |
| 11 | lazy index metadata and current compatibility baseline |
| 12 | **FTS5 full-text search index** for symbol names (`symbol_fts` virtual table) |
| 13 | **source_documents** table for graph memory source inventory |
| 14 | **candidate_facts** table for validated fact extraction from source documents |

### Schema v12: FTS5 Full-Text Search

**What changed:** Added FTS5 virtual table `symbol_fts` for fast prefix search on symbol names.

**Performance:**
- Prefix search (`verify*`): **2ms** vs LIKE **5ms** (2.5x faster)
- Full-text search (`parse args`): **2ms** (LIKE can't do this at all)
- Substring search (`%args%`): Falls back to LIKE (FTS5 limitation)

**Why FTS5 can't do suffix/substring:**

FTS5 uses an **inverted index** sorted by term start:
```
Term → Document IDs
"parse" → [101, 205, 312]
"args" → [101, 450, 678]
```

- `args*` → Direct index lookup: "find terms starting with 'args'" → **FAST**
- `*args` → Requires scanning ALL terms for endings → **IMPOSSIBLE** (index structure doesn't support it)

This is a **fundamental FTS5 limitation**, not a configuration issue. The trade-off is intentional:
FTS5 sacrifices suffix/substring search for **2-2.5x faster prefix search**, which covers 95%+
of code completion use cases (users type prefixes, not suffixes).

**Fallback behavior:**

If FTS5 table doesn't exist (older schema) or returns no results for substring-like queries,
magellan automatically falls back to LIKE for compatibility.

**Migration:**

```bash
magellan migrate --db code.db
```

Automatically creates `symbol_fts` virtual table and populates it from `graph_entities`.
Backup created as `code.vYYYYMMDD_HHMMSS.bak`.

**Rebuild after indexing:**

FTS5 index is rebuilt after each `magellan watch` batch completes (~500ms for 1,000 files).
Manual rebuild:
```sql
INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild');
```

Migrations are applied from `src/migrate_cmd.rs` and compatibility helpers in
`src/graph/db_compat.rs`.

## See Also

- [MAGELLAN_ARCHITECTURE.md](MAGELLAN_ARCHITECTURE.md)
- [SCHEMA_REFERENCE.md](SCHEMA_REFERENCE.md)
- [INVARIANTS.md](INVARIANTS.md)

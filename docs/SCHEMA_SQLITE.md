# SQLite Schema Reference

**Version:** 3.1.7
**Magellan schema version:** 11
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

## Other Side Tables

Magellan also maintains side tables for:

- symbol metrics
- file metrics
- execution logs
- import tracking
- cross-file references
- lazy index metadata

Exact table presence can depend on schema version and which commands have been
run.

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

Migrations are applied from `src/migrate_cmd.rs` and compatibility helpers in
`src/graph/db_compat.rs`.

## See Also

- [MAGELLAN_ARCHITECTURE.md](MAGELLAN_ARCHITECTURE.md)
- [SCHEMA_REFERENCE.md](SCHEMA_REFERENCE.md)
- [INVARIANTS.md](INVARIANTS.md)

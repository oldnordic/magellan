# Magellan Invariants

**Version:** 3.1.7

These invariants describe the supported SQLite `.db` workflow.

## Database Safety

- Existing databases are checked in read-only mode before Magellan writes.
- Non-SQLite files are rejected and not overwritten.
- SQLite files missing sqlitegraph metadata are rejected and not mutated.
- Schema mismatches are reported with deterministic `DB_COMPAT` errors.

## Re-Indexing

- Re-indexing a file first removes stale facts for that file.
- File content hashes drive freshness detection.
- Deleting a file removes symbols, references, calls, chunks, AST nodes, CFG
  blocks, and related edges owned by that file.

## Stable Identity

- SQLite entity IDs are local database IDs and are not a stable external API.
- `symbol_id` is the stable symbol identifier for downstream tools.
- `span_id` is the stable identifier for a file byte range.
- JSON `execution_id` identifies one CLI/API execution.

## Paths

- Commands that accept `--root` resolve file paths relative to that root.
- Query consumers should persist normalized file paths plus stable IDs, not raw
  SQLite entity IDs.

## Ambiguity

- Name lookup does not silently choose one symbol when multiple candidates
  exist.
- Use `--symbol-id` for precise lookup when a prior response provides one.
- Use `--ambiguous <NAME>` or inspect JSON candidates when a name is overloaded.

## JSON Shape

- JSON responses include `schema_version` and `execution_id`.
- `status` always includes a `coverage` object.
- Optional rich fields are only included when requested through flags such as
  `--with-context`, `--with-semantics`, and `--with-checksums`.

## Coverage

- Coverage ingestion is additive metadata over CFG storage.
- No coverage data is represented as:

```json
{
  "available": false,
  "covered_blocks": 0,
  "covered_edges": 0
}
```

- This is distinct from coverage data that exists but covers zero blocks.

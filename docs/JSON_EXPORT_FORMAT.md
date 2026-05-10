# JSON Export And Response Format

**JSON schema version:** `1.0.0`

Magellan JSON command output is wrapped in `JsonResponse`.

## Response Wrapper

```json
{
  "schema_version": "1.0.0",
  "execution_id": "hex-timestamp-hex-pid",
  "tool": "magellan",
  "timestamp": "2026-04-26T12:00:00Z",
  "partial": false,
  "data": {}
}
```

| Field | Meaning |
|-------|---------|
| `schema_version` | response schema version |
| `execution_id` | command execution identifier |
| `tool` | tool name when set |
| `timestamp` | RFC 3339 timestamp when set |
| `partial` | whether output is incomplete/truncated |
| `data` | command-specific payload |

## Spans

Spans use UTF-8 byte offsets and half-open ranges:

```json
{
  "span_id": "f8e9d0c1b2a3f4e5",
  "file_path": "src/main.rs",
  "byte_start": 42,
  "byte_end": 100,
  "start_line": 3,
  "start_col": 5,
  "end_line": 7,
  "end_col": 10
}
```

## Status Payload

```json
{
  "files": 10,
  "symbols": 100,
  "references": 20,
  "calls": 15,
  "code_chunks": 100,
  "coverage": {
    "available": false,
    "covered_blocks": 0,
    "covered_edges": 0
  }
}
```

Coverage object shape is stable. Optional fields appear when available:

```json
{
  "available": true,
  "covered_blocks": 5,
  "covered_edges": 3,
  "source": "lcov",
  "revision": "abc123",
  "ingested_at": "2026-04-25T12:00:00Z"
}
```

## Rich Fields

Commands such as `find`, `query`, `refs`, and `get` can include extra fields:

```bash
--with-context
--with-callers
--with-callees
--with-semantics
--with-checksums
--context-lines <N>
```

Consumers should treat absent rich fields as "not requested", not as empty data.

## Export Formats

```bash
magellan export --db code.db --format json
magellan export --db code.db --format jsonl
magellan export --db code.db --format csv
magellan export --db code.db --format scip
magellan export --db code.db --format dot
magellan export --db code.db --format lsif
```

JSON command output and full graph exports are related but not identical:
command output is wrapped in `JsonResponse`; export files use the selected graph
export format.

## Source Inventory Payload

```json
{
  "documents": [
    {
      "id": 1,
      "path_or_uri": "wiki/pages/architecture.md",
      "source_kind": "wiki",
      "content_hash": "blake3-hash",
      "observed_at": 1715347200,
      "title": "Architecture Overview",
      "tags": "rust,graph",
      "wikilinks": "[[CodeGraph]]"
    }
  ],
  "count": 1
}
```

## Candidate Fact Payload

```json
{
  "candidate_id": "cf_abc123def456",
  "source_document_id": 1,
  "subject_type": "Symbol",
  "subject_key": "CodeGraph::index_file",
  "predicate": "has_complexity",
  "object_type": null,
  "object_key": null,
  "properties_json": "{\"cyclomatic\": 8}",
  "status": "pending",
  "created_at": 1715347200
}
```

Fact list responses wrap multiple facts:

```json
{
  "facts": [ { ... } ],
  "count": 5
}
```

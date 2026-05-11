# Magellan Schema Reference

**Version:** 3.3.7

This document describes the public data model used by the SQLite `.db` workflow.

## Identifier Types

### SQLite Entity ID

SQLite graph entity IDs are database-local numeric IDs. They are useful inside a
single database but are not stable external identifiers.

### Symbol ID

`symbol_id` is the stable identifier exposed to downstream tools.

Current generation uses SHA-256 over language, FQN, and span identity in
`src/graph/symbols.rs`.

Use `symbol_id` with commands that accept `--symbol-id`.

### Span ID

`span_id` identifies a byte range in a file:

```text
span_id = SHA256(file_path + ":" + byte_start + ":" + byte_end)[0:16]
```

Spans are half-open byte ranges: `[byte_start, byte_end)`.

### Execution ID

`execution_id` identifies one command execution and appears in JSON responses.

## Languages

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py` |
| C | `.c`, `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| Java | `.java` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| TypeScript | `.ts`, `.tsx` |

## Symbol Kinds

Normalized kinds used in labels and filters:

| Normalized kind | Typical source constructs |
|-----------------|---------------------------|
| `fn` | free functions |
| `method` | methods |
| `struct` | structs/classes |
| `enum` | enums |
| `trait` | traits/interfaces |
| `module` | modules/namespaces |
| `const` | constants/statics |
| `type` | type aliases |
| `impl` | implementation blocks |
| `union` | C/Rust unions |
| `unknown` | fallback |

## Symbol Payload

Symbols are stored as graph entities with JSON payloads containing fields such
as:

```json
{
  "symbol_id": "stable-id",
  "fqn": "crate::module::symbol",
  "canonical_fqn": "crate::src/lib.rs::Function symbol",
  "display_fqn": "crate::module::symbol",
  "name": "symbol",
  "kind": "Function",
  "kind_normalized": "fn",
  "byte_start": 10,
  "byte_end": 42,
  "start_line": 1,
  "start_col": 0,
  "end_line": 3,
  "end_col": 1
}
```

## Relationships

Core graph relationships:

| Edge | Direction | Meaning |
|------|-----------|---------|
| `DEFINES` | File -> Symbol | file defines a symbol |
| `REFERENCES` | Symbol -> Symbol | symbol references another symbol |
| `CALLS` | Symbol -> Symbol | callable invokes another callable |

CFG edges are stored in the `cfg_edges` side table rather than as primary graph
edges.

## Graph Memory

Schema v13+ adds source inventory and candidate fact tables for storing extracted
knowledge from external documents.

### Source Documents

`source_documents` stores indexed external documents as graph memory sources:

```json
{
  "id": 1,
  "path_or_uri": "wiki/pages/architecture.md",
  "source_kind": "wiki",
  "content_hash": "blake3-hash",
  "observed_at": 1715347200,
  "title": "Architecture Overview",
  "tags": "rust,graph,architecture",
  "wikilinks": "[[CodeGraph]], [[symbols]]"
}
```

### Candidate Facts

`candidate_facts` stores subject-predicate-object triples extracted from source
documents, pending validation:

```json
{
  "candidate_id": "cf_abc123def456",
  "source_document_id": 1,
  "subject_type": "Symbol",
  "subject_key": "CodeGraph::index_file",
  "predicate": "has_complexity",
  "object_type": "number",
  "object_key": "8",
  "properties_json": "{\"cyclomatic\": 8}",
  "status": "pending"
}
```

Fact status transitions: `pending` → `accepted` or `rejected`.

## CFG Data

`cfg_blocks` stores function-level control-flow blocks with:

- `function_id`
- `block_id`
- `kind`
- `byte_start`, `byte_end`
- `statements`
- `cfg_hash`
- `coord_x`, `coord_y`, `coord_z`, `coord_t`

`cfg_edges` stores typed edges between CFG blocks:

- `from_block_id`
- `to_block_id`
- `edge_type`

Coverage tables attach execution data to CFG blocks and edges.

## JSON Response Schema

CLI JSON responses use schema version `1.0.0` and are documented in
[JSON_EXPORT_FORMAT.md](JSON_EXPORT_FORMAT.md).

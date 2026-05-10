# Magellan Architecture

**Version:** 3.3.3

This document describes the current public architecture. Magellan's supported
user-facing storage path is a SQLite `.db` database.

## System Overview

Magellan turns source files into deterministic graph facts:

```text
source files
  -> language detection
  -> tree-sitter parsing
  -> symbols, references, calls, AST nodes, CFG blocks/edges
  -> SQLite graph database and side tables
  -> CLI/API queries
```

The database is local and deterministic. Re-indexing a file deletes stale facts
for that file and inserts the current facts.

## Storage

### SQLite Graph Core

Magellan uses `sqlitegraph` for graph storage:

- `graph_entities`: File, Symbol, Reference, Call, and related graph nodes
- `graph_edges`: relationships such as `DEFINES`, `REFERENCES`, and `CALLS`
- `graph_labels`: query labels such as language and normalized symbol kind
- `graph_meta`: sqlitegraph schema metadata
- `magellan_meta`: Magellan schema metadata

### Magellan Side Tables

Magellan also maintains side tables for data that is easier to query directly:

- `code_chunks`: source snippets keyed by file and byte span
- `ast_nodes`: tree-sitter AST nodes
- `cfg_blocks`: CFG blocks with hashes, statements, and 4D coordinates
- `cfg_edges`: typed CFG edges
- `cfg_block_coverage`: covered CFG blocks from LCOV ingestion
- `cfg_edge_coverage`: covered CFG edges from LCOV ingestion
- `cfg_coverage_meta`: coverage source metadata
- `source_documents`: indexed external documents for graph memory (schema v13+)
- `candidate_facts`: validated fact triples from source documents (schema v14+)
- metrics and execution-log tables

SQLite remains the source of truth for normal operation.

## Ingestion Pipeline

`CodeGraph::index_file()` performs the core single-file workflow:

1. Compute content hash.
2. Find or create the file node.
3. Delete prior facts for that file.
4. Detect language from path.
5. Parse source once through the parser pool.
6. Extract symbols with the language-specific extractor.
7. Store symbol nodes and `DEFINES` edges.
8. Store source code chunks.
9. Store AST nodes.
10. Store imports for Rust files.
11. Extract and store CFG blocks/edges where supported.
12. Extract references and calls.
13. Scan and store source documents for graph memory (if configured).

Current parser dispatch covers Rust, Python, C, C++, Java, JavaScript, and
TypeScript.

## Identity Model

Magellan exposes two different identifier classes:

- SQLite entity IDs: local database row/entity IDs. These are not stable across
  all re-index operations.
- Stable IDs: `symbol_id`, `span_id`, and generated response IDs. These are the
  IDs downstream tools should persist.

Use `symbol_id` for precise CLI/API lookup when possible.

## Query Model

Commands are organized around facts:

- symbol lookup: `find`, `query`, `files`
- relationships: `refs`, `cross-file-refs`
- source retrieval: `get`, `get-file`, `chunks`, `chunk-by-span`,
  `chunk-by-symbol`
- structure: `ast`, `find-ast`
- graph algorithms: `reachable`, `dead-code`, `cycles`, `condense`, `paths`,
  `slice`
- graph memory: `source-inventory`, `candidate-fact`
- database health: `status`, `doctor`, `migrate`, `verify`
- maintenance: `refresh`, `backfill`, `index`, `delete`

## Coverage Data

Coverage ingestion is optional:

```bash
magellan ingest-coverage --db code.db --lcov coverage/lcov.info
```

Coverage data is attached to CFG side tables and surfaced by `status`. The JSON
shape is stable: `coverage.available`, `coverage.covered_blocks`, and
`coverage.covered_edges` are always present.

## Optional Features

Default builds use SQLite and internal parsers.

Optional features:

| Feature | Purpose |
|---------|---------|
| `external-tools-cfg` | C/C++ and Java CFG extraction through installed external tools |
| `llvm-cfg` | optional LLVM-based C/C++ CFG support |
| `bytecode-cfg` | placeholder for Java bytecode CFG work |
| `web-ui` | optional web UI server |
| `geometric-backend` | experimental geometric index code for source builds |

The public command documentation assumes the default SQLite `.db` workflow.

## Compatibility Preflight

Before opening an existing SQLite database, Magellan performs a read-only
compatibility preflight:

- rejects non-SQLite files without overwriting them
- rejects SQLite files missing `graph_meta`
- rejects missing `graph_meta.id = 1`
- rejects sqlitegraph schema mismatches

This happens before Magellan writes side tables, so incompatible databases are
not partially mutated.

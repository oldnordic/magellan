# Magellan API Integration

**Version:** 3.1.7

This guide is for downstream tools that use Magellan as a Rust library or invoke
the CLI and parse JSON output.

## Primary API

Use `CodeGraph` with a SQLite `.db` path:

```rust
use magellan::CodeGraph;

let mut graph = CodeGraph::open("code.db")?;
let source = std::fs::read("src/main.rs")?;
graph.index_file("src/main.rs", &source)?;

let symbols = graph.symbols_in_file("src/main.rs")?;
```

`CodeGraph::open()` creates a new database when the path does not exist. For
existing databases it runs compatibility checks before writing.

## Common Operations

```rust
let file_count = graph.count_files()?;
let symbol_count = graph.count_symbols()?;
let reference_count = graph.count_references()?;
let call_count = graph.count_calls()?;
let chunk_count = graph.count_chunks()?;

let symbols = graph.symbols_in_file("src/lib.rs")?;
let refs = graph.references_to_symbol("symbol_name")?;
```

For precise lookup across ambiguous names, prefer stable `symbol_id` fields from
query output.

## CLI JSON Contract

For tool integration through the CLI, use:

```bash
magellan <command> --output json
magellan <command> --output pretty
```

JSON responses include:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "hex-timestamp-hex-pid",
  "data": {}
}
```

See [JSON_EXPORT_FORMAT.md](JSON_EXPORT_FORMAT.md) for the response wrapper and
common span types.

## Stable Status Shape

`status` always includes coverage fields:

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

When coverage data exists, `source`, `revision`, and `ingested_at` are present
when known.

## Import/Export Integration

Magellan can exchange data through CLI formats:

```bash
magellan export --db code.db --format json
magellan export --db code.db --format jsonl
magellan export --db code.db --format csv
magellan export --db code.db --format scip --output graph.scip
magellan export --db code.db --format lsif --output graph.lsif
magellan import-lsif --db code.db dependency.lsif
```

## Schema Versions

- Magellan database schema: `11`
- JSON response schema: `1.0.0`

Consumers should treat both as compatibility boundaries.

## Path Handling

Normalize file paths consistently when integrating:

- pass project-relative paths when possible
- use `--root` on commands that accept it
- do not persist SQLite row IDs as cross-run identifiers
- persist `symbol_id`, `span_id`, FQN, and file path instead

## Optional Features

The default public integration path does not require optional features.

`external-tools-cfg` can be enabled for additional C/C++ and Java CFG extraction
using installed external tools. Integrators should treat this as an optional
enhancement and gracefully handle its absence.

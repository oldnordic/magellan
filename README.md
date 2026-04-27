# Magellan

**Version:** 3.1.7

Magellan is a deterministic codebase indexing tool. It watches or scans source
trees, extracts symbols, references, calls, AST nodes, code chunks, CFG data, and
coverage metadata, then stores those facts in a local SQLite database for fast
CLI and downstream-tool queries.

Magellan is intentionally fact-oriented: it records what is present in source
code and leaves higher-level reasoning to tools such as llmgrep, Mirage, and
Splice.

## Current Storage Model

The supported user-facing database is SQLite:

```text
code.db
```

Use `.db` files for normal operation. Historical alternative backend material has
been removed from the public documentation because it is not part of the current
supported workflow.

Optional source builds may include experimental geometric index code, but the
SQLite database remains the source of truth.

## Features

- Multi-language symbol extraction with tree-sitter:
  Rust, Python, C, C++, Java, JavaScript, and TypeScript
- Stable symbol IDs, canonical FQNs, display FQNs, and byte/line spans
- File watching and one-shot indexing
- References and call graph queries
- AST node storage and AST queries
- Code chunks for source retrieval and LLM context
- CFG blocks and CFG edges for control-flow analysis
- Coverage ingestion from LCOV into CFG coverage side tables
- Graph algorithms: reachability, dead code, cycles, condensation, paths, slice
- JSON, pretty JSON, human output, and graph exports
- LSIF import/export and SCIP export
- `doctor` checks for schema and database health

## Quick Start

```bash
cargo build --release

# Build an index
target/release/magellan watch --root . --db .magellan/code.db --scan-initial

# Check database contents
target/release/magellan status --db .magellan/code.db

# Query symbols in a file
target/release/magellan query --db .magellan/code.db --file src/main.rs

# Find symbols
target/release/magellan find --db .magellan/code.db --name main

# Show incoming or outgoing references/calls
target/release/magellan refs --db .magellan/code.db --name main --direction out

# Index or delete one file
target/release/magellan index --db .magellan/code.db --file src/lib.rs
target/release/magellan delete --db .magellan/code.db --file src/lib.rs

# Recompute derived metrics
target/release/magellan backfill --db .magellan/code.db
```

## Coverage

Magellan can ingest LCOV data and attach it to CFG blocks and edges:

```bash
magellan ingest-coverage --db .magellan/code.db --lcov coverage/lcov.info
magellan status --db .magellan/code.db --output pretty
```

Status JSON always includes a stable `coverage` object:

```json
{
  "coverage": {
    "available": false,
    "covered_blocks": 0,
    "covered_edges": 0
  }
}
```

When coverage exists, `source`, `revision`, and `ingested_at` are included.

## Useful Commands

```bash
magellan --help
magellan --help-full
magellan --backends

magellan doctor --db code.db
magellan doctor --db code.db --fix

magellan files --db code.db --symbols
magellan chunks --db code.db --limit 20
magellan ast --db code.db --file src/main.rs
magellan find-ast --db code.db --kind function_item

magellan reachable --db code.db --symbol <SYMBOL_ID>
magellan dead-code --db code.db --entry <SYMBOL_ID>
magellan cycles --db code.db
magellan condense --db code.db --members
magellan paths --db code.db --start <SYMBOL_ID> --max-depth 8
magellan slice --db code.db --target <SYMBOL_ID> --direction backward

magellan export --db code.db --format json --output graph.json
magellan export --db code.db --format scip --output graph.scip
magellan import-lsif --db code.db path/to/index.lsif
```

## External CFG Tools

The optional `external-tools-cfg` feature enables extra CFG extraction paths for
C/C++ and Java using installed external tools:

```bash
cargo build --release --features external-tools-cfg
cargo test --features external-tools-cfg --test external_tools_tests
```

The default build does not require clang, javac, LLVM libraries, or Java bytecode
libraries.

## Documentation

- [MANUAL.md](MANUAL.md): command reference and workflows
- [CHANGELOG.md](CHANGELOG.md): release notes
- [docs/MAGELLAN_ARCHITECTURE.md](docs/MAGELLAN_ARCHITECTURE.md): architecture
- [docs/SCHEMA_SQLITE.md](docs/SCHEMA_SQLITE.md): SQLite schema
- [docs/SCHEMA_REFERENCE.md](docs/SCHEMA_REFERENCE.md): stable IDs and data model
- [docs/API_INTEGRATION.md](docs/API_INTEGRATION.md): Rust/API integration notes
- [docs/JSON_EXPORT_FORMAT.md](docs/JSON_EXPORT_FORMAT.md): JSON response shape
- [docs/CONTEXT_API_CONTRACT.md](docs/CONTEXT_API_CONTRACT.md): context API contract
- [docs/TESTING.md](docs/TESTING.md): verification commands

## License

GPL-3.0

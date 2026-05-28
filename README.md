# Magellan

**Version:** 4.2.0

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

Use `.db` files for normal operation.

**Schema version:** 17 (telemetry events, cfg-aware CFG blocks, project metadata, FTS5 full-text search, graph memory tables)

## Features

- Multi-language symbol extraction with tree-sitter:
  Rust, Python, C, C++, Java, JavaScript, TypeScript, Go, and CUDA
- Stable symbol IDs, canonical FQNs, display FQNs, and byte/line spans
- File watching and one-shot indexing
- References and call graph queries
- AST node storage and AST queries
- Code chunks for source retrieval and editor context
- CFG blocks and CFG edges for control-flow analysis
- `#[cfg]` attribute extraction: CFG blocks inherit cfg conditions from function attributes
- Cargo.toml manifest parsing: features, dependencies, and test/bench/example targets
- `.magellan.toml` project configuration with include/exclude filters
- Auto-detect project layout from `Cargo.toml`, `pyproject.toml`, `go.mod`, `package.json`, `tsconfig.json`, `pom.xml`, `CMakeLists.txt`
- Coverage ingestion from LCOV into CFG coverage side tables
- Graph algorithms: reachability, dead code, cycles, condensation, paths, slice
- Source inventory: index wiki pages, specs, and other non-code documents
- Candidate facts: structured knowledge triples linked to source documents
- JSON, pretty JSON, human output, and graph exports
- LSIF import/export and SCIP export
- `doctor` checks for schema and database health

## Quick Start

```bash
cargo build --release

# Initialize project configuration
target/release/magellan init --path .

# Build an index
target/release/magellan watch --root ./src --db .magellan/code.db --scan-initial

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

# Refresh from git working tree changes
target/release/magellan refresh --db .magellan/code.db

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

## Service Daemon

Magellan can run as a background daemon with per-project filesystem watchers
and a JSON-RPC control socket for integration with downstream tools.

```bash
magellan service start                          # start daemon
magellan service register --root /path --name myproject \
  --include src/ --include tests/ --exclude target/
magellan service list                            # list enabled projects
magellan service status                          # detailed project info
magellan service stats                           # per-project DB statistics
magellan service events                          # daemon event log
magellan service stop                            # graceful shutdown
```

The daemon stores databases at `~/.magellan/<name>/<name>.db`. The JSON-RPC
socket (at `$XDG_RUNTIME_DIR/magellan.sock`) supports project management,
cross-project queries, and an evolution loop for automated refactoring
candidates. See `docs/API_INTEGRATION.md` for the full method reference.

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

magellan source-inventory --db code.db --scan ./wiki markdown
magellan source-inventory --db code.db --kind wiki
magellan candidate-fact submit --db code.db --from-source 1 --subject-type Task --subject-key "task-1" --predicate assigned_to
magellan candidate-fact list --db code.db --status pending

# Natural language query routing (callers, CFG, cycles, impact, semantic search, find)
magellan ask --db code.db "who calls index_file"
magellan ask --db code.db "cfg for parse_watch_args"
magellan ask --db code.db "blast zone of handle_request"
magellan ask --all "who calls index_file"

# Grounded investigation packet (term extraction → symbol find → callers/callees/impact/context)
magellan navigate --db code.db "parse_watch_args error handling"
magellan navigate --db code.db "handle_request" --concise
magellan navigate --db code.db "CodeGraph open sync" --depth 3 --with-llmgrep --with-mirage
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

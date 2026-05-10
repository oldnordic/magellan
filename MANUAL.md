# Magellan Manual

**Version:** 3.3.3

This manual documents the current user-facing Magellan CLI. The supported normal
workflow uses a SQLite `.db` database.

## Database

Use a `.db` file:

```bash
magellan watch --root . --db .magellan/code.db --scan-initial
```

SQLite is the source-of-truth storage path for current Magellan usage. Older
alternative backend instructions are intentionally absent from this manual
because they are not part of the supported public workflow.

## Output Formats

Most commands accept:

```bash
--output human
--output json
--output pretty
```

JSON output is wrapped in a stable response envelope:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "hex-timestamp-hex-pid",
  "data": {}
}
```

## Indexing

### Watch A Project

```bash
magellan watch --root <DIR> --db <FILE> [--scan-initial] [--watch-only]
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `--scan-initial` | Scan source files before watching |
| `--watch-only` | Watch future changes without an initial scan |
| `--debounce-ms <N>` | Debounce filesystem events |
| `--validate` | Run validation checks |
| `--validate-only` | Validate without indexing |
| `--gitignore-aware` | Honor ignore rules |
| `--no-gitignore` | Disable ignore filtering |

### Index One File

```bash
magellan index --db code.db --file src/lib.rs [--root .]
```

### Delete One File

```bash
magellan delete --db code.db --file src/lib.rs [--root .]
```

### Refresh From Git

```bash
magellan refresh --db code.db [--dry-run] [--include-untracked]
magellan refresh --db code.db [--staged | --unstaged]
magellan refresh --db code.db --force
```

`refresh` uses git status to re-index changed files and remove deleted files.

## Status And Health

### Status

```bash
magellan status --db code.db
magellan status --db code.db --output pretty
```

Status reports file, symbol, reference, call, chunk, and coverage counts.

JSON status always includes:

```json
{
  "coverage": {
    "available": false,
    "covered_blocks": 0,
    "covered_edges": 0
  }
}
```

### Doctor

```bash
magellan doctor --db code.db
magellan doctor --db code.db --fix
```

`doctor` checks database readability, schema state, indexes, and coverage schema
health. `--fix` applies supported repairs.

### Migration

```bash
magellan migrate --db code.db
magellan migrate --db code.db --dry-run
magellan migrate --db code.db --no-backup
```

Current Magellan schema version: `14`.

**Schema v12 changes:** Added FTS5 full-text search index for fast prefix search.
Migration is automatic and creates a backup. See [docs/SCHEMA_SQLITE.md](docs/SCHEMA_SQLITE.md)
for FTS5 performance details and limitations.

## Query Commands

### Symbols In A File

```bash
magellan query --db code.db --file src/main.rs
magellan query --db code.db --file src/main.rs --kind fn
magellan query --db code.db --symbol parse_args --show-extent
```

Rich output flags:

```bash
--with-context
--with-callers
--with-callees
--with-semantics
--with-checksums
--context-lines <N>
```

### Find Symbols

```bash
magellan find --db code.db --name parse_args
magellan find --db code.db --name parse_args --path src/main.rs
magellan find --db code.db --symbol-id <SYMBOL_ID>
magellan find --db code.db --ambiguous parse_args
```

### References And Calls

```bash
magellan refs --db code.db --name parse_args --direction in
magellan refs --db code.db --name parse_args --direction out
magellan refs --db code.db --symbol-id <SYMBOL_ID> --direction out
```

### Cross-File References

```bash
magellan cross-file-refs --db code.db --fqn crate::module::symbol
magellan cross-file-refs --db code.db --fqn crate::module::symbol --output pretty
```

### Registry (Cross-Project Discovery)

```bash
# Scan for databases
magellan registry scan --root /home/feanor/Projects
magellan registry scan --root . --output json

# List discovered databases
magellan registry list --root /home/feanor/Projects
```

### Configuration

```bash
# Show current configuration
magellan config show
magellan config show --output json

# Initialize default config
magellan config init
```

Config is stored in `~/.config/magellan/config.toml`.

## Source Retrieval

```bash
magellan get --db code.db --file src/main.rs --symbol main
magellan get-file --db code.db --file src/main.rs
magellan chunks --db code.db --limit 20
magellan chunk-by-span --db code.db --file src/main.rs --start 0 --end 100
magellan chunk-by-symbol --db code.db --symbol main
```

## AST Queries

```bash
magellan ast --db code.db --file src/main.rs
magellan ast --db code.db --file src/main.rs --position 120
magellan find-ast --db code.db --kind function_item
```

## Labels And Collisions

```bash
magellan label --db code.db --list
magellan label --db code.db --label rust --label fn
magellan label --db code.db --label fn --count

magellan collisions --db code.db
magellan collisions --db code.db --field fqn --limit 20
```

## Graph Algorithms

Algorithm commands use stable symbol IDs.

```bash
magellan reachable --db code.db --symbol <SYMBOL_ID>
magellan reachable --db code.db --symbol <SYMBOL_ID> --reverse

magellan dead-code --db code.db --entry <SYMBOL_ID>
magellan cycles --db code.db
magellan cycles --db code.db --symbol <SYMBOL_ID>
magellan condense --db code.db --members

magellan paths --db code.db --start <SYMBOL_ID> --end <SYMBOL_ID>
magellan paths --db code.db --start <SYMBOL_ID> --max-depth 8 --max-paths 50

magellan slice --db code.db --target <SYMBOL_ID> --direction backward
magellan slice --db code.db --target <SYMBOL_ID> --direction forward --verbose
```

## Context Analysis Commands

Context commands provide symbol-centric context for automated code analysis — definition, callers, callees, impact analysis, and source code snippets.

### Build Context Index

```bash
magellan context build --db code.db
```

Builds the `.magellan/<project>.context.json` summary index. Required once per database before using summary commands.

### Project Summary

```bash
magellan context summary --db code.db
```

Shows project name, version, language, file/symbol counts, and entry points.

### List Symbols (Paginated)

```bash
magellan context list --db code.db
magellan context list --db code.db --kind fn --page 2 --project magellan
magellan context list --db code.db --output json
```

Multi-DB: pass a directory to `--db` and all `.magellan/*.db` files are queried.

### Symbol Detail

```bash
magellan context symbol --db code.db --name parse_args
magellan context symbol --db code.db --name parse_args --callers --callees
magellan context symbol --db code.db --name parse_args --with-source --depth 2
magellan context symbol --db code.db --name parse_args --file src/main.rs --output json
```

Flags:

| Flag | Meaning |
|------|---------|
| `--name <NAME>` | Symbol name to look up (**required**) |
| `--file <PATH>` | Limit search to specific file (optional) |
| `--callers` | Include caller references |
| `--callees` | Include callee references |
| `--with-source` | Include source code snippet |
| `--depth <N>` | Recursive lookup depth (default: 1) |
| `--project <NAME>` | Filter to single project in multi-DB mode |
| `--output <FORMAT>` | `human` (default), `json`, or `pretty` |

### File Context

```bash
magellan context file --db code.db --path src/main.rs
```

Shows symbols in file, language, public symbols, imports.

### Impact Analysis (Blast Radius)

Find all symbols that transitively call the target — "what breaks if I change this?"

```bash
magellan context impact --db code.db --name parse_args --depth 3
magellan context impact --db code.db --name parse_args --file src/main.rs --depth 2 --output json
```

### Affected Analysis (Dependency Reach)

Find all symbols that the target transitively calls — "what does this symbol depend on?"

```bash
magellan context affected --db code.db --name run_main --depth 3
magellan context affected --db code.db --name run_main --output json
```

### Multi-DB Queries

All context commands except `build`, `summary`, and `file` support multi-DB mode:

```bash
magellan context list --db .magellan/ --output json
magellan context symbol --db .magellan/ --name main --callers
magellan context impact --db .magellan/ --symbol extract_symbols --depth 2
```

When `--db` points to a directory, all `.magellan/*.db` files are queried and results are tagged by project name.

## Coverage

```bash
magellan ingest-coverage --db code.db --lcov coverage/lcov.info
magellan status --db code.db --output pretty
```

Coverage is stored in side tables:

- `cfg_block_coverage`
- `cfg_edge_coverage`
- `cfg_coverage_meta`

## Import And Export

```bash
magellan export --db code.db --format json
magellan export --db code.db --format jsonl
magellan export --db code.db --format csv
magellan export --db code.db --format scip --output graph.scip
magellan export --db code.db --format dot --output graph.dot
magellan export --db code.db --format lsif --output graph.lsif

magellan import-lsif --db code.db path/to/index.lsif
```

Export filters:

```bash
--no-symbols
--no-references
--no-calls
--include-collisions
--collisions-field <fqn|display_fqn|canonical_fqn>
--minify
```

## Context And Enrichment

```bash
magellan context build --db code.db
magellan context summary --db code.db
magellan context list --db code.db --kind fn --page-size 50
magellan context symbol --db code.db --name main --callers --callees
magellan context file --db code.db --path src/main.rs

magellan enrich --db code.db
magellan enrich --db code.db --file src/main.rs --timeout 30
```

`enrich` uses available language tools such as rust-analyzer, clangd, or jdtls
when present. Missing tools degrade gracefully.

## External CFG Tools

Default builds use internal parsers. Optional external CFG support can be built:

```bash
cargo build --release --features external-tools-cfg
cargo test --features external-tools-cfg --test external_tools_tests
```

This feature uses installed external tools for C/C++ and Java CFG extraction.

## Supported Languages

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py` |
| C | `.c`, `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| Java | `.java` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| TypeScript | `.ts`, `.tsx` |

Unsupported extensions are ignored during directory scans and return zero
symbols when indexed directly.

# Magellan Manual

**Version:** 3.1.7

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

Current Magellan schema version: `11`.

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

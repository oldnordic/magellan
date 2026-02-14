# Magellan

**Version:** 2.4.2

A deterministic codebase mapping tool. Watches source files, extracts AST-level facts, and builds a searchable graph database of symbols and references.

## Purpose

**Magellan is the foundation of the Code Intelligence ecosystem.** Built to power:

- **[LLMGrep](https://github.com/oldnordic/llmgrep)** — Semantic code search using natural language queries
- **[Mirage](https://github.com/oldnordic/mirage)** — AI-powered code navigation assistant  
- **[Splice](https://github.com/oldnordic/splice)** — Intelligent code refactoring engine

While usable standalone, Magellan is designed as infrastructure for downstream tools to answer questions like "find all callers of this function" with millisecond latency on multi-million line codebases.

## Features

- **Watch** directories for file changes (Create/Modify/Delete)
- **Extract** AST-level facts: functions, classes, methods, enums, modules (7 languages)
- **Track** symbol references and build call graphs (caller → callee)
- **Store** AST nodes for hierarchical code structure analysis
- **Compute** metrics: fan-in, fan-out, LOC, complexity per file/symbol
- **Run** graph algorithms: reachability, dead code detection, cycles, paths, slicing
- **Export** to JSON, JSONL, CSV, SCIP, DOT formats

## Quick Start

```bash
# Install
cargo install magellan

# Start watching a project
magellan watch --root /path/to/project --db ~/.cache/magellan/project.db --scan-initial

# Query symbols in a file
magellan query --db ~/.cache/magellan/project.db --file src/main.rs

# Find a symbol
magellan find --db ~/.cache/magellan/project.db --name main

# Show call references
magellan refs --db ~/.cache/magellan/project.db --name parse_args --path src/main.rs

# Check status
magellan status --db ~/.cache/magellan/project.db
```

## Installation

```bash
cargo install magellan
```

Or build from source with specific backend:

```bash
# V3 backend (recommended for production - fastest, no SQLite dependency)
cargo build --release --features native-v3

# SQLite backend (default - best compatibility)
cargo build --release --features sqlite-backend
```

## Backends

| Feature | Description | File | Use Case |
|---------|-------------|------|----------|
| `native-v3` | **High-performance binary backend** with KV store | `.v3` | Production (recommended) |
| `sqlite-backend` | Stable SQLite backend | `.db` | Compatibility, debugging |

Both backends have **full feature parity** (v2.4.0):
- Graph operations, symbol indexing, call traversal
- AST nodes, code chunks, execution logging
- File/symbol metrics, graph algorithms

## Supported Languages

| Language | Extensions |
|----------|------------|
| Rust | .rs |
| C / C++ | .c, .h, .cpp, .cc, .cxx, .hpp |
| Java | .java |
| JavaScript / TypeScript | .js, .mjs, .ts, .tsx |
| Python | .py |

## Documentation

- **[MANUAL.md](MANUAL.md)** — Complete command reference, architecture, and examples
- **[CHANGELOG.md](CHANGELOG.md)** — Version history and release notes
- **[AGENTS.md](AGENTS.md)** — Development guidelines for contributors

## What Magellan Does NOT Do

- No semantic analysis or type checking
- No LSP server or language features
- No async runtimes or background thread pools
- No config files
- No web APIs or network services

## License

MIT

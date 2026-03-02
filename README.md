# Magellan

**Version:** 3.0.0

A deterministic codebase mapping tool. Watches source files, extracts AST-level facts, and builds a searchable graph database of symbols and references.

## Purpose

**Magellan is the foundation of the Code Intelligence ecosystem.** Built to power:

- **[LLMGrep](https://github.com/oldnordic/llmgrep)** — Semantic code search using natural language queries
- **[Mirage](https://github.com/oldnordic/mirage)** — AI-powered code navigation assistant
- **[Splice](https://github.com/oldnordic/splice)** — Intelligent code refactoring engine

While usable standalone, Magellan is designed as infrastructure for downstream tools to answer questions like "find all callers of this function" with millisecond latency on multi-million line codebases.

## Features

- **Watch** directories for file changes (Create/Modify/Delete) with async I/O
- **Extract** AST-level facts: functions, classes, methods, enums, modules (7 languages)
- **Track** symbol references and build call graphs (caller → callee)
- **Store** AST nodes for hierarchical code structure analysis
- **Compute** metrics: fan-in, fan-out, LOC, complexity per file/symbol
- **Run** graph algorithms: reachability, dead code detection, cycles, paths, slicing
- **Export** to JSON, JSONL, CSV, SCIP, DOT, **LSIF** formats
- **LLM Context Queries** - Summarized, paginated context for efficient AI integration
- **LSP Enrichment** - Type signatures from rust-analyzer, jdtls, clangd
- **Self-Diagnostics** - `magellan doctor` command for troubleshooting

## Quick Start

```bash
# Install
cargo install magellan

# Start watching a project (with progress bar!)
magellan watch --root /path/to/project --db ~/.cache/magellan/project.db --scan-initial

# Query symbols in a file
magellan query --db ~/.cache/magellan/project.db --file src/main.rs

# Find a symbol (with better error messages)
magellan find --db ~/.cache/magellan/project.db --name main

# Show call references
magellan refs --db ~/.cache/magellan/project.db --name parse_args --path src/main.rs

# LLM Context Queries (NEW in v3.0.0)
magellan context summary --db code.db           # Project overview (~50 tokens)
magellan context list --db code.db --kind fn    # List functions (paginated)
magellan context symbol --db code.db --name main --callers --callees

# Self-diagnostics (NEW in v3.0.0)
magellan doctor --db code.db
magellan doctor --db code.db --fix  # Auto-fix issues

# LSP Enrichment (NEW in v3.0.0)
magellan enrich --db code.db  # Extract type signatures

# Cross-repo navigation (NEW in v3.0.0)
magellan export --db code.db --format lsif --output project.lsif
magellan import-lsif --db code.db --input dependency.lsif
```

## Proof of Seriousness

Real commands, real output. No marketing.

### 1. Self-Diagnostics (`magellan doctor --fix`)

```bash
$ magellan doctor --db ~/.cache/magellan/project.db --fix

🔍 Magellan Doctor - Diagnosing issues...

Checking database file... ✅ OK
Checking database readability... ✅ OK
Checking schema version... ✅ OK
Checking symbol index... ✅ OK (2271 symbols)
Checking file index... ✅ OK (143 files)
Checking call graph... ✅ OK (2733 calls)
Checking database size... ✅ OK (14.8 MB)
Checking WAL file... ✅ OK (0.0 MB)
Checking context index (v3.0.0)... ⚠️  MISSING
   Context index not built
   Auto-fix: Building context index...
   ✅ Context index built

==================================================
⚠️  Found 1 issue(s), 1 fixed
```

### 2. Progress Transparency (`magellan watch --scan-initial`)

```bash
$ magellan watch --root . --db project.db --scan-initial

Using SQLite backend: "project.db"
Scanning: src/main.rs [=====>              ] 23/143 (16%) ETA: 2s
Scanning: src/graph/mod.rs [==========>       ] 67/143 (47%) ETA: 1s
Scanning: src/indexer.rs [================>   ] 121/143 (85%) ETA: 0s
Scanned 143 files
Magellan watching: .
Database: project.db
```

### 3. Type Signatures (`magellan enrich`)

```bash
$ magellan enrich --db project.db

Found 1 analyzer(s):
  - rust-analyzer

Enriching "src/main.rs" with rust-analyzer
    Found signature for 'main': fn main() -> Result<(), anyhow::Error>
    Found signature for 'run_indexer': pub fn run_indexer(root_path: PathBuf, db_path: PathBuf) -> Result<()>
  Enriched 2 symbols

Enrichment complete:
  Files processed: 143
  Symbols enriched: 487
  Errors: 0
```

### 4. LLM Workflow (Magellan + LLMGrep + Splice)

```bash
# Step 1: Build context for LLM
$ magellan context summary --db project.db
magellan 3.0.0 written in Rust, 143 files, 2271 symbols (1942 functions, 150 structs)

# Step 2: Find all callers of a function
$ llmgrep search --db project.db --query "run_indexer" --mode calls --output human
total: 3
/home/user/project/src/main.rs:52:0 main Function score=100
/home/user/project/src/watch_cmd.rs:23:0 run_watch Function score=80
/home/user/project/src/indexer.rs:92:0 run_indexer_n Function score=80

# Step 3: Safe refactoring with Splice
$ splice refactor --db project.db --function run_indexer --rename "start_indexer"
Checking refactoring safety...
  ✅ All callers updated (3 locations)
  ✅ Type signatures preserved
  ✅ No breaking changes detected
```

---

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
| `native-v3` | **High-performance binary backend** with KV store | `.v3` | Recommended for performance |
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
- No config files
- No web APIs or network services

## License

GPL-3.0-or-later

# Magellan

**Version:** 2.4.0

A deterministic codebase mapping tool. Watches source files, extracts AST-level facts, and builds a searchable graph database of symbols and references.

## Purpose

**Magellan is the foundation of the Code Intelligence ecosystem.** It was built specifically to power:

- **[LLMGrep](https://github.com/oldnordic/llmgrep)** — Semantic code search using natural language queries against indexed codebases
- **[Mirage](https://github.com/oldnordic/mirage)** — AI-powered code navigation and comprehension assistant
- **[Splice](https://github.com/oldnordic/splice)** — Intelligent code refactoring and transformation engine

While Magellan can be used standalone, it is designed as infrastructure. The graph database it builds enables downstream tools to answer questions like "find all callers of this function" or "what code depends on this symbol" with millisecond latency on multi-million line codebases.

## What Magellan Does

- Watches directories for file changes (Create/Modify/Delete)
- Extracts AST-level facts: functions, classes, methods, enums, modules
- Tracks symbol references: function calls and type references (7 languages)
- Builds call graphs: caller → callee relationships across indexed files (7 languages)
- Stores AST nodes for hierarchical code structure analysis (v1.9.0)
- Graph algorithms: reachability, dead code detection, cycles, paths, slicing (v2.0.0)
- Stores code chunks for token-efficient LLM context (v1.8.0)
- Computes metrics: fan-in, fan-out, LOC, complexity per file/symbol (v1.8.0)
- Safely extracts UTF-8 content from byte offsets without panicking on multi-byte characters (v1.8.0)
- Persists everything to a sqlitegraph database
- Handles errors gracefully - keeps running even when files are unreadable
- Shuts down cleanly on SIGINT/SIGTERM

## What Magellan Does NOT Do

- No semantic analysis or type checking
- No LSP server or language features
- No async runtimes or background thread pools
- No config files
- No web APIs or network services
- No automatic database cleanup

## Backend Feature Parity (v2.4.0)

Magellan has two storage backends with full feature parity:

| Feature | SQLite Backend | V3 Backend |
|---------|---------------|------------|
| Graph operations (nodes/edges) | ✓ | ✓ |
| Symbol indexing/querying | ✓ | ✓ |
| Call graph traversal | ✓ | ✓ |
| AST nodes storage/query | ✓ | ✓ |
| Code chunks storage | ✓ | ✓ |
| Execution logging | ✓ | ✓ |
| File/symbol metrics | ✓ | ✓ |
| Graph algorithms (cycles, dead code, etc.) | ✓ | ✓ |

**Recommendation:** Use V3 backend (`--features native-v3`) for production - it's faster and has zero SQLite dependency.

## Installation

```bash
cargo install magellan
```

Or build from source:
```bash
git clone https://github.com/oldnordic/magellan
cd magellan
cargo build --release

# Binary will be at target/release/magellan
```

### Requirements

- Rust 1.70+
- Linux/macOS (signal handling uses Unix signals)
- SQLite 3 (via sqlitegraph dependency)

### Features

- **Help**: Use `--help` or `-h` with any command to see usage information
- **Native-v3 Backend**: Build with `--features native-v3` for optimal performance (recommended)
  - High-performance binary backend with KV store side tables
  - All data in single `.v3` file (no SQLite dependency)
- **Native-v2 Backend**: Build with `--features native-v2` for improved performance (legacy)
- **LLVM IR CFG (optional)**: Build with `--features llvm-cfg` for C/C++ (requires Clang)
- **Bytecode CFG (optional)**: Build with `--features bytecode-cfg` for Java (requires JVM bytecode)

**Backend Selection:**

Magellan supports multiple backends via feature flags:

| Feature | Description | Use Case |
|---------|-------------|----------|
| `native-v3` | **High-performance binary backend** with KV store | Production (recommended) |
| `sqlite-backend` | Stable SQLite backend (default) | Compatibility, debugging |
| `native-v2` | Legacy binary backend (deprecated) | Legacy support only |

**Note:** Only one backend feature should be enabled at a time. The V3 backend stores all data (graph + side tables) in a single `.v3` file with zero SQLite dependency.

**Optional CFG Feature Flags (v2.1.0):**

Magellan includes optional CFG extraction enhancements:

| Feature | Description | Requires |
|---------|-------------|----------|
| `llvm-cfg` | LLVM IR-based CFG for C/C++ | Clang installation |
| `bytecode-cfg` | JVM bytecode CFG for Java | javac compilation |

These are **optional enhancements** — Magellan works fine without them. AST-based CFG (included by default) works for all languages.

```bash
# Build with LLVM IR CFG support (requires Clang)
cargo build --release --features llvm-cfg

# Build with bytecode CFG support (Java)
cargo build --release --features bytecode-cfg
```

**Note:** The optional features add infrastructure only. Full LLVM IR and bytecode CFG implementation is planned for future releases. See `docs/CFG_LIMITATIONS.md` for details.

### Quick Start

```bash
# Start watching a project with initial scan
magellan watch --root /path/to/project --db ~/.cache/magellan/project.db --scan-initial

# Check status
magellan status --db /path/to/magellan.db

# List all indexed files
magellan files --db /path/to/magellan.db

# Query symbols in a file (or run --explain for selector help)
magellan query --db /path/to/magellan.db --file /path/to/file.rs
# Print the selector cheat sheet
magellan query --db /path/to/magellan.db --explain
# Show the byte/line span for a specific symbol
magellan query --db /path/to/magellan.db --file src/lib.rs --symbol main --show-extent

# Find a symbol by name (v1.5: use --symbol-id for precise lookup)
magellan find --db /path/to/magellan.db --name main
magellan find --db /path/to/magellan.db --symbol-id <SYMBOL_ID>
# Show all candidates for an ambiguous name
magellan find --db /path/to/magellan.db --ambiguous main
# List all symbols that match a glob pattern
magellan find --db /path/to/magellan.db --list-glob "handler_*"

# Show call references
magellan refs --db /path/to/magellan.db --name main --path /path/to/file.rs --direction out

# Query by labels
magellan label --db /path/to/magellan.db --list
magellan label --db /path/to/magellan.db --label rust --label fn
magellan label --db /path/to/magellan.db --label struct --show-code

# Get code chunks without re-reading files
magellan get --db /path/to/magellan.db --file /path/to/file.rs --symbol main
magellan get-file --db /path/to/magellan.db --file /path/to/file.rs

# List and query chunks (v1.8.0)
magellan chunks --db /path/to/magellan.db --limit 50 --kind fn
magellan chunk-by-symbol --db /path/to/magellan.db --symbol main
magellan chunk-by-span --db /path/to/magellan.db --file src/main.rs --start 100 --end 200

# List ambiguous symbols (v1.5)
magellan collisions --db /path/to/magellan.db

# Export to various formats (v1.5: jsonl, csv, scip, dot)
magellan export --db /path/to/magellan.db > codegraph.json
magellan export --db /path/to/magellan.db --format jsonl > codegraph.jsonl
magellan export --db /path/to/magellan.db --format scip --output codegraph.scip
magellan export --db /path/to/magellan.db --format dot | dot -Tpng -o graph.png

# Migrate database to latest schema (v1.5)
magellan migrate --db /path/to/magellan.db
```

## Commands

### watch

```bash
magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--scan-initial] [--gitignore-aware] [--no-gitignore]
```

Watch a directory for source file changes and index them into the database.

| Argument | Description |
|----------|-------------|
| `--root <DIR>` | Directory to watch recursively (required) |
| `--db <FILE>` | Path to sqlitegraph database (required) |
| `--debounce-ms <N>` | Debounce delay in milliseconds (default: 500) |
| `--scan-initial` | Scan directory for source files on startup |
| `--gitignore-aware` | Enable .gitignore filtering (default: enabled) |
| `--no-gitignore` | Disable .gitignore filtering (index all files) |

**Gitignore Support (v2.1.0):**

By default, Magellan respects `.gitignore` files when watching directories. This means:
- Files and directories matching `.gitignore` patterns are not indexed
- Common build artifacts are automatically skipped (`target/`, `node_modules/`, `__pycache__/`, etc.)
- You can run `magellan watch --root .` without manually excluding dependencies

To disable gitignore filtering and index all files:
```bash
magellan watch --root . --db ./magellan.db --no-gitignore
```

### status

```bash
magellan status --db <FILE>
```

Show database statistics.

```
$ magellan status --db ./magellan.db
files: 30
symbols: 349
references: 262
```

### files

```bash
magellan files --db <FILE>
```

List all indexed files.

```
$ magellan files --db ./magellan.db
30 indexed files:
  /path/to/src/main.rs
  /path/to/src/lib.rs
  ...
```

### query

```bash
magellan query --db <FILE> --file <PATH> [--kind <KIND>] [--symbol <NAME>] [--show-extent]
magellan query --db <FILE> --explain
```

List symbols in a file, optionally filtered by kind or symbol name. `--symbol <NAME>` narrows
output to a specific identifier, and `--show-extent` prints byte/line spans plus node IDs when
used with `--symbol`. `--explain` prints a selector cheat sheet covering available filters and
their syntax. Each result line shows both the human-friendly kind and a normalized tag in square
brackets (e.g., `[fn]`, `[struct]`) so automation can ingest the output deterministically.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--file <PATH>` | File path to query (required) |
| `--kind <KIND>` | Filter by symbol kind (optional) |
| `--symbol <NAME>` | Limit results to a specific symbol (optional) |
| `--show-extent` | Print byte + line ranges for the selected symbol (requires `--symbol`) |
| `--explain` | Show selector documentation instead of querying |

Valid kinds: Function, Method, Class, Interface, Enum, Module, Union, Namespace, TypeAlias

```
$ magellan query --db ./magellan.db --file src/main.rs --kind Function
/path/to/src/main.rs:
  Line   13: Function     print_usage
  Line   64: Function     parse_args
```

### find

```bash
magellan find --db <FILE> --name <NAME> [--path <PATH>] [--symbol-id <ID>] [--ambiguous <NAME>] [--first]
magellan find --db <FILE> --list-glob "<PATTERN>"
```

Find a symbol by name or preview all symbols that match a glob expression. Glob listings include
node IDs for deterministic scripting (e.g., feeding results to refactoring tooling).

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--name <NAME>` | Symbol name to find |
| `--symbol-id <ID>` | Stable SymbolId for precise lookup (v1.5) |
| `--ambiguous <NAME>` | Show all candidates for an ambiguous name (v1.5) |
| `--path <PATH>` | Limit search to specific file (optional) |
| `--list-glob <PATTERN>` | List all symbol names that match the glob (mutually exclusive with `--name`) |
| `--first` | Use first match when ambiguous (deprecated; use --symbol-id) |

```
$ magellan find --db ./magellan.db --name main
Found "main":
  File:     /path/to/src/main.rs
  Kind:     Function
  Location: Line 229, Column 0

$ magellan find --db ./magellan.db --ambiguous main
Ambiguous name "main" has 3 candidates:
  [1] a1b2c3d4e5f67890123456789012ab - src/bin/main.rs::Function main
  [2] b2c3d4e5f678901234567890123cd - src/lib.rs::Function main
  [3] c3d4e5f6789012345678901234de - tests/integration_test.rs::Function main
```

### refs

```bash
magellan refs --db <FILE> --name <NAME> --path <PATH> [--direction <in|out>]
```

Show incoming or outgoing calls for a symbol. Incoming calls include callers from
other indexed files when the target symbol name is unique in the database.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--name <NAME>` | Symbol name (required) |
| `--path <PATH>` | File path containing the symbol (required) |
| `--direction <in|out>` | Show incoming (in) or outgoing (out) calls (default: in) |

```
$ magellan refs --db ./magellan.db --name parse_args --path src/main.rs --direction in
Calls TO "parse_args":
  From: main (Function) at /path/to/src/main.rs:237
```

### verify

```bash
magellan verify --root <DIR> --db <FILE>
```

Compare database state vs filesystem and report differences.

Exit codes: 0 = up to date, 1 = issues found

### export

```bash
magellan export --db <FILE> [--format json|jsonl|csv|scip|dot] [--output <PATH>] [--minify] [--include-collisions]
```

Export all graph data to various formats.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--format <FORMAT>` | Export format: json (default), jsonl, csv, scip, dot |
| `--output <PATH>` | Write to file instead of stdout |
| `--minify` | Use compact JSON (no pretty-printing) |
| `--no-symbols` | Exclude symbols from export |
| `--no-references` | Exclude references from export |
| `--no-calls` | Exclude calls from export |
| `--include-collisions` | Include collision groups (JSON only) |

**Export Versions:**

| Version | Changes |
|---------|---------|
| 2.0.0 | Added `symbol_id`, `canonical_fqn`, `display_fqn` fields to SymbolExport |

**Format-Specific Version Encoding:**

- **JSON**: Top-level `version` field
- **JSONL**: First line is `{"type":"Version","version":"2.0.0"}`
- **CSV**: Header comment `# Magellan Export Version: 2.0.0`
- **SCIP**: Metadata includes version information
- **DOT**: No version field (graphviz format)

**Examples:**

```bash
# JSON export (default)
magellan export --db ./magellan.db > codegraph.json

# JSON Lines (one JSON object per line)
magellan export --db ./magellan.db --format jsonl > codegraph.jsonl

# CSV export
magellan export --db ./magellan.db --format csv > codegraph.csv

# SCIP export (binary, requires --output)
magellan export --db ./magellan.db --format scip --output codegraph.scip

# DOT graph format (pipe to graphviz)
magellan export --db ./magellan.db --format dot | dot -Tpng -o graph.png

# Include collision information in JSON
magellan export --db ./magellan.db --include-collisions > codegraph.json
```

**CSV Export Notes (v2.1.0):**
- CSV format uses a `record_type` column to distinguish Symbol, Reference, and Call records
- All rows have consistent column counts (empty strings for non-applicable fields)
- Version header: `# Magellan Export Version: 2.0.0`
- Collision groups are not included in CSV export (use JSON with `--include-collisions`)

```bash
# Export only symbols (no references, no calls)
magellan export --db ./magellan.db --format csv --no-references --no-calls > symbols.csv

# Export only calls
magellan export --db ./magellan.db --format csv --no-symbols --no-references > calls.csv
```

### collisions

```bash
magellan collisions --db <FILE> [--field <FIELD>] [--limit <N>]
```

List ambiguous symbols that share the same FQN or display FQN (v1.5).

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--field <FIELD>` | Field to check: fqn, display_fqn, canonical_fqn (default: display_fqn) |
| `--limit <N>` | Maximum groups to show (default: 50) |

```
$ magellan collisions --db ./magellan.db
Collisions by display_fqn:

main (3)
  [1] a1b2c3d4e5f67890123456789012ab src/bin/main.rs
       my_crate::src/bin/main.rs::Function main
  [2] b2c3d4e5f678901234567890123cd src/lib.rs
       my_crate::src/lib.rs::Function main
  [3] c3d4e5f6789012345678901234de tests/integration_test.rs
       my_crate::tests/integration_test.rs::Function main
```

### migrate

```bash
magellan migrate --db <FILE> [--dry-run] [--no-backup]
```

Upgrade a Magellan database to the current schema version (v1.5).

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--dry-run` | Check version without migrating |
| `--no-backup` | Skip backup creation (not recommended) |

**Migration Behavior:**

- Creates timestamped backup before migration (`<db>.v<timestamp>.bak`)
- Uses SQLite transaction for atomicity (rollback on error)
- Shows old version and new version before running
- No-op if database already at current version

**Schema Version 4 (v1.5 BLAKE3 SymbolId):**

Version 4 introduces BLAKE3-based SymbolId and canonical_fqn/display_fqn fields:
- New symbols get 32-character BLAKE3 hash IDs (128 bits)
- Existing symbols have `symbol_id: null` in exports
- To get BLAKE3 IDs for all symbols, re-index after migration

### Security

#### Database File Placement

Magellan's database (`--db <FILE>`) stores all indexed code information.

**Recommended:** Place `.db` files outside watched directories.

Placing the database inside a watched directory can cause:
- The watcher to process the database as if it's a source file
- Export operations to include binary database content
- Circular file system events

**Examples:**

```bash
# Recommended: database outside watched directory
magellan watch --root /path/to/project --db ~/.cache/magellan/project.db --scan-initial

# Discouraged: database inside watched directory
magellan watch --root . --db ./magellan.db --scan-initial
```

#### Recommended Database Locations

- **Linux/macOS:** `~/.cache/magellan/` or `~/.local/share/magellan/`
- **Windows:** `%LOCALAPPDATA%\magellan\`
- **CI/CD:** Use a cache directory outside the workspace

#### Path Traversal Protection

Magellan validates all file paths to prevent directory traversal attacks:
- Paths with `../` patterns are validated before access
- Symlinks pointing outside the project root are rejected
- Absolute paths outside the watched directory are blocked

These protections are implemented in `src/validation.rs` and applied during:
- Watcher event processing
- Directory scanning
- File indexing operations

### label

```bash
magellan label --db <FILE> [--label <LABEL>]... [--list] [--count] [--show-code]
```

Query symbols by labels. Labels are automatically assigned during indexing:
- **Language labels**: `rust`, `python`, `javascript`, `typescript`, `c`, `cpp`, `java`
- **Symbol kind labels**: `fn`, `method`, `struct`, `class`, `enum`, `interface`, `module`, `union`, `namespace`, `typealias`

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--label <LABEL>` | Label to query (can be specified multiple times for AND semantics) |
| `--list` | List all available labels with counts |
| `--count` | Count entities with specified label(s) |
| `--show-code` | Show actual source code for each symbol |

```
$ magellan label --db ./magellan.db --list
12 labels in use:
  rust (349)
  fn (120)
  struct (45)
  method (89)
  ...

$ magellan label --db ./magellan.db --label rust --label fn
120 symbols with labels [rust, fn]:
  main (fn) in src/main.rs [0-36]
  new (fn) in src/user.rs [91-138]
  ...

$ magellan label --db ./magellan.db --label rust --label fn --show-code
120 symbols with labels [rust, fn]:
  main (fn) in src/main.rs [0-36]
    fn main() {
        println!("Hello");
    }
```

### get

```bash
magellan get --db <FILE> --file <PATH> --symbol <NAME>
```

Get code chunks for a specific symbol. Uses stored code chunks so you don't need to re-read source files.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--file <PATH>` | File path (required) |
| `--symbol <NAME>` | Symbol name (required) |

### get-file

```bash
magellan get-file --db <FILE> --file <PATH>
```

Get all code chunks from a file. Useful for getting complete file contents without re-reading the source.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--file <PATH>` | File path (required) |

### chunks

```bash
magellan chunks --db <FILE> [--limit N] [--file PATTERN] [--kind KIND] [--output FORMAT]
```

List all code chunks in the database (v1.8.0). Code chunks are source code snippets stored by byte span during file indexing, enabling token-efficient queries without re-reading files.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--limit N` | Limit number of chunks returned |
| `--file PATTERN` | Filter by file path pattern (substring match) |
| `--kind KIND` | Filter by symbol kind (fn, struct, method, class, etc.) |
| `--output FORMAT` | Output format: human, json, pretty |

```
$ magellan chunks --db ./magellan.db --kind fn --limit 10
10 chunks:
  src/main.rs:100-200 [fn] main
  src/lib.rs:50-120 [fn] helper
```

### chunk-by-span

```bash
magellan chunk-by-span --db <FILE> --file <PATH> --start <N> --end <N> [--output FORMAT]
```

Get a code chunk by file path and exact byte range (v1.8.0). Useful for retrieving code when you know the precise byte offsets from tree-sitter.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--file <PATH>` | File path containing the chunk (required) |
| `--start <N>` | Byte offset where chunk starts (required) |
| `--end <N>` | Byte offset where chunk ends (required) |
| `--output FORMAT` | Output format: human, json, pretty |

### chunk-by-symbol

```bash
magellan chunk-by-symbol --db <FILE> --symbol <NAME> [--file PATTERN] [--output FORMAT]
```

Get all code chunks for a symbol name (v1.8.0). Performs a global search across all files, unlike `get` which requires a specific file path.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--symbol <NAME>` | Symbol name to find (required) |
| `--file PATTERN` | Filter by file path pattern (optional) |
| `--output FORMAT` | Output format: human, json, pretty |

```
$ magellan chunk-by-symbol --db ./magellan.db --symbol main
Found 3 chunks for "main":
  src/bin/main.rs:100-200 [fn] main
  src/lib.rs:50-150 [fn] main
  tests/test.rs:10-50 [fn] main
```

### ast

```bash
magellan ast --db <FILE> --file <PATH> [--position <N>] [--output FORMAT]
```

Show AST tree for a file (v1.9.0). Displays the hierarchical structure of Abstract Syntax Tree nodes extracted during indexing.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--file <PATH>` | File path to query (required) |
| `--position <N>` | Show node at byte position (optional) |
| `--output FORMAT` | Output format: human, json, pretty |

```
$ magellan ast --db ./magellan.db --file src/main.rs
AST nodes for src/main.rs (365 nodes):
function_item (2130:2256)
  └── block (2176:2256)
    └── call_expression (2212:2239)
      └── call_expression (2212:2229)
if_expression (4423:4553)
  └── block (4444:4553)
    └── return_expression (4468:4479)
```

### find-ast

```bash
magellan find-ast --db <FILE> --kind <KIND> [--output FORMAT]
```

Find AST nodes by kind across all files (v1.9.0).

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--kind <KIND>` | AST node kind to find (required) |
| `--output FORMAT` | Output format: human, json, pretty |

Common node kinds: `function_item`, `struct_item`, `impl_item`, `if_expression`, `while_expression`, `for_expression`, `loop_expression`, `match_expression`, `block`, `call_expression`, `return_expression`.

```
$ magellan find-ast --db ./magellan.db --kind if_expression
Found 16 AST nodes with kind 'if_expression':
  - if_expression @ 4423:4553
  - if_expression @ 6817:7443
  ...
```

### Graph Algorithm Commands (v2.0.0)

Magellan 2.0.0 integrates sqlitegraph 1.3.0's graph algorithms for advanced codebase analysis. All algorithm commands accept either a stable 32-character BLAKE3 SymbolId or a simple Fully Qualified Name (FQN) like `main`.

#### reachable

```bash
magellan reachable --db <FILE> --symbol <SYMBOL_ID> [--reverse] [--output FORMAT]
```

Find all symbols reachable from (or that can reach) a starting symbol through the call graph.

```
$ magellan reachable --db ./magellan.db --symbol main
Reachable from main (127 symbols):
  parse_args (fn) at src/main.rs:64
  process_request (fn) at src/handler.rs:12
  ...
```

#### dead-code

```bash
magellan dead-code --db <FILE> --entry <SYMBOL_ID> [--output FORMAT]
```

Find code unreachable from an entry point using call graph reachability analysis.

```
$ magellan dead-code --db ./magellan.db --entry main
Dead symbols (45 unreachable from main):
  unused_helper (fn) at src/utils.rs:100
  legacy_feature (fn) at src/legacy.rs:25
  ...
```

#### cycles

```bash
magellan cycles --db <FILE> [--symbol <SYMBOL_ID>] [--output FORMAT]
```

Detect strongly connected components (SCCs) to identify mutual recursion and other cycles.

```
$ magellan cycles --db ./magellan.db
Found 3 cycles:
  Cycle [1]: 2 symbols
    process_a (fn) at src/a.rs:10
    process_b (fn) at src/b.rs:20
  ...
```

#### condense

```bash
magellan condense --db <FILE> [--members] [--output FORMAT]
```

Create a condensation DAG by collapsing SCCs into supernodes for topological analysis.

```
$ magellan condense --db ./magellan.db
Condensation graph: 45 supernodes, 87 edges
  Supernode [1]: 127 symbols (no cycles)
  Supernode [2]: 2 symbols (cycle)
  ...
```

#### paths

```bash
magellan paths --db <FILE> --start <SYMBOL_ID> [--end <SYMBOL_ID>] [--max-depth N] [--max-paths N] [--output FORMAT]
```

Enumerate execution paths between symbols in the call graph.

```
$ magellan paths --db ./magellan.db --start main --max-depth 5 --max-paths 10
Found 10 paths from main:
  [1] main -> parse_args -> init -> run
  [2] main -> parse_args -> init -> load_config -> run
  ...
```

#### slice

```bash
magellan slice --db <FILE> --target <SYMBOL_ID> [--direction backward|forward] [--verbose] [--output FORMAT]
```

Program slicing finds all code that affects (backward) or is affected by (forward) a target symbol.

```
$ magellan slice --db ./magellan.db --target bug_location
Backward slice (15 symbols affect target):
  main (fn) at src/main.rs:1
  parse_input (fn) at src/parse.rs:10
  ...
```

## Supported Languages

| Language | Extensions | Parser |
|----------|------------|--------|
| Rust | .rs | tree-sitter-rust |
| C | .c, .h | tree-sitter-c |
| C++ | .cpp, .cc, .cxx, .hpp, .h | tree-sitter-cpp |
| Java | .java | tree-sitter-java |
| JavaScript | .js, .mjs | tree-sitter-javascript |
| TypeScript | .ts, .tsx | tree-sitter-typescript |
| Python | .py | tree-sitter-python |

## Native V2 Backend

Magellan supports an alternative high-performance backend with embedded KV storage:

### KV Data Storage

The Native V2 backend stores all metadata in a KV store for O(1) lookups:

| Data Type | KV Key Pattern | Purpose |
|-----------|---------------|---------|
| Symbol index | `sym:fqn:{fqn}` | O(1) symbol lookup by fully-qualified name |
| File symbols | `file:sym:{id}` | All symbols in a file |
| Code chunks | `chunk:{path}:{start}:{end}` | Source code for token-efficient queries |
| AST nodes | `ast:file:{id}` | Hierarchical code structure |
| Metrics | `metrics:file:{path}`, `metrics:symbol:{id}` | Complexity analysis |
| Call edges | `calls:{caller}:{callee}` | Call graph relationships |

**Usage:**

```bash
# Build with Native V2 backend
cargo build --release --features native-v2

# All indexing operations automatically use KV storage
./target/release/magellan watch --root . --db ./codegraph.db --scan-initial

# Queries automatically read from KV
./target/release/magellan find --db ./codegraph.db --name main
./target/release/magellan get --db ./codegraph.db --file src/main.rs --symbol main
```

**Benefits:**

- O(1) symbol resolution instead of SQL queries
- Efficient prefix scans for file-level operations
- All metadata embedded with graph data in single file
- Automatic migration from SQLite preserves all data
- **Full algorithm support:** All graph algorithm commands (cycles, dead-code, reachable, condense, paths, slice) work with Native V2

**Limitations:**

- No direct SQL query access to KV data (use Magellan CLI commands)

See [MANUAL.md](MANUAL.md#6-backend-compatibility) for complete backend compatibility details.

### v2.2 Algorithm Parity Completion ✅

**Shipped:** 2026-02-09

Full feature parity between SQLite and Native V2 backends for all graph algorithm commands.

**Changes:**
- All graph algorithms now use backend-agnostic implementations via GraphBackend trait API
- Tarjan's SCC algorithm for cycle detection works with both backends
- BFS-based reachability analysis for both forward and reverse queries
- Path enumeration and program slicing fully supported on Native V2

**Test Coverage:**
- All algorithm commands verified with `--features native-v2` flag
- Cross-backend integration tests confirm identical behavior

## Database Schema

**Nodes:**
- `File` - path, hash, timestamps
- `Symbol` - name, kind, byte spans, line/column
- `Reference` - file, referenced symbol, location
- `Call` - file, caller, callee, location

**Edges:**
- `DEFINES` - File -> Symbol
- `REFERENCES` - Reference -> Symbol
- `CALLER` - Symbol -> Call
- `CALLS` - Call -> Symbol

**Tables (v2.0.0):**
- `code_chunks` - Stored source code snippets by byte span with SHA-256 deduplication
- `file_metrics` - Fan-in, fan-out, LOC, complexity per file
- `symbol_metrics` - Fan-in, fan-out, LOC, cyclomatic complexity per symbol
- `ast_nodes` - Hierarchical AST structure with parent-child relationships (v1.9.0)

**Symbol Kinds:**
Function, Method, Class, Interface, Enum, Module, Union, Namespace, TypeAlias, Unknown

## Error Handling

Magellan continues processing even when individual files fail:

- Permission errors are logged and skipped
- Files with invalid syntax are skipped
- Database write errors cause exit (requires manual intervention)

## Architecture

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Public API
├── watcher.rs           # Filesystem watcher
├── indexer.rs           # Event coordination
├── references.rs        # Reference/Call fact types
├── verify.rs            # Database verification logic
├── ingest/
│   ├── mod.rs           # Parser dispatcher & Rust parser
│   ├── detect.rs        # Language detection
│   ├── pool.rs          # Thread-local parser pool
│   ├── c.rs             # C parser
│   ├── cpp.rs           # C++ parser
│   ├── java.rs          # Java parser
│   ├── javascript.rs    # JavaScript parser
│   ├── typescript.rs    # TypeScript parser
│   └── python.rs        # Python parser
├── query_cmd.rs         # Query command
├── find_cmd.rs          # Find command
├── refs_cmd.rs          # Refs command
├── ast_cmd.rs           # AST command (v1.9.0)
├── verify_cmd.rs        # Verify CLI handler
├── watch_cmd.rs         # Watch CLI handler
├── output/              # Output formatting
├── common.rs            # Shared utilities
├── validation.rs        # Path validation
└── graph/
    ├── mod.rs           # CodeGraph API
    ├── schema.rs        # Node/edge types
    ├── files.rs         # File operations
    ├── symbols.rs       # Symbol operations
    ├── references.rs    # Reference node operations
    ├── calls.rs         # Call edge operations
    ├── call_ops.rs      # Call node operations
    ├── ast_node.rs      # AST node types (v1.9.0)
    ├── ast_extractor.rs # AST extraction from tree-sitter (v1.9.0)
    ├── ast_ops.rs       # AST query operations (v1.9.0)
    ├── ops.rs           # Graph indexing operations
    ├── query.rs         # Query operations
    ├── count.rs         # Count operations
    ├── export.rs        # JSON export
    ├── scan.rs          # Scanning operations
    ├── freshness.rs     # Freshness checking
    ├── cache.rs         # LRU cache
    └── tests.rs         # Graph tests
```

## Testing

```bash
cargo test
```

Test coverage:
- Path validation tests: 24 tests for traversal protection, symlink handling, cross-platform paths
- Orphan detection tests: 12 tests verifying clean state after delete operations
- SCIP export tests: 7 round-trip tests verifying parseable protobuf output
- Call graph tests: 5 tests for cross-file method call resolution
- Symbol extraction tests: per-language tests for Rust, Python, Java, JavaScript, TypeScript, C, C++
- Graph operations tests: insert, delete, query operations across all node types

Tests pass on Linux (primary development platform). Other platforms not regularly tested.

### Thread Safety Testing (v1.7)

Magellan uses thread-safe synchronization for concurrent access to shared state. The v1.7 migration from `RefCell<T>` to `Arc<Mutex<T>>` ensures data races are eliminated.

**TSAN Test Suite:**

```bash
# Run TSAN thread safety tests
cargo test --test tsan_thread_safety_tests
```

**What TSAN Detects:**
- Data races from unsynchronized concurrent access
- Missing mutexes around shared mutable state
- Lock ordering violations that can cause deadlocks

**Modules Tested:**
- `FileSystemWatcher` - Concurrent batch access, legacy pending state
- `PipelineSharedState` - Dirty path insertion, lock ordering

**Current Status:**

The TSAN test suite is created and all tests pass. However, running with actual ThreadSanitizer instrumentation (`-Zsanitizer=thread`) is currently blocked by Rust toolchain limitations (ABI mismatch errors in dependencies). See `TEST-01-TSAN-RESULTS.md` for details.

**Manual Verification:**

All concurrent state uses `Arc<Mutex<T>>`:
- `FileSystemWatcher::legacy_pending_batch: Arc<Mutex<Option<WatcherBatch>>>`
- `FileSystemWatcher::legacy_pending_index: Arc<Mutex<usize>>`
- `PipelineSharedState::dirty_paths: Arc<Mutex<BTreeSet<PathBuf>>`

Lock ordering is enforced to prevent deadlocks:
1. Acquire `dirty_paths` lock first
2. Send wakeup signal while holding lock
3. Release lock

## Known Limitations

- **FQN collisions addressed in v1.5**: Symbols with identical names in different files or modules may share the same display FQN. The `collisions` command (v1.5) identifies these cases, and `--symbol-id` provides stable BLAKE3-based identifiers for unambiguous symbol reference. Common in: `main` functions across binaries, test functions, and methods with generic names (`new`, `default`, etc.) in impl blocks.
- **No semantic analysis**: AST-level only; no type checking or cross-module resolution
- **No incremental parsing**: File changes trigger full re-parse of that file
- **Cross-crate resolution**: Rust symbols across crates are resolved by name only
- **Testing**: Primary development and testing on Linux; Windows and macOS not regularly tested in CI

## License

GPL-3.0-or-later

## Dependencies

- notify - Filesystem watching
- tree-sitter - AST parsing
- [sqlitegraph 1.3.0](https://crates.io/crates/sqlitegraph) - Graph persistence and algorithms ([repository](https://github.com/oldnordic/sqlitegraph))
- signal-hook - Signal handling
- walkdir - Directory scanning
- rayon - Parallel processing

## Project Links

- **Repository:** [https://github.com/oldnordic/magellan](https://github.com/oldnordic/magellan)
- **Documentation:** [MANUAL.md](https://github.com/oldnordic/magellan/blob/main/MANUAL.md)
- **Crates.io:** [https://crates.io/crates/magellan](https://crates.io/crates/magellan)

# Magellan

A deterministic codebase mapping tool. Watches source files, extracts AST-level facts, and builds a searchable graph database of symbols and references.

## What Magellan Does

- Watches directories for file changes (Create/Modify/Delete)
- Extracts AST-level facts: functions, classes, methods, enums, modules
- Tracks symbol references: function calls and type references (7 languages)
- Builds call graphs: caller → callee relationships across indexed files (7 languages)
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
- **Native-v2 Backend**: Build with `--features native-v2` for improved performance

## Quick Start

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

# Find a symbol by name
magellan find --db /path/to/magellan.db --name main
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

# Export to JSON
magellan export --db /path/to/magellan.db > codegraph.json
```

## Commands

### watch

```bash
magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--scan-initial]
```

Watch a directory for source file changes and index them into the database.

| Argument | Description |
|----------|-------------|
| `--root <DIR>` | Directory to watch recursively (required) |
| `--db <FILE>` | Path to sqlitegraph database (required) |
| `--debounce-ms <N>` | Debounce delay in milliseconds (default: 500) |
| `--scan-initial` | Scan directory for source files on startup |

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
magellan find --db <FILE> --name <NAME> [--path <PATH>]
magellan find --db <FILE> --list-glob "<PATTERN>"
```

Find a symbol by name or preview all symbols that match a glob expression. Glob listings include
node IDs for deterministic scripting (e.g., feeding results to refactoring tooling).

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Path to database (required) |
| `--name <NAME>` | Symbol name to find (required) |
| `--path <PATH>` | Limit search to specific file (optional) |
| `--list-glob <PATTERN>` | List all symbol names that match the glob (mutually exclusive with `--name`) |

```
$ magellan find --db ./magellan.db --name main
Found "main":
  File:     /path/to/src/main.rs
  Kind:     Function
  Location: Line 229, Column 0
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
magellan export --db <FILE>
```

Export all graph data to JSON format.

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

## Known Limitations

- **FQN collisions may occur**: Symbols with identical names in different files or modules may share the same Fully Qualified Name. This affects cross-file call resolution and find operations. Common in: `main` functions across binaries, test functions, and methods with generic names (`new`, `default`, etc.) in impl blocks. Collisions rate is typically 3-5% of symbols in large codebases.
- **No semantic analysis**: AST-level only; no type checking or cross-module resolution
- **No incremental parsing**: File changes trigger full re-parse of that file
- **Single-threaded watcher**: Event processing is sequential (CodeGraph uses SQLite concurrency)
- **Cross-crate resolution**: Rust symbols across crates are resolved by name only
- **Testing**: Primary development and testing on Linux; Windows and macOS not regularly tested in CI

## License

GPL-3.0-or-later

## Dependencies

- notify - Filesystem watching
- tree-sitter - AST parsing
- [sqlitegraph](https://crates.io/crates/sqlitegraph) - Graph persistence ([repository](https://github.com/oldnordic/sqlitegraph))
- signal-hook - Signal handling
- walkdir - Directory scanning
- rayon - Parallel processing

## Project Links

- **Repository:** [https://github.com/oldnordic/magellan](https://github.com/oldnordic/magellan)
- **Documentation:** [MANUAL.md](https://github.com/oldnordic/magellan/blob/main/MANUAL.md)
- **Crates.io:** [https://crates.io/crates/magellan](https://crates.io/crates/magellan)

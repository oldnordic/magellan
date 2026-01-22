# Magellan Operator Manual

**Version 0.5.3** | *Last Updated: 2026-01-13*

Comprehensive instructions for operating Magellan.

---

## Table of Contents

1. [Installation](#1-installation)
2. [Quick Start](#2-quick-start)
3. [Position Conventions](#3-position-conventions)
4. [Command Reference](#4-command-reference)
5. [Known Limitations](#5-known-limitations)
6. [Supported Languages](#6-supported-languages)
7. [Database Schema](#7-database-schema)
8. [Error Handling](#8-error-handling)
9. [Troubleshooting](#9-troubleshooting)
10. [Security Best Practices](#10-security-best-practices)

---

## 1. Installation

### 1.1 System Requirements

**Minimum:**
- Rust 1.70+
- Linux kernel 3.10+ or macOS 10.12+
- 50MB free RAM
- 10MB free disk space (plus database growth)

### 1.2 Building from Source

```bash
git clone https://github.com/feanor/magellan
cd magellan
cargo build --release

# Verify installation
./target/release/magellan --help

# Install to system
sudo cp target/release/magellan /usr/local/bin/
sudo chmod +x /usr/local/bin/magellan
```

---

## 2. Quick Start

```bash
# Navigate to your project
cd /path/to/project

# Initial scan
magellan watch --root . --db ./magellan.db --scan-initial

# In another terminal, check status
magellan status --db ./magellan.db

# List indexed files
magellan files --db ./magellan.db

# Query symbols in a file or print selector help
magellan query --db ./magellan.db --file src/main.rs
magellan query --db ./magellan.db --explain

# Show symbol extents
magellan query --db ./magellan.db --file src/lib.rs --symbol main --show-extent

# Find a symbol
magellan find --db ./magellan.db --name main

# Preview symbols via glob
magellan find --db ./magellan.db --list-glob "handler_*"

# Query by labels (NEW in 0.5.0)
magellan label --db ./magellan.db --list
magellan label --db ./magellan.db --label rust --label fn

# Get code chunks without re-reading files (NEW in 0.5.0)
magellan get --db ./magellan.db --file src/main.rs --symbol main
magellan get-file --db ./magellan.db --file src/main.rs

# Export to JSON
magellan export --db ./magellan.db > codegraph.json
```

---

## 3. Position Conventions

Magellan follows tree-sitter conventions for all position data. This ensures consistency across all supported languages and compatibility with tree-sitter-based tooling.

### 3.1 Line Positions

**Line positions are 1-indexed.**

- The first line in a file is line 1, not line 0
- This matches the convention used by tree-sitter and most text editors
- When displaying positions to users, Magellan uses 1-indexed line numbers

**Example:**
```
Line 1: fn main() {    <- Line 1 is the first line
Line 2:     println!("Hello");
Line 3: }
```

### 3.2 Column Positions

**Column positions are 0-indexed.**

- The first character in a line is at column 0
- Column values represent byte offsets within a line
- This matches the tree-sitter convention for precise positioning

**Example:**
```
fn main() {
^  ^   ^
0  2   5  <- Column positions (0-indexed)
```

### 3.3 Byte Offsets

**Byte offsets are 0-indexed from the start of the file.**

- The first byte in a file is at offset 0
- Byte offsets span the entire file, not individual lines
- Use these for direct file seeking and range operations

**Example:**
```
fn main() {
^       ^
0       9  <- Byte offsets from file start
```

### 3.4 Position Data in JSON

When exporting to JSON or querying with `--show-extent`, positions are reported as:

```json
{
  "name": "function_name",
  "byte_start": 1024,
  "byte_end": 2048,
  "start_line": 42,
  "start_col": 0
}
```

- `byte_start`, `byte_end`: 0-indexed byte offsets from file start
- `start_line`: 1-indexed line number
- `start_col`: 0-indexed column offset within the line

### 3.5 Why These Conventions?

Magellan uses tree-sitter parsers for all supported languages. Tree-sitter's position conventions were chosen to:

- **Align with editor conventions**: Most editors display line numbers starting at 1
- **Enable precise positioning**: 0-indexed columns allow direct byte offset calculation
- **Maintain consistency**: All languages use the same conventions through tree-sitter
- **Support cross-language tooling**: Tools can process position data uniformly

---

## 4. Command Reference

### 4.1 watch

```bash
magellan watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--scan-initial]
```

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--root <DIR>` | Path | - | Directory to watch (required) |
| `--db <FILE>` | Path | - | Database path (required) |
| `--debounce-ms <N>` | Integer | 500 | Debounce delay in milliseconds |
| `--scan-initial` | Flag | - | Scan directory on startup |

### 3.2 status

```bash
magellan status --db <FILE>
```

Shows database statistics.

```
$ magellan status --db ./magellan.db
files: 30
symbols: 349
references: 262
```

### 3.3 files

```bash
magellan files --db <FILE>
```

Lists all indexed files.

```
$ magellan files --db ./magellan.db
30 indexed files:
  /path/to/src/main.rs
  /path/to/src/lib.rs
```

### 3.4 query

```bash
magellan query --db <FILE> --file <PATH> [--kind <KIND>] [--symbol <NAME>] [--show-extent]
magellan query --db <FILE> --explain
```

Lists symbols in a file and includes normalized kind tags (`[fn]`, `[struct]`, etc.) so scripts can parse the output. Use `--symbol <NAME>` to narrow the results; combine it with `--show-extent` to print byte and line/column ranges plus node IDs. `--explain` prints the selector cheat sheet and usage examples.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Database path (required) |
| `--file <PATH>` | File path to query (required unless `--explain`) |
| `--kind <KIND>` | Filter by symbol kind (Function, Method, Class, Interface, Enum, Module, Union, Namespace, TypeAlias) |
| `--symbol <NAME>` | Limit to a specific symbol (optional) |
| `--show-extent` | Show byte/line ranges and node IDs (requires `--symbol`) |
| `--explain` | Print selector documentation |

```
$ magellan query --db ./magellan.db --file src/main.rs --kind Function
/path/to/src/main.rs:
  Line   13: Function     print_usage
  Line   64: Function     parse_args
```

### 3.5 find

```bash
magellan find --db <FILE> --name <NAME> [--path <PATH>]
magellan find --db <FILE> --list-glob "<PATTERN>"
```

Finds a symbol by name or previews all symbols that match a glob expression. Output includes the normalized kind tag and node IDs for deterministic scripting.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Database path (required) |
| `--name <NAME>` | Symbol name (required unless `--list-glob`) |
| `--path <PATH>` | Limit to specific file (optional) |
| `--list-glob <PATTERN>` | Emit every symbol matching the glob (mutually exclusive with `--name`) |

```
$ magellan find --db ./magellan.db --name main
Found "main":
  File:     /path/to/src/main.rs
  Kind:     Function
  Location: Line 229, Column 0
```

### 3.6 refs

```bash
magellan refs --db <FILE> --name <NAME> --path <PATH> [--direction <in|out>]
```

Shows incoming or outgoing calls. Incoming calls include callers from other
indexed files when the target symbol name is unique in the database.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Database path (required) |
| `--name <NAME>` | Symbol name (required) |
| `--path <PATH>` | File path containing symbol (required) |
| `--direction <in|out>` | Direction (default: in) |

```
$ magellan refs --db ./magellan.db --name main --path src/main.rs --direction out
Calls FROM "main":
  To: print_usage at /path/to/src/main.rs:233
  To: parse_args at /path/to/src/main.rs:237
```

### 3.7 verify

```bash
magellan verify --root <DIR> --db <FILE>
```

Compares database state vs filesystem.

Exit codes: 0 = up to date, 1 = issues found

```
$ magellan verify --root ./src --db ./magellan.db
Database verification: ./src
New files (3):
  + src/new.rs
  + src/helper.rs
Total: 2 issues
```

### 3.8 export

```bash
magellan export --db <FILE>
```

Exports graph data to JSON.

### 3.9 label

```bash
magellan label --db <FILE> [--label <LABEL>]... [--list] [--count] [--show-code]
```

Query symbols by labels. Labels are automatically assigned during indexing:
- **Language labels**: `rust`, `python`, `javascript`, `typescript`, `c`, `cpp`, `java`
- **Symbol kind labels**: `fn`, `method`, `struct`, `class`, `enum`, `interface`, `module`, `union`, `namespace`, `typealias`

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--db <FILE>` | Path | - | Database path (required) |
| `--label <LABEL>` | String | - | Label to query (can be specified multiple times) |
| `--list` | Flag | - | List all labels with counts |
| `--count` | Flag | - | Count entities with specified label(s) |
| `--show-code` | Flag | - | Show source code for each result |

**Multi-label queries use AND semantics** - symbols must have ALL specified labels.

```
$ magellan label --db ./magellan.db --list
12 labels in use:
  rust (349)
  fn (120)
  struct (45)
  method (89)

$ magellan label --db ./magellan.db --label rust --label fn
120 symbols with labels [rust, fn]:
  main (fn) in src/main.rs [0-36]
  new (fn) in src/user.rs [91-138]

$ magellan label --db ./magellan.db --label rust --label fn --show-code
120 symbols with labels [rust, fn]:
  main (fn) in src/main.rs [0-36]
    fn main() {
        println!("Hello");
    }
```

### 3.10 get

```bash
magellan get --db <FILE> --file <PATH> --symbol <NAME>
```

Get code chunks for a specific symbol. Uses stored code chunks so you don't need to re-read source files.

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--db <FILE>` | Path | - | Database path (required) |
| `--file <PATH>` | Path | - | File path (required) |
| `--symbol <NAME>` | String | - | Symbol name (required) |

### 3.11 get-file

```bash
magellan get-file --db <FILE> --file <PATH>
```

Get all code chunks from a file. Useful for getting complete file contents without re-reading the source.

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--db <FILE>` | Path | - | Database path (required) |
| `--file <PATH>` | Path | - | File path (required) |

---

## 5. Known Limitations

### 5.1 In-Memory Databases

Magellan uses SQLite Shared connections for concurrent access (via `sqlitegraph` and
`ChunkStore`), which don't work with `:memory:` databases. Each thread would get its
own separate in-memory database, breaking the shared state assumption.

Additionally, operations that retrieve the database file path will fail for
`:memory:` databases because in-memory databases have no file path.

**Workaround:** Use file-based databases for all operations requiring concurrent
access or path retrieval.

---

## 6. Supported Languages

| Language | Extensions | Symbol Extraction | Reference Extraction | Call Graph |
|----------|------------|-------------------|---------------------|------------|
| Rust | .rs | ✅ | ✅ | ✅ |
| Python | .py | ✅ | ✅ | ✅ |
| C | .c, .h | ✅ | ✅ | ✅ |
| C++ | .cpp, .cc, .cxx, .hpp, .h | ✅ | ✅ | ✅ |
| Java | .java | ✅ | ✅ | ✅ |
| JavaScript | .js, .mjs | ✅ | ✅ | ✅ |
| TypeScript | .ts, .tsx | ✅ | ✅ | ✅ |

---

## 7. Database Schema

### 7.1 Node Types

**File Node:**
```json
{
  "path": "/absolute/path/to/file.rs",
  "hash": "sha256:abc123...",
  "last_indexed_at": 1735339600,
  "last_modified": 1735339500
}
```

**Symbol Node:**
```json
{
  "name": "function_name",
  "kind": "Function|Method|Class|Interface|Enum|Module|Union|Namespace|TypeAlias|Unknown",
  "byte_start": 1024,
  "byte_end": 2048,
  "start_line": 42,
  "start_col": 0
}
```

**Call Node:**
```json
{
  "file": "/absolute/path/to/file.rs",
  "caller": "calling_function",
  "callee": "called_function",
  "start_line": 80
}
```

### 7.2 Edge Types

| Edge Type | Source | Target | Meaning |
|-----------|--------|--------|---------|
| `DEFINES` | File | Symbol | File defines this symbol |
| `REFERENCES` | Reference | Symbol | Reference refers to symbol |
| `CALLER` | Symbol | Call | Caller emits a call |
| `CALLS` | Call | Symbol | Call targets callee |

---

## 8. Error Handling

### 8.1 Error Messages

**Permission Denied:**
```
ERROR /path/to/file.rs Permission denied (os error 13)
```
- File is skipped
- Other files continue processing

**Syntax Error:**
- File is silently skipped
- No symbols extracted

**Database Locked:**
- Only one process may access database at a time
- Magellan exits cleanly

### 8.2 Recovery

```bash
# Check database integrity
sqlite3 magellan.db "PRAGMA integrity_check;"

# Rebuild from scratch if needed
rm magellan.db
magellan watch --root . --db magellan.db --scan-initial
```

---

## 9. Troubleshooting

### Files not being indexed

Check file extension is supported:
```bash
find ./watched/dir -name "*.rs"
find ./watched/dir -name "*.py"
```

Use `--scan-initial` for first use:
```bash
magellan watch --root . --db magellan.db --scan-initial
```

### Database shows stale data

```bash
# Verify database state
magellan verify --root . --db ./magellan.db

# Re-scan if needed
magellan watch --root . --db ./magellan.db --scan-initial &
sleep 5
pkill -f "magellan watch"
```

---

## 10. Security Best Practices

### 10.1 Database Placement

Magellan stores all indexed data in the file specified by `--db <FILE>`.
The location of this file affects both security and performance.

**Why Database Location Matters**

If the database is placed inside a watched directory:
- The watcher may process the database as a source file
- Export operations could include binary database content
- File system events may cause circular processing

**Recommended Locations by Platform**

**Linux/macOS:**
```bash
# XDG cache directory (recommended)
magellan watch --root ~/project --db ~/.cache/magellan/project.db

# XDG data directory (for long-term storage)
magellan watch --root ~/project --db ~/.local/share/magellan/project.db

# Home directory (simple alternative)
magellan watch --root ~/project --db ~/.$PROJECT_NAME.db
```

**Windows:**
```cmd
REM Local app data (recommended)
magellan watch --root C:\project --db %LOCALAPPDATA%\magellan\project.db

REM User profile (simple alternative)
magellan watch --root C:\project --db %USERPROFILE%\project.db
```

**CI/CD Environments:**
```bash
# Use a cache directory outside the workspace
magellan watch --root . --db $CI_PROJECT_DIR/../cache/magellan.db
```

**What to Avoid:**

```bash
# AVOID: Database inside watched directory
magellan watch --root . --db ./magellan.db

# AVOID: Database in source code directory
magellan watch --root ~/src/project --db ~/src/project/.magellan.db
```

### 10.2 Path Traversal Protection

Magellan includes protection against directory traversal attacks that attempt
to access files outside the watched directory.

**Automatic Protections**

Magellan's path validation (`src/validation.rs`) automatically:
- Rejects paths with 3+ parent directory patterns (`../../../etc/passwd`)
- Validates resolved paths against the project root
- Checks symlinks to ensure they don't escape the watched directory
- Rejects mixed traversal patterns (`./subdir/../../etc`)

**Validation Points**

Path validation is applied at:
- Watcher event processing (every file change event)
- Directory scanning (recursive directory walk)
- File indexing operations (before reading file contents)

**Example Attack Prevention**

```bash
# This input is automatically rejected:
magellan watch --root ~/project --db ~/db  # But if a malicious event tries:
# ../../../../../etc/passwd  -> Rejected (suspicious traversal)
# ./subdir/../../../etc  -> Rejected (mixed pattern)

# Symlinks outside root are rejected:
ln -s /etc/passwd project/link
magellan watch --root project --db ~/db  # link rejected
```

**Security Auditing**

To verify path protection is working:
```bash
# Run path validation tests
cargo test path_validation

# Check implementation
grep -r "validate_path" src/
```

### 10.3 File Permission Recommendations

**Database File Permissions**

The database contains complete code structure information. Restrict access:

```bash
# Set restrictive permissions on database directory
mkdir -p ~/.cache/magellan
chmod 700 ~/.cache/magellan

# Database files inherit directory permissions
magellan watch --root ~/project --db ~/.cache/magellan/project.db
```

**Source Directory Permissions**

Magellan needs read access to source files:
- Read permission on source files
- Execute permission on source directories (for traversal)
- Write permission only needed for database location

**Multi-User Environments**

For shared systems:
```bash
# Create group-writable cache directory
sudo groupadd magellan
sudo usermod -a -G magellan $USER
sudo mkdir -p /var/cache/magellan
sudo chgrp magellan /var/cache/magellan
sudo chmod 770 /var/cache/magellan

# Use shared cache
magellan watch --root ~/project --db /var/cache/magellan/$USER-project.db
```

### 10.4 Secure Operation Patterns

**Production Monitoring**

```bash
# Run with nohup for persistence
nohup magellan watch --root /app/src --db /var/cache/mag/app.db \
  --scan-initial > /var/log/magellan.log 2>&1 &

# Check status separately
magellan status --db /var/cache/mag/app.db
```

**Docker Environments**

```dockerfile
# Use volume for database outside source mount
docker run -v /src:/app:ro -v /cache:/data magellan \
  watch --root /app --db /data/app.db --scan-initial
```

**Verification Before Deployment**

```bash
# 1. Test path validation
cargo test path_validation_tests

# 2. Verify database location
magellan status --db /var/cache/mag/app.db

# 3. Check for accidental database inclusion in exports
magellan export --db /var/cache/mag/app.db | grep -v "sqlite"
```

---

## Architecture

### Threading Model

Magellan uses a hybrid threading model:

- **Watcher:** Single-threaded design using `RefCell` for interior mutability.
  Appropriate for the debounced event coalescing logic, which runs on one thread.

- **CodeGraph:** Thread-safe design for concurrent database access from multiple
  indexer threads. The graph database layer uses SQLite's built-in concurrency
  support to handle simultaneous access.

**Note:** `RefCell` is not thread-safe. If the watcher becomes multi-threaded in
the future, replace `RefCell<T>` with `Arc<Mutex<T>>` to maintain thread safety.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error or issues found (verify command) |

---

## License

GPL-3.0-or-later

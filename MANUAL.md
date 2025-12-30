# Magellan Operator Manual

**Version 0.3.0** | *Last Updated: 2025-12-30*

Comprehensive instructions for operating Magellan.

---

## Table of Contents

1. [Installation](#1-installation)
2. [Quick Start](#2-quick-start)
3. [Command Reference](#3-command-reference)
4. [Supported Languages](#4-supported-languages)
5. [Database Schema](#5-database-schema)
6. [Error Handling](#7-error-handling)
7. [Troubleshooting](#9-troubleshooting)

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

# Query symbols in a file
magellan query --db ./magellan.db --file src/main.rs

# Find a symbol
magellan find --db ./magellan.db --name main

# Export to JSON
magellan export --db ./magellan.db > codegraph.json
```

---

## 3. Command Reference

### 3.1 watch

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
magellan query --db <FILE> --file <PATH> [--kind <KIND>]
```

Lists symbols in a file.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Database path (required) |
| `--file <PATH>` | File path to query (required) |
| `--kind <KIND>` | Filter by symbol kind |

Valid kinds: Function, Method, Class, Interface, Enum, Module, Union, Namespace, TypeAlias

```
$ magellan query --db ./magellan.db --file src/main.rs --kind Function
/path/to/src/main.rs:
  Line   13: Function     print_usage
  Line   64: Function     parse_args
```

### 3.5 find

```bash
magellan find --db <FILE> --name <NAME> [--path <PATH>]
```

Finds a symbol by name.

| Argument | Description |
|----------|-------------|
| `--db <FILE>` | Database path (required) |
| `--name <NAME>` | Symbol name (required) |
| `--path <PATH>` | Limit to specific file (optional) |

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

Shows incoming or outgoing calls.

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

---

## 4. Supported Languages

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

## 5. Database Schema

### 5.1 Node Types

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

### 5.2 Edge Types

| Edge Type | Source | Target | Meaning |
|-----------|--------|--------|---------|
| `DEFINES` | File | Symbol | File defines this symbol |
| `REFERENCES` | Reference | Symbol | Reference refers to symbol |
| `CALLS` | Symbol | Symbol | Caller calls callee |

---

## 6. Error Handling

### 6.1 Error Messages

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

### 6.2 Recovery

```bash
# Check database integrity
sqlite3 magellan.db "PRAGMA integrity_check;"

# Rebuild from scratch if needed
rm magellan.db
magellan watch --root . --db magellan.db --scan-initial
```

---

## 7. Troubleshooting

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

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error or issues found (verify command) |

---

## License

GPL-3.0-or-later

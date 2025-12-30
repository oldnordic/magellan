# Magellan CLI Query Commands - Implementation Plan

**Phase**: CLI Query Enhancement
**Last Updated**: 2025-12-28
**Current State**: Planning
**Parent Contract**: `docs/CONTRACT.md`

---

## Overview

Add query commands to Magellan CLI to expose existing library query capabilities.

**Problem**: Users must query the database directly with `sqlite3` or use the `export` command which dumps everything. The CLI should provide human-readable query commands.

**Solution**: Add CLI commands that wrap existing `CodeGraph` query methods from `src/graph/query.rs` and `src/graph/mod.rs`.

---

## Existing Library API (Already Implemented)

**File**: `src/graph/mod.rs` lines 114-226

| Method | Location | Purpose |
|--------|----------|---------|
| `symbols_in_file(path)` | `query::symbols_in_file()` | Get all symbols in a file |
| `symbols_in_file_with_kind(path, kind)` | `query::symbols_in_file_with_kind()` | Get symbols filtered by kind |
| `symbol_id_by_name(path, name)` | `query::symbol_id_by_name()` | Find symbol node ID by name |
| `references_to_symbol(symbol_id)` | `query::references_to_symbol()` | Get references to a symbol |
| `calls_from_symbol(path, name)` | `calls::calls_from_symbol()` | Get outgoing calls |
| `callers_of_symbol(path, name)` | `calls::callers_of_symbol()` | Get incoming calls |
| `get_file_node(path)` | `files::get_file_node()` | Get file metadata |
| `all_file_nodes()` | `files::all_file_nodes()` | Get all files |

**File**: `src/ingest/mod.rs` lines 19-45

| SymbolKind | Description |
|------------|-------------|
| Function | Function definition |
| Method | Method inside a class/impl |
| Class | Rust struct, Python/Java/C++/JS/TS class |
| Interface | Rust trait, Java/TS interface |
| Enum | Enum definition |
| Module | Rust mod, Python module, Java package |
| Union | C/C++ union |
| Namespace | C++/TS namespace |
| TypeAlias | TypeScript type, Rust type alias |
| Unknown | Unknown symbol type |

---

## Proposed CLI Commands

### 1. `query` - List symbols in a file

```bash
magellan query --db <FILE> --file <PATH> [--kind <KIND>]
```

**Wraps**: `CodeGraph::symbols_in_file_with_kind()`

**Options**:
- `--db <FILE>` - Path to sqlitegraph database (required)
- `--file <PATH>` - File path to query (required)
- `--kind <KIND>` - Filter by symbol kind (optional)

**Symbol kinds**: Function, Method, Class, Interface, Enum, Module, Union, Namespace, TypeAlias

**Output format** (human-readable):
```
/path/to/file.rs:
  Line 12:  Function     main
  Line 45:  Class        Point
  Line 78:  Function     distance
```

### 2. `find` - Find a symbol by name

```bash
magellan find --db <FILE> --name <NAME> [--path <PATH>]
```

**Wraps**: `CodeGraph::symbol_id_by_name()` + symbol lookup

**Options**:
- `--db <FILE>` - Path to sqlitegraph database (required)
- `--name <NAME>` - Symbol name to find (required)
- `--path <PATH>` - File path to limit search (optional, searches all files if omitted)

**Output format**:
```
Found "main":
  File:     /path/to/file.rs
  Kind:     Function
  Location: Line 12, Column 0
  Node ID:  123
```

### 3. `refs` - Show references/calls for a symbol

```bash
magellan refs --db <FILE> --name <NAME> [--path <PATH>] [--direction <in|out>]
```

**Wraps**: `CodeGraph::callers_of_symbol()` / `CodeGraph::calls_from_symbol()`

**Options**:
- `--db <FILE>` - Path to sqlitegraph database (required)
- `--name <NAME>` - Symbol name (required)
- `--path <PATH>` - File path containing the symbol (optional if name is unique)
- `--direction <in|out>` - Show incoming (in) or outgoing (out) calls (default: in)

**Output format**:
```
References TO "main":
  From: helper (Function) at /path/to/file.rs:45
  From: process (Function) at /path/to/other.rs:12
```

### 4. `files` - List all indexed files

```bash
magellan files --db <FILE>
```

**Wraps**: `CodeGraph::all_file_nodes()`

**Options**:
- `--db <FILE>` - Path to sqlitegraph database (required)

**Output format**:
```
30 indexed files:
  /home/feanor/Projects/magellan/src/lib.rs
  /home/feanor/Projects/magellan/src/main.rs
  ...
```

---

## Implementation Phases

### Phase 1: Core Query Command
**Task**: Implement `magellan query --db --file [--kind]`

**Files to modify**:
- `src/main.rs` - Add `Query` command variant and parsing
- `src/query_cmd.rs` - New file for query command logic

**Deliverables**:
- Parse `--kind` argument to `SymbolKind` enum
- Call `symbols_in_file_with_kind()`
- Format output as human-readable table
- Test with `.rs` files (Rust symbols)

### Phase 2: Find Command
**Task**: Implement `magellan find --db --name [--path]`

**Files to modify**:
- `src/main.rs` - Add `Find` command variant
- `src/find_cmd.rs` - New file for find command logic

**Deliverables**:
- Search all files if `--path` omitted
- Display symbol location and metadata
- Handle multiple matches

### Phase 3: References Command
**Task**: Implement `magellan refs --db --name [--path] [--direction]`

**Files to modify**:
- `src/main.rs` - Add `Refs` command variant
- `src/refs_cmd.rs` - New file for refs command logic

**Deliverables**:
- Call `callers_of_symbol()` for `--direction in`
- Call `calls_from_symbol()` for `--direction out`
- Format reference list with file locations

### Phase 4: Files Command
**Task**: Implement `magellan files --db`

**Files to modify**:
- `src/main.rs` - Add `Files` command variant
- Add inline handler in `main.rs` (simple enough)

**Deliverables**:
- List all indexed files
- Show file count

### Phase 5: Help Text Update
**Task**: Update usage help with new commands

**Files to modify**:
- `src/main.rs` - Update `print_usage()`

**Deliverables**:
- Document all new commands
- Update examples

---

## Design Decisions

### Output Format
Human-readable text, NOT JSON. JSON export already exists via `export` command.

Rationale: CLI is for interactive use. JSON can be obtained via `export` or sqlite3.

### Command Names
Chosen for brevity and clarity:
- `query` - Ask "what symbols in this file?"
- `find` - Ask "where is this symbol?"
- `refs` - Ask "what references this symbol?"
- `files` - Ask "what files are indexed?"

### Symbol Kind Parsing
Case-insensitive matching for user convenience:
- `class` → `SymbolKind::Class`
- `function` or `fn` → `SymbolKind::Function`
- `interface` or `trait` → `SymbolKind::Interface`

### Error Handling
- File not found: Clear message, exit code 1
- Database not found: Clear message, exit code 1
- No matches found: "No results found", exit code 0 (not an error)

---

## Constraints (from CONTRACT.md)

- NO LLM integration
- NO semantic inference
- Facts only (what's persisted in sqlitegraph)
- Deterministic output (sorted results)
- No new database schema changes
- Use existing query methods only

---

## Testing Strategy

Per TDD rules:
1. Write failing CLI integration test first
2. Prove test fails
3. Implement command
4. Prove test passes
5. Repeat for each command

**Test file**: `tests/cli_query_tests.rs`

---

*Created: 2025-12-28*

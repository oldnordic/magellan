# Magellan CLI Search Command - Implementation Plan

**Date**: 2025-12-27
**Purpose**: Design a CLI command to search and navigate codebase using the graph database

---

## 1. Current Database State

### Schema
```
graph_entities (id, kind, name, file_path, data)
graph_edges (id, from_id, to_id, edge_type, data)
graph_labels (entity_id, label)
graph_properties (entity_id, key, value)
```

### Current Data (Odincode)
| Entity Type | Count | Description |
|-------------|-------|-------------|
| File | 213 | Source files |
| Symbol | 2,767 | Functions, structs, enums, traits, impls |
| Reference | 2,529 | Symbol references |
| Call | 1,322 | Function calls |

### Edge Types
| Edge Type | Count | Direction | Meaning |
|-----------|-------|-----------|---------|
| DEFINES | 2,767 | File → Symbol | File defines symbol |
| REFERENCES | 2,529 | Reference → Symbol | Reference targets symbol |
| CALLER | 1,322 | Symbol → Call | Symbol makes call |
| CALLS | 1,322 | Call → Symbol | Call targets symbol |

### Symbol Data Structure
```json
{
  "name": "parse_args",
  "kind": "Function",
  "byte_start": 1843,
  "byte_end": 3706,
  "start_line": 74,
  "start_col": 0,
  "end_line": 132,
  "end_col": 1
}
```

### Call Data Structure
```json
{
  "caller": "parse_args",
  "callee": "parse_mode",
  "file": "/path/to/file.rs",
  "byte_start": 3623,
  "byte_end": 3677,
  "start_line": 128,
  "start_col": 29,
  "end_line": 128,
  "end_col": 83
}
```

---

## 2. Research: Existing Tools

### [code-graph-rag](https://github.com/vitali87/code-graph-rag)
**Key Features:**
- Multi-language parsing (Rust included)
- Natural language queries via LLM
- Interactive CLI with commands:
  - `start` - Query the codebase
  - `optimize` - AI-powered code optimization
  - `export` - Export graph data
- Real-time graph updates
- Surgical code editing

**Query Examples:**
- "Show me all classes that contain 'user' in their name"
- "Find functions related to database operations"
- "What methods does the User class have?"
- "Show me functions that handle authentication"

**Graph Schema (for reference):**
- Nodes: Project, Package, Module, Class, Function, Method, File, Folder
- Edges: CONTAINS_*, DEFINES, DEFINES_METHOD, CALLS, DEPENDS_ON_EXTERNAL

### [Sourcegraph](https://sourcegraph.com/blog/code-search-to-code-intelligence)
**Key Features:**
- Structural search (not just text)
- Cross-repository navigation
- Precise code analysis
- Pattern-based queries

### [FalkorDB Code Graph](https://www.falkordb.com/blog/code-graph/)
**Key Features:**
- Graph visualization
- Natural language querying
- Real-time updates

---

## 3. Proposed CLI Design

### Command Structure
```bash
magellan query --db <FILE> <query-type> [arguments]
```

### Query Types

| Query Type | Arguments | Description | Example |
|------------|-----------|-------------|---------|
| `find` | `--name <pattern>` `--kind <kind>` | Find symbols by name/kind | `find --name "*Config*" --kind Struct` |
| `calls` | `--symbol <name>` `--file <path>` | Show what a function calls | `calls --symbol main --file src/main.rs` |
| `called-by` | `--symbol <name>` `--file <path>` | Show who calls a function | `called-by --symbol parse_args` |
| `refs` | `--symbol <name>` `--file <path>` | Show references to symbol | `refs --symbol MyStruct` |
| `file` | `--path <path>` | Show symbols in file | `file --path src/lib.rs` |
| `path` | `--from <symbol>` `--to <symbol>` | Find path between symbols | `path --from main --to process` |
| `impact` | `--symbol <name>` `--file <path>` | **Show affected code if changed** | `impact --symbol parse_args` |
| `list` | `--kind <kind>` | List all of a kind | `list --kind Function` |

---

## 4. Detailed Query Specifications

### 4.1 `find` - Search for Symbols
```bash
# Find by name pattern
magellan query --db mag.db find --name "parse*"

# Find by kind
magellan query --db mag.db find --kind Function

# Find by name and kind
magellan query --db mag.db find --name "*Test*" --kind Function

# Find by file
magellan query --db mag.db find --file "src/lib.rs"
```

**Output Format:**
```
src/lib.rs:42: fn parse_args() -> Result<Args>
src/lib.rs:128: fn parse_mode(s: &str) -> Mode
src/cli/args.rs:15: struct Args
```

### 4.2 `calls` - Forward Call Graph
```bash
# What does main call?
magellan query --db mag.db calls --symbol main --file src/main.rs
```

**Output Format:**
```
main calls:
  -> parse_args (src/cli/args.rs:74)
  -> run_indexer (src/indexer.rs:12)
  -> println (std)
```

### 4.3 `called-by` - Reverse Call Graph
```bash
# Who calls parse_args?
magellan query --db mag.db called-by --symbol parse_args
```

**Output Format:**
```
parse_args is called by:
  <- main (src/main.rs:15)
  <- test_parse_empty_args (tests/cli_tests.rs:185)
  <- test_parse_version_flag (tests/cli_tests.rs:195)
```

### 4.4 `refs` - Find References
```bash
# Where is MyStruct referenced?
magellan query --db mag.db refs --symbol MyStruct
```

**Output Format:**
```
MyStruct referenced at:
  src/lib.rs:42 (variable declaration)
  src/usage.rs:15 (field access)
  tests/test.rs:78 (method call)
```

### 4.5 `file` - List File Contents
```bash
# Show all symbols in a file
magellan query --db mag.db file --path src/lib.rs
```

**Output Format:**
```
src/lib.rs:
  [Struct] Config (line 10)
  [Function] parse_args (line 42)
  [Function] run (line 85)
  [Impl] Config (line 120)
    [Method] validate (line 125)
```

### 4.6 `path` - Find Path Between Symbols
```bash
# Find call path from main to database_query
magellan query --db mag.db path --from main --to execute_query
```

**Output Format:**
```
Path from main to execute_query:
  main -> parse_args
  parse_args -> execute_query
```

### 4.7 `impact` - Change Impact Analysis ⭐
```bash
# What happens if I change parse_args?
magellan query --db mag.db impact --symbol parse_args
```

**Output Format:**
```
If you change 'parse_args' (src/cli/args.rs:74):

Direct callers (3):
  main (src/main.rs:15)
  test_parse_empty_args (tests/cli_tests.rs:185)
  test_parse_version_flag (tests/cli_tests.rs:195)

Transitively affected (12 functions total):
  main -> parse_args -> { parse_mode, run }
  test_parse_empty_args -> parse_args
  test_parse_version_flag -> parse_args -> { parse_mode }

Affected files (5):
  src/main.rs
  src/cli/args.rs
  tests/cli_tests.rs
  src/executor.rs
  src/modes.rs

Risk level: MEDIUM
```

**With `--verbose`:**
```
If you change 'parse_args' (src/cli/args.rs:74):

=== DIRECT CALLERS (3) ===
• main (src/main.rs:15)
  └─ Entry point - HIGH RISK

• test_parse_empty_args (tests/cli_tests.rs:185)
  └─ Test function - LOW RISK

• test_parse_version_flag (tests/cli_tests.rs:195)
  └─ Test function - LOW RISK

=== TRANSITIVE IMPACT (BFS depth 3) ===
Level 1 (3 callers):
  main, test_parse_empty_args, test_parse_version_flag

Level 2 (6 functions):
  parse_mode, run, validate, check_version, ...

Level 3 (3 functions):
  execute_query, handle_command, process_input

=== AFFECTED FILES (5) ===
• src/main.rs (uses parse_args)
• src/cli/args.rs (defines parse_args)
• tests/cli_tests.rs (tests parse_args)
• src/executor.rs (depends on parse_mode called by parse_args)
• src/modes.rs (depends on parse_mode)

=== RECOMMENDATION ===
Review tests before changing.
```

---

## 5. Output Formats

### 5.1 Human-Readable (Default)
```
src/lib.rs:42: fn parse_args() -> Result<Args>
```

### 5.2 JSON (for piping/automation)
```bash
magellan query --db mag.db find --name "test*" --format json
```

```json
{
  "results": [
    {
      "name": "test_parse_args",
      "kind": "Function",
      "file": "tests/cli_tests.rs",
      "line": 185
    }
  ]
}
```

### 5.3 Verbose (with details)
```bash
magellan query --db mag.db calls --symbol main --verbose
```

```
main calls:
  -> parse_args (src/cli/args.rs:74)
     Function defined at line 74
     Signature: fn parse_args() -> Result<Args>
     Total calls in this function: 3
```

---

## 6. Implementation Phases

### Phase 1: Core Queries (MVP)
- `find` - Search symbols by name/kind
- `calls` - Forward call graph
- `called-by` - Reverse call graph
- `file` - List file contents

**Complexity**: Low-Medium
**Dependencies**: Existing graph query methods

### Phase 2: Enhanced Queries
- `refs` - Find references
- `path` - Shortest path between symbols
- `list` - List all by kind
- `--format json` output

**Complexity**: Medium
**Dependencies**: BFS for path finding

### Phase 3: Advanced Features
- `--format markdown` for documentation
- `--output <file>` for saving results
- Regular expression patterns for name matching
- Combine filters (name + kind + file)

**Complexity**: Medium-High
**Dependencies**: None

### Phase 4: Interactive Mode (Optional)
```bash
magellan query --db mag.csv --interactive
```

```
> find --name *Config*
Found 3 symbols

> calls main
main calls:
  -> parse_args
  -> run
> exit
```

---

## 7. Technical Implementation Notes

### 7.1 New Module Structure
```
src/
  cli/
    query_cmd.rs     # Query command implementation
    query/
      mod.rs         # Query module exports
      find.rs        # Find symbols
      calls.rs       # Call graph queries
      refs.rs        # Reference queries
      path.rs        # Path finding
      output.rs      # Output formatting
```

### 7.2 Reusing Existing APIs
- `symbols_in_file()` - for `file` query
- `symbols_in_file_with_kind()` - for `find --kind`
- `calls_from_symbol()` - for `calls` query
- `callers_of_symbol()` - for `called-by` query
- `entity_ids()` + `get_node()` - for general queries

### 7.3 New APIs Needed
```rust
// For find by name pattern
fn symbols_by_name_pattern(&self, pattern: &str) -> Result<Vec<SymbolFact>>

// For references query
fn references_to_symbol_by_name(&self, symbol_name: &str) -> Result<Vec<ReferenceFact>>

// For path finding (BFS on call graph)
fn find_call_path(&self, from: &str, to: &str) -> Result<Vec<String>>
```

### 7.4 SQL Queries (for reference)
```sql
-- Find symbols by name pattern
SELECT id, kind, name, file_path, data
FROM graph_entities
WHERE kind = 'Symbol' AND name LIKE 'parse%';

-- Find incoming calls to a symbol
SELECT ge.*
FROM graph_entities ge
JOIN graph_edges e ON e.from_id = ge.id
WHERE e.edge_type = 'CALLS'
AND e.to_id = (SELECT id FROM graph_entities WHERE name = 'target_name');

-- Find path between symbols (requires recursive CTE or BFS in code)
```

---

## 8. Example Workflows

### 8.1 "What code handles CLI arguments?"
```bash
$ magellan query --db mag.db find --name "*arg*"
src/cli/args.rs:1: struct Args
src/cli/args.rs:15: enum Mode
src/cli/args.rs:42: fn parse_args()
```

### 8.2 "Who calls the database connection?"
```bash
$ magellan query --db mag.db called-by --symbol db_connect
db_connect is called by:
  <- main (src/main.rs:10)
  <- test_integration (tests/integration_tests.rs:42)
```

### 8.3 "Show me the call chain for feature X"
```bash
$ magellan query --db mag.db path --from main --to feature_x_run
Path from main to feature_x_run:
  main -> parse_args
  parse_args -> execute_command
  execute_command -> feature_x_run
```

### 8.4 "Find all test functions"
```bash
$ magellan query --db mag.db find --kind Function --name "*test*"
tests/api_tests.rs:10: fn test_api_success()
tests/api_tests.rs:42: fn test_api_failure()
tests/integration_tests.rs:15: fn test_database()
```

---

## 8. Roadmap Items (Needed before codemcp upgrades)

Feedback from codemcp (`docs/pr5.md`) and local testing introduced the following CLI + query upgrades. Each item keeps Magellan predictable while exposing richer metadata.

1. **`--explain-query` Flag** *(Status: ✅ implemented via `magellan query --explain` in `src/query_cmd.rs`)*  
   - *Goal*: Print cheat sheet describing `name`, `references:symbol`, `file:path`, and glob syntax so agents stop guessing.
   - *Implementation*: Add the flag handler that dumps the supported selectors + examples generated from `graph::query`.

2. **Normalized Symbol Kind Metadata** *(Status: ✅ persisted through `SymbolFact.kind_normalized` and JSON export)*  
   - *Goal*: codemcp should infer `kind` (fn/struct/enum/trait/const/static/mod) without user input.
   - *Implementation*: Extend `graph::symbols::SymbolFact` with `kind_normalized` derived at ingest time; persist to `graph_entities.data`; update CLI output + JSON to include the normalized type.

3. **`symbol_extent` API** *(Status: ✅ exposed via `CodeGraph::symbol_extents` and `magellan query --symbol ... --show-extent`)*  
   - *Goal*: codemcp needs byte ranges + line spans to implement `read_symbol`.
   - *Implementation*: New query `symbol_extent --symbol <name> --file <path>` returning file path, byte_start/end, start/end line/col; reuse `graph::symbols::lookup_symbol`.

4. **Glob-Based Symbol Search** *(Status: ✅ `magellan find --list-glob` returns deterministic node IDs using `globset`)*  
   - *Goal*: Bulk renames require enumerating `test_*` style sets before Splice runs.
   - *Implementation*: Teach `find` to accept glob patterns (backed by `globset`). Add `--list-glob <pattern>` that resolves to symbol IDs and prints them deterministically.

5. **Call to Action for codemcp**
   - Once the above ship, codemcp can:
     - Drop mandatory `kind` parameter in `refactor_rename`.
     - Provide inline query usage hints.
     - Implement `read_symbol` tool using `symbol_extent`.
     - Build `bulk_rename` wrappers on top of glob support.

---

## 9. Success Criteria

- [ ] All Phase 1 queries working
- [ ] JSON output format available
- [ ] Output shows file:line:column format
- [ ] Can query by symbol name patterns
- [ ] Can traverse call graph both directions
- [ ] All modules under 300 LOC
- [ ] Zero compilation warnings
- [ ] Test coverage for all query types

---

## 10. Open Questions

1. **Pattern matching**: Should we support regex or just glob-style wildcards (`*`, `?`)?
2. **Case sensitivity**: Should `find` be case-insensitive by default?
3. **Limit results**: Should we add `--limit` option for large result sets?
4. **Multiple files**: Should `file` query accept multiple paths?
5. **Cross-file references**: Should `refs` show which file each reference is from?

---

*Last Updated: 2025-12-27*

**Sources:**
- [code-graph-rag](https://github.com/vitali87/code-graph-rag)
- [Graph-Based Retrieval: How AI Code Agents Navigate](https://medium.com/data-science-collective/graph-based-retrieval-how-ai-code-agents-navigate-million-line-codebases-96f22d702902)
- [Sourcegraph: From code search to code intelligence](https://sourcegraph.com/blog/code-search-to-code-intelligence)
- [FalkorDB Code Graph](https://www.falkordb.com/blog/code-graph/)
- [Codebase Parser: Graph + Vector Tool](https://medium.com/@rikhari/codebase-parser-a-graph-vector-powered-tool-to-understand-visualize-and-query-any-codebase-90d065c24f15)

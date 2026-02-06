# CLI Patterns Reference

This document describes the CLI design patterns used by Magellan, intended to be followed by other tools in the integrated toolset (splice, llmtransform, llmastsearch, llmsearch, llfilewrite).

## Table of Contents

1. [Command Structure](#command-structure)
2. [Global Flags](#global-flags)
3. [Command Categories](#command-categories)
4. [Flag Patterns](#flag-patterns)
5. [Help Text Conventions](#help-text-conventions)
6. [Exit Codes](#exit-codes)
7. [Error Handling](#error-handling)
8. [Output Formats](#output-formats)

---

## Command Structure

### Basic Pattern

```
toolname <command> [arguments]
```

All tools follow this pattern:
- **toolname**: The executable name (e.g., `magellan`)
- **command**: The action to perform (required)
- **arguments**: Command-specific flags and values

### Global Flags (Before Command)

Some flags may be accepted before the command for tool-wide configuration:

```
toolname --global-flag <command> [arguments]
```

Magellan supports:
- `--version`, `-V`: Show version information
- `--help`, `-h`: Show usage information

---

## Global Flags

These flags are available across most commands:

| Flag | Values | Description |
|------|--------|-------------|
| `--output` | `human`, `json`, `pretty` | Output format (default: human) |
| `--db` | `<PATH>` | Path to database file |

### Output Format Values

- **`human`** (or `text`): Human-readable text output, default
- **`json`**: Compact JSON for programmatic consumption
- **`pretty`**: Formatted JSON with indentation

---

## Command Categories

### 1. Indexing Commands

Commands that build or maintain the index:

| Command | Purpose | Required Flags |
|---------|---------|----------------|
| `watch` | Watch directory and index changes | `--root`, `--db` |
| `migrate` | Upgrade database schema | `--db` |

#### Watch Arguments

```
watch --root <DIR> --db <FILE> [--debounce-ms <N>] [--watch-only] [--validate] [--validate-only]
```

- `--root <DIR>`: Directory to watch recursively (required)
- `--db <FILE>`: Path to database (required)
- `--debounce-ms <N>`: Debounce delay in milliseconds (default: 500)
- `--watch-only`: Skip initial scan, only watch for changes
- `--scan-initial`: Scan directory on startup (default: true)
- `--validate`: Enable pre/post-run validation
- `--validate-only`: Run validation without indexing

#### Migrate Arguments

```
migrate --db <FILE> [--dry-run] [--no-backup]
```

- `--db <FILE>`: Path to database (required)
- `--dry-run`: Check version without migrating
- `--no-backup`: Skip backup creation

### 2. Query Commands

Commands that retrieve data from the index:

| Command | Purpose | Required Flags |
|---------|---------|----------------|
| `status` | Show database statistics | `--db` |
| `query` | List symbols in a file | `--db`, `--file` |
| `find` | Find a symbol by name | `--db` |
| `refs` | Show calls for a symbol | `--db`, `--name`, `--path` |
| `get` | Get source code for a symbol | `--db`, `--file`, `--symbol` |
| `get-file` | Get all code for a file | `--db`, `--file` |
| `files` | List indexed files | `--db` |
| `label` | Query symbols by label | `--db` |

#### Status Arguments

```
status --db <FILE> [--output <FORMAT>]
```

#### Query Arguments

```
query --db <FILE> --file <PATH> [--kind <KIND>] [--with-context] [--with-callers] [--with-callees] [--with-semantics] [--with-checksums] [--context-lines <N>]
```

- `--file <PATH>`: File path to query (required unless `--explain`)
- `--kind <KIND>`: Filter by symbol kind
- `--with-context`: Include source code context lines
- `--with-callers`: Include caller references
- `--with-callees`: Include callee references
- `--with-semantics`: Include symbol kind and language
- `--with-checksums`: Include content checksums
- `--context-lines <N>`: Number of context lines (default: 3, max: 100)

#### Find Arguments

```
find --db <FILE> (--name <NAME> | --symbol-id <ID> | --ambiguous <NAME>) [--path <PATH>] [--first] [--output <FORMAT>]
```

- `--name <NAME>`: Symbol name to find
- `--symbol-id <ID>`: Stable SymbolId for precise lookup
- `--ambiguous <NAME>`: Show all candidates for ambiguous name
- `--path <PATH>`: Limit search to specific file
- `--first`: Use first match when ambiguous (deprecated)

#### Refs Arguments

```
refs --db <FILE> --name <NAME> --path <PATH> [--symbol-id <ID>] [--direction <in|out>] [--output <FORMAT>]
```

- `--name <NAME>`: Symbol name to query (required)
- `--path <PATH>`: File path containing the symbol (required)
- `--symbol-id <ID>`: Use SymbolId instead of name
- `--direction <in|out>`: Show incoming (in) or outgoing (out) calls (default: in)

#### Get Arguments

```
get --db <FILE> --file <PATH> --symbol <NAME> [--with-context] [--with-semantics] [--with-checksums] [--context-lines <N>]
```

#### Get-File Arguments

```
get-file --db <FILE> --file <PATH>
```

#### Files Arguments

```
files --db <FILE> [--symbols] [--output <FORMAT>]
```

- `--symbols`: Show symbol count per file

#### Label Arguments

```
label --db <FILE> [--label <LABEL>...] [--list] [--count] [--show-code]
```

- `--label <LABEL>`: Label to query (can specify multiple for AND)
- `--list`: List all available labels with counts
- `--count`: Count entities with specified labels
- `--show-code`: Show source code for matches

### 3. Export Commands

Commands that export data in various formats:

| Command | Purpose | Required Flags |
|---------|---------|----------------|
| `export` | Export graph data | `--db` |
| `collisions` | List ambiguous symbols | `--db` |

#### Export Arguments

```
export --db <FILE> [--format json|jsonl|csv|scip] [--output <PATH>] [--minify] [--no-symbols] [--no-references] [--no-calls] [--include-collisions] [--collisions-field <FIELD>]
```

- `--format <FORMAT>`: Export format (default: json)
- `--output <PATH>`: Write to file instead of stdout
- `--minify`: Use compact JSON
- `--no-symbols`: Exclude symbols from export
- `--no-references`: Exclude references from export
- `--no-calls`: Exclude calls from export
- `--include-collisions`: Include collision groups
- `--collisions-field <FIELD>`: Collision field (fqn, display_fqn, canonical_fqn)

#### Collisions Arguments

```
collisions --db <FILE> [--field <FIELD>] [--limit <N>] [--output <FORMAT>]
```

- `--field <FIELD>`: Field to check (fqn, display_fqn, canonical_fqn)
- `--limit <N>`: Maximum groups to show (default: 50)

### 4. Validation Commands

Commands that verify data integrity:

| Command | Purpose | Required Flags |
|---------|---------|----------------|
| `verify` | Verify database vs filesystem | `--root`, `--db` |

#### Verify Arguments

```
verify --root <DIR> --db <FILE>
```

---

## Flag Patterns

### Boolean Flags

- Presence enables the feature: `--validate`, `--minify`
- Negation disables: `--no-backup`, `--no-symbols`

### Path Arguments

- Use `--root <DIR>` for directories
- Use `--file <PATH>` for files
- Use `--db <FILE>` for database paths
- Use `--path <PATH>` for generic file paths
- Use `--output <PATH>` for output destinations

### Selection Arguments

- `--name <NAME>`: Select by name
- `--symbol-id <ID>`: Select by stable ID
- `--kind <KIND>`: Select by type/category
- `--label <LABEL>`: Select by tag/category

### Numeric Arguments

- `--debounce-ms <N>`: Milliseconds
- `--limit <N>`: Count limit
- `--context-lines <N>`: Line count (default: 3, max: 100)
- `--max-depth <N>`: Depth limit

---

## Help Text Conventions

### Usage Summary

```bash
toolname --help
```

Output format:

```
ToolName - Brief description

Usage:
  toolname <command> [arguments]
  toolname --help

Commands:
  command1  Brief description
  command2  Brief description

Global arguments:
  --flag <VALUE>   Description
```

### Per-Command Help

Each command should document:
1. Required arguments
2. Optional arguments with defaults
3. Exit codes (non-obvious ones)

### Description Style

- Use present tense: "Show database statistics" (not "Shows")
- Be concise: Prefer "Watch directory" over "Watch the directory"
- Group related flags: Put all output flags together

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Usage error (invalid arguments) |
| 3 | Database error |
| 4 | File not found |
| 5 | Validation failed |

### Error Response Format (JSON mode)

```json
{
  "error": "error_category",
  "message": "Human-readable error message",
  "code": "ERROR_CODE",
  "span": { ... },
  "remediation": "Suggested fix"
}
```

---

## Error Handling

### Argument Validation

- Return exit code 1 with usage hint for missing required arguments
- Validate argument values before processing
- Show specific error: `--db is required`

### File Operations

- Return exit code 4 for file not found
- Return exit code 3 for database errors
- Include file path in error message

### Validation Errors

- Return exit code 5 for validation failures
- Output structured errors in JSON mode
- Show detailed error information in human mode

---

## Output Formats

### Human Output

- Plain text, readable by humans
- Line-based for simple parsing
- Key-value pairs for status

Example:
```
files: 42
symbols: 1337
references: 256
```

### JSON Output

All JSON responses include:

```json
{
  "schema_version": "1.0.0",
  "execution_id": "abc123-def456",
  "tool": "toolname",
  "timestamp": "2026-01-24T12:00:00Z",
  "data": { ... }
}
```

### Pretty JSON Output

Same structure as JSON, but formatted with 2-space indentation.

---

## Version Information

```bash
toolname --version
toolname -V
```

Output format:
```
toolname VERSION (COMMIT DATE) rustc RUSTC_VERSION
```

Example:
```
magellan 0.5.0 (abc123def 2026-01-24) rustc 1.83.0
```

---

## Extension Guidelines for Other Tools

When adding commands to other tools:

1. **Follow the command categories**: Indexing, Query, Export, Validation
2. **Use consistent flag names**: `--db`, `--output`, `--format`
3. **Include `--help` and `--version`**: Required for all tools
4. **Support `--output json`**: For programmatic consumption
5. **Return appropriate exit codes**: Use standard codes where applicable
6. **Document required arguments clearly**: In help text

### Example: Adding a New Tool Command

```bash
newtool export --db <FILE> [--format json|csv] [--output <PATH>]
```

Follows Magellan's pattern:
- `--db`: Standard database path flag
- `--format`: Standard format selection
- `--output`: Standard output destination

---

## Common Command Patterns

### Pattern: Selection by ID vs Name

```bash
# By name (may be ambiguous)
tool query --name <NAME>

# By stable ID (precise)
tool query --symbol-id <ID>

# Show all ambiguous candidates
tool query --ambiguous <NAME>
```

### Pattern: Context Enhancement

```bash
# Basic output
tool query --db db.db --file path.rs

# With context lines
tool query --db db.db --file path.rs --with-context

# With relationships
tool query --db db.db --file path.rs --with-callers --with-callees

# With semantics
tool query --db db.db --file path.rs --with-semantics
```

### Pattern: Output Format Selection

```bash
# Human (default)
tool status --db db.db

# Compact JSON
tool status --db db.db --output json

# Pretty JSON
tool status --db db.db --output pretty
```

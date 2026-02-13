# Code Graph Integration Summary

**Date:** 2026-02-11
**Purpose:** Document GSD workflow updates for efficient code graph usage

## Overview

Subagents now have a comprehensive workflow guide for using magellan and llmgrep CLI tools to query the code graph database instead of reading full files. This reduces:
- Token usage (query database vs read files)
- Context bloat (targeted results vs full file contents)
- Repetitive work (cached queries)

## Files Modified

### 1. `/home/feanor/.claude/get-shit-done/references/code-graph-workflow.md`

**New file** - Comprehensive workflow guide covering:

**Magellan Commands:**
- `status` - Check database health
- `find` - Locate symbol by name
- `refs` - Show call relationships (in/out)
- `query` - List symbols in a file
- `get` - Get source code for symbol
- `chunks` - Get code chunks in file
- `cycles` - Detect circular dependencies
- `dead-code` - Find unreachable code
- `reachable` - Find what's reachable from symbol
- `paths` - Enumerate execution paths

**llmgrep Commands:**
- `search` - Semantic code search with filters (path, kind, mode, context)

**Workflow Examples:**
- Finding a function and its callers
- Understanding file structure
- Searching for specific patterns
- Getting implementation for editing
- Impact analysis before refactoring

### 2. `/home/feanor/.claude/get-shit-done/bin/gsd-tools.js`

**Changes:**
- Removed obsolete `graph-state`, `graph-plan`, `graph-symbols` commands (were based on wrong assumptions)
- Removed graph command routing from main()
- Updated usage documentation to reference workflow guide instead

**Rationale:** The graph tools (magellan/llmgrep) are CLI binaries that should be called directly by subagents via Bash, not wrapped in gsd-tools.js. The workflow guide provides the correct patterns.

### 3. `/home/feanor/.claude/get-shit-done/workflows/execute-plan.md`

**Changes:**
- Added `@/home/feanor/.claude/get-shit-done/references/code-graph-workflow.md` to required_reading

### 4. `/home/feanor/.claude/get-shit-done/workflows/execute-phase.md`

**Changes:**
- Updated `@/home/feanor/.claude/get-shit-done/references/code-graph-workflow.md` to required_reading

## Key Workflows

### Finding a symbol

```bash
# Find symbol location
magellan find --db .codemcp/codegraph.db --name "symbol_name"

# Get incoming references (what calls this?)
magellan refs --db .codemcp/codegraph.db --name "symbol_name" --direction in

# Get outgoing references (what does this call?)
magellan refs --db .codemcp/codegraph.db --name "symbol_name" --direction out
```

### Understanding a file

```bash
# List symbols in file (no file read needed)
magellan query --db .codemcp/codegraph.db --file "src/main.rs"
```

### Searching codebase

```bash
# Semantic search by name
llmgrep search --db .codemcp/codegraph.db --query "auth" --output human

# With filters
llmgrep search --db .codemcp/codegraph.db --query "auth" --kind Function --path "src/api/" --output human
```

## Database Location

**Always use:** `.codemcp/codegraph.db`

**Binaries:**
- `magellan` → `/home/feanor/.local/bin/magellan`
- `llmgrep` → `/home/feanor/.local/bin/llmgrep`

## Usage Pattern

1. **Before any query:** Verify database exists
   ```bash
   magellan status --db .codemcp/codegraph.db
   ```

2. **Query for specific information:** Use appropriate command
   ```bash
   magellan find --db .codemcp/codegraph.db --name "symbol"
   ```

3. **Get source when needed:** Only read implementation when editing
   ```bash
   magellan get --db .codemcp/codegraph.db --file "src/file.rs" --symbol "symbol"
   ```

## Benefits

| Aspect | Without Graph | With Graph |
|---------|---------------|-------------|
| Token usage | Read full files | Query for specific data |
| Context efficiency | Full file contents | Targeted results only |
| Speed | File I/O | Indexed database query |
| Reliability | Must read correctly | Structured queries |

## Testing

All commands tested and verified working:
- magellan status: ✓
- magellan find: ✓
- magellan refs: ✓
- magellan query: ✓
- magellan get: ✓
- magellan chunks: ✓
- llmgrep search: ✓
- llmgrep search with filters: ✓
- magellan cycles: ✓

See individual test results in session history.

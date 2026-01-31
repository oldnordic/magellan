# Magellan Scripts Collection

This directory contains project-agnostic scripts for code analysis using Magellan.

## Quick Start

1. **Copy to your project** (or use directly from Magellan):
   ```bash
   cp -r /path/to/magellan/scripts /your/project/scripts
   ```

2. **Configure** (optional, via environment variables):
   ```bash
   export PROJECT_NAME=myproject
   export DB_DIR=.codemcp
   export SRC_DIR=src
   ```

3. **Start the watcher**:
   ```bash
   ./scripts/magellan-workflow.sh start
   ```

## Available Scripts

### Core Analysis

| Script | Purpose | Example |
|--------|---------|---------|
| `magellan-workflow.sh` | Main workflow: start/stop watcher, search, get, refs | `./scripts/magellan-workflow.sh search "MyFunction"` |
| `blast-zone.sh` | Impact analysis: what gets affected by a change | `./scripts/blast-zone.sh "process_file" --max-depth 2` |
| `call-chain.sh` | Show call chains forward/backward | `./scripts/call-chain.sh "handle_request" --direction backward` |
| `unreachable.sh` | Find unreachable public functions | `./scripts/unreachable.sh` |
| `module-deps.sh` | Module dependency analysis | `./scripts/module-deps.sh --format dot` |

### AST Analysis (New - requires schema v5)

| Script | Purpose | Example |
|--------|---------|---------|
| `ast-query.sh` | Query AST nodes by kind, file, show tree structure | `./scripts/ast-query.sh --kind if_expression --tree` |
| `complexity.sh` | Calculate cyclomatic complexity using AST | `./scripts/complexity.sh --threshold 10` |
| `nesting.sh` | Find deeply nested code using AST | `./scripts/nesting.sh --threshold 4` |

## AST Analysis Examples

### Query AST Nodes

```bash
# Show all AST nodes for a file as a tree
./scripts/ast-query.sh --file src/main.rs --tree

# Find all if expressions
./scripts/ast-query.sh --kind if_expression

# Count nodes by kind
./scripts/ast-query.sh --count

# Show top 10 most common node kinds
./scripts/ast-query.sh --top 10
```

### Complexity Analysis

```bash
# Show all functions with complexity > 10
./scripts/complexity.sh --threshold 10

# Show top 20 most complex functions
./scripts/complexity.sh --top 20

# Analyze specific file
./scripts/complexity.sh --file src/main.rs

# Export as CSV for further analysis
./scripts/complexity.sh --format csv > complexity.csv
```

### Nesting Analysis

```bash
# Find deeply nested code (depth > 4)
./scripts/nesting.sh

# Use different threshold
./scripts/nesting.sh --threshold 3

# Analyze specific file
./scripts/nesting.sh --file src/main.rs

# Show detailed breakdown
./scripts/nesting.sh --details
```

## Configuration

All scripts respect these environment variables:

- `PROJECT_NAME` - Database filename (default: magellan)
- `DB_DIR` - Database directory (default: .codemcp)
- `SRC_DIR` - Source directory to index (default: src)

## Requirements

- `magellan` - Code graph CLI tool (install via `cargo install --path .`)
- `llmgrep` - Symbol search CLI (installed with magellan)
- `jq` - JSON processor (for formatted output)
- `sqlite3` - For metrics queries

## Installing Magellan

From the Magellan project directory:
```bash
cargo install --path .
```

This installs both `magellan` and `llmgrep` to your PATH.

## Schema Version Requirements

- **Core scripts (blast-zone, call-chain, unreachable, module-deps)**: Work with schema v4+
- **AST scripts (ast-query, complexity, nesting)**: Require schema v5+ (ast_nodes table)

To upgrade your database:
```bash
magellan migrate --db .codemcp/magellan.db
```

## Common Node Kinds for AST Queries

| Kind | Description |
|------|-------------|
| `function_item` | Function definitions |
| `struct_item` | Struct definitions |
| `enum_item` | Enum definitions |
| `impl_item` | Implementation blocks |
| `if_expression` | If statements/expressions |
| `while_expression` | While loops |
| `for_expression` | For loops |
| `loop_expression` | Loop blocks |
| `match_expression` | Match expressions |
| `block` | Code blocks |
| `call_expression` | Function calls |
| `return_expression` | Return statements |
| `let_declaration` | Variable declarations |
| `unsafe_block` | Unsafe blocks |

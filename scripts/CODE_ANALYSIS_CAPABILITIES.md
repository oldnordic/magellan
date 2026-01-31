# Code Analysis Tools for Magellan - What's Possible

## Available Tools

| Tool | Version | Purpose |
|------|---------|---------|
| magellan | 1.9.0 | Code graph database, stores symbols/refs/calls/**AST** |
| llmgrep | 1.9.0 | Search symbols by mode (symbols/references/calls) |

### Analysis Capabilities

| Feature | magellan | llmgrep | Combined Script |
|---------|----------|---------|-----------------|
| **Find symbols** | ✅ `find --name` | ✅ `search --mode symbols` | ✅ |
| **Find references** | ✅ `refs --direction in/out` | ✅ `--mode references` | ✅ |
| **Find calls** | ❌ | ✅ `--mode calls` | ✅ |
| **Get source code** | ✅ `get` | ❌ | ✅ |
| **List files** | ✅ `files --symbols` | ❌ | ✅ |
| **Query by label** | ✅ `label --list` | ❌ | ✅ |
| **Query AST nodes** | ✅ `ast` | ❌ | ✅ |
| **Find by AST kind** | ✅ `find-ast` | ❌ | ✅ |
| **JSON output** | ✅ | ✅ `--output json` | ✅ |

---

## PathLite: What We CAN Build Now (Enhanced with AST v5)

### 1. Call Chain Analysis (Forward & Backward) ✅

```bash
# Using llmgrep (simpler)
llmgrep --db magellan.db search --query "symbol" --mode references --output json

# Or use the script
./scripts/call-chain.sh "my_function"
./scripts/call-chain.sh "MyStruct::method" --direction backward
```

**Implemented:** `scripts/call-chain.sh`

### 2. Blast Zone (Impact Analysis) ✅

- Traverse graph forward from a symbol
- Find all downstream dependents
- Mark affected symbols
- Useful for refactoring safety checks

**Implemented:** `scripts/blast-zone.sh`

### 3. Unreachable Code Detection (Function-Level) ✅

- Find entry points (main, pub fn in lib.rs, tests)
- Traverse call graph
- Mark reachable symbols
- Report unreachable public functions

**Limitation:** Only detects unreferenced functions, not dead code within functions

**Implemented:** `scripts/unreachable.sh`

### 4. Module Dependency Graph ✅

- File-to-file reference counting
- Module-level dependency matrix
- Hotspot analysis (most referenced modules)

**Implemented:** `scripts/module-deps.sh`

### 5. Symbol Usage Audit ✅

- Check if symbol is wired (`check-wire`)
- Find all callers
- Find all callees

**Implemented:** `magellan-workflow.sh check-wire`

### 6. Metrics-Based Hotspots (Phase 34+) ✅

- File-level complexity scores
- Fan-in/fan-out analysis
- Pre-computed for fast queries

**Implemented:** `magellan-workflow.sh hotspots`

### 7. Cyclomatic Complexity via AST (Phase 36+) ✅ **NEW**

- Count decision points (if/while/for/loop/match)
- Per-function complexity scoring
- Identify overly complex functions

**Implemented:** `scripts/complexity.sh`

### 8. Nesting Depth Analysis (Phase 36+) ✅ **NEW**

- Find deeply nested code blocks
- Track nesting hierarchy using AST parent relationships
- Identify refactoring candidates

**Implemented:** `scripts/nesting.sh`

### 9. AST Structure Queries (Phase 36+) ✅ **NEW**

- Query nodes by kind (function, if, for, match, etc.)
- Show tree structure with parent-child relationships
- Position-based queries (find node at byte offset)

**Implemented:** `scripts/ast-query.sh`, `magellan ast`, `magellan find-ast`

### 10. Guard/Invariant Verification ⚠️ (Call-Chain Level)

- Define guards as graph constraints
- Verify constraint satisfaction via reachability
- "Call chain to X must pass through Y"

**Limitation:** Cannot verify guard is on same execution path (requires CFG)

---

## PathFull: What Requires AST/CFG

These features genuinely require AST/CFG infrastructure:

### 1. Intra-Function Control Flow Graph ❌

**Why:** Magellan doesn't track branches, if-statements, loops at statement level

**What would need:**
- AST-level tracking of basic blocks
- Block entry/exit points
- Branch edge types (TRUE/FALSE/FALLTHROUGH)

### 2. Exact Path Enumeration ❌

**Why:** Requires CFG to enumerate all execution paths through a function

**What would need:**
- CFG with all edges
- Path enumeration algorithm
- Feasibility analysis

### 3. Dominator Analysis ❌

**Why:** Requires CFG and dominator tree algorithm

**What would need:**
- Complete CFG
- Dominator algorithm (Lengauer-Tarjan)
- Post-dominator analysis

### 4. Precise Unreachable Code Within Functions ❌

**Why:** Requires CFG to find dead code branches

**What would need:**
- CFG with all edges
- Entry point analysis at basic block level
- Reachability within function

---

## Database Schema (v5)

```sql
graph_entities         -- Nodes (symbols with file_path, kind, data)
graph_edges            -- Edges (from_id, to_id, edge_type)
graph_labels           -- Labels (entity_id, label)
graph_properties        -- Properties (entity_id, key, value)
file_metrics           -- Phase 34: File-level metrics (fan-in/out, LOC, complexity)
symbol_metrics        -- Phase 34: Symbol-level metrics
ast_nodes              -- Phase 36: AST hierarchy (id, parent_id, kind, byte_start, byte_end)
```

## Edge Types Available

- `CALLER` - Function calls another function
- `CALLS` - Function calls another
- `DEFINES` - Symbol defines another
- `REFERENCES` - General references

## Labels Available

Examples depend on your project's code:
- Language tags (rust, python, etc.)
- Kind tags (fn, struct, enum, class, etc.)

---

## Scripts Available

### Core Scripts
1. **magellan-workflow.sh** - Main workflow for watcher management, search, get, refs
2. **call-chain.sh** - Forward/backward call analysis
3. **blast-zone.sh** - Impact analysis
4. **unreachable.sh** - Find unreachable public functions
5. **module-deps.sh** - Module dependency graph

### AST Scripts (v5+)
1. **ast-query.sh** - Query AST nodes by kind, file, show tree structure
2. **complexity.sh** - Cyclomatic complexity using AST decision points
3. **nesting.sh** - Find deeply nested code using parent relationships

## Example Usage

```bash
# Start the watcher
./scripts/magellan-workflow.sh start

# Check if a function is wired
./scripts/magellan-workflow.sh check-wire "my_function"

# Show call chain
./scripts/call-chain.sh "MyClass::process"

# Impact analysis
./scripts/blast-zone.sh "dequantize" --max-depth 2

# Find unreachable code
./scripts/unreachable.sh

# Module dependencies
./scripts/module-deps.sh --format dot > deps.dot
dot -Tpng deps.dot -o deps.png

# Show hotspots (Phase 34+)
./scripts/magellan-workflow.sh hotspots 50

# AST queries (Phase 36+)
./scripts/ast-query.sh --count
./scripts/ast-query.sh --kind if_expression

# Complexity analysis (Phase 36+)
./scripts/complexity.sh --threshold 10

# Nesting analysis (Phase 36+)
./scripts/nesting.sh --threshold 4
```

---

## Configuration

All scripts can be configured via environment variables:

- `PROJECT_NAME` - Database filename (default: magellan)
- `DB_DIR` - Database directory (default: .codemcp)
- `SRC_DIR` - Source directory (default: src)

Example:
```bash
PROJECT_NAME=myproject DB_DIR=/path/to/db ./scripts/magellan-workflow.sh start
```

---

## Key Insights

1. **llmgrep --mode calls** - finds who-calls-who relationships
2. **magellan refs supports --direction** - critical for forward/backward chains
3. **Graph is function-level** - CFG requires AST
4. **Multiple "forward" symbols** - need precise symbol identification (name + path or symbol-id)
5. **PathLite is valuable now** - 80% of value for 20% of effort
6. **PathFull requires more investment** - Remaining 20% for 80% more effort
7. **Phase 34 metrics** - Pre-computed tables enable fast hotspots and complexity queries
8. **Phase 36 AST nodes** - Enable complexity, nesting, and structure queries
9. **Schema v5 required** for AST scripts - run `magellan migrate` to upgrade

---

## Integration with codemcp MCP Server

When using the codemcp MCP server, these scripts complement the available tools:

- **codemcp**: Semantic search, symbol discovery, impact analysis
- **scripts**: Task-specific workflows, formatted output, batch operations

Use codemcp for interactive exploration and scripts for automated analysis.

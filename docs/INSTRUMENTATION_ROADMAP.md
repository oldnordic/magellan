# Instrumentation & Correctness Roadmap

**Philosophy**: Measurable improvement, not vibes. Turn "experimental" into provably better.

---

## 1. Per-Language Instrumentation

### Stats Collection
```sql
-- Reference outcome tracking
CREATE TABLE reference_outcomes (
    language TEXT,
    file_path TEXT,
    symbol_name TEXT,
    outcome TEXT,  -- 'found' | 'patched' | 'rejected' | 'shadowed' | 'ambiguous'
    reason TEXT,
    byte_start INTEGER,
    byte_end INTEGER,
    timestamp INTEGER
);
```

### Metrics to Track
| Metric | Description | Target |
|--------|-------------|--------|
| refs_found | Total references detected | - |
| refs_patched | Successfully renamed | >95% |
| refs_rejected | Rejected (shadowing, ambiguity) | <5% |
| parse_failures | Files that failed to parse | <1% |
| false_positives | Wrong refs patched | 0% |
| false_negatives | Missed refs | 0% |

### CLI Output
```
$ magellan rename --name foo --to bar --db ./mag.db
Language    Found    Patched  Rejected  Rate
---------    -----    -------  --------  -----
Rust         142      140      2         98.6%
Python       89       87       2         97.8%
C            45       44       1         97.8%
TOTAL        276      271      5         98.2%
```

---

## 2. Failure Taxonomy

### Categories
```
SHADOWING        - Symbol shadowed by local definition
AMBIGUITY        - Parse ambiguity (multiple valid parses)
PARSE_FAIL       - Syntax error in source
TYPE_MISMATCH    - Type incompatible for operation
CROSS_FILE       - Reference crosses file boundary (not resolved)
NOT_FOUND        - Symbol definition not found
DUPLICATE        - Multiple definitions with same name
INVALID_SPAN     - Byte span validation failed
```

### Storage
```sql
CREATE TABLE failures (
    op_id TEXT,
    language TEXT,
    file_path TEXT,
    category TEXT,
    context TEXT,
    timestamp INTEGER
);

-- Per-language summary
CREATE VIEW failure_rates AS
SELECT language, category, COUNT(*) as count,
       CAST(COUNT(*) AS FLOAT) / SUM(COUNT(*)) OVER (PARTITION BY language) as rate
FROM failures GROUP BY language, category;
```

---

## 3. Graph Diff (Before/After)

### Snapshot Model
```sql
CREATE TABLE graph_snapshots (
    snapshot_id TEXT PRIMARY KEY,
    op_id TEXT,
    timestamp INTEGER,
    node_count INTEGER,
    edge_count INTEGER
);

CREATE TABLE snapshot_nodes (
    snapshot_id TEXT,
    node_id INTEGER,
    node_type TEXT,
    data JSON
);

CREATE TABLE snapshot_edges (
    snapshot_id TEXT,
    from_node INTEGER,
    to_node INTEGER,
    edge_type TEXT
);
```

### Diff Output
```
$ magellan diff --before op_123 --after op_456
Nodes: +12 -3 (+9 net)
Edges: +28 -5 (+23 net)

Changes:
  + Symbol: bar (Function) at src/main.rs:42
  - Symbol: foo (Function) at src/main.rs:42
  ~ Reference: src/main.rs:85 foo → bar
  ~ Reference: src/lib.rs:23 foo → bar
```

---

## 4. Three-Tier Operation Model

| Tier | Description | Precision | Speed | Safety |
|------|-------------|-----------|-------|--------|
| `--tier text` | Text-based search/replace | Low | Fast | Unsafe |
| `--tier ast` | Tree-sitter AST, single-file | High | Medium | Safe |
| `--tier graph` | Cross-file, type-aware | Highest | Slow | Safest |

### Example
```bash
# Fast but risky - text only
magellan rename --tier text --name foo --to bar

# Safe single-file - AST-aware
magellan rename --tier ast --name foo --to bar

# Full cross-file - graph analysis
magellan rename --tier graph --name foo --to bar
```

---

## 5. Standard CLI Contract

### Input
```bash
# JSON via stdin or --input
echo '{"query": "symbol:foo"}' | magellan query
magellan query --input query.json
```

### Output
```bash
# JSON output
magellan query --output json --file src/main.rs

# Human-readable (default)
magellan query --file src/main.rs
```

### Exit Codes
| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (generic) |
| 2 | Validation failed |
| 3 | Parse errors |
| 4 | Ambiguous operation |
| 5 | Nothing to do |

### Report Formats
```bash
magellan rename --report md > report.md
magellan rename --report html > report.html
magellan rename --report json > report.json
```

### Operation ID
```bash
# Every mutating operation gets an ID
magellan rename --name foo --to bar
# → "op_id: op_20251230_123456_abc"

# Replay/resume
magellan --resume op_20251230_123456_abc
```

---

## 6. Hard Workflow Tool (LLM-Facing)

### Enforced Pipeline
```
QUERY → PLAN → MUTATE(1) → VALIDATE → COMMIT/ROLLBACK
         ↓                                      ↓
      snapshot                              diff_report
```

### No Shortcuts
```bash
# codemcp integration enforces this
codemcp rename foo bar
  1. query("foo") → finds all occurrences
  2. plan() → generates mutation plan
  3. mutate(1) → applies ONE change at a time
  4. validate() → LSP/compile check
  5. commit() OR rollback() → atomic decision
```

### Checkpoint System
```sql
CREATE TABLE checkpoints (
    op_id TEXT PRIMARY KEY,
    phase TEXT,  -- 'query' | 'plan' | 'mutate' | 'validate' | 'commit' | 'rollback'
    state JSON,
    timestamp INTEGER
);
```

---

## 7. Truth Storage (Append-Only Timeline)

### Schema
```sql
CREATE TABLE operations (
    op_id TEXT PRIMARY KEY,
    tool TEXT,
    command TEXT,
    args_json TEXT,
    timestamp INTEGER
);

CREATE TABLE operation_spans (
    op_id TEXT,
    file_path TEXT,
    byte_start INTEGER,
    byte_end INTEGER,
    old_hash TEXT,
    new_hash TEXT
);

CREATE TABLE operation_diagnostics (
    op_id TEXT,
    level TEXT,  -- 'info' | 'warn' | 'error'
    message TEXT,
    context JSON
);

CREATE TABLE operation_diffs (
    op_id TEXT,
    file_path TEXT,
    diff_unified TEXT
);
```

### Timeline Query
```bash
# What happened to this file?
magellan timeline --file src/main.rs

# What did this operation change?
magellan timeline --op op_123 --show diff
```

---

## 8. Span-Safe Batch Operations

### Verified Operations
```rust
// Reverse-order to prevent span shifting
pub fn verified_span_replace(
    graph: &mut CodeGraph,
    replacements: Vec<(Span, String)>,
) -> Result<Vec<Diff>> {
    // 1. Sort by byte_end DESCENDING
    // 2. Verify no overlaps
    // 3. Validate each span against current source
    // 4. Apply atomically per-file
    // 5. Return diffs
}

pub fn verified_span_delete(
    graph: &mut CodeGraph,
    spans: Vec<Span>,
) -> Result<Vec<Diff>> {
    // Same validation, deletion instead of replacement
}
```

### Per-File Atomicity
```sql
BEGIN TRANSACTION;
  -- Apply all changes to one file
  -- Update graph nodes
  -- Verify integrity
COMMIT; -- or ROLLBACK on error
```

---

## 9. Language Scope

### Core (Built-in)
| Language | Status | Priority |
|----------|--------|----------|
| Rust | ✅ Complete | P0 |
| C/C++ | ✅ Complete | P0 |
| Java | ✅ Complete | P1 |
| Python | ✅ Complete | P1 |
| JavaScript/TypeScript | ✅ Complete | P1 |

### Plugin Territory
| Language | Notes |
|----------|-------|
| Go | WASM plugin |
| Ruby | WASM plugin |
| PHP | WASM plugin |
| C# | WASM plugin |

---

## 10. Plugin Layer

### Toollet Manifest
```toml
[toollet]
name = "magellan-go"
version = "0.1.0"
language = "Go"
runtime = "wasm"

[capabilities]
can_parse = [".go"]
can_query_symbols = true
can_find_references = true
can_rename = true
cross_file = false  -- Single-file only

[permissions]
require = ["read_workspace", "modify_files"]
```

### Permission Rules
```rust
pub enum Capability {
    ReadWorkspace,
    ModifyFiles { extensions: Vec<String> },
    NetworkAccess { hosts: Vec<String> },
    ExecuteCommand { command: String },
}

pub fn validate_toollet(
    manifest: &ToolletManifest,
    requested_caps: &[Capability],
) -> Result<()>;
```

### No Arbitrary Code
- Toollets declare capabilities upfront
- User grants permissions per-run or via config
- WASM sandboxed execution
- No native code without explicit grant

---

## 11. Deterministic Resumes

### Checkpoint Replay
```bash
# Operation failed mid-way
magellan rename --name foo --to bar
# → Error at src/lib.rs:142: validation failed

# Resume from last checkpoint
magellan --resume op_20251230_123456_abc
# → Picks up from before failed mutation
# → No chat context needed
```

### Checkpoint Storage
```sql
CREATE TABLE checkpoints (
    op_id TEXT,
    sequence INTEGER,
    phase TEXT,
    state_json TEXT,
    can_resume BOOLEAN,
    PRIMARY KEY (op_id, sequence)
);
```

### No Chat Dependency
```bash
# Pure operation replay - zero LLM needed
magelland replay --op op_123 --from validate
```

---

## 12. Correctness Metrics

### Per-Language Coverage
```bash
$ magellan metrics --language rust
Reference Finding:
  True Positives:  142 (98.6%)
  False Positives: 2   (1.4%)
  False Negatives: 0   (0%)

Rename Success:
  Patched: 140
  Rejected: 2  (shadowing)
  Failed: 0

Cross-File:
  Intra-file: 138
  Inter-file: 4
  Missed: 0
```

### Boring Improvements Shipping
```
Week 1: Add shadowing detection → +2% patch rate
Week 2: Fix span validation → -5 false positives
Week 3: Add type-aware resolution → +3 inter-file refs
Week 4: Optimize C++ parsing → 20% faster
```

### What Gets Measured Gets Improved
| Metric | Week 1 | Week 2 | Week 3 | Week 4 |
|--------|--------|--------|--------|--------|
| Patch Rate | 92% | 94% | 96% | 98% |
| False Positives | 15 | 8 | 3 | 0 |
| Parse Speed | 100% | 95% | 80% | 80% |

---

## Implementation Priority

| Phase | Focus | Deliverable |
|-------|-------|-------------|
| P0 | Failure taxonomy | `CREATE TABLE failures` + CLI reporting |
| P0 | Stats collection | Per-language outcome tracking |
| P1 | Graph diff | Snapshot + diff CLI |
| P1 | Three-tier ops | `--tier` flag on all mutations |
| P2 | CLI contract | JSON in/out, exit codes, --report |
| P2 | Span-safe ops | `verified_span_replace` API |
| P3 | Checkpoints | Append-only timeline + --resume |
| P3 | Plugin layer | WASM toollet skeleton |
| P4 | Deterministic resumes | Full replay system |

---

## Core Philosophy

> "Make every tool 3-tier: text|ast|graph. Text=fast/unsafe-ish, ast=precise, graph=cross-file."

> "Ship boring improvements, not vibes."

> "Turn experimental into measurably improving."

---

*Created: 2025-12-30*

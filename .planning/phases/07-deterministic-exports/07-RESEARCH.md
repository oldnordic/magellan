# Phase 7: Deterministic Exports - Research

**Researched:** 2026-01-19
**Domain:** Graph export (JSON/JSONL, DOT, CSV)
**Confidence:** HIGH

## Summary

This phase implements deterministic export of the code graph to three formats: JSON/JSONL for structured data interchange, DOT (Graphviz) for call graph visualization, and CSV for spreadsheet/pipeline consumption. The codebase already has a foundational JSON export in `src/graph/export.rs` that establishes patterns for deterministic ordering and stable IDs.

The primary challenge is extending the existing export functionality to support multiple formats with filtering capabilities while maintaining determinism (same input produces identical output). The existing `GraphExport` structure and sorting patterns provide a solid foundation to build upon.

**Primary recommendation:** Extend the existing `export.rs` module with format-specific submodules (`json`, `dot`, `csv`) and a unified `export` function that dispatches based on format. Add CLI commands following the existing pattern in `main.rs`.

## Standard Stack

### Core (Already in Use)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde` | 1.0 | Serialization framework | Already used, de facto standard |
| `serde_json` | 1.0 | JSON/JSONL output | Already in codebase |
| `anyhow` | 1.0 | Error handling | Already used throughout |

### New Dependencies Required
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `csv` | 1.3 | CSV writing | [BurntSushi's csv crate](https://docs.rs/csv/latest/csv/tutorial/index.html) - de facto standard, excellent Serde integration, proper quoting/escaping |

### No External Dependencies For DOT
| Format | Library | Why |
|--------|---------|-----|
| DOT | Hand-rolled | Simple text format, [specification is stable](https://graphviz.org/doc/info/lang.html), no crate needed for basic output |

**Installation:**
```bash
# Add to Cargo.toml [dependencies]
csv = "1.3"
```

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| csv crate | Hand-rolled CSV | Quoting/escaping is complex (RFC 4180), csv crate handles edge cases (newlines in fields, unicode) |
| Hand-rolled DOT | dot crate (petgraph) | petgraph's DOT is for Rust data structures, not custom formatting; hand-rolled gives full control |

## Architecture Patterns

### Recommended Project Structure
```
src/graph/
├── export.rs           # Existing - extend with new exports
│   ├── mod exports::json
│   ├── mod exports::jsonl
│   ├── mod exports::dot
│   └── mod exports::csv
├── mod.rs              # Re-export export functions
└── ...

src/
├── export_cmd.rs       # NEW - unified export CLI
└── main.rs             # Extend with export command variants
```

### Pattern 1: Unified Export Function with Format Dispatch
**What:** Central export function that takes format enum and dispatches to format-specific writers.

**When to use:** Core export entrypoint for CLI and library users.

**Example:**
```rust
// Source: src/graph/export.rs (extended)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    JsonL,
    Dot,
    Csv,
}

pub struct ExportConfig {
    pub format: ExportFormat,
    pub include_symbols: bool,
    pub include_references: bool,
    pub include_calls: bool,
    pub minify: bool,
    pub filters: Option<ExportFilters>,
}

pub fn export(graph: &mut CodeGraph, config: ExportConfig) -> Result<String> {
    match config.format {
        ExportFormat::Json => json::export_json(graph, config),
        ExportFormat::JsonL => jsonl::export_jsonl(graph, config),
        ExportFormat::Dot => dot::export_dot(graph, config),
        ExportFormat::Csv => csv::export_csv(graph, config),
    }
}
```

### Pattern 2: Deterministic Sorting for Stable Output
**What:** All exports MUST sort records before output to ensure deterministic output.

**When to use:** Every export format, before writing.

**Example:**
```rust
// Source: src/graph/export.rs (existing pattern, lines 156-161)
// Sort for deterministic output
files.sort_by(|a, b| a.path.cmp(&b.path));
symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
references.sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)));
calls.sort_by(|a, b| (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee)));
```

### Pattern 3: CLI Command Extension
**What:** Follow existing `main.rs` pattern for command parsing and execution tracking.

**When to use:** Adding new export commands.

**Example:**
```rust
// Source: src/main.rs (extended)
enum Command {
    // ... existing commands ...
    Export {
        db_path: PathBuf,
        format: ExportFormat,
        output: Option<PathBuf>,
        filters: ExportFilters,
    },
}

// Parse args with format flag
"export" => {
    let mut format = ExportFormat::Json; // default
    let mut output: Option<PathBuf> = None;
    let mut filters = ExportFilters::default();

    // Parse --format, -o, --file, --symbol, etc.
    // ...
}

// Dispatch to export_cmd module
Ok(Command::Export { db_path, format, output, filters }) => {
    if let Err(e) = export_cmd::run_export(db_path, format, output, filters) {
        eprintln!("Error: {}", e);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
```

### Pattern 4: Stable Symbol IDs in Exports
**What:** Include `symbol_id` in exports for cross-run correlation.

**When to use:** JSON/JSONL exports of symbols, references, calls.

**Example:**
```rust
// Source: src/graph/schema.rs (existing, lines 23-28)
/// Symbol node payload stored in sqlitegraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Stable symbol ID derived from (language, fqn, span_id)
    #[serde(default)]
    pub symbol_id: Option<String>,
    // ...
}

// Include in export structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolExport {
    pub symbol_id: Option<String>,  // NEW: include for correlation
    pub name: Option<String>,
    pub kind: String,
    // ...
}
```

### Anti-Patterns to Avoid
- **Non-deterministic iteration:** Using `HashMap` without sorting breaks determinism. Always collect to `Vec` and sort before output.
- **Inconsistent ID formats:** Mixing node IDs (i64) with symbol IDs (String) confuses consumers. Use stable symbol_id everywhere for user-facing exports.
- **Hand-rolled CSV escaping:** CSV quoting (RFC 4180) has many edge cases. Always use the `csv` crate.
- **Missing UTF-8 handling:** DOT labels with special characters need escaping. Always use `quote` method for labels.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CSV writing | Manual string concatenation | `csv::Writer` | Handles quoting, escaping, newlines in fields, UTF-8 |
| JSON serialization | Manual formatting | `serde_json::to_string` | Handles escaping, pretty-printing, minify option |
| Deterministic sorting | `BTreeMap` | Sort `Vec` after collection | More flexible, allows multi-key sorting |
| Stable IDs | Random/temporary IDs | `generate_symbol_id` | Already exists, cross-run stable |

**Key insight:** CSV is deceptively complex. The [RFC 4180 spec](https://www.ietf.org/rfc/rfc4180) has edge cases that cause bugs: fields containing commas, quotes, newlines. The `csv` crate handles all of these correctly with proper Serde integration.

## Common Pitfalls

### Pitfall 1: Non-Deterministic HashMap Iteration
**What goes wrong:** `HashMap` iteration order is randomized (Rust security feature). Same data produces different output on each run.

**Why it happens:** Rust uses random hash seed for DoS protection.

**How to avoid:** Always collect to `Vec`, sort, then iterate:
```rust
// WRONG - non-deterministic
for (k, v) in my_map { }

// RIGHT - deterministic
let mut items: Vec<_> = my_map.into_iter().collect();
items.sort_by(|a, b| a.0.cmp(&b.0));
for (k, v) in items { }
```

**Warning signs:** "Sometimes the order changes," "tests fail intermittently"

### Pitfall 2: Mixing Node IDs with Symbol IDs
**What goes wrong:** Using sqlitegraph's internal `entity_id` (i64) in exports creates unstable references—IDs change on re-index.

**Why it happens:** `entity_id` is auto-increment, not content-based.

**How to avoid:** Always use `symbol_id` (SHA-256 based) in exports:
```rust
// WRONG - unstable
pub struct SymbolExport {
    pub node_id: i64,  // changes on re-index
    // ...
}

// RIGHT - stable
pub struct SymbolExport {
    pub symbol_id: Option<String>,  // content-based hash
    // ...
}
```

### Pitfall 3: Forgetting to Flush Writers
**What goes wrong:** Output truncated or incomplete.

**Why it happens:** `csv::Writer` and file writers buffer internally.

**How to avoid:** Always flush before returning:
```rust
let mut wtr = csv::Writer::from_path(path)?;
// ... write records ...
wtr.flush()?;  // CRITICAL
```

### Pitfall 4: DOT Label Escaping
**What goes wrong:** Graphviz parsing errors, broken rendering.

**Why it happens:** Special characters in labels (quotes, braces) need escaping.

**How to avoid:** Quote all labels and escape internal quotes:
```rust
fn escape_dot_label(s: &str) -> String {
    format!("\"{}\"", s.replace('"', r#"\""#))
}

// In DOT output:
println!("{} [label={}];", node_id, escape_dot_label(symbol_name));
```

### Pitfall 5: JSON vs JSONL Confusion
**What goes wrong:** Outputting invalid JSONL (not one JSON object per line).

**Why it happens:** Using `to_string_pretty` or `to_vec` instead of line-by-line serialization.

**How to avoid:** JSONL must be exactly one JSON object per line, no pretty-printing:
```rust
// JSON - single pretty-printed object
serde_json::to_string_pretty(&export)?  // multi-line, indented

// JSONL - one compact JSON per line
for record in records {
    let line = serde_json::to_string(&record)?;  // compact
    writeln!(out, "{}", line)?;
}
```

## Code Examples

### JSON Export (Extended from Existing)
```rust
// Source: src/graph/export.rs (extend existing pattern)
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExport {
    pub files: Vec<FileExport>,
    pub symbols: Vec<SymbolExport>,      // Add symbol_id
    pub references: Vec<ReferenceExport>, // Add target_symbol_id, target_name
    pub calls: Vec<CallExport>,          // Add caller_symbol_id, callee_symbol_id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolExport {
    pub symbol_id: Option<String>,      // NEW - stable identifier
    pub name: Option<String>,
    pub kind: String,
    pub kind_normalized: Option<String>,
    pub file: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

pub fn export_json(graph: &mut CodeGraph, minify: bool) -> Result<String> {
    // ... collect data (existing pattern) ...

    // Sort for deterministic output (existing pattern)
    files.sort_by(|a, b| a.path.cmp(&b.path));
    symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));

    let export = GraphExport { files, symbols, references, calls };

    if minify {
        serde_json::to_string(&export)
    } else {
        serde_json::to_string_pretty(&export)
    }
}
```

### JSONL Export (New)
```rust
// Source: src/graph/export/jsonl.rs
use super::{FileExport, SymbolExport, ReferenceExport, CallExport};
use anyhow::Result;
use std::io::Write;

pub enum JsonlRecord {
    File(FileExport),
    Symbol(SymbolExport),
    Reference(ReferenceExport),
    Call(CallExport),
}

pub fn export_jsonl<W: Write>(
    graph: &mut CodeGraph,
    writer: &mut W,
    config: &ExportConfig,
) -> Result<()> {
    let mut records = Vec::new();

    // Collect and sort (deterministic order)
    if config.include_symbols {
        let mut symbols = collect_symbols(graph)?;
        symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
        records.extend(symbols.into_iter().map(JsonlRecord::Symbol));
    }

    // Write one JSON per line
    for record in records {
        let line = serde_json::to_string(&record)?;
        writeln!(writer, "{}", line)?;
    }

    Ok(())
}
```

### DOT Export (New)
```rust
// Source: src/graph/export/dot.rs
use anyhow::Result;
use crate::graph::{CodeGraph, CallNode, SymbolNode};

pub fn export_dot(graph: &mut CodeGraph, config: &ExportConfig) -> Result<String> {
    let mut output = String::new();

    // Header - use strict digraph for determinism
    output.push_str("strict digraph call_graph {\n");

    // Default node attributes
    output.push_str("  node [shape=box, style=rounded];\n");

    // Collect all call nodes
    let mut calls = Vec::new();
    for entity_id in graph.calls.backend.entity_ids()? {
        if let Ok(node) = graph.calls.backend.get_node(entity_id) {
            if node.kind == "Call" {
                if let Ok(call) = serde_json::from_value::<CallNode>(node.data) {
                    calls.push(call);
                }
            }
        }
    }

    // Sort deterministically
    calls.sort_by(|a, b| {
        a.file.cmp(&b.file)
            .then_with(|| a.caller.cmp(&b.caller))
            .then_with(|| a.callee.cmp(&b.callee))
    });

    // Emit edges
    for call in calls {
        let caller_label = escape_dot_label(&format!("{}\\n{}", call.caller, call.file));
        let callee_label = escape_dot_label(&format!("{}\\n{}", call.callee, call.file));
        output.push_str(&format!("  \"{}\" -> \"{}\";\n", caller_label, callee_label));
    }

    output.push_str("}\n");
    Ok(output)
}

fn escape_dot_label(s: &str) -> String {
    // Replace quotes and backslashes, wrap in quotes
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', r#"\""#))
}
```

### CSV Export (New)
```rust
// Source: src/graph/export/csv.rs
use anyhow::Result;
use csv::Writer;
use std::io::Write;

pub fn export_csv<W: Write>(
    graph: &mut CodeGraph,
    writer: W,
    config: &ExportConfig,
) -> Result<()> {
    let mut wtr = Writer::from_writer(writer);

    if config.include_symbols {
        write_symbols(&mut wtr, graph)?;
    }

    if config.include_calls {
        write_calls(&mut wtr, graph)?;
    }

    wtr.flush()?;
    Ok(())
}

fn write_symbols<W: Write>(wtr: &mut Writer<W>, graph: &mut CodeGraph) -> Result<()> {
    // Header
    wtr.write_record(&[
        "symbol_id", "name", "kind", "file", "byte_start", "byte_end",
        "start_line", "start_col", "end_line", "end_col",
    ])?;

    // Collect and sort
    let mut symbols = collect_symbols(graph)?;
    symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));

    // Write records
    for sym in symbols {
        wtr.serialize(sym)?;
    }

    Ok(())
}
```

### CLI Command (New)
```rust
// Source: src/export_cmd.rs
use anyhow::Result;
use std::path::PathBuf;
use crate::{CodeGraph, graph::export::{ExportFormat, ExportConfig, export}};
use crate::output::generate_execution_id;

pub fn run_export(
    db_path: PathBuf,
    format: ExportFormat,
    output: Option<PathBuf>,
    config: ExportConfig,
) -> Result<()> {
    let mut graph = CodeGraph::open(&db_path)?;
    let exec_id = generate_execution_id();

    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &["export".to_string()],
        None,
        &db_path.to_string_lossy(),
    )?;

    let result = export(&mut graph, config)?;

    // Write to stdout or file
    match output {
        Some(path) => std::fs::write(path, result)?,
        None => println!("{}", result),
    }

    graph.execution_log().finish_execution(
        &exec_id, "success", None, 0, 0, 0
    )?;

    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single JSON format | Multiple formats (JSON/JSONL/DOT/CSV) | Phase 7 | Users can choose output for their toolchain |
| No stable IDs | Stable symbol_id in all exports | Phase 7 | Cross-run correlation, diff-friendly |
| Non-deterministic order | Deterministic sorting | Already implemented (export.rs:156-161) | Reproducible exports, version control friendly |

**Existing in codebase:**
- `src/graph/export.rs` - JSON export with deterministic sorting (HIGH confidence pattern)
- Stable symbol IDs via `generate_symbol_id` (src/graph/symbols.rs:69-92)

**To be added:**
- JSONL format (line-delimited JSON for streaming)
- DOT format (Graphviz call graphs)
- CSV format (spreadsheet/pipeline friendly)
- CLI filtering (file, symbol, kind filters)

## Open Questions

1. **CSV multi-file vs single output:** Should `--combined` write one file with type column or concatenate with section headers?
   - **What we know:** Both approaches are valid. Context allows `--combined` flag.
   - **What's unclear:** Which default is more ergonomic.
   - **Recommendation:** Default to separate files (symbols.csv, refs.csv, calls.csv) for clarity. `--combined` produces export.csv with type column.

2. **DOT clustering granularity:** Should `--cluster` group by file, module, or directory?
   - **What we know:** DOT supports subgraphs with `cluster_` prefix for visual grouping.
   - **What's unclear:** What level of granularity users prefer.
   - **Recommendation:** Start with file-level clustering (one subgraph per file). Module-level requires understanding language-specific module boundaries.

3. **Filter flag naming:** Should format-specific flags have prefixes (e.g., `--dot-cluster` vs `--cluster`)?
   - **What we know:** Context allows "mixed filter approach" with per-format extensions.
   - **What's unclear:** Flag naming consistency pattern.
   - **Recommendation:** Use unprefixed flags where unambiguous (`--file`, `--symbol`), prefixed for format-specific (`--dot-cluster`, `--csv-separator`).

## Sources

### Primary (HIGH confidence)
- **Existing codebase:**
  - `src/graph/export.rs` - Current JSON export implementation with deterministic sorting
  - `src/graph/schema.rs` - Node definitions with stable symbol_id
  - `src/graph/query.rs` - Query patterns for data extraction
  - `src/main.rs` - CLI command pattern, execution tracking
  - `src/refs_cmd.rs` - Command module pattern example
- **[DOT Language Specification](https://graphviz.org/doc/info/lang.html)** - Official DOT grammar, verified 2024-09-28
- **[csv crate tutorial](https://docs.rs/csv/latest/csv/tutorial/index.html)** - Authoritative CSV writing guide

### Secondary (MEDIUM confidence)
- **[Rust csv crate documentation](https://docs.rs/csv/latest/csv/)** - API reference for CSV writing
- **[WebSearch: CSV best practices 2025](https://www.topetl.com/blog/top-10-best-practices-for-csv-transformation)** - General CSV patterns (verified against csv crate docs)

### Tertiary (LOW confidence)
- None - all findings verified against code or official documentation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Existing codebase patterns + official documentation
- Architecture: HIGH - Directly extends existing `export.rs` patterns
- CLI patterns: HIGH - Existing `main.rs` provides clear template
- CSV handling: HIGH - csv crate is mature, well-documented
- DOT format: HIGH - Specification is stable and simple
- Determinism requirements: HIGH - Existing sorting patterns proven

**Research date:** 2026-01-19
**Valid until:** 30 days (stable dependencies, simple text formats)

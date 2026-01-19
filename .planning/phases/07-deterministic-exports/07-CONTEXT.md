# Phase 7: Deterministic Exports - Context

**Gathered:** 2026-01-19
**Status:** Ready for planning

## Phase Boundary

Export the indexed graph into stable, diff-friendly formats for downstream tooling:
- JSON/JSONL export with stable IDs and deterministic ordering
- DOT (Graphviz) export for caller→callee graphs
- CSV export for core entities (symbols, references, calls)

All exports must be deterministic (same input → identical output).

---

## Implementation Decisions

### JSON/JSONL Structure
- Support both JSON and JSONL formats via `--format` flag (json/jsonl)
- JSON for small graphs (single file), JSONL for large (one line per record)
- Configurable content via `--include` flag (symbols, refs, calls, chunks, etc.)
- References include both `target_symbol_id` and target name for two-way lookup
- Pretty-printed by default, `--minify` flag for compact output

### DOT Graph Format
- Full call graph by default, with optional filter flags (`--file`, `--symbol`, `--max-depth`)
- Node labels: symbol name + kind + file path (full details)
- Edge styling: stable IDs internally with readable labels as display attribute
- Optional clustering via `--cluster` flag to group symbols by file/module

### CSV Schema Design
- One file per type by default (symbols.csv, refs.csv, calls.csv)
- `--combined` flag for single export.csv with type column
- Column ordering: logical grouping (IDs first, then name, spans, metadata)
- Symbol references encode both symbol_id and coordinates (file_path, symbol_name)

### CLI Invocation
- Unified `export` command with `--format` flag (json/jsonl/dot/csv)
- Aliases for direct access: `export-json`, `export-dot`, `export-csv`
- Default to stdout, `-o/--output` flag for file output
- Mixed filter approach: core filters shared (`--file`, `--symbol`, `--kind`), formats can add specific ones

### Claude's Discretion
- Exact filter flag naming and per-format extensions
- JSON schema details (field names, nesting structure beyond core entities)
- DOT styling (colors, shapes, layout engine hints)
- CSV quoting and escaping specifics

---

## Specific Ideas

No specific requirements — open to standard approaches for export tooling.

---

## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 07-deterministic-exports*
*Context gathered: 2026-01-19*

---
phase: 07-deterministic-exports
verified: 2026-01-19T16:00:00Z
status: passed
score: 12/12 must-haves verified
---

# Phase 7: Deterministic Exports Verification Report

**Phase Goal:** Users can export the graph into stable, diff-friendly formats for downstream tooling.
**Verified:** 2026-01-19T16:00:00Z
**Status:** PASSED
**Verification Mode:** Initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can export graph to JSON with stable symbol_ids and deterministic ordering | VERIFIED | `export_json()` in export.rs lines 302-404; symbol_id field in SymbolExport (line 248) |
| 2 | User can export graph to JSONL (one JSON record per line) for streaming | VERIFIED | `export_jsonl()` in export.rs lines 449-557; JsonlRecord with #[serde(tag = "type")] (line 434) |
| 3 | User can export call graph to DOT format for Graphviz visualization | VERIFIED | `export_dot()` in export.rs lines 570-670; "strict digraph call_graph" header (line 579) |
| 4 | User can export graph to CSV format for spreadsheet/pipeline consumption | VERIFIED | `export_csv()` in export.rs lines 941-1056; SymbolCsvRow/ReferenceCsvRow/CallCsvRow structs (lines 836-921) |
| 5 | Exports include symbol_id, caller_symbol_id, callee_symbol_id for stable correlation | VERIFIED | SymbolExport.symbol_id (line 248); CallExport.caller_symbol_id/callee_symbol_id (lines 285-289); all CSV rows have stable ID fields |
| 6 | JSON output is deterministic (same input produces identical output) | VERIFIED | All export functions use deterministic sorting: files.sort_by() (line 390), symbols.sort_by() (line 391), etc. |
| 7 | User can control minified vs pretty-printed output via --minify flag | VERIFIED | --minify flag parsed in main.rs (line 301); export_graph() respects config.minify (lines 816-820) |
| 8 | DOT export uses strict digraph for deterministic output | VERIFIED | export_dot() output starts with "strict digraph call_graph {" (line 579) |
| 9 | User can filter DOT export by file, symbol, kind, or max-depth | VERIFIED | ExportFilters struct (lines 61-72); filter application in export_dot() (lines 596-601) |
| 10 | User can cluster DOT output by file/module via --cluster flag | VERIFIED | --cluster flag parsed in main.rs (line 346-348); cluster logic in export_dot() (lines 631-652) |
| 11 | CSV exports use proper quoting/escaping per RFC 4180 via csv crate | VERIFIED | csv::Writer used in export_csv() (line 1043); csv dependency in Cargo.toml (line 53) |
| 12 | Exported output has schema contract with stable IDs | VERIFIED | All export structures (SymbolExport, CallExport, ReferenceExport, CSV rows) include stable ID fields with #[serde(default)] |

**Score:** 12/12 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/graph/export.rs` | Export functions with stable IDs | VERIFIED | 1056 lines; contains ExportFormat, ExportConfig, export_json(), export_jsonl(), export_dot(), export_csv() |
| `src/export_cmd.rs` | Unified export command handler | VERIFIED | 140 lines; run_export() function with full parameter handling (lines 30-130) |
| `Cargo.toml` | csv dependency | VERIFIED | Line 53: `csv = "1.3"` |
| `tests/cli_export_tests.rs` | Comprehensive export tests | VERIFIED | 1106 lines; 18 tests covering JSON, JSONL, CSV, DOT formats |
| `src/graph/mod.rs` | Public export module | VERIFIED | Lines 7, 37: `pub mod export;` and `pub use export::{ExportConfig, ExportFormat};` |
| `src/lib.rs` | Re-export export types | VERIFIED | Line 20: `pub use graph::{CodeGraph, ExportConfig, ExportFormat, ...};` |
| `src/main.rs` | Command::Export CLI integration | VERIFIED | Lines 123-130 (Command enum), 262-375 (arg parsing), 1121-1144 (handler) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-------|-----|--------|---------|
| `main.rs` | `export_cmd.rs` | Command::Export dispatch to run_export() | VERIFIED | Lines 1121-1140: extract fields and call export_cmd::run_export() |
| `export_cmd.rs` | `graph/export.rs` | export_graph() call with ExportConfig | VERIFIED | Line 103: `export_graph(&mut graph, &config)?` |
| `export_graph()` | format-specific functions | ExportFormat match | VERIFIED | Lines 712-825: match on config.format dispatches to JSON/JSONL/DOT/CSV |
| `SymbolExport` | SymbolNode.symbol_id | Field assignment during export | VERIFIED | Line 330: `symbol_id: symbol_node.symbol_id` |
| `CallExport` | CallNode symbol IDs | Field assignment during export | VERIFIED | Lines 372-373: `caller_symbol_id, callee_symbol_id` from CallNode |
| CSV writer | csv crate | csv::Writer::from_writer() | VERIFIED | Line 1043: csv::Writer instantiated with RFC 4180 compliant serialization |
| DOT export | Call nodes | entity_ids() and get_node() for Call collection | VERIFIED | Lines 583-593: collect Call nodes from graph backend |

### Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| EXP-01: Export graph snapshot to JSON/JSONL with stable IDs and deterministic ordering | SATISFIED | export_json() and export_jsonl() with symbol_id fields; deterministic sorting |
| EXP-02: Export DOT for caller-callee graphs | SATISFIED | export_dot() with strict digraph format; filtering and clustering support |
| EXP-03: Export CSV for core entities with stable IDs | SATISFIED | export_csv() with SymbolCsvRow/ReferenceCsvRow/CallCsvRow; stable IDs included |

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| `src/graph/export.rs` (lines 13, 20, 22) | Comments mentioning "placeholder" for Dot/Csv | INFO | These are outdated comments - the formats are now fully implemented. The code shows full implementations. |
| `src/graph/export.rs` (line 356) | `target_symbol_id: None` for references | INFO | Per plan (Task 3 comment), symbol lookup for references is deferred; acceptable for v1 |

No blocker or warning anti-patterns found. The code is substantive with no TODO/FIXME/HACK comments indicating incomplete work.

### Human Verification Required

1. **Export command help completeness**
   - Test: Run `magellan` and check if export help mentions --cluster, --file, --symbol, --kind, --max-depth flags
   - Expected: Help should document all filter flags for DOT export
   - Why human: Help text review requires human judgment on completeness

2. **Graphviz DOT rendering**
   - Test: Take a real codebase with call relationships, export to DOT, pipe to `dot -Tpng` or view in Graphviz
   - Expected: DOT renders without parse errors; visual representation shows call graph
   - Why human: Requires external Graphviz tool installation and visual verification

3. **CSV spreadsheet compatibility**
   - Test: Open exported CSV in Excel, Google Sheets, or similar
   - Expected: CSV opens correctly with proper column separation; special characters display properly
   - Why human: Requires external spreadsheet application and visual verification

### Test Coverage Summary

**18 tests in cli_export_tests.rs:**
- 7 JSON export tests (basic, symbol_ids, JSONL format, deterministic, minify, file output, filters)
- 5 CSV export tests (basic, quoting, deterministic, symbol_ids, file output)
- 6 DOT export tests (basic, deterministic, label escaping, clustering, file filter, file output)

**All tests pass:** `running 18 tests ... test result: ok. 18 passed; 0 failed`

### Determinism Verification

All export functions use deterministic sorting:
- Files: `files.sort_by(|a, b| a.path.cmp(&b.path))` (line 390)
- Symbols: `symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)))` (line 391)
- References: `references.sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)))` (lines 392-393)
- Calls: `calls.sort_by(|a, b| (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee)))` (line 394)

### Stable ID Fields

All export structures include stable IDs:
- SymbolExport: `pub symbol_id: Option<String>` (line 248)
- ReferenceExport: `pub target_symbol_id: Option<String>` (line 269) - intentionally None for v1
- CallExport: `pub caller_symbol_id: Option<String>`, `pub callee_symbol_id: Option<String>` (lines 286, 289)
- CSV rows: All include record_type and stable ID fields (lines 836-921)

### Gap Summary

**No gaps found.** All phase goals have been achieved:
- JSON/JSONL export with stable IDs and deterministic ordering
- DOT export for Graphviz with filtering and clustering
- CSV export with RFC 4180 compliance
- Unified CLI export command with comprehensive flag support
- 18 passing tests covering all export formats

**Minor notes:**
1. Help text could be enhanced to document DOT filter flags (--cluster, --file, --symbol, --kind, --max-depth)
2. Reference target_symbol_id is intentionally None (acceptable per plan)

---

_Verified: 2026-01-19T16:00:00Z_
_Verifier: Claude (gsd-verifier)_

---
phase: 07-deterministic-exports
plan: 01
subsystem: [export, api]
tags: [json, jsonl, serde, deterministic, stable-ids, export-format]

# Dependency graph
requires:
  - phase: 05-stable-identity
    provides: symbol_id generation in SymbolNode and CallNode
  - phase: 06-query-ux
    provides: command module pattern (refs_cmd, files_cmd, etc.)
provides:
  - JSON and JSONL export formats with stable symbol IDs
  - ExportFormat and ExportConfig types for format selection
  - Unified export CLI command with content filtering
  - Deterministic export output for reproducibility
affects: [07-02-dot-format, 07-03-csv-format]

# Tech tracking
tech-stack:
  added: [csv = "1.3" (placeholder for Plan 03)]
  patterns: [export_config pattern, jsonl tagged records, command module pattern]

key-files:
  created: [src/export_cmd.rs, tests/cli_export_tests.rs]
  modified: [src/graph/export.rs, src/graph/mod.rs, src/lib.rs, src/main.rs, Cargo.toml]

key-decisions:
  - "ExportFormat enum with Json/JsonL/Dot/Csv variants - Dot and CSV are placeholders"
  - "ExportConfig struct with include_symbols/include_references/include_calls filters"
  - "JsonlRecord with #[serde(tag = \"type\")] for type discrimination in JSONL"
  - "Preserve existing export_json() for backward compatibility, add export_graph() for new features"

patterns-established:
  - "Pattern: Export config object for format/content filtering - unified entry point"
  - "Pattern: Tagged enum serialization for JSONL record type discrimination"
  - "Pattern: Command module following refs_cmd.rs/files_cmd.rs structure"

# Metrics
duration: 10min
completed: 2026-01-19
---

# Phase 7: Deterministic Exports (JSON/JSONL) Summary

**JSON and JSONL export with stable symbol IDs, deterministic ordering, and unified CLI command**

## Performance

- **Duration:** 10 min
- **Started:** 2026-01-19T14:01:46Z
- **Completed:** 2026-01-19T14:12:35Z
- **Tasks:** 6
- **Files modified:** 7
- **Tests added:** 7

## Accomplishments

- JSON export with stable symbol_id field for symbols
- JSONL (line-delimited JSON) export with type discriminator per record
- Deterministic sorting for reproducible output across runs
- Minify flag for compact JSON output
- Unified export command with --format, --output, --minify, --no-* flags
- Comprehensive test coverage for all export features

## Task Commits

Each task was committed atomically:

1. **Task 1: Add CSV dependency and ExportFormat/ExportConfig types** - `90cbc6b` (feat)
2. **Task 2: Add stable ID fields to export structures** - `ce128f9` (feat)
3. **Task 3: Implement JSONL export function** - `a923936` (feat)
4. **Task 4: Add export_graph function with config support** - `9bc22ca` (feat)
5. **Task 5: Create export_cmd.rs module with unified export command** - `252d2dc` (feat)
6. **Task 6: Add comprehensive export tests** - `022e202` (test)

**Plan metadata:** N/A (direct execution)

## Files Created/Modified

- `src/export_cmd.rs` - New module with run_export() function following refs_cmd.rs pattern
- `src/graph/export.rs` - Extended with ExportFormat, ExportConfig, export_jsonl(), export_graph()
- `src/graph/mod.rs` - Made export module public, re-exported ExportFormat/ExportConfig
- `src/lib.rs` - Added ExportFormat, ExportConfig to public exports
- `src/main.rs` - Updated Command::Export, argument parser, handler, usage text
- `tests/cli_export_tests.rs` - New test file with 7 comprehensive tests
- `Cargo.toml` - Added csv = "1.3" dependency (placeholder for Plan 03)

## Decisions Made

- Kept existing `export_json()` unchanged for backward compatibility
- Added `export_graph(config)` as new unified entry point
- Used `#[serde(tag = "type")]` for JSONL record type discrimination
- All stable ID fields use `#[serde(default)]` for backward compatibility
- JSONL uses compact JSON (not pretty-printed) for streaming efficiency
- Content filters (--no-symbols, --no-references, --no-calls) for partial exports

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all tasks completed as specified without issues.

## User Setup Required

None - no external service configuration required. Export is a local CLI command.

## Next Phase Readiness

- JSON/JSONL export fully functional with stable IDs
- ExportFormat enum has Dot and Csv placeholders for Plans 02-03
- ExportConfig filters in place for future filtering capabilities
- Ready for Phase 07-02 (DOT format) and Phase 07-03 (CSV format)

---
*Phase: 07-deterministic-exports*
*Completed: 2026-01-19*

---
phase: 13-scip-tests-docs
plan: 01
subsystem: export
tags: [scip, protobuf, code-intelligence, sourcegraph]

# Dependency graph
requires:
  - phase: 11-fqn-extraction
    provides: FQN-based symbol identification needed for SCIP symbol encoding
  - phase: 01-09
    provides: Export infrastructure and SCIP stub
provides:
  - SCIP export implementation producing valid protobuf parseable by scip crate
  - SCIP index with metadata (tool info, project root, protocol version)
  - SCIP documents with language, position_encoding, occurrences, and symbols
  - Language-specific symbol encoding (rust, python, java, javascript, typescript, cpp, c)
affects: [round-trip-tests, documentation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Protobuf message construction via direct field assignment (MessageField::some)
    - EnumOrUnknown wrapper for SCIP enum types (PositionEncoding, ProtocolVersion, SymbolRole, Kind)
    - Language-specific FQN separators (:: for rust/cpp, . for python/java/js/ts)

key-files:
  modified:
    - src/graph/export/scip.rs - SCIP export implementation with metadata, documents, occurrences, symbols

key-decisions:
  - Use direct field assignment for protobuf types instead of setter methods (scip 0.6.1 API)
  - Use EnumOrUnknown::new() for SCIP enum fields
  - Use "magellan" as scheme and language as package in SCIP symbol format
  - Set position_encoding to UTF8CodeUnitOffsetFromLineStart for all documents
  - Use ReadAccess (8) for reference occurrences instead of undefined Reference role

patterns-established:
  - SCIP symbol format: "magellan language/namespace1/namespace2/symbol."
  - Symbol occurrences use 4-element range [line_start, col_start, line_end, col_end]
  - SymbolInformation stored in document.symbols, Occurrence in document.occurrences
  - Global symbol map built for cross-file reference resolution (future enhancement)

# Metrics
duration: 13min
completed: 2026-01-20
---

# Phase 13: SCIP Tests + Documentation - Plan 01 Summary

**SCIP export implementation using scip crate v0.6.1 producing valid protobuf with metadata, documents, occurrences, and symbol information**

## Performance

- **Duration:** 13 minutes (824 seconds)
- **Started:** 2026-01-19T23:58:21Z
- **Completed:** 2026-01-20T01:12:05Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments

- Implemented SCIP Index construction with metadata (ToolInfo, project_root, ProtocolVersion)
- Added language-specific symbol encoding supporting Rust, Python, Java, JavaScript, TypeScript, C, and C++
- Mapped Magellan symbol kinds to SCIP Kind enum (Function, Method, Class, Enum, Namespace, etc.)
- Created SymbolInformation with kind and display_name for each symbol
- Set proper position encoding (UTF8CodeUnitOffsetFromLineStart) for all documents
- Added 10 unit tests for symbol encoding helper functions

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement SCIP Index construction with metadata** - `38b8e32` (feat)
2. **Task 2: Map Magellan symbols to SCIP occurrences with language mapping** - `234cada` (feat)
3. **Task 3: Add SCIP symbol encoding helper tests** - `1a0b034` (test)

**Plan metadata:** TBD (docs commit)

## Files Created/Modified

- `src/graph/export/scip.rs` - SCIP export implementation (293 lines added)
  - `export_scip()` - Main export function building Index with metadata and documents
  - `magellan_symbol_to_scip()` - FQN to SCIP symbol encoding with language separators
  - `map_symbol_kind()` - Maps Magellan kinds to SCIP Kind enum
  - Unit tests for symbol encoding (10 tests)

## Decisions Made

- Use direct field assignment for protobuf types (scip 0.6.1 uses pub fields instead of setters)
- Use `EnumOrUnknown::new()` wrapper for all SCIP enum types
- SCIP symbol format: `magellan language/descriptors/symbol.` with language-specific separators
- Set position_encoding to UTF8CodeUnitOffsetFromLineStart (matches Magellan's UTF-8 byte offsets)
- Use ReadAccess (8) for reference occurrences (no Reference role exists in SCIP)
- Store SymbolInformation in document.symbols separately from Occurrence in document.occurrences

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Initial compilation errors due to scip crate API using direct field access instead of setter methods
- Fixed by reading scip crate source code to understand actual protobuf structure
- SymbolInformation is stored in Document.symbols, not in Occurrence (corrected after checking API)

## Verification

- All 254 library tests pass (244 existing + 10 new SCIP tests)
- SCIP export produces non-empty Vec<u8> (184 bytes for test with 2 symbols)
- Round-trip verification: exported SCIP bytes parse successfully with `Index::parse_from_bytes()`
- Parsed Index contains:
  - metadata: true
  - documents: 1 (for /test/main.rs)
  - occurrences: 2 (definitions for main and helper)
  - symbols: 2 (SymbolInformation for each function)
- Language detection works correctly (rust detected for .rs files)

## Next Phase Readiness

- SCIP export implementation complete
- Ready for Plan 13-02: SCIP round-trip tests to validate export format
- Symbol encoding helper function fully tested across all supported languages
- Position encoding set correctly for UTF-8 compatibility

---
*Phase: 13-scip-tests-docs*
*Completed: 2026-01-20*

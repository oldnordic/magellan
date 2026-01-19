---
phase: 05-stable-identity
plan: 01
subsystem: symbol-identity
tags: [symbol_id, sha256, fqn, stable-identifiers, deterministic-hashing]

# Dependency graph
requires:
  - phase: 04-canonical-span-model
    provides: Span::generate_id function for position-based stable IDs
provides:
  - Stable symbol_id field on SymbolNode for cross-run symbol correlation
  - FQN (fully-qualified name) tracking in SymbolFact for symbol identification
  - generate_symbol_id() function for deterministic symbol ID generation
  - Comprehensive test suite verifying symbol_id determinism properties
affects:
  - 05-02 execution tracking (symbol_id enables execution correlation)
  - future query/indexing phases (symbol_id as stable reference)
  - export functionality (symbol_id in JSON output)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - SHA-256 hash-based stable ID generation
    - Colon-separated hash inputs for determinism (language:fqn:span_id)
    - 16-character hex format (64-bit) for stable IDs

key-files:
  modified:
    - src/ingest/mod.rs - Added fqn field to SymbolFact
    - src/graph/schema.rs - Added symbol_id field to SymbolNode
    - src/graph/symbols.rs - Added generate_symbol_id() and generate_span_id() functions
    - src/graph/mod.rs - Exported generate_symbol_id function
    - src/graph/query.rs - Updated fallback SymbolNode constructions
    - src/graph/call_ops.rs - Updated SymbolFact construction
    - src/graph/files.rs - Updated SymbolFact construction
    - src/graph/references.rs - Updated SymbolFact construction
    - src/ingest/c.rs - Set fqn during extraction
    - src/ingest/cpp.rs - Set fqn during extraction
    - src/ingest/java.rs - Set fqn during extraction
    - src/ingest/javascript.rs - Set fqn during extraction
    - src/ingest/python.rs - Set fqn during extraction
    - src/ingest/typescript.rs - Set fqn during extraction

key-decisions:
  - "Use SHA-256 for symbol_id generation (platform-independent, deterministic)"
  - "Hash format: language:fqn:span_id (colon-separated for clear separation)"
  - "16 hex characters from first 8 bytes (64-bit space, good distribution)"
  - "FQN defaults to simple name in v1 (future: hierarchical FQN for nested symbols)"
  - "generate_span_id() mirrors Span::generate_id() to avoid circular dependencies"

patterns-established:
  - "Stable ID pattern: SHA-256 of colon-separated inputs, 16 hex chars output"
  - "v1 compatibility: Optional fields with #[serde(default)] for backward compatibility"
  - "Determinism testing: Test same inputs produce same ID, different inputs produce different IDs"

# Metrics
duration: 10min
completed: 2026-01-19
---

# Phase 5 Plan 1: Stable Symbol ID Generation Summary

**SHA-256 based stable symbol_id generation using language:fqn:span_id hash with 16-character hex output for cross-run symbol correlation**

## Performance

- **Duration:** 10 minutes
- **Started:** 2026-01-19T12:07:58Z
- **Completed:** 2026-01-19T12:18:14Z
- **Tasks:** 5
- **Files modified:** 14

## Accomplishments

- Added `fqn: Option<String>` field to `SymbolFact` for fully-qualified name tracking
- Added `symbol_id: Option<String>` field to `SymbolNode` for stable symbol identification
- Implemented `generate_symbol_id()` function using SHA-256 hash of `language:fqn:span_id`
- Implemented `generate_span_id()` helper mirroring `Span::generate_id()` pattern
- Updated `insert_symbol_node()` to generate and store stable symbol_id during indexing
- Added comprehensive test suite verifying determinism properties (9 tests)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add fqn field to SymbolFact** - `975f7d2` (feat)
   - Modified all language parsers and graph modules to set fqn

2. **Task 2: Add symbol_id field to SymbolNode schema** - `f9ad085` (feat)
   - Updated query.rs fallback constructions

3. **Task 3: Add symbol_id generation function to symbols module** - `0ef88e5` (feat)
   - Added module documentation explaining symbol_id stability

4. **Task 4: Update insert_symbol_node to generate and store symbol_id** - `96cafec` (fix)
   - Fixed Language enum conversion using as_str() instead of to_string()

5. **Task 5: Add symbol_id determinism tests** - `7c27663` (test)
   - Added 9 tests covering determinism, format, and variance

## Files Created/Modified

- `src/ingest/mod.rs` - Added fqn: Option<String> field to SymbolFact, set during extraction
- `src/graph/schema.rs` - Added symbol_id: Option<String> field to SymbolNode with documentation
- `src/graph/symbols.rs` - Added generate_symbol_id(), generate_span_id(), and comprehensive tests
- `src/graph/mod.rs` - Exported generate_symbol_id for public API
- `src/graph/query.rs` - Updated fallback SymbolNode constructions to include symbol_id
- `src/graph/call_ops.rs` - Updated SymbolFact reconstruction to set fqn
- `src/graph/files.rs` - Updated SymbolFact reconstruction to set fqn
- `src/graph/references.rs` - Updated SymbolFact reconstruction to set fqn
- `src/ingest/c.rs` - Set fqn during symbol extraction
- `src/ingest/cpp.rs` - Set fqn during symbol extraction
- `src/ingest/java.rs` - Set fqn during symbol extraction
- `src/ingest/javascript.rs` - Set fqn during symbol extraction
- `src/ingest/python.rs` - Set fqn during symbol extraction
- `src/ingest/typescript.rs` - Set fqn during symbol extraction

## Decisions Made

1. **SHA-256 for symbol_id**: Platform-independent hash function ensures consistent results across architectures
2. **Hash format `language:fqn:span_id`**: Colon-separated inputs provide clear separation while maintaining determinism
3. **16 hex character output**: First 8 bytes (64 bits) provides good collision resistance while keeping IDs compact
4. **FQN defaults to name in v1**: Simple name-based FQN for top-level symbols; future versions will support hierarchical FQN (e.g., `module::Struct::method`)
5. **`generate_span_id()` mirrors `Span::generate_id()`**: Avoids circular dependency between graph and output modules

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed Language enum to string conversion**
- **Found during:** Task 4 (Update insert_symbol_node)
- **Issue:** Language enum doesn't implement Display, calling `.to_string()` fails compilation
- **Fix:** Changed from `l.to_string()` to `l.as_str().to_string()` using Language's as_str() method
- **Files modified:** src/graph/symbols.rs
- **Verification:** cargo check passes, all tests pass
- **Committed in:** 96cafec

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Bug fix necessary for compilation. No scope creep.

## Issues Encountered

None - all tasks completed as specified with one auto-fixed bug.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Symbol ID generation complete and tested
- Stable symbol_id available in SymbolNode for cross-run correlation
- Ready for Phase 5 Plan 2 (Execution Tracking with symbol_id correlation)
- No blockers or concerns

---
*Phase: 05-stable-identity*
*Completed: 2026-01-19*

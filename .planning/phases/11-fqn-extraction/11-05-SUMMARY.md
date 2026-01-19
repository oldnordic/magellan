---
phase: 11-fqn-extraction
plan: 05
subsystem: graph-query
tags: [fqn, symbol-lookup, reference-indexing, call-graph]

# Dependency graph
requires:
  - phase: 11-fqn-extraction
    plan: 02
    provides: Rust parser FQN extraction with ScopeStack
  - phase: 11-fqn-extraction
    plan: 03
    provides: Dot-separated language FQN extraction (Java, Python, JS, TS)
  - phase: 11-fqn-extraction
    plan: 04
    provides: C/C++ parser FQN extraction with namespace tracking
provides:
  - FQN-based symbol lookup maps replacing simple name maps
  - FQN collision detection and warning system
  - SymbolNode schema with fqn field for unique identification
affects:
  - Phase 11-06 (FQN migration utilities)
  - Phase 12 (transactional deletes - uses FQN for symbol identification)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - FQN as primary key for symbol lookup (HashMap<String, i64>)
    - Fallback pattern: fqn.or(name).unwrap_or_default()
    - Collision detection with WARN-level logging

key-files:
  created: []
  modified:
    - src/graph/schema.rs - Added fqn field to SymbolNode
    - src/graph/query.rs - index_references uses FQN map with collision detection
    - src/graph/references.rs - Accepts symbol_fqn_to_id parameter
    - src/graph/calls.rs - index_calls uses FQN-based lookup
    - src/graph/symbols.rs - Persists fqn field when creating SymbolNode

key-decisions:
  - "FQN as primary key: Symbol lookup maps use fqn instead of name to eliminate collisions"
  - "Backward compatibility: Fall back to name when fqn is None for legacy data"
  - "Collision warnings: Emit WARN-level messages when multiple symbols share the same FQN"

patterns-established:
  - "FQN map building: symbol_fqn_to_id HashMap with collision detection"
  - "Fallback pattern: fqn.or(name).unwrap_or_default() for legacy compatibility"
  - "Parameter naming: symbol_fqn_to_id instead of symbol_name_to_id for clarity"

# Metrics
duration: 3min
completed: 2026-01-19
---

# Phase 11: FQN-Based Symbol Lookup Summary

**FQN-based symbol lookup maps in query.rs, calls.rs, and references.rs eliminate first-match-wins collisions from simple name keys**

## Performance

- **Duration:** 3 min
- **Started:** 2026-01-19T20:39:58Z
- **Completed:** 2026-01-19T20:43:24Z
- **Tasks:** 3 (4th task completed as part of task 1)
- **Files modified:** 5

## Accomplishments

- SymbolNode schema now includes `fqn: Option<String>` field for unique identification
- All symbol lookup maps use FQN as key instead of simple name (query.rs, calls.rs, references.rs)
- FQN collision detection emits WARN-level messages to stderr
- Backward compatibility maintained via fallback to name when fqn is None

## Task Commits

Each task was committed atomically:

1. **Task 1: Update query.rs to use FQN-based symbol lookup** - `f7b484d` (feat)
2. **Task 2: Update references.rs to accept FQN-based map** - `33365b6` (feat)
3. **Task 3: Update calls.rs for FQN-based symbol matching** - `327a6a1` (feat)

**Plan metadata:** TBD (docs: complete plan)

_Note: Task 4 was completed as part of Task 1 when the fqn field was added to SymbolNode_

## Files Created/Modified

- `src/graph/schema.rs` - Added `fqn: Option<String>` field to SymbolNode struct
- `src/graph/symbols.rs` - Persists fqn field when creating SymbolNode from SymbolFact
- `src/graph/query.rs` - index_references builds symbol_fqn_to_id map with collision detection
- `src/graph/references.rs` - Accepts symbol_fqn_to_id parameter, uses FQN for reference matching
- `src/graph/calls.rs` - index_calls builds symbol_fqn_to_id map with same pattern

## Decisions Made

- **FQN as primary key**: Changed from simple name to FQN for symbol lookup to eliminate first-match-wins behavior when multiple symbols have the same name in different scopes
- **Backward compatibility**: Use `fqn.or(name).unwrap_or_default()` fallback pattern to handle legacy symbols that don't have FQN populated yet
- **Collision detection**: Emit WARN-level eprintln messages when multiple symbols share the same FQN to help users identify problematic code

## Deviations from Plan

None - plan executed exactly as written.

## Authentication Gates

None - no external services required for this plan.

## Next Phase Readiness

- All graph query modules now use FQN-based lookup
- SymbolNode schema complete and backward compatible
- Ready for Phase 11-06: FQN migration utilities
- Existing databases will need re-index to populate FQN fields (expected per Phase 11 context)

---
*Phase: 11-fqn-extraction*
*Completed: 2026-01-19*

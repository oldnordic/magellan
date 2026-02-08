---
phase: 52-eliminate-native-v2-stubs
plan: 01
subsystem: kv-storage
tags: [kv, metadata, json, serde, encoding]

# Dependency graph
requires:
  - phase: 51-fix-native-v2-compilation-errors
    provides: Working native-v2 backend with all type mismatches resolved
provides:
  - KV key construction functions for all metadata types (chunks, execution logs, metrics, AST nodes, CFG blocks)
  - JSON encoding/decoding functions for metadata serialization
  - Serde derives on ExecutionRecord for JSON compatibility
affects: [52-02, 52-03, 52-04, 52-05, 52-06] # All subsequent storage implementation plans

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Colon-separated key namespaces (chunk:, execlog:, metrics:, cfg:, ast:)
    - Path escaping with "::" to avoid colon collisions
    - Generic encoding functions to avoid private module dependencies
    - JSON encoding for metadata (human-readable, debuggable)

key-files:
  created: []
  modified:
    - src/kv/keys.rs - Added 6 key construction functions
    - src/kv/encoding.rs - Added 6 JSON encoding/decoding functions
    - src/graph/execution_log.rs - Added serde derives to ExecutionRecord

key-decisions:
  - "Generic type parameters for encoding functions avoid exposing private modules (ast_node, schema)"
  - "Path escaping with '::' prevents colon-based key collisions in file paths"
  - "JSON encoding chosen over binary for human readability and debuggability"

patterns-established:
  - "Key namespace pattern: {type}:{subtype}:{identifier} format for all KV keys"
  - "Encoding wrapper pattern: encode_* and decode_* functions delegate to encode_json/decode_json"
  - "?Sized bound pattern: Allow encoding of slices and other DSTs"

# Metrics
duration: 8min
completed: 2026-02-08
---

# Phase 52 Plan 01: KV Key Patterns and Encoding Functions Summary

**KV key construction and JSON encoding infrastructure for metadata storage with namespace separation and generic type parameters**

## Performance

- **Duration:** 8 minutes
- **Started:** 2026-02-08T00:18:37Z
- **Completed:** 2026-02-08T00:26:45Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- **6 new key construction functions** for all metadata types (chunks, execution logs, file/symbol metrics, CFG blocks, AST nodes)
- **6 new JSON encoding/decoding functions** with generic type parameters to avoid private module dependencies
- **Serde derives added to ExecutionRecord** for JSON serialization compatibility
- **19 total tests** verifying key format correctness and namespace separation
- **10 JSON encoding tests** demonstrating round-trip correctness for complex types

## Task Commits

Each task was committed atomically:

1. **Task 1: Add KV key patterns for metadata storage** - `70baa00` (feat)
2. **Task 2: Add JSON encoding functions for metadata types** - `66e57be` (feat)
3. **Task 3: Add serde derives to ExecutionRecord** - `6bc7d1d` (feat)

**Plan metadata:** TBD (docs: complete plan)

## Files Created/Modified

- `src/kv/keys.rs` - Added 6 key construction functions with path escaping and namespace separation
- `src/kv/encoding.rs` - Added 6 JSON encoding/decoding functions with generic type parameters
- `src/graph/execution_log.rs` - Added serde::Serialize and serde::Deserialize derives

## Decisions Made

1. **Generic type parameters for encoding functions** - Using `<T: serde::Serialize>` instead of concrete types like `CfgBlock` avoids exposing private modules (`ast_node`, `schema`) while maintaining type safety
2. **Path escaping with "::"** - File paths containing colons (e.g., Windows paths or module names) are escaped to prevent key namespace collisions
3. **?Sized bound for encode_json** - Allows encoding slices (`&[T]`) and other dynamically-sized types without requiring conversion to Vec
4. **JSON encoding over binary** - Human-readable format simplifies debugging and inspection of KV store contents

## Deviations from Plan

None - plan executed exactly as written. All three tasks completed as specified with no auto-fixes or deviations.

## Issues Encountered

1. **Private module access** - Initial attempt to use `crate::graph::schema::CfgBlock` in function signatures failed because modules are private. Fixed by using generic type parameters with wrapper functions.
2. **Dynamically-sized type encoding** - Initial `encode_json` signature didn't support slices. Fixed by adding `?Sized` bound to allow encoding DSTs.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All KV key patterns defined and tested
- JSON encoding/decoding infrastructure ready for metadata storage implementation
- ExecutionRecord now serializable for KV storage
- **Ready for Plan 52-02:** Store code chunks in KV backend using chunk_key and encode_json

## Test Results

All tests pass (31 total KV tests):
- 19 key format tests (namespace verification, path escaping, collision detection)
- 10 encoding tests (round-trip for structs, vecs, complex nested types)
- 2 module tests (public API signatures, compilation verification)

## Namespace Verification

Confirmed no key namespace collisions:
- `chunk:` - Code chunks by file path and byte range
- `execlog:` - Execution records by execution ID
- `metrics:file:` - File-level metrics
- `metrics:symbol:` - Symbol-level metrics
- `cfg:func:` - CFG blocks by function ID
- `ast:file:` - AST nodes by file ID
- `sym:fqn:`, `sym:id:`, `sym:rev:`, `sym:fqn_of:` - Symbol index keys
- `file:path:`, `file:sym:` - File index keys

All 12 namespace prefixes are distinct with no overlap.

## Self-Check: PASSED

All claims verified:
- ✓ 6 key functions exist in keys.rs (chunk_key, execution_log_key, file_metrics_key, symbol_metrics_key, cfg_blocks_key, ast_nodes_key)
- ✓ 6 encoding functions exist in encoding.rs (encode_json, decode_json, encode_cfg_blocks, decode_cfg_blocks, encode_ast_nodes, decode_ast_nodes)
- ✓ ExecutionRecord has serde derives (1 occurrence confirmed)
- ✓ 31 KV tests pass (19 keys + 10 encoding + 2 module)
- ✓ File sizes exceed minimums (keys.rs: 493 lines, encoding.rs: 369 lines)
- ✓ All 3 task commits exist (70baa00, 66e57be, 6bc7d1d)

---
*Phase: 52-eliminate-native-v2-stubs*
*Completed: 2026-02-08*

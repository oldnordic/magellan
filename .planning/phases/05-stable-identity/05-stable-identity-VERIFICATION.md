---
phase: 05-stable-identity
verified: 2026-01-19T14:00:00Z
status: passed
score: 5/5 must-haves verified
---

# Phase 5: Stable Identity + Execution Tracking Verification Report

**Phase Goal:** Users can correlate runs and results across time using stable IDs and per-run execution_id.
**Verified:** 2026-01-19T14:00:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth                                                              | Status     | Evidence |
| --- | ------------------------------------------------------------------ | ---------- | -------- |
| 1   | symbol_id field exists on SymbolNode struct                         | ✓ VERIFIED | `src/graph/schema.rs:28` has `pub symbol_id: Option<String>` |
| 2   | symbol_id is generated during symbol insertion using SHA-256        | ✓ VERIFIED | `src/graph/symbols.rs:156` generates symbol_id via `generate_symbol_id()` |
| 3   | symbol_id values are deterministic for same (language, fqn, span_id) | ✓ VERIFIED | `test_symbol_id_deterministic` passes (line 238-243) |
| 4   | Fully-qualified name (fqn) is tracked in SymbolFact                 | ✓ VERIFIED | `src/ingest/mod.rs:83` has `pub fqn: Option<String>` |
| 5   | execution_log table exists in SQLite database                       | ✓ VERIFIED | `src/graph/execution_log.rs:49-65` creates execution_log table |
| 6   | execution_id is generated for every run and recorded in database    | ✓ VERIFIED | `src/main.rs:714-716` generates execution_id, `start_execution()` at line 730 |
| 7   | execution_id appears in all JSON responses                           | ✓ VERIFIED | `JsonResponse::new(response, exec_id)` in query_cmd.rs:335, find_cmd.rs:299, refs_cmd.rs:202 |

**Score:** 7/7 observable truths verified

### Required Artifacts

| Artifact | Expected    | Status | Details |
| -------- | ----------- | ------ | ------- |
| `src/graph/schema.rs` | SymbolNode with symbol_id field | ✓ VERIFIED | Line 28: `pub symbol_id: Option<String>` with documentation |
| `src/ingest/mod.rs` | SymbolFact with fqn field | ✓ VERIFIED | Line 83: `pub fqn: Option<String>` |
| `src/graph/symbols.rs` | generate_symbol_id function | ✓ VERIFIED | Lines 69-92: SHA-256 based generation, 16 hex char output |
| `src/graph/execution_log.rs` | ExecutionLog module | ✓ VERIFIED | 434 lines, complete with start/finish methods, 6 tests passing |
| `src/graph/mod.rs` | ExecutionLog initialization | ✓ VERIFIED | Lines 109-110: ExecutionLog initialized in CodeGraph::open |
| `src/graph/db_compat.rs` | Schema version increment | ✓ VERIFIED | Line 34: `MAGELLAN_SCHEMA_VERSION: i64 = 2` |
| `src/output/command.rs` | SymbolMatch with symbol_id | ✓ VERIFIED | Line 443: `pub symbol_id: Option<String>` with skip_serializing_if |
| `src/query_cmd.rs` | execution_id in JSON response | ✓ VERIFIED | Line 335: `JsonResponse::new(response, exec_id)` |
| `src/main.rs` | ExecutionTracker for commands | ✓ VERIFIED | Lines 696-765: ExecutionTracker struct with start/finish methods |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| src/graph/symbols.rs | src/ingest/mod.rs | fact.fqn | ✓ WIRED | Line 152-153: Uses `fact.fqn.as_deref()` for symbol_id generation |
| src/graph/symbols.rs | src/graph/schema.rs | SymbolNode.symbol_id | ✓ WIRED | Line 159: `symbol_id: Some(symbol_id)` in SymbolNode construction |
| src/graph/mod.rs | src/graph/execution_log.rs | ExecutionLog::new() | ✓ WIRED | Lines 109-110: Initialization in CodeGraph::open |
| src/query_cmd.rs | src/output/command.rs | SymbolMatch::new with symbol_id | ✓ WIRED | Lines 319-324: Passes symbol_id from symbol_nodes_in_file_with_ids |
| src/main.rs | src/graph/mod.rs | CodeGraph::execution_log() | ✓ WIRED | Line 730: `graph.execution_log().start_execution()` |
| src/query_cmd.rs | src/output/command.rs | JsonResponse::new with exec_id | ✓ WIRED | Line 335: `JsonResponse::new(response, exec_id)` |

### Requirements Coverage

| Requirement | Status | Evidence |
| ----------- | ------ | -------- |
| OUT-05: Every response includes stable identifiers | ✓ SATISFIED | execution_id in all JsonResponse::new calls, symbol_id in SymbolMatch |
| ID-02: span_id stable across runs | ✓ SATISFIED | Span::generate_id() from Phase 4 (not re-verified here) |
| ID-03: symbol_id stable across runs | ✓ SATISFIED | generate_symbol_id() with SHA-256 of language:fqn:span_id |
| ID-04: execution_id generated and recorded | ✓ SATISFIED | generate_execution_id() called in all commands, ExecutionLog tracks it |
| DB-03: execution log table | ✓ SATISFIED | execution_log table with all required columns created by ExecutionLog |

### Anti-Patterns Found

None - no TODO/FIXME/placeholder comments found in key files. One unused method warning (`set_error`) which is intentional for future error handling.

### Human Verification Required

None - all automated checks pass. The following items could benefit from human testing but are not blockers:

1. **End-to-end execution tracking** - Run `magellan query --db test.db --file main.rs --output json` and verify:
   - execution_id appears in JSON response
   - symbol_id appears in symbol results
   - execution_log table has a new row after command completes

2. **Cross-run symbol ID stability** - Index the same file twice, verify symbol_id values are identical across runs

3. **Execution log audit trail** - Query execution_log table directly to verify args, outcome, timestamps are recorded correctly

### Gaps Summary

No gaps found. All phase goals achieved:

1. **symbol_id generation**: SHA-256 based, deterministic, using language:fqn:span_id format
2. **SymbolNode schema**: symbol_id field added and populated during insertion
3. **SymbolFact fqn tracking**: fqn field added, set during extraction in all language parsers
4. **ExecutionLog module**: Complete implementation following ChunkStore pattern
5. **Schema version**: Incremented to 2, properly documented
6. **JSON output**: symbol_id included in SymbolMatch with skip_serializing_if
7. **Execution tracking**: All commands generate execution_id and record start/finish in execution_log
8. **Tests**: All tests pass (9 symbol_id tests, 6 execution_log tests, 4 SymbolMatch symbol_id tests)

---

_Verified: 2026-01-19T14:00:00Z_
_Verifier: Claude (gsd-verifier)_

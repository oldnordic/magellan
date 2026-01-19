---
phase: 06-query-ux
verified: 2026-01-19T13:31:00Z
status: passed
score: 4/4 truths verified
---

# Phase 06: Query UX Verification Report

**Phase Goal:** Users can query the indexed graph from the CLI with deterministic, span-aware, ID-stable results.

**Verified:** 2026-01-19T13:31:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth   | Status     | Evidence       |
| --- | ------- | ---------- | -------------- |
| 1   | User can look up symbol definitions by name/kind and receive stable symbol_id | VERIFIED | find_cmd.rs line 289 propagates s.symbol_id to SymbolMatch::new |
| 2   | User can look up references to a symbol and receive target_symbol_id | VERIFIED | refs_cmd.rs line 188-191 populates target_symbol_id from caller/callee symbol_ids |
| 3   | User can query callers/callees with deterministic ordering and stable IDs | VERIFIED | refs_cmd.rs line 164-168 sorts by file_path, byte_start; CallFact has caller/callee symbol_ids |
| 4   | User can list indexed files with optional symbol counts | VERIFIED | files_cmd.rs implements run_files with --symbols flag, deterministic sort at line 60 |

**Score:** 4/4 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| src/find_cmd.rs | Symbol lookup with stable IDs, 280+ lines | VERIFIED | 398 lines, propagates symbol_id on lines 289, 362 |
| src/output/command.rs | ReferenceMatch with target_symbol_id field | VERIFIED | Lines 599-600 define target_symbol_id with skip_serializing_if |
| src/refs_cmd.rs | Refs command with target ID in output | VERIFIED | 208 lines, output_json_mode includes target_symbol_id |
| src/graph/schema.rs | CallNode with caller/callee symbol_id fields | VERIFIED | Lines 60-64 define caller_symbol_id and callee_symbol_id in CallNode |
| src/references.rs | CallFact with caller/callee symbol_id fields | VERIFIED | Lines 44-49 define caller_symbol_id and callee_symbol_id in CallFact |
| src/graph/call_ops.rs | Call indexing with symbol_id storage | VERIFIED | Lines 165-170 populate caller/callee symbol_ids from stable_symbol_ids map |
| src/files_cmd.rs | Files command implementation | VERIFIED | 105 lines, run_files function with with_symbols and output_format params |
| src/main.rs | CLI integration for commands | VERIFIED | Command::Find, Command::Refs, Command::Files defined; mod files_cmd included |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| src/find_cmd.rs:output_json_mode | SymbolMatch::new | Pass s.symbol_id | WIRED | Line 289: SymbolMatch::new(..., s.symbol_id) |
| src/find_cmd.rs:output_json_mode | symbol_nodes_in_file_with_ids | Fetch symbol_id | WIRED | Line 67: query::symbol_nodes_in_file_with_ids(graph, file_path) |
| src/refs_cmd.rs:output_json_mode | ReferenceMatch::new | Pass target_symbol_id | WIRED | Line 193: ReferenceMatch::new(..., target_symbol_id) |
| src/refs_cmd.rs:output_json_mode | CallFact.symbol_ids | Use populated fields | WIRED | Lines 187-191: target_symbol_id from call.caller_symbol_id/call.callee_symbol_id |
| src/graph/call_ops.rs:index_calls | CallNode | Store symbol_ids | WIRED | Lines 165-170: Look up and assign caller/callee symbol_ids |
| src/graph/call_ops.rs:insert_call_node | CallNode | Include symbol_ids | WIRED | Lines 246-247: caller_symbol_id, callee_symbol_id from call |
| src/main.rs:Command::Files | files_cmd::run_files | Module function call | WIRED | Lines 1097-1098: files_cmd::run_files(db_path, with_symbols, output_format) |
| src/files_cmd.rs:run_files | symbols_in_file | Count symbols | WIRED | Line 47: symbols_in_file(&mut graph, file_path) |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| ----------- | ------ | -------------- |
| Users can look up symbol definitions by name/kind and receive span-aware results with stable IDs | SATISFIED | None |
| Users can look up references to a symbol and receive span-aware results with stable IDs | SATISFIED | None |
| Users can query callers/callees and results are deterministically ordered and include stable IDs | SATISFIED | None |
| Users can list symbols (and counts) for a given file/module | SATISFIED | None |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| src/main.rs | 861 | Unused function run_files (duplicate of files_cmd::run_files) | Warning | Dead code, not blocking |

### Human Verification Required

None. All observable truths can be verified programmatically.

### Gaps Summary

No gaps found. All phase 06 must-haves are implemented and wired correctly.

## Notes

1. **Deterministic Ordering Verified:**
   - find_cmd.rs: Lines 269-274 sort by file, start_line, start_col
   - refs_cmd.rs: Lines 164-168 sort by file_path, byte_start
   - files_cmd.rs: Line 60 sorts files alphabetically

2. **Symbol ID Propagation Chain:**
   - SymbolNode has symbol_id (from Phase 05)
   - CallNode has caller/callee symbol_ids (populated during indexing)
   - CallFact has caller/callee symbol_ids (extracted from CallNode)
   - ReferenceMatch uses target_symbol_id from CallFact
   - SymbolMatch uses symbol_id from symbol_nodes_in_file_with_ids

3. **Test Coverage:**
   - test_find_includes_symbol_id_in_json: VERIFIED (passes)
   - test_refs_includes_target_symbol_id_in_json: VERIFIED (passes)
   - test_files_with_symbol_counts: VERIFIED (passes)

4. **Minor Issues:**
   - Dead code in main.rs (lines 861-902): Unused run_files function that duplicates files_cmd::run_files. This is leftover from the refactoring but does not block goal achievement.

5. **All Compilation Checks Pass:**
   - cargo check passes with only warnings about dead code (unused run_files in main.rs)
   - All tests pass

---
_Verified: 2026-01-19T13:31:00Z_
_Verifier: Claude (gsd-verifier)_

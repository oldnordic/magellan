---
phase: 02-deterministic-watch--indexing-pipeline
verified: 2026-01-19T11:00:00Z
status: passed
score: 9/9 must-haves verified
---

# Phase 2: Deterministic Watch & Indexing Pipeline Verification Report

**Phase Goal:** Users can continuously index a repo via watch mode and trust it to behave deterministically under file event storms.

**Verified:** 2026-01-19
**Status:** PASSED
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth                                                                                                      | Status     | Evidence                                                          |
| --- | ---------------------------------------------------------------------------------------------------------- | ---------- | ----------------------------------------------------------------- |
| 1   | Re-indexing the same file multiple times fully replaces all derived facts (no ghost nodes/edges).         | ✓ VERIFIED | `reconcile_file_path` in ops.rs:255-334 calls `delete_file_facts` before re-indexing |
| 2   | After reconcile/delete, there are no orphan edges: every edge endpoint refers to an existing entity.       | ✓ VERIFIED | `delete_edges_touching_entities` in schema.rs + edge cleanup in ops.rs:238 |
| 3   | A dirty-path reconcile uses filesystem state + content hash to deterministically decide: delete vs skip vs reindex. | ✓ VERIFIED | Hash comparison in ops.rs:272-290 determines Unchanged vs Reindexed    |
| 4   | Watch mode (by default) starts a watcher immediately, completes scan-initial baseline, then applies buffered changes. | ✓ VERIFIED | `run_watch_pipeline` in indexer.rs:231-306 implements baseline-then-drain |
| 5   | Watcher events are coalesced deterministically into batches processed in sorted path order.              | ✓ VERIFIED | `WatcherBatch` uses BTreeSet in watcher.rs:313-337; processing sorted in indexer.rs:339-376 |
| 6   | User can apply gitignore-style rules + include/exclude globs and Magellan deterministically skips files.   | ✓ VERIFIED | `FileFilter` in filter.rs with precedence order; gitignore loaded at line 72-115 |
| 7   | Per-file failures (unreadable/parse errors) do not stop watch; the failure is recorded as structured diagnostics. | ✓ VERIFIED | Error handling in scan.rs:94-133 wraps each file in Result; continues on error |

**Score:** 7/7 truths verified

### Required Artifacts

| Artifact                              | Expected                          | Status      | Details |
| ------------------------------------- | --------------------------------- | ----------- | ------- |
| `src/graph/ops.rs`                    | reconcile_file_path API           | ✓ VERIFIED  | Lines 255-334: hash comparison, delete, re-index |
| `src/graph/references.rs`             | delete_references_in_file          | ✓ VERIFIED  | Lines 32-63: sorts IDs, deletes Reference nodes |
| `src/graph/call_ops.rs`               | delete_calls_in_file              | ✓ VERIFIED  | Lines 32-63: sorts IDs, deletes Call nodes |
| `src/graph/schema.rs`                 | Edge cleanup for orphan prevention | ✓ VERIFIED  | delete_edges_touching_entities function |
| `src/graph/query.rs`                  | Query helpers for orphan detection | ✓ VERIFIED  | edge_endpoints, count_entities functions |
| `src/indexer.rs`                      | Batch processor + watch pipeline   | ✓ VERIFIED  | run_watch_pipeline, process_dirty_paths |
| `src/watcher.rs`                      | Debounced watcher                  | ✓ VERIFIED  | notify 8.2.0 + notify-debouncer-mini 0.7.0, WatcherBatch |
| `src/diagnostics/watch_diagnostics.rs` | Structured diagnostics            | ✓ VERIFIED  | WatchDiagnostic, SkipReason, DiagnosticStage with Ord |
| `src/graph/filter.rs`                 | FileFilter with gitignore support  | ✓ VERIFIED  | FileFilter::should_skip with precedence |
| `tests/watch_buffering_tests.rs`       | Buffering regression tests         | ✓ VERIFIED  | 6 tests for baseline, drain, deterministic ordering |
| `tests/ignore_rules_tests.rs`          | Filtering regression tests        | ✓ VERIFIED  | 7 tests for gitignore, include/exclude, error containment |

### Key Link Verification

| From              | To                        | Via                                              | Status | Details |
| ----------------- | ------------------------- | ------------------------------------------------ | ------ | ------- |
| src/indexer.rs    | src/graph/ops.rs          | reconcile_file_path                              | ✓ WIRED | indexer.rs:345 calls graph.reconcile_file_path |
| src/graph/ops.rs  | src/graph/references.rs   | delete_file_facts -> delete_references_in_file    | ✓ WIRED | ops.rs:223 calls graph.references.delete_references_in_file |
| src/graph/ops.rs  | src/graph/call_ops.rs      | delete_file_facts -> delete_calls_in_file        | ✓ WIRED | ops.rs:226 calls graph.calls.delete_calls_in_file |
| src/graph/ops.rs  | src/graph/schema.rs        | delete_file_facts -> delete_edges_touching_entities | ✓ WIRED | ops.rs:238-239 calls delete_edges_touching_entities |
| src/indexer.rs    | src/watcher.rs            | recv_batch_timeout -> WatcherBatch                | ✓ WIRED | indexer.rs:321 receives batches from watcher |
| src/graph/scan.rs | src/diagnostics           | scan_directory_with_filter -> WatchDiagnostic     | ✓ WIRED | scan.rs:67-103 collects diagnostics for skips/errors |
| src/main.rs       | src/watch_cmd.rs          | watch command -> run_watch_pipeline               | ✓ WIRED | main.rs:920 calls watch_cmd::run_watch |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| ----------- | ------ | -------------- |
| WATCH-01    | ✓ SATISFIED | scan_initial=true by default (main.rs:185), --watch-only flag (line 211-213) |
| WATCH-02    | ✓ SATISFIED | BTreeSet for dirty paths (indexer.rs:167), sorted processing (indexer.rs:339-376) |
| WATCH-03    | ✓ SATISFIED | ignore crate 0.4.25 (Cargo.toml:51), FileFilter precedence (filter.rs:148-200) |
| WATCH-04    | ✓ SATISFIED | reconcile_file_path with delete before index (ops.rs:255-334) |
| WATCH-05A   | ✓ SATISFIED | Per-path Result wrapping in scan (scan.rs:94-133), diagnostics collected |
| WATCH-05B   | DEFERRED | JSON output deferred to Phase 3 per plan |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| None  | —    | No blockers found | —        | All code is substantive and wired |

### Human Verification Required

### 1. Watch mode storm handling

**Test:** Run `cargo run -- watch --root . --db /tmp/magellan.db` and rapidly save a file multiple times (editor storm).
**Expected:** Magellan coalesces the events and produces consistent final state without spamming output.
**Why human:** Cannot programmatically verify real-time event coalescing behavior under actual editor workload.

### 2. Deterministic output ordering

**Test:** Run watch on a repo, observe that diagnostics and file processing output appear in deterministic (sorted) order.
**Expected:** Same files + rules produce same output ordering across multiple runs.
**Why human:** Visual verification of stdout ordering across multiple invocations.

### 3. Gitignore integration correctness

**Test:** Create a .gitignore file with patterns, run scan/watch, verify expected files are skipped.
**Expected:** Files matching .gitignore patterns are reported with "IgnoredByGitignore" diagnostics.
**Why human:** Integration with actual .gitignore files requires filesystem verification.

---

_Verified: 2026-01-19_
_Verifier: Claude (gsd-verifier)_

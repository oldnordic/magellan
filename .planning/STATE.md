# STATE: Magellan

## Project Reference

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

**Product framing:** Local-first CLI indexer whose outputs are reliable enough to be treated as an API contract (schema-versioned, deterministic, span-aware, stable IDs).

## Current Position

- **Current phase:** Phase 6 — Query UX (Definitions, References, Call Graph, File Listing)
- **Status:** In progress (3/4 plans executed)
- **Last activity:** 2026-01-19 - Completed 06-03 (Call Graph Symbol IDs)
- **Next action:** Continue with Phase 6 query UX improvements

**Progress bar:** [███       ] 17% (3/18 total plans executed - Phase 6 in progress)

## Success Definition (v1)

Magellan v1 is "done" when a user can:
- Run watch/index against a repo deterministically (baseline then incremental),
- Query defs/refs/callers/callees with span-aware stable IDs,
- Export graph data deterministically,
- Validate runs and correlate outputs/logs to an `execution_id`,
- Script all of the above via schema-versioned JSON without stdout contamination.

## Performance / Quality Metrics (track as work progresses)

- **Determinism:** Same command on unchanged inputs -> byte-for-byte identical JSON (ignoring allowed fields).
- **Span fidelity:** Spans are UTF-8 byte offsets, half-open; line/col mapping consistent with editor display.
- **Watcher robustness:** Editor-save storms do not cause nondeterministic final DB state.
- **Reliability:** Per-file errors never crash watch; errors appear as structured diagnostics.

## Accumulated Context

### Key Decisions (from PROJECT.md)
- CLI-first tool; no server responsibilities.
- `--db <FILE>` required; no hidden state.
- Watch defaults to scan-initial then watch.
- Structured JSON + stable IDs + deterministic ordering as v1 contract.

### Key Decisions (from Phase 1 / Plan 01)
- Pin sqlitegraph to crates.io v1.0.0 as the Phase 1 persistence compatibility baseline.
- Track Cargo.lock so sqlitegraph resolution is reproducible across environments and CI.

### Key Decisions (from Phase 1 / Plan 02)
- Add a read-only sqlitegraph DB compatibility preflight that checks `graph_meta.schema_version` before any writes.
- Treat `:memory:` and non-existent DB paths as "new DB" (compat OK) to preserve test ergonomics.

### Key Decisions (from Phase 1 / Plan 03)
- Add a Magellan-owned `magellan_meta` table (single row, `id=1`) storing `magellan_schema_version` and `sqlitegraph_schema_version`; refuse opens deterministically on mismatch.
- Prevent bounded watcher tests from hanging by adding an idle timeout to `run_indexer_n` (notify can coalesce events).

### Key Decisions (from Phase 2 / Plan 01)
- Add `reconcile_file_path` as the deterministic primitive for all file updates (create/modify/delete based on actual file state, not event type)
- Implement `delete_file_facts` to delete ALL derived data for a file path (symbols, references, calls, chunks, file node)
- Use deterministic sorting for multi-entity deletion (gather IDs, sort ascending, delete in order)

### Key Decisions (from Phase 2 / Plan 02)
- Upgrade to notify 8.2.0 + notify-debouncer-mini 0.7.0 for standard debouncing (instead of hand-rolled solution)
- Change scan_initial default to true (baseline scan by default) with --watch-only opt-out flag
- Use BTreeSet<PathBuf> for dirty path buffering to ensure deterministic lexicographic ordering
- Implement bounded sync_channel(1) for wakeup ticks with non-blocking try_send to prevent watcher blocking
- Use reconcile_file_path for all watcher events (debouncer doesn't preserve event types)
- Add WatcherBatch type containing only sorted paths (no timestamps) for determinism

### Key Decisions (from Phase 2 / Plan 03)
- Use `ignore` crate v0.4.25 instead of hand-rolled gitignore parsing for correctness
- Deterministic filtering precedence: Internal > Gitignore > Include > Exclude
- Sort diagnostics at output time for deterministic ordering regardless of walkdir order
- Internal ignores always win over gitignore for security (db files, .git/, target/, etc.)
- Per-path Result wrapping ensures watch continues on bad files

### Key Decisions (from Phase 3 Research)
- Use hash-based execution_id generation (timestamp + pid) instead of uuid crate for simplicity
- Schema version "1.0.0" for JSON output contract
- Stdout = JSON data only; stderr = logs/diagnostics
- Span representation follows existing SymbolFact pattern (byte + line/col, half-open)
- Phase 4 will implement proper stable span_id (placeholder for now)

### Key Decisions (from Phase 3 / Plan 01)
- OutputFormat enum with Human and Json variants for CLI flag parsing
- JsonResponse<T> wrapper with schema_version, execution_id, data, partial fields
- Execution ID format: 16 hex chars from timestamp (8) + PID (8)
- output_json() function using serde_json::to_string_pretty for human-readable JSON

### Key Decisions (from Phase 3 / Plan 02)
- StatusResponse and ErrorResponse types for JSON error/status reporting
- ErrorResponse used for structured error output in JSON mode
- StatusResponse includes files, symbols, references, calls, code_chunks counts

### Key Decisions (from Phase 3 / Plan 03)
- Span type with hash-based span_id placeholder (proper generation in Phase 4)
- SymbolMatch for query/find results with match_id, span, name, kind, parent
- ReferenceMatch for refs results with match_id, span, referenced_symbol, reference_kind
- Global --output flag parsed once in main() and passed to command runners
- Deterministic sorting for all JSON arrays (file_path, start_line, start_col ordering)

### Key Decisions (from Phase 4 / Plan 01)
- SHA-256 for span_id generation (platform-independent, deterministic)
- 16 hex characters from first 8 bytes of SHA-256 hash
- Format: file_path + ':' + byte_start(big-endian) + ':' + byte_end(big-endian)
- No content hashing in span_id (position-based only for stability)

### Key Decisions (from Phase 4 / Plan 02)
- Helper functions for byte/line/col conversion isolated to test module
- Test helpers use 0-indexed lines/col, Span stores 1-indexed for user-friendliness
- UTF-8 safety verified via .get() and .is_char_boundary() standard library methods
- Unicode escape sequences used in tests for portability

### Key Decisions (from Phase 4 / Plan 03)
- Module-level documentation should explain half-open semantics with concrete example
- Span struct docstring must include safety section for UTF-8 slicing using .get()
- All public types should have examples demonstrating usage
- Method documentation should explain algorithm and stability guarantees
- Span ID format is part of Magellan's stable API contract

### Key Decisions (from Phase 5 / Plan 01)
- Add fqn (fully-qualified name) field to SymbolFact for stable symbol_id generation
- For v1, fqn is set to simple symbol name (hierarchical FQN deferred)
- Add symbol_id field to SymbolNode schema (SHA-256 hash of language:fqn:span_id)
- generate_symbol_id() function provides deterministic stable ID generation
- Use SHA-256 for symbol_id (platform-independent, deterministic)
- Hash format: language:fqn:span_id (colon-separated)
- 16 hex characters from first 8 bytes (64-bit space)

### Key Decisions (from Phase 5 / Plan 03)
- symbol_id is Option<String> with skip_serializing_if for backward compatibility
- New symbol_nodes_in_file_with_ids() query function returns (node_id, SymbolFact, Option<String>)
- Command handlers use symbol_nodes_in_file_with_ids() for JSON output paths only
- SymbolMatch now includes symbol_id field for stable symbol correlation in JSON API

### Key Decisions (from Phase 5 / Plan 04)
- ExecutionLog exposed from CodeGraph via public execution_log() method
- ExecutionTracker helper in main.rs with start/finish/error/set_counts methods
- All CLI commands record execution in execution_log table (timestamps, args, outcome)
- JSON responses include execution_id for correlation with execution_log records
- Single execution_id per command generated at start, used throughout lifecycle
- Human-output commands still track execution even without JSON response
- Watch mode treated as single execution with outcome set at exit

### Key Decisions (from Phase 6 / Plan 01)
- Find command must support --output flag like status/query commands for explicit format control
- symbol_id propagation must be consistent across all query paths (find-by-name and glob-listing)
- ReferenceMatch::new requires 4th parameter (target_symbol_id) for future reference ID tracking

### Key Decisions (from Phase 6 / Plan 02)
- Use skip_serializing_if for target_symbol_id to maintain backward compatibility with existing JSON consumers
- Add --output flag parsing to refs command (consuming but not storing, using global output_format)
- Build symbol_id lookup map to efficiently fetch target IDs without repeated graph queries

### Key Decisions (from Phase 6 / Plan 03)
- Use #[serde(default)] on optional symbol_id fields for backward compatibility with existing Call nodes
- Build stable_symbol_ids lookup map as (file_path, symbol_name) -> Option<String> during indexing
- For "in" direction (callers), target_symbol_id = caller_symbol_id
- For "out" direction (callees), target_symbol_id = callee_symbol_id
- Use symbol_id directly from CallFact in refs JSON output instead of database lookup

### Known Risks / Watch-outs
- Mixed coordinate systems (byte vs char; inclusive vs exclusive).
- "Stable IDs" accidentally derived from unstable sources (rowid, node id, iteration order).
- Watch event storms creating nondeterministic intermediate states.
- Nested .gitignore files not yet supported (only root .gitignore/.ignore loaded)

### Blockers
- None currently (roadmap created). If constraints change (e.g., interop export moved to v2), revisit Phase 9.

## Session Continuity

- **Last session:** 2026-01-19T13:21:09Z
- **Stopped at:** Completed 06-03 (Call Graph Symbol IDs)
- **Resume file:** None

If resuming later, start by:
1. Open `.planning/phases/06-query-ux/06-03-SUMMARY.md` for context on completed work.
2. Run `cargo test --workspace` to verify baseline health.
3. Phase 6-03 complete - ready for next plan in Phase 6.

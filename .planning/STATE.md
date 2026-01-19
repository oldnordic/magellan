# STATE: Magellan

## Project Reference

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

**Product framing:** Local-first CLI indexer whose outputs are reliable enough to be treated as an API contract (schema-versioned, deterministic, span-aware, stable IDs).

## Current Position

- **Current phase:** Phase 3 — CLI Output Contract (Schema + Determinism + Stdout Discipline)
- **Status:** Phase 3 complete (3/3 plans executed)
- **Last activity:** 2026-01-19 - Completed 03-03 (JSON Output for Query Commands)
- **Next action:** Execute Phase 4 plans (04-stable-ids)

**Progress bar:** [████████░░] 42% (0/3 plans executed in Phase 4, 11/12 total plans executed)

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

### Known Risks / Watch-outs
- Mixed coordinate systems (byte vs char; inclusive vs exclusive).
- "Stable IDs" accidentally derived from unstable sources (rowid, node id, iteration order).
- Watch event storms creating nondeterministic intermediate states.
- Nested .gitignore files not yet supported (only root .gitignore/.ignore loaded)
- span_id generation deferred to Phase 4 (using placeholders in Phase 3)

### Blockers
- None currently (roadmap created). If constraints change (e.g., interop export moved to v2), revisit Phase 9.

## Session Continuity

- **Last session:** 2026-01-19T10:47:05Z
- **Stopped at:** Completed Phase 3 - all 3 plans (03-01, 03-02, 03-03) executed
- **Resume file:** None

If resuming later, start by:
1. Open `.planning/phases/03-cli-output-contract/03-03-SUMMARY.md` for context on completed work.
2. Run `cargo test --workspace` to verify baseline health.
3. Execute Phase 4 plans: `/gsd:execute-phase 04-stable-ids`

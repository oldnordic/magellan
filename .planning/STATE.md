# STATE: Magellan

## Project Reference

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

**Product framing:** Local-first CLI indexer whose outputs are reliable enough to be treated as an API contract (schema-versioned, deterministic, span-aware, stable IDs).

## Current Position

- **Current phase:** Phase 1 — Persistence Compatibility Baseline
- **Status:** Phase complete
- **Last activity:** 2026-01-18 - Completed 01-03-PLAN.md
- **Next action:** Begin Phase 2 planning (or next roadmap phase)

**Progress bar:** [██████████] 100% (3/3 plans complete)

## Success Definition (v1)

Magellan v1 is “done” when a user can:
- Run watch/index against a repo deterministically (baseline then incremental),
- Query defs/refs/callers/callees with span-aware stable IDs,
- Export graph data deterministically,
- Validate runs and correlate outputs/logs to an `execution_id`,
- Script all of the above via schema-versioned JSON without stdout contamination.

## Performance / Quality Metrics (track as work progresses)

- **Determinism:** Same command on unchanged inputs → byte-for-byte identical JSON (ignoring allowed fields).
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

### Known Risks / Watch-outs
- Mixed coordinate systems (byte vs char; inclusive vs exclusive).
- “Stable IDs” accidentally derived from unstable sources (rowid, node id, iteration order).
- Watch event storms creating nondeterministic intermediate states.

### Blockers
- None currently (roadmap created). If constraints change (e.g., interop export moved to v2), revisit Phase 9.

## Session Continuity

- **Last session:** 2026-01-18T22:59:13Z
- **Stopped at:** Completed 01-03-PLAN.md
- **Resume file:** None

If resuming later, start by:
1. Open `.planning/ROADMAP.md` and confirm Phase 2 scope.
2. Run `cargo test --workspace` to verify baseline health.
3. Keep a running log of determinism acceptance tests/golden fixtures as they’re introduced.

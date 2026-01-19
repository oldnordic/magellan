# STATE: Magellan

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-19)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** Planning v1.1 milestone

## Current Position

**Milestone:** v1.0 Magellan — SHIPPED 2026-01-19
**Status:** All v1 phases complete (9 phases, 29 plans)
**Last activity:** 2026-01-19 - Completed Phase 9 (SCIP Export), archived v1.0 milestone
**Next action:** Plan v1.1 milestone

**Progress bar:** [██████████] 100% (29/29 plans executed - v1.0 complete)

## Success Definition (v1)

Magellan v1 is "done" when a user can:
- ✓ Run watch/index against a repo deterministically (baseline then incremental)
- ✓ Query defs/refs/callers/callees with span-aware stable IDs
- ✓ Export graph data deterministically (JSON/JSONL/DOT/CSV/SCIP)
- ✓ Validate runs and correlate outputs/logs to an `execution_id`
- ✓ Script all of the above via schema-versioned JSON without stdout contamination

## v1.0 Summary

**Shipped:**
- Deterministic watch mode with debounced event batching
- Schema-versioned JSON output with stdout/stderr discipline
- Stable span and symbol IDs (span_id, symbol_id, execution_id)
- Query surface: definitions, references, callers/callees, file listing
- Export formats: JSON, JSONL, DOT, CSV, SCIP
- Validation hooks (pre/post-run) with orphan detection

**Stats:**
- ~18,000 lines of Rust
- 9 phases, 29 plans
- 26 days (2025-12-24 → 2026-01-19)

**Known concerns for v1.1:**
- Symbol name collisions in HashMap resolution
- Incomplete FQN (simple names instead of hierarchical)
- Full symbol scan for references (performance)
- SCIP export lacks integration tests

## Performance / Quality Metrics (track as work progresses)

- **Determinism:** ✓ Same command on unchanged inputs -> byte-for-byte identical JSON (ignoring allowed fields)
- **Span fidelity:** ✓ Spans are UTF-8 byte offsets, half-open; line/col mapping consistent with editor display
- **Watcher robustness:** ✓ Editor-save storms do not cause nondeterministic final DB state
- **Reliability:** ✓ Per-file errors never crash watch; errors appear as structured diagnostics

## Accumulated Context

### Key Decisions (from PROJECT.md)
- CLI-first tool; no server responsibilities
- `--db <FILE>` required; no hidden state
- Watch defaults to scan-initial then watch
- Structured JSON + stable IDs + deterministic ordering as v1 contract

### Key Decisions (from Phase 9 / SCIP Export)
- SCIP export uses scip crate v0.6.1 with protobuf 3.7 for binary output
- Symbol format: `magellan rust . . . escaped_symbol_name` (double-space escaping)
- Position encoding: 0-indexed lines, UTF-8 byte offsets (converted from Magellan's 1-indexed)
- Language detection via file extension mapping (rust, python, javascript, typescript, go, java, kotlin, c, cpp, csharp, php, ruby, swift, bash, json, toml, markdown)
- LSIF export deferred to v2 (deprecated by Sourcegraph)

### Key Decisions (from Phase 8 / Validation)
- Validation module follows VerifyReport pattern with passed bool and errors Vec
- Error codes use SCREAMING_SNAKE_CASE for machine-readability
- Orphan detection uses sqlitegraph neighbors() with NeighborQuery and BackendDirection
- Pre-run validation: DB parent, root path, input path existence
- Post-run validation: orphan references and orphan calls
- Validation failures exit with code 1 for CI/CD integration

### Key Decisions (from Phase 5 / Stable Identity)
- SHA-256 for span_id and symbol_id generation (platform-independent, deterministic)
- 16 hex characters from first 8 bytes of hash
- span_id format: file_path + ':' + byte_start + ':' + byte_end
- symbol_id format: language:fqn:span_id (colon-separated)
- For v1, fqn is set to simple symbol name (hierarchical FQN deferred to v1.1)
- Execution ID format: 16 hex chars from timestamp (8) + PID (8)

### Key Decisions (from Phase 2 / Watch Pipeline)
- `reconcile_file_path` as deterministic primitive for all file updates
- `delete_file_facts` to delete ALL derived data (symbols, references, calls, chunks, file node)
- notify 8.2.0 + notify-debouncer-mini 0.7.0 for debouncing
- BTreeSet<PathBuf> for dirty path buffering (deterministic lexicographic ordering)
- ignore crate v0.4.25 for gitignore-style filtering
- Deterministic filtering precedence: Internal > Gitignore > Include > Exclude

### Known Risks / Watch-outs
- Mixed coordinate systems (byte vs char; inclusive vs exclusive) — resolved in v1.0
- "Stable IDs" accidentally derived from unstable sources (rowid, node id) — resolved with SHA-256
- Watch event storms creating nondeterministic intermediate states — resolved with debouncing
- Nested .gitignore files not yet supported (only root .gitignore/.ignore loaded) — v1.1
- Symbol name collisions in `symbol_name_to_id` HashMap — v1.1
- Incomplete FQN (simple names) — v1.1

### Blockers
- None currently

## Session Continuity

- **Last session:** 2026-01-19T17:13:00Z
- **Stopped at:** v1.0 milestone complete, archived
- **Resume file:** None

If resuming later, start by:
1. Read `.planning/PROJECT.md` for current project state
2. Read `.planning/MILESTONES.md` for milestone history
3. Run `cargo test --workspace` to verify baseline health
4. v1.0 is shipped - ready for v1.1 planning

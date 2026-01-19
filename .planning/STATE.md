# STATE: Magellan

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-19)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** Phase 10 - Path Traversal Validation

## Current Position

**Milestone:** v1.1 Correctness + Safety
**Phase:** 10 of 13 (Path Traversal Validation)
**Plan:** 3 of 4 in current phase
**Status:** In progress
**Last activity:** 2026-01-19 — Completed 10-03: Integrate Path Validation into Scan

**Progress bar:** [███░░░░░░░░] 11% v1.1 (2/18 plans) | [██████████] 100% v1.0 (29/29 plans)

## Success Definition (v1.1)

Magellan v1.1 is "done" when:
- All file access validates paths cannot escape project root
- Symbol lookup uses fully-qualified names (FQN) as keys
- Delete operations are atomic with no orphaned data
- SCIP export verified by round-trip tests
- Security best practices documented

## v1.0 Summary

**Shipped:** 2025-12-24 → 2026-01-19 (26 days)

**Delivered:**
- Deterministic watch mode with debounced event batching
- Schema-versioned JSON output with stdout/stderr discipline
- Stable span and symbol IDs (span_id, symbol_id, execution_id)
- Query surface: definitions, references, callers/callees, file listing
- Export formats: JSON, JSONL, DOT, CSV, SCIP
- Validation hooks (pre/post-run) with orphan detection

**Stats:**
- ~18,000 lines of Rust
- 9 phases, 29 plans completed

## v1.1 Roadmap

**Phases:**
- Phase 10: Path Traversal Validation (4 plans) - Security baseline
- Phase 11: FQN Extraction (6 plans) - Correctness foundation
- Phase 12: Transactional Deletes (4 plans) - Data integrity
- Phase 13: SCIP Tests + Docs (4 plans) - Validation and documentation

## Performance / Quality Metrics

- **Determinism:** ✓ Same command on unchanged inputs -> byte-for-byte identical JSON
- **Span fidelity:** ✓ UTF-8 byte offsets, half-open; line/col mapping consistent
- **Watcher robustness:** ✓ Editor-save storms do not cause nondeterministic DB state
- **Reliability:** ✓ Per-file errors never crash watch

## Accumulated Context

### Key Decisions (v1.0)
- CLI-first tool; `--db <FILE>` required; no hidden state
- SHA-256 for span_id and symbol_id (platform-independent, deterministic)
- SCIP export uses scip crate v0.6.1 with protobuf 3.7
- For v1.0, FQN set to simple symbol name (deferred hierarchical to v1.1)
- Validation module with VerifyReport pattern and orphan detection

### Key Decisions (v1.1 Planning)
- Path validation first (security baseline, no dependencies)
- FQN extraction for correctness (requires per-language scope tracking)
- Transactional deletes following generation/mod.rs pattern
- SCIP round-trip tests to verify export format

### Key Decisions (Phase 10-01: Path Validation Module)
- Symlink policy: resolve-then-validate, reject escapes
- Single-parent paths (../) with shallow depth flagged as suspicious
- Double-parent paths (../../) allowed for nested project structures
- Three or more parents always flagged (>= 3 ../ patterns)
- Mixed patterns (./subdir/../) always flagged regardless of count

### Key Decisions (Phase 10-03: Scan Integration)
- Path validation placed after `is_dir()` check, before filter application
- WalkDir's `follow_links=false` provides primary defense against symlink escapes
- Path validation is defense-in-depth - WalkDir should keep us within bounds, but we validate anyway
- All PathValidationError variants handled with appropriate diagnostics

### Blockers / Concerns

**Phase 11 (FQN):**
- Changing symbol_id breaks all existing references - migration plan required
- Per-language edge cases (anonymous namespaces, closures, trait impls, generics)

**Phase 10 (Path):**
- Cross-platform path testing needed (Linux, macOS, Windows)

## Session Continuity

- **Last session:** 2026-01-19
- **Stopped at:** Completed 10-03: Integrate Path Validation into Scan
- **Resume file:** None

If resuming later, start by:
1. Read `.planning/ROADMAP.md` for phase structure
2. Read `.planning/PROJECT.md` for requirements and constraints
3. Read `.planning/phases/10-path-traversal-validation/10-03-SUMMARY.md` for what was built
4. Run `cargo test --workspace` to verify baseline health
5. Execute `/gsd:execute-plan 10-path-traversal-validation/10-04` for cross-platform tests

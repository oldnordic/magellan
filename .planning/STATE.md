# STATE: Magellan

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-19)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** Phase 11 - FQN Extraction

## Current Position

**Milestone:** v1.1 Correctness + Safety
**Phase:** 11 of 13 (FQN Extraction)
**Plan:** 1 of 6 in current phase
**Status:** In progress
**Last activity:** 2026-01-19 — Completed Phase 11-01: ScopeStack Infrastructure

**Progress bar:** [████░░░░░░░░] 28% v1.1 (5/18 plans) | [██████████] 100% v1.0 (29/29 plans)

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

### Key Decisions (Phase 10-02: Watcher Integration)
- root_path in WatcherConfig automatically set to watched directory
- WARNING-level logging for rejected paths (defensive: don't crash on bad events)
- Silent skip for CannotCanonicalize errors (normal for deleted files)
- Path validation called for each event path in extract_dirty_paths()

### Key Decisions (Phase 10-03: Scan Integration)
- scan_directory_with_filter validates each path before processing
- WalkDir follows symlinks=false to prevent automatic following
- Validation is defense-in-depth (WalkDir should keep us in dir_path)
- Paths escaping root logged as diagnostics and skipped

### Key Decisions (Phase 10-04: Cross-Platform Tests)
- Tests use cfg attributes for platform-specific behavior
- Test structure separates general validation from symlink-specific tests
- Accept both SuspiciousTraversal and CannotCanonicalize for nonexistent paths
- Documentation covers all three platforms (Linux, macOS, Windows)

### Key Decisions (Phase 10 Verification)
- All 5 must-haves verified against actual codebase
- 63/63 tests passing across all test suites
- Path validation integrated at watcher.rs:349 and scan.rs:76
- Symlinks resolved-then-validated with proper rejection for escapes

### Key Decisions (Phase 11-01: ScopeStack Infrastructure)
- ScopeStack uses Vec<String> for component storage with language-specific separator
- ScopeSeparator enum provides type-safe :: vs . separator selection
- Anonymous symbols (empty name) use parent scope via fqn_for_symbol("")
- Push/pop pattern for entering/exiting scopes during tree-sitter traversal
- Module + type-level scope tracking (excludes impl blocks, closures, local scopes)

### Blockers / Concerns

**Phase 11 (FQN):**
- Changing symbol_id breaks all existing references - migration plan required
- Per-language edge cases (anonymous namespaces, closures, trait impls, generics)

## Session Continuity

- **Last session:** 2026-01-19
- **Stopped at:** Completed Phase 11-01: ScopeStack Infrastructure
- **Resume file:** None

If resuming later, start by:
1. Read `.planning/ROADMAP.md` for phase structure
2. Read `.planning/PROJECT.md` for requirements and constraints
3. Read `.planning/phases/11-fqn-extraction/11-01-SUMMARY.md` for plan results
4. Run `cargo test --workspace` to verify baseline health
5. Execute next plan in Phase 11 (11-02 through 11-06)

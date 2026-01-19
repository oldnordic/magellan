# STATE: Magellan

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-19)

**Core value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.
**Current focus:** Phase 12 - Transactional Deletes

## Current Position

**Milestone:** v1.1 Correctness + Safety
**Phase:** 12 of 13 (Transactional Deletes)
**Plan:** 1 of 4 in current phase
**Status:** In progress
**Last activity:** 2026-01-19 — Completed Phase 12-01: Transactional Delete Implementation

**Progress bar:** [███████░░░] 61% v1.1 (11/18 plans) | [██████████] 100% v1.0 (29/29 plans)

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

### Key Decisions (Phase 11-02: Rust Parser FQN Extraction)
- function_signature_item node kind for trait method declarations (not function_item)
- impl_item does not create symbols; only tracks scope for methods
- mod_item, trait_item create symbols AND track child scope
- walk_tree_with_scope pattern: scope boundary nodes push/pop in match arms

### Key Decisions (Phase 11-03: Dot-Separated Language FQN Extraction)
- All dot-separated languages use ScopeSeparator::Dot for FQN construction
- Java package scope split on '.' to create com.example.Class.method FQNs
- extract_symbol_with_fqn handles type scope nodes for symbol creation
- Package declaration symbol extracted before pushing to scope stack

### Key Decisions (Phase 11-04: C/C++ Parser FQN Extraction)
- C has no namespaces, so FQN = simple name (no changes needed)
- C++ uses ScopeSeparator::DoubleColon for namespace::symbol FQNs
- Namespace handling with push/pop pattern for nested namespaces
- Anonymous namespaces use empty name, parent scope for FQN

### Key Decisions (Phase 11-05: FQN-Based Symbol Lookup)
- Symbol lookup maps use FQN as key instead of simple name
- FQN collision detection emits WARN-level messages
- Backward compatibility: fqn.or(name).unwrap_or_default() fallback pattern
- SymbolNode schema includes fqn field for unique identification

### Key Decisions (Phase 11-06: FQN Implementation Complete)
- symbol_id generation explicitly uses FQN: hash(language, fqn, span_id)
- Fixed bug: symbol_fact_from_node was using name instead of fqn field
- C++ namespaces now create symbols with proper FQNs
- Database version bumped from 2 to 3 (breaking change for symbol_id)
- Migration error message provides clear re-index instructions
- Integration tests verify FQN extraction across Rust, Java, Python, C++

### Blockers / Concerns

**Phase 11 (FQN):**
- None - complete. Database version bump handles migration.

**Phase 12 (Transactional Deletes):**
- Plan 12-01 complete: delete_file_facts() now uses IMMEDIATE transaction
- Plans 12-02, 12-03, 12-04 remain

### Key Decisions (Phase 12-01: Transactional Delete Implementation)
- Use TransactionBehavior::Immediate for write locking during delete operations
- In-memory index removal occurs only after successful database commit
- Automatic rollback on any failure via Rust's Drop trait
- Transaction pattern follows generation/mod.rs style with explicit commit error handling

## Session Continuity

- **Last session:** 2026-01-19
- **Stopped at:** Completed Phase 12-01: Transactional Delete Implementation
- **Resume file:** None

If resuming later, start by:
1. Read `.planning/ROADMAP.md` for phase structure
2. Read `.planning/PROJECT.md` for requirements and constraints
3. Read `.planning/phases/12-transactional-deletes/12-01-SUMMARY.md` for plan results
4. Run `cargo test --workspace` to verify baseline health
5. Execute next plan: Phase 12-02 - Remaining Transactional Cleanup

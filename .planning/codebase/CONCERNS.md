# Codebase Concerns

**Analysis Date:** 2026-01-19

## Tech Debt

### Cross-File Symbol Resolution (Not Implemented)
- Issue: References and calls are only resolved within the same file. Cross-file references return zero results.
- Files: `src/references.rs`, `src/graph/tests.rs:58-69`
- Impact: ~50% effective precision. Call graphs are incomplete. Queries miss references across files.
- Fix approach: Implement import-aware AST analysis to track `use` statements and resolve symbols across module boundaries. See `docs/CROSS_FILE_ROADMAP.md` for 6-phase implementation plan.

### Name Collision Handling (TODO)
- Issue: When multiple symbols share the same name across files, only the first one is kept. No disambiguation strategy implemented.
- Files: `src/graph/query.rs:195-196`
- Impact: Symbol lookups return wrong results when names collide.
- Fix approach: Implement disambiguation using module paths or return all matches with context for user selection.

### Main.rs File Size
- Issue: Main file is 923 lines, exceeding the 300 LOC per file guideline.
- Files: `src/main.rs`
- Impact: Harder to navigate and maintain. CLI argument parsing mixed with command execution.
- Fix approach: Extract argument parsing to separate `args.rs` module. Move command runners to individual files in a `cli/` directory.

### Large Ingest Modules
- Issue: Language-specific ingest files are 500-650 lines each.
- Files: `src/ingest/typescript.rs` (646 LOC), `src/ingest/python.rs` (588 LOC), `src/ingest/javascript.rs` (586 LOC), `src/ingest/java.rs` (559 LOC), `src/ingest/cpp.rs` (554 LOC), `src/ingest/c.rs` (483 LOC)
- Impact: Files approaching double the recommended 300 LOC limit.
- Fix approach: Consider splitting into smaller modules by functionality (symbol extraction vs reference extraction vs utilities).

## Known Bugs

### Cross-File Reference Resolution Fails
- Symptoms: Test `test_cross_file_references` explicitly fails. References from one file to symbols defined in another file are not indexed.
- Files: `src/graph/tests.rs:24-71`
- Trigger: Calling a function defined in a different file
- Workaround: None. This is a fundamental limitation of the current implementation.
- Status: Known and documented. The test exists to demonstrate the bug.

### Native V2 Backend Edge Operations Broken
- Symptoms: Edge insertion fails with "Corrupt node record 0: Invalid V2 node record version 0"
- Files: `tests/native_v2_backend.rs`
- Trigger: Using `native-v2` feature flag for sqlitegraph backend
- Workaround: Use default SQLite backend (already default)
- Status: Documented in `docs/NATIVE_V2_BACKEND_FINDINGS.md`. Issue is upstream in sqlitegraph.

## Security Considerations

### No Validation of File Paths
- Risk: Malicious paths could cause unintended filesystem access
- Files: `src/main.rs` (all command handlers), `src/watcher.rs`
- Current mitigation: None documented
- Recommendations: Add path validation to prevent directory traversal attacks when `--root` or `--db` arguments point outside expected workspace

### SQLite Injection via Query Parameters
- Risk: User-provided symbol names and file paths are used in database queries
- Files: `src/graph/query.rs`, `src/graph/ops.rs`
- Current mitigation: rusqlite's parameterized queries provide some protection
- Recommendations: Audit all SQL construction to ensure parameterized queries are used consistently

### Unbounded File Event Processing
- Risk: Large file change storms could exhaust memory
- Files: `src/watcher.rs`
- Current mitigation: Debounce configuration (default 500ms)
- Recommendations: Add maximum buffer size and backpressure handling

## Performance Bottlenecks

### Full Re-Indexing on File Changes
- Problem: Each file change triggers full re-parsing and re-indexing without incremental update capability
- Files: `src/watcher.rs`, `src/indexer.rs`
- Cause: No differential update mechanism - entire file is re-processed
- Improvement path: Implement incremental AST parsing where only changed subtrees are updated

### No Query Result Caching
- Problem: Repeated queries re-scan the database without caching
- Files: `src/graph/query.rs`, `src/graph/ops.rs`
- Cause: No caching layer in CodeGraph API
- Improvement path: Add LRU cache for frequently queried symbols and references

### Synchronous Indexing During Watch
- Problem: File indexing blocks event processing during watch mode
- Files: `src/watcher.rs`, `src/watch_cmd.rs`
- Cause: Single-threaded event loop
- Improvement path: Move indexing to worker thread pool with bounded queue

### Large Database Export Can Be Slow
- Problem: `export` command loads entire graph into memory before serializing
- Files: `src/graph/export.rs`
- Cause: Single-shot JSON serialization of all nodes/edges
- Improvement path: Implement streaming JSON/JSONL export with bounded memory usage

## Fragile Areas

### Database Schema Compatibility
- Files: `src/graph/db_compat.rs` (418 LOC)
- Why fragile: Hand-rolled schema version checking. Changes to sqlitegraph upstream could break compatibility detection.
- Safe modification: Always update expected schema version constants when upgrading sqlitegraph dependency. Run `tests/phase1_persistence_compatibility.rs` after any changes.
- Test coverage: Good - dedicated compatibility test suite exists

### Symbol Name Mapping
- Files: `src/graph/query.rs:190-201`
- Why fragile: Name-to-ID mapping using `HashMap` can lose information when duplicate names exist.
- Safe modification: Replace with `HashMap<String, Vec<i64>>` to track all symbol IDs for each name.
- Test coverage: Minimal - no tests for name collision scenarios

### File Hash Computation
- Files: `src/graph/files.rs`
- Why fragile: SHA-256 hash is computed on entire file content. Large files or encoding issues could cause problems.
- Safe modification: Consider incremental hashing or streaming for large files.
- Test coverage: Basic hash computation tests exist (`src/graph/tests.rs:6-22`)

### Tree-Sitter Parser Initialization
- Files: `src/references.rs:194`, `src/references.rs:207`, `src/references.rs:232`
- Why fragile: Parser created via `.expect()` on `new()`. Failure mode is panic.
- Safe modification: Change to `?` propagation or handle gracefully.
- Test coverage: Test-only usage of `.expect()` acceptable, but production code should avoid.

## Scaling Limits

### Database Size
- Current capacity: Unknown, no documented limits
- Limit: SQLite practical limit is ~140 TB, but single-query performance degrades with large datasets
- Scaling path: Implement sharding or per-module databases for monorepos

### In-Memory Graph Loading
- Current capacity: Entire graph loaded into `CodeGraph` struct
- Limit: Available RAM
- Scaling path: Move to cursor-based access pattern that doesn't require full graph in memory

### Watch Mode File Count
- Current capacity: Limited by inotify limits (typically ~8192 watches per user)
- Limit: OS-level file descriptor limits
- Scaling path: Document how to increase inotify limits; implement recursive watch optimization

### Multi-Language Parser State
- Current capacity: One parser instance per language per extractor
- Limit: Each parser holds compiled grammar in memory (~1-5 MB each)
- Scaling path: Shared parser instances or parser pooling

## Dependencies at Risk

### sqlitegraph (v1.0.0)
- Risk: Hard dependency on specific schema version. Upstream schema changes require Magellan schema version bump.
- Impact: Database compatibility breaks on sqlitegraph version mismatch
- Migration plan: Use `src/graph/db_compat.rs` version checking. Document upgrade path for users.

### tree-sitter-* Grammar Versions (v0.21)
- Risk: Grammar updates could change AST structure, breaking symbol extraction
- Impact: Missing symbols or incorrect positions after grammar update
- Migration plan: Pin grammar versions in Cargo.lock. Run full test suite on grammar updates.

### notify (v7.0)
- Risk: File watcher API changes could break watch mode
- Impact: Watch mode fails to detect file changes
- Migration plan: Minimal surface area usage. Abstraction via `src/watcher.rs` provides isolation.

## Missing Critical Features

### Delete Propagation
- Problem: When a file is deleted, references to its symbols are not cleaned up
- Files: `src/watch_cmd.rs`, `src/graph/files.rs`
- Blocks: Accurate reference counts, stale reference detection
- Status: Addressed in Phase 2 of roadmap (`02-01-PLAN.md` - delete_file_facts)

### Include/Exclude Rules
- Problem: No way to specify patterns to ignore during indexing (tests, build directories, vendor)
- Files: `src/indexer.rs`, `src/watch_cmd.rs`
- Blocks: Efficient indexing of real-world projects
- Status: Planned for Phase 2 (`02-03-PLAN.md`)

### JSON Output Mode
- Problem: CLI output is human-only, no machine-readable option
- Files: `src/main.rs`, all command modules
- Blocks: Scripting, LLM integration, automated workflows
- Status: Planned for Phase 3 (CLI Output Contract)

### Execution Tracking
- Problem: No `execution_id` to correlate runs or operations
- Files: None - not implemented
- Blocks: Diff/compare operations, operation replayability
- Status: Planned for Phase 5 (Stable Identity + Execution Tracking)

## Test Coverage Gaps

### Name Collision Resolution
- What's not tested: Multiple symbols with same name in different files
- Files: `src/graph/query.rs`, `src/references.rs`
- Risk: Silent incorrect results
- Priority: High

### Cross-File References (Currently Broken)
- What's not tested: Working cross-file resolution (because feature doesn't exist)
- Files: `src/references.rs`, `src/graph/references.rs`
- Risk: N/A - feature gap, not a gap in testing of existing functionality
- Priority: N/A - covered by Phase 2 implementation

### Error Paths in Watcher
- What's not tested: Watcher behavior when filesystem errors occur
- Files: `src/watcher.rs`
- Risk: Silent failures, watch mode stops working
- Priority: Medium

### Large File Handling
- What's not tested: Files >10k LOC, non-ASCII encodings, unusual syntax
- Files: All ingest modules
- Risk: Parse failures or incorrect results on edge cases
- Priority: Low

### Database Corruption Recovery
- What's not tested: Behavior when database is corrupted or partially written
- Files: `src/graph/db_compat.rs`
- Risk: Unclear error messages, data loss
- Priority: Medium

### Concurrent Access
- What's not tested: Multiple Magellan instances accessing same database
- Files: All database access
- Risk: Database locking, corruption
- Priority: Low (documented as single-user tool)

### Empty Return Values
- What's not tested: Commands that return empty results (no matches found)
- Files: `src/find_cmd.rs`, `src/refs_cmd.rs`, `src/query_cmd.rs`
- Risk: Poor UX when queries return nothing
- Priority: Low

---

*Concerns audit: 2026-01-19*

# Codebase Concerns

**Analysis Date:** 2026-01-19

## Tech Debt

**Symbol Name Collisions:**
- Issue: `symbol_name_to_id` HashMap in `src/graph/query.rs::index_references()` keeps first occurrence
- Files: `src/graph/query.rs:263-282`
- Impact: Cross-file references may target wrong symbol when names collide
- Fix approach: Use fully-qualified names (fqn) as key, or implement disambiguation logic

**Legacy Single-Event Watcher API:**
- Issue: `FileSystemWatcher` has deprecated `try_recv_event()` and `recv_event()` methods
- Files: `src/watcher.rs:136-261`
- Impact: Code complexity, maintenance burden
- Fix approach: Remove after confirming no external consumers

**Incomplete FQN (Fully Qualified Names):**
- Issue: `SymbolFact.fqn` set to simple name instead of proper hierarchical name
- Files: `src/ingest/mod.rs:188`, `src/graph/query.rs:152`
- Impact: symbol_id generation may not be truly unique for nested symbols
- Fix approach: Build proper FQN from AST traversal (module::struct::method)

## Known Bugs

**None documented:**
- No active bug trackers found in codebase
- Issues tracked via GitHub (repository level)

## Security Considerations

**Path Traversal:**
- Risk: Malicious file paths could cause unintended file access
- Files: `src/graph/filter.rs:239-243` (relative_path computation)
- Current mitigation: Path stripping via `strip_prefix()`
- Recommendations: Validate paths don't escape root directory

**Database File Location:**
- Risk: Writing .db files in watched directory could cause feedback loop
- Files: `src/watcher.rs:339-351` (is_database_file check)
- Current mitigation: Database files excluded from watching
- Recommendations: Document requirement to place db outside watched dir

## Performance Bottlenecks

**Full Symbol Scan for References:**
- Problem: `index_references()` scans all symbols in database
- Files: `src/graph/query.rs:254-288`
- Cause: Cross-file reference resolution requires symbol map
- Improvement path: Cache symbol map, incremental updates

**Linear Search in reconcile_file_path:**
- Problem: `find_file_node()` does HashMap lookup but `find_or_create_file_node()` may still iterate
- Files: `src/graph/files.rs` (not directly read, inferred from usage)
- Cause: File index rebuild on open
- Improvement path: Persist file index to database

## Fragile Areas

**Symbol ID Generation:**
- Files: `src/graph/symbols.rs` (generate_symbol_id function)
- Why fragile: Depends on FQN being correct; currently using simple name
- Safe modification: Update FQN extraction first, then symbol_id generation
- Test coverage: `tests/` has symbol tests but FQN correctness not explicitly tested

**SCIP Export:**
- Files: `src/graph/export/scip.rs`
- Why fragile: Complex mapping from Magellan graph to SCIP format
- Safe modification: Add round-trip tests (export -> parse -> verify)
- Test coverage: Unit tests present (`test_to_scip_*`), but no integration tests

**Delete Operations:**
- Files: `src/graph/ops.rs:delete_file_facts()`
- Why fragile: Must delete from multiple tables (symbols, references, calls, chunks, edges)
- Safe modification: Always use `delete_file_facts()` as authoritative path
- Test coverage: Orphan detection tests in `src/graph/validation.rs`

## Scaling Limits

**File Count:**
- Current capacity: Tested with small to medium projects
- Limit: SQLite scaling (typically millions of rows)
- Scaling path: Partitioning, sharding for very large codebases

**Symbol Count:**
- Current capacity: Thousands of symbols per project
- Limit: In-memory HashMap sizes
- Scaling path: Streaming queries, pagination

## Dependencies at Risk

**sqlitegraph v1.0.0:**
- Risk: External crate with custom API
- Impact: Breaking changes would require significant refactoring
- Migration plan: Abstract graph operations behind trait, version compatibility checks in `src/graph/db_compat.rs`

**tree-sitter grammars:**
- Risk: Grammar updates may break parsing
- Impact: Symbol extraction could fail or produce incorrect results
- Migration plan: Pin grammar versions, test with known code samples

## Missing Critical Features

**Incremental Reference Indexing:**
- Problem: References re-indexed from all symbols on every file change
- Blocks: Efficient watch mode for large projects
- Fix approach: Cache symbol-to-ID mapping, update incrementally

**Cross-File Call Resolution:**
- Problem: Call indexing uses symbol name matching (first match wins)
- Blocks: Accurate call graphs for projects with name collisions
- Fix approach: Use FQN or symbol_id for disambiguation

## Test Coverage Gaps

**SCIP Export Round-Trips:**
- What's not tested: Exported SCIP files parsed and verified
- Files: `src/graph/export/scip.rs`
- Risk: Export format errors undetected
- Priority: Medium (SCIP is new feature)

**FQN Correctness:**
- What's not tested: Fully-qualified names are hierarchical
- Files: `src/ingest/mod.rs`
- Risk: symbol_id collisions for nested symbols
- Priority: High (affects stable identity)

**Cross-File Reference Accuracy:**
- What's not tested: References target correct symbol across files
- Files: `src/graph/query.rs`, `src/graph/references.rs`
- Risk: Incorrect reference resolution in multi-file projects
- Priority: Medium (current implementation uses first-match)

**Watcher Event Ordering:**
- What's not tested: Events processed in deterministic order
- Files: `src/indexer.rs`, `src/watcher.rs`
- Risk: Non-deterministic behavior in concurrent scenarios
- Priority: Low (BTreeSet ensures sorting)

---
*Concerns audit: 2026-01-19*

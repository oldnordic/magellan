# Codebase Concerns

**Analysis Date:** 2026-02-10

## Tech Debt

**Native-V2 Migration Incompleteness:**
- Issue: Native-V2 backend exists but dependent tools (llmgrep, Mirage, splice) are not updated
- Files: Multiple tools in separate repositories need migration
- Impact: Performance benefits (10-100x faster symbol lookup, 70%+ smaller databases) are not accessible to users
- Fix approach: Follow the migration plan in docs/NATIVE-V2-MIGRATION.md, starting with llmgrep

**Backend-Dependent Algorithm Implementation:**
- Issue: Algorithms module uses unsafe downcasting for SQLite backend when native-v2 is enabled
- Files: `src/graph/algorithms.rs:92-100`
- Impact: Type safety violations, creates brittleness when switching backends
- Fix approach: Implement GraphBackend trait methods for algorithms or create proper abstraction layer

**Feature Flag Inconsistencies:**
- Issue: Native-V2 features are conditionally compiled but not consistently used throughout codebase
- Files: `src/kv/mod.rs` has #[cfg(feature = "native-v2")] but other modules don't check
- Impact: Code paths may be incorrect, runtime errors possible when features are mixed
- Fix approach: Audit all native-v2 code and ensure proper feature gating

**SQLite Backend Hardcoded Paths:**
- Issue: Many functions assume SQLite backend and use sqlitegraph::SqliteGraphBackend directly
- Files: `src/graph/algorithms.rs`, `src/graph/ops.rs`
- Impact: Prevents proper backend abstraction, violates dependency inversion
- Fix approach: Use GraphBackend trait consistently everywhere

## Known Bugs

**Reference Extraction Bug in Tests:**
- Issue: `src/graph/tests.rs:63` - Reference extraction from file2 doesn't work correctly
- Symptoms: Tests fail to find expected references across files
- Files: `src/graph/tests.rs`
- Trigger: When running graph reference tests
- Workaround: Manual verification of references

**AST Indexing Inconsistencies:**
- Issue: Missing file_id column causes issues with AST node re-indexing
- Symptoms: Old if_expression nodes not properly updated
- Files: `src/graph/ast_tests.rs:164`
- Trigger: Re-indexing files with conditional expressions
- Workaround: Manual database cleanup

**WAL Buffer Flush Issues:**
- Issue: KV indexing requires manual WAL flush after population
- Symptoms: Data not visible to other processes immediately
- Files: `src/kv/mod.rs:145`
- Trigger: Concurrent access to same database
- Workaround: Manual flush call (already implemented but shows underlying issue)

## Security Considerations

**File Path Traversal Risks:**
- Risk: Malicious file paths could escape project root
- Files: `src/watcher/mod.rs:551-562`
- Current mitigation: Path validation checks in watcher
- Recommendations: Add comprehensive path canonicalization and sandboxing

**Debug Information Exposure:**
- Risk: Debug eprintln! statements leak internal state
- Files: Multiple files (src/kv/mod.rs, src/graph/ops.rs, etc.)
- Current mitigation: None
- Recommendations: Replace with proper logging framework or conditional compilation

**Symbol Index Injection:**
- Risk: FQN-based lookups could be vulnerable to crafted names
- Files: `src/kv/mod.rs` symbol indexing
- Current mitigation: Input sanitization
- Recommendations: Additional validation of FQN components

## Performance Bottlenecks

**Large CLI Module:**
- Problem: `src/cli.rs` (2306 lines) handles all commands
- Files: `src/cli.rs`
- Cause: Monolithic implementation
- Improvement path: Split into command-specific modules

**Inefficient Symbol Lookup without KV:**
- Problem: SQLite backend does full table scans for symbol lookups
- Files: `src/graph/query.rs`
- Cause: Missing indexes on FQN column
- Improvement path: Ensure proper indexes exist for SQLite backend

**Memory Usage in Ingest Module:**
- Problem: Large file parsing loads entire source into memory
- Files: `src/ingest/mod.rs`, various language parsers
- Cause: No streaming or lazy loading
- Improvement path: Implement incremental parsing for large files

## Fragile Areas

**Algorithms Module Backend Abstraction:**
- Files: `src/graph/algorithms.rs:92-100`
- Why fragile: Uses unsafe downcasting to specific backend types
- Safe modification: Add proper trait methods to GraphBackend
- Test coverage: Currently has backend-specific tests

**Symbol Index Population Logic:**
- Files: `src/kv/mod.rs:109-146`
- Why fragile: Complex KV operations with many failure points
- Safe modification: Extract into smaller, testable functions
- Test coverage: Limited integration tests for KV operations

**Watcher Event Handling:**
- Files: `src/watcher/mod.rs`
- Why fragile: Complex state management with multiple concurrent operations
- Safe modification: Simplify state machine, add robust error handling
- Test coverage: Has pubsub tests but limited stress testing

## Scaling Limits

**Concurrent Indexing Operations:**
- Current capacity: Limited by single-threaded indexing in `src/graph/ops.rs`
- Limit: Global lock on graph operations
- Scaling path: Implement parallel indexing per file with merge strategy

**Large File Processing:**
- Current capacity: Single file processing
- Limit: Memory usage grows with file size
- Scaling path: Streaming parser for multi-MB files

## Dependencies at Risk

**sqlitegraph Version Dependency:**
- Risk: Heavy dependency on sqlitegraph implementation details
- Impact: Version updates may break code
- Migration plan: Abstract over specific versions, implement compatibility layer

**Tree-sitter Parsers:**
- Risk: Multiple language-specific parsers may become outdated
- Impact: Support for new language features delayed
- Migration plan: Implement plugin architecture for parsers

## Missing Critical Features

**Transaction Rollback Support:**
- Problem: No rollback mechanism for failed indexing operations
- Blocks: Safe atomic operations
- Priority: High for data integrity

**Incremental Indexing for Large Codebases:**
- Problem: Full re-indexing required for any change
- Blocks: Performance on large repositories
- Priority: Medium for scalability

**Database Schema Migration Tool:**
- Problem: Manual migration between versions
- Blocks: Upgrades without data loss
- Priority: Medium for maintainability

## Test Coverage Gaps

**Native-V2 Backend Testing:**
- What's not tested: Most features only tested with SQLite backend
- Files: Core indexing, query, and algorithm modules
- Risk: Native-V2 regressions may go undetected
- Priority: High - native-v2 is the future direction

**Concurrent Access Patterns:**
- What's not tested: Real-world concurrent usage scenarios
- Files: Watcher and indexing operations
- Risk: Data races under load
- Priority: High for reliability

**Error Path Testing:**
- What's not tested: Database corruption, disk full, out of memory
- Files: All modules with I/O operations
- Risk: System may fail catastrophically under stress
- Priority: Medium for robustness

---

*Concerns audit: 2026-02-10*
```
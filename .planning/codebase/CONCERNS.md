# Codebase Concerns

**Analysis Date:** 2026-02-08

## Tech Debt

**Cross-file reference indexing:**
- Issue: Only same-file references are indexed, cross-file references are not properly created
- Files: `src/graph/tests.rs:63-75`
- Impact: Query and find commands miss inter-file relationships, limiting tool's effectiveness
- Fix approach: Implement proper cross-file reference tracking in the indexing pipeline

**Missing caller/callee reference tracking:**
- Issue: Query and find commands have disabled caller/callee tracking
- Files: `src/query_cmd.rs:45-46`, `src/find_cmd.rs:130-131`
- Impact: Users cannot see call relationships between symbols
- Fix approach: Implement reference tracking for callers/callees in query results

**AST node storage not integrated:**
- Issue: Symbol Identity v1.5 includes AST node storage but it's not integrated
- Files: `src/graph/symbols.rs:97`
- Impact: Cannot store and retrieve AST nodes directly in KV store
- Fix approach: Integrate AST node storage with KV backend implementation

**SQLite-specific code in generic trait:**
- Issue: Labels are SQLite-specific but hardcoded in generic GraphBackend trait
- Files: `src/graph/symbols.rs:256`
- Impact: Limits portability to other backends
- Fix approach: Move labels functionality to backend-specific implementations or add trait methods

**ASM-based CFG extraction not implemented:**
- Issue: Optional ASM analyzer integration is marked TODO
- Files: `src/graph/bytecode_cfg.rs:64`
- Impact: Java bytecode CFG extraction uses slower AST-based approach
- Fix approach: Implement ASM-based CFG extraction for Java when bytecode-cfg feature is enabled

## Known Bugs

**Cross-file reference bug:**
- Symptoms: References between files are not indexed, queries miss inter-file relationships
- Files: `src/graph/tests.rs:63-75`
- Trigger: Indexing code in multiple files
- Workaround: Manual file merging or single-file analysis

**Missing file_id column affects re-indexing:**
- Symptoms: Old AST nodes persist after re-indexing due to missing file_id column
- Files: `src/graph/ast_tests.rs:164`
- Trigger: Re-indexing files that have been modified
- Workaround: Manual database cleanup

## Performance Considerations

**Large main.rs file:**
- Problem: 2874 lines in single file makes it hard to maintain
- Files: `src/main.rs`
- Cause: All command logic in one file
- Improvement path: Split into smaller modules, extract command handlers

**Multiple unwrap() calls in error paths:**
- Problem: 64+ unwrap() calls across codebase can cause panics
- Files: Multiple files including `src/ingest/pool.rs`, `src/graph/algorithms.rs`
- Cause: Insufficient error handling in complex operations
- Improvement path: Replace with proper Result handling and custom error types

**Heavy dependency on CLI parsing:**
- Problem: Multiple CLI libraries could impact startup time
- Files: `src/output/command.rs`
- Cause: Complex command line argument handling
- Improvement path: Simplify CLI structure, reduce dependencies

## Fragile Areas

**Validation module:**
- Files: `src/validation.rs`
- Why fragile: Heavy use of unwrap() and temp file operations
- Safe modification: Replace with proper error handling, add more comprehensive tests
- Test coverage: Good test coverage but panic points exist

**CFG extraction:**
- Files: `src/graph/cfg_extractor.rs`
- Why fragile: Complex AST traversal logic, platform-specific code for clang
- Safe modification: Add more unit tests, isolate platform-specific code
- Test coverage: Comprehensive test coverage for CFG patterns

**Memory management:**
- Files: `src/ingest/pool.rs`, `src/generation/mod.rs`
- Why fragile: Rc/Ref usage without clear ownership patterns
- Safe modification: Use Arc/Mutex for shared state, implement proper cleanup
- Test coverage: Limited testing for concurrent access patterns

## Scaling Limits

**SQLite database size:**
- Current capacity: Limited by SQLite WAL file handling
- Limit: Performance degrades with large codebases (>1M LOC)
- Scaling path: Implement native V2 backend for better performance

**Indexing speed:**
- Current capacity: Depends on file count and complexity
- Limit: Large codebases take significant time to index
- Scaling path: Parallelize indexing, implement incremental updates

## Dependencies at Risk

**SQLiteGraph version dependency:**
- Risk: Hardcoded to version 1.5.3, may lag behind updates
- Impact: Potential security vulnerabilities or missing features
- Migration plan: Implement version range or abstract version dependency

**Tree-sitter grammars:**
- Risk: Grammars may become outdated
- Impact: Parsing errors for newer language features
- Migration plan: Update grammars regularly, test with modern code

## Missing Critical Features

**Incremental indexing:**
- Problem: Full re-indexing required for every change
- Blocks: Real-time analysis for large codebases
- Implementation needed: Change detection, partial re-indexing

**Export format diversity:**
- Problem: Limited export formats beyond SCIP and CSV
- Blocks: Integration with other tools
- Implementation needed: Additional format support (JSON, GraphML, etc.)

## Test Coverage Gaps

**Error handling paths:**
- What's not tested: Error cases in indexing, database failures
- Files: `src/graph/`, `src/ingest/`
- Risk: Silent failures or crashes under error conditions
- Priority: High

**Configuration validation:**
- What's not tested: Invalid CLI arguments, malformed config files
- Files: `src/validation.rs`
- Risk: Poor user experience with invalid input
- Priority: Medium

**Integration testing:**
- What's not tested: End-to-end workflows with multiple commands
- Files: Various integration test modules
- Risk: Commands fail in combination but work individually
- Priority: Medium

*Concerns audit: 2026-02-08*
```
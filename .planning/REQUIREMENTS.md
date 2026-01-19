# Requirements: Magellan v1.1

**Defined:** 2026-01-19
**Core Value:** Produce correct, deterministic symbol + reference + call graph data from real codebases, continuously, without stopping on bad files.

## v1.1 Requirements

### FQN Correctness

- [ ] **FQN-01**: Extract fully-qualified names (FQN) during tree-sitter traversal for all indexed languages
- [ ] **FQN-02**: Switch symbol lookup maps from simple name → symbol_id to FQN → symbol_id
- [ ] **FQN-03**: Implement per-language scope tracking (Rust `::`, Python `.`, Java `.`)
- [ ] **FQN-04**: symbol_id generation uses hash(language, FQN, span_id) - never simple names
- [ ] **FQN-05**: Treat simple names as display-only for user-facing output
- [ ] **FQN-06**: Emit warnings when FQN collisions are detected

### Path Traversal Security

- [ ] **PATH-01**: Implement path canonicalization before validation for all file access
- [ ] **PATH-02**: Create `validate_path_within_root()` function that rejects paths escaping project root
- [ ] **PATH-03**: Add tests for traversal attempts (`../`, `..\\`, symlinks, UNC paths)
- [ ] **PATH-04**: Integrate path validation into watcher.rs event filtering
- [ ] **PATH-05**: Integrate path validation into scan.rs directory walking
- [ ] **PATH-06**: Handle cross-platform path differences (Windows backslash, macOS case-insensitivity)

### Delete Operations Safety

- [ ] **DELETE-01**: Wrap `delete_file_facts()` in explicit rusqlite IMMEDIATE transaction
- [ ] **DELETE-02**: Add row-count assertions to verify all derived data is deleted
- [ ] **DELETE-03**: Implement error injection tests for transaction rollback verification
- [ ] **DELETE-04**: Add invariant test: delete file → no dangling edges (orphan detection)

### SCIP Testing

- [ ] **SCIP-01**: Export SCIP then parse to verify format correctness
- [ ] **SCIP-02**: Add at least one integration test using scip crate parser

### Documentation

- [ ] **DOC-01**: Document .db location recommendations (place outside watched directory)
- [ ] **DOC-02**: Update user documentation with security best practices

## v1.2 Requirements (Deferred)

### Performance & Caching

- **PERF-01**: Implement sqlitegraph caching for reference indexing (deferred to v1.2)
- **PERF-02**: Persist file index to avoid rebuilding (deferred to v1.2)

### Cross-File Accuracy

- **XREF-01**: Cross-file reference accuracy tests (deferred to v1.2)

### Gitignore Support

- **GIT-01**: Nested .gitignore file support (deferred to v1.2)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Semantic analysis / type checking | Magellan is "facts only" by design |
| LSP server / IDE language features | CLI-only tool |
| Async runtime / background thread pools | Keep deterministic + simple |
| Multi-root workspaces | v2 feature |
| LSIF export | Deprecated in favor of SCIP |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| PATH-01 | Phase 10 | Pending |
| PATH-02 | Phase 10 | Pending |
| PATH-03 | Phase 10 | Pending |
| PATH-04 | Phase 10 | Pending |
| PATH-05 | Phase 10 | Pending |
| PATH-06 | Phase 10 | Pending |
| FQN-01 | Phase 11 | Pending |
| FQN-02 | Phase 11 | Pending |
| FQN-03 | Phase 11 | Pending |
| FQN-04 | Phase 11 | Pending |
| FQN-05 | Phase 11 | Pending |
| FQN-06 | Phase 11 | Pending |
| DELETE-01 | Phase 12 | Pending |
| DELETE-02 | Phase 12 | Pending |
| DELETE-03 | Phase 12 | Pending |
| DELETE-04 | Phase 12 | Pending |
| SCIP-01 | Phase 13 | Pending |
| SCIP-02 | Phase 13 | Pending |
| DOC-01 | Phase 13 | Pending |
| DOC-02 | Phase 13 | Pending |

**Coverage:**
- v1.1 requirements: 20 total
- Mapped to phases: 20
- Unmapped: 0 ✓

---
*Requirements defined: 2026-01-19*
*Last updated: 2026-01-19 after v1.1 roadmap creation*

# Project Research Summary

**Project:** Magellan
**Domain:** Deterministic, local-first codebase mapping CLI (watcher + tree-sitter AST fact extraction + SQLite-backed graph + JSON/NDJSON outputs)
**Milestone:** v1.1 - Correctness + Safety
**Researched:** 2026-01-19
**Confidence:** HIGH

## Executive Summary

Magellan is a deterministic code graph indexer that combines tree-sitter parsing, SQLite graph persistence, and file watching to produce code intelligence data. The v1.1 milestone focuses on three correctness and safety improvements: (1) fully-qualified names (FQN) for unique symbol identification, (2) path traversal validation to prevent security vulnerabilities, and (3) transactional delete safety for graph integrity.

The research strongly recommends an implementation order: path validation first (security baseline), then FQN extraction and transactional deletes in parallel. Path validation is independent and blocks all other work from a safety perspective. FQN extraction requires scope-aware AST traversal per language but involves no schema changes (the `fqn` field already exists in `SymbolFact`). Transactional deletes follow an established pattern from `generation/mod.rs` and are isolated to `delete_file_facts()`.

The main risks are well-documented and avoidable: FQN changes will invalidate all existing symbol_ids (requiring full re-index), incomplete scope tracking can still cause collisions across language-specific edge cases, and path validation must handle symlinks correctly. The mitigation strategy includes comprehensive testing with nested symbol fixtures, malicious path tests, and transaction rollback injection.

## Key Findings

### Recommended Stack

Magellan v1.1 builds on the existing stack (Rust 2021, tree-sitter, sqlitegraph, rusqlite) with targeted additions for correctness. No major framework changes are required.

**Core technologies (locked, no change):**
- Rust 2021: Single static CLI binary
- tree-sitter 0.22: CST parsing with byte spans
- sqlitegraph 1.0.0: Graph persistence layer
- rusqlite 0.31.0: SQLite access
- sha2 0.10: Hashing for symbol_id generation

**v1.1 additions:**
- camino 1.2.2: UTF-8 path handling for cross-platform determinism
- path-security 0.1.0: Path traversal validation (actively maintained, Oct 2025)
- indexmap 2.7.0: Ordered HashMap for (fqn, file_id) composite keys (already in use)

**Transaction patterns (no new dependencies):**
- rusqlite::TransactionBehavior::Immediate: Prevents deadlocks in concurrent scenarios
- Pattern already established in `src/generation/mod.rs:110-138`

### Expected Features

The v1.1 milestone focuses on three foundational correctness features. For broader feature context (table stakes, differentiators, anti-features), see the existing v1.0 SUMMARY.md.

**Must have (v1.1 blockers):**
- FQN extraction: Scope-aware symbol naming (e.g., `crate::module::Struct::method`)
- Path validation: Canonicalization-before-validation for all file access
- Transactional deletes: Atomic all-or-nothing deletion of file-derived data

**Should have (v1.1 important):**
- FQN collision detection: Warnings when two symbols would have same FQN
- Path canonicalization: Store canonical UTF-8 paths in File nodes
- Delete verification: Optional orphan detection for testing

**Can defer (v1.2+):**
- SCIP export format: Full SCIP serialization (deferred; FQN structure prepared)
- Soft canonicalization: For paths that don't exist yet
- Advanced disambiguation: SCIP-style disambiguator field for overloads

### Architecture Approach

Magellan v1.1 requires minimal architectural changes. The three features integrate cleanly with existing module boundaries and follow established patterns.

**Major components (unchanged):**
1. CLI/Commands — Stable flags, exit codes, stdout/stderr discipline
2. Indexing orchestrator — Schedules discovery, parse, extract, normalize, persist
3. Watcher + debouncer — File events to reindex jobs (path validation added)
4. Ingest/language adapters — Extract symbols/refs/calls (scope tracking added)
5. Graph persistence (SQLite) — Stores facts/edges (transaction wrapping added)

**v1.1 additions:**
- `src/validation.rs` (new): Centralized path validation utilities
- `src/ingest/scope.rs` (optional): Scope tracking for FQN computation

**Critical architectural finding:** The `fqn` field already exists in `SymbolFact` and `symbol_id` generation already uses it. The "FQN-as-key refactor" is about populating FQN correctly during tree-sitter traversal, not changing the schema.

### Critical Pitfalls

The v1.1 milestone specifically addresses three pitfalls documented in PITFALLS.md:

1. **Incomplete FQN construction** — Using simple names causes symbol_id collisions and cross-file mislinks. Avoid by implementing hierarchical AST traversal with language-specific scope tracking (mod, impl, class, namespace).

2. **Path traversal vulnerabilities** — Without validation, malicious input can access files outside workspace (CVE-2025-68705 demonstrates active threat). Avoid by canonicalizing paths, verifying they start with project root, and handling symlinks explicitly.

3. **SQLite transaction misuse** — Multi-step deletes without transactions leave orphaned records on error. Avoid by wrapping `delete_file_facts` in IMMEDIATE transactions using the pattern from `generation/mod.rs`.

For comprehensive pitfall coverage (span bugs, non-ASCII panics, determinism issues, watcher event storms), see PITFALLS.md.

## Implications for Roadmap

Based on research, suggested phase structure for v1.1:

### Phase 1: Path Traversal Validation

**Rationale:** Security-critical baseline. Must be implemented first to ensure all subsequent operations are safe. No dependencies on other features.

**Delivers:**
- `src/validation.rs` module with path validation utilities
- Updated `watcher.rs` to validate incoming event paths
- Updated `scan.rs` to validate paths during directory walk
- Tests for traversal attempts (`../etc/passwd`), symlinks, UNC paths

**Addresses:** Path traversal security (Pitfall 12)

**Avoids:** CVE-2025-68705 class vulnerabilities

### Phase 2: FQN Extraction

**Rationale:** Correctness foundation. FQN changes invalidate existing symbol_ids, so this should be completed early in the milestone. No dependencies on other features.

**Delivers:**
- `ScopeStack` struct in `src/ingest/mod.rs` for tracking nesting during `walk_tree()`
- Language-specific scope tracking in `src/ingest/{rust,python,java,etc}.rs`
- FQN format: `crate::module::item::name` (Rust), `module.Class.method` (Python)
- Data migration plan: full re-index of all files

**Addresses:** FQN collisions (Pitfall 11)

**Avoids:** Symbol ID collisions, wrong cross-file links

### Phase 3: Transactional Deletes

**Rationale:** Data integrity. Isolated change following an established pattern. No dependencies on other features.

**Delivers:**
- Wrapped `delete_file_facts()` in rusqlite IMMEDIATE transaction
- Error injection tests for rollback verification
- Optional orphan detection for test mode

**Addresses:** Orphaned data on partial failures (Pitfall 14)

**Avoids:** Inconsistent database states, SQLITE_BUSY errors

### Phase Ordering Rationale

- Path validation first: Security baseline for all operations. No dependencies.
- FQN extraction and transactional deletes can proceed in parallel: Independent features with no coupling.
- FQN before FQN-dependent features: Future phases (ambiguity reporting, query UX) depend on stable FQN.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (FQN scope tracking):** Each language has unique scoping rules (Rust traits, Python classes, Java packages). Needs per-language implementation and extensive testing.
- **Phase 2 (Data migration):** Changing symbol_id breaks all existing references. Migration plan and re-index strategy required.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Path validation):** Established security patterns; `path-security` crate provides clear API.
- **Phase 3 (Transactional deletes):** Pattern already exists in `generation/mod.rs:110-138`.

## sqlitegraph API Notes

### Key Constraints

1. **NOT thread-safe for concurrent writes** — Use MVCC snapshots for concurrent reads: `graph.snapshot()`
2. **No native high-level caching API** — Need to build caching layer manually if needed
3. **Labels/Properties query functions not exported** — Use raw SQL helpers (see `SQLITEGRAPH_API_GUIDE.md`)

### v1.1 Relevant APIs

```rust
// For rebuilding indexes (FileOps pattern in src/graph/files.rs:119-140)
pub fn entity_ids(&self) -> Result<Vec<i64>, SqliteGraphError>;
pub fn get_node(&self, id: i64) -> Result<GraphEntity, SqliteGraphError>;

// Indexed queries (uses graph_labels and graph_properties indexes)
add_label(graph, node_id, "rust")?;
add_property(graph, node_id, "fqn", "crate::module::foo")?;
```

### Transaction Access Pattern

```rust
// Use chunks.connect() to get rusqlite connection for explicit transaction control
let conn = graph.chunks.connect()?;
let tx = conn.unchecked_transaction()?;
// ... operations ...
tx.commit()?;
```

### Indexes Available

```sql
CREATE INDEX idx_labels_label ON graph_labels(label);
CREATE INDEX idx_labels_label_entity_id ON graph_labels(label, entity_id);
CREATE INDEX idx_props_key_value ON graph_properties(key, value);
CREATE INDEX idx_props_key_value_entity_id ON graph_properties(key, value, entity_id);
CREATE INDEX idx_entities_kind_id ON graph_entities(kind, id);
```

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Concrete crate versions; official docs verified; aligns with Magellan constraints |
| Features | HIGH | Strong consensus patterns (SCIP, Rust security); existing Magellan codebase analyzed |
| Architecture | HIGH | Module boundaries verified from source; minimal changes required |
| Pitfalls | HIGH | Failure modes well-documented; CVE references for path traversal; internal audit notes |

**Overall confidence:** HIGH

### Gaps to Address

- **Per-language FQN edge cases:** Anonymous namespaces, closures, trait impls, generics — handle explicitly during implementation
- **Symlink behavior policy:** Decide whether to reject symlinks or follow-then-validate
- **Cross-platform path testing:** Plan to test on Linux, macOS, and Windows
- **Data migration timing:** Coordinate symbol_id change with user communication

## Sources

### Primary (HIGH confidence)
- Tree-sitter official docs — Node::id uniqueness limits, Point 0-based coords
- SQLite official docs — Transaction behavior, WAL mode, DEFERRED/IMMEDIATE/EXCLUSIVE
- sqlitegraph source — `/home/feanor/Projects/sqlitegraph/`
- Magellan source code — Verified existing patterns in `src/ingest/mod.rs`, `src/graph/symbols.rs`, `src/generation/mod.rs`

### Secondary (MEDIUM confidence)
- Sourcegraph SCIP Protocol — Symbol grammar and descriptor format
- Rust security best practices 2025 — Path traversal patterns
- camino crate documentation — UTF-8 path normalization
- path-security crate — Path traversal validation (updated Oct 2025)

### Tertiary (LOW confidence)
- Academic research on FQN resolution — arXiv/ACM papers on AI-based FQN inference (not directly applicable)

---
*Research completed: 2026-01-19*
*Ready for roadmap: yes*

# Feature Research: Magellan v1.1 (Correctness + Safety)

**Domain:** Deterministic codebase mapping / code graph indexing tools (local developer CLI)
**Milestone:** v1.1 - FQN-as-key refactor, path traversal validation, transactional deletes
**Researched:** 2026-01-19
**Overall confidence:** MEDIUM

## Executive Summary

Magellan v1.1 focuses on three core correctness and safety areas:
1. **Fully-Qualified Names (FQN)** - Transitioning from simple symbol names to proper FQN to eliminate symbol collisions
2. **Path Traversal Security** - Adding validation to prevent directory escape attacks in file watching
3. **Transactional Delete Safety** - Ensuring graph integrity when deleting files and derived data

Research into SCIP protocol, Rust security patterns, and SQLite graph integrity reveals established patterns for each area. The key finding is that SCIP's descriptor grammar provides a production-ready FQN format, and SQLite's foreign key cascade mechanism is the standard approach for graph integrity.

## Key Findings

**FQN:** Sourcegraph SCIP protocol defines a standardized symbol grammar with descriptors (scheme/package/descriptor list) that form fully-qualified names. SCIP is the de facto standard for code intelligence interchange.

**Path Security:** Rust path traversal vulnerabilities (CVE-2025-68705 in RustFS) demonstrate the need for canonicalization-before-validation. Recommended crates include `path-security` (updated Oct 2025) and `soft-canonicalize` (Dec 2025).

**Transactional Deletes:** SQLite's `ON DELETE CASCADE` is the documented pattern for referential integrity. Complex graph structures require careful handling of cascade paths to avoid cycles (per SQLite forum discussions).

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features expected in a code indexing tool. Missing = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Unique symbol identifiers** | Users expect "find references" to return the correct symbol, not multiple symbols with the same name | HIGH | FQN eliminates collisions between identical names in different scopes/modules |
| **Hierarchical symbol names** | All modern code intelligence tools use fully-qualified names (SCIP, LSP, compiler internals) | MEDIUM | Format: `package/module/Type.method` or equivalent language-specific syntax |
| **Deterministic FQN generation** | Same code must produce same FQN across runs for ID stability | MEDIUM | Requires consistent AST traversal and canonical path handling |
| **Path canonicalization** | Watchers must handle symlinks, relative paths, `./` prefixes correctly | LOW-MEDIUM | `std::fs::canonicalize()` is standard but requires file existence |
| **Path boundary validation** | Tools watching filesystems must not escape project root | HIGH | Security requirement; prevents reading arbitrary files |
| **Orphan-free deletion** | Deleting a file must not leave dangling references/calls in graph | MEDIUM | Requires cascade delete or explicit edge cleanup |
| **Atomic reindexing** | File updates must replace all old data with new data atomically | MEDIUM | Prevents partial states where old and new data coexist |

### Differentiators (Competitive Advantage)

Features that set Magellan apart. Not required, but valuable.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **SCIP-compatible FQN format** | Enables interoperability with Sourcegraph ecosystem; export-ready | MEDIUM | SCIP descriptor grammar is well-specified; adopting it positions Magellan for future SCIP export |
| **Zero-trust path validation** | Works even when files don't exist (pre-indexing validation) | MEDIUM | `soft-canonicalize` crate provides canonicalization without file existence requirement |
| **Deterministic delete ordering** | Delete operations are reproducible and auditable | LOW | Sort entity IDs before deletion; enables debugging and testing |
| **Explicit cascade semantics** | Clear documentation of what gets deleted when | LOW | Unlike implicit cascade, Magellan documents exact delete behavior |
| **Cross-file symbol disambiguation** | Correctly resolve symbols across modules/languages | HIGH | FQN enables unambiguous cross-file references |

### Anti-Features (Commonly Requested, But Problematic)

Features that seem good but create problems.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Simple name-based symbol lookup** | Easier to implement, works for small projects | Collisions on common names (`main`, `run`, `handle`) cause incorrect results | Use FQN as primary key; expose simple names only as display property |
| **Ad-hoc path string manipulation** | Fast to implement, no dependencies | Prone to security bugs; fails on edge cases (symlinks, case-insensitive FS) | Use `camino` for typed UTF-8 paths; `dunce` for Windows compatibility |
| **Manual edge cleanup on delete** | Seems more controllable than cascade | Easy to forget edge types; creates orphans over time | Use SQLite foreign keys with CASCADE; or explicit cleanup via `delete_edges_touching_entities` |
| **Re-parse entire project on delete** | Conceptually simple; guaranteed correctness | Eliminates incremental update benefits; slow on large repos | Delete only derived data from deleted file; reconcile remaining references |
| **Heuristic symbol disambiguation** | Avoids implementing full FQN | Non-deterministic; changes based on indexing order | Require explicit FQN; fail parsing when FQN cannot be determined |

---

## Feature Dependencies

```
[Path canonicalization + validation]
    requires--> [Robust path handling crate]
    requires--> [Project root boundary enforcement]

[FQN extraction from AST]
    requires--> [Language-specific scope traversal]
    requires--> [SCIP-compatible symbol grammar]
    enables--> [Unique symbol IDs]
    enables--> [Cross-file reference resolution]

[Transactional delete safety]
    requires--> [Entity ID tracking]
    requires--> [Edge-to-entity mapping]
    requires--> [Deterministic delete ordering]
    enables--> [Orphan-free graph]
    enables--> [Correct incremental updates]
```

---

## Detailed Findings by Area

### 1. FQN (Fully-Qualified Names) in Code Indexing Tools

#### How Other Tools Handle FQN

**Sourcegraph SCIP Protocol** (HIGH confidence - official spec)
- **Format:** `scheme/package/descriptor+` where descriptors form a hierarchy
- **Symbol Grammar:** Each descriptor has `name`, `disambiguator`, and `suffix` (Namespace/Type/Method/Parameter/etc.)
- **Example:** `rust/crate mypackage/MyType#myMethod(+1).`
- **Key Property:** Descriptors together form a fully-qualified name that uniquely identifies the symbol across the package
- **Source:** [scip.proto - Symbol descriptor grammar](https://github.com/sourcegraph/scip/blob/main/scip.proto)

**Language Server Protocol (LSP)**
- Uses string-based identifiers for symbols
- Format is language-server specific; no standardized FQN format
- Relies on language server implementations for disambiguation

**ripgrep**
- Does not handle FQN; operates on text patterns, not symbols
- Not relevant for FQN requirements (different domain)

**GitHub CLI (gh)**
- Primarily works with repository-level operations
- Symbol-level operations delegate to other tools (codesearch, etc.)

#### SCIP Symbol Grammar (Authoritative)

From the SCIP protocol buffer specification:

```
symbol ::= scheme ' ' package ' ' descriptor+
descriptor ::= name '.' suffix | name disambiguator suffix
suffix ::= Namespace | Type | Method | Parameter | Macro | Meta | Local
```

**Key elements:**
- `scheme`: Language identifier (e.g., "rust", "python", "typescript")
- `package`: Manager/name/version tuple (e.g., "cargo mycrate 1.0.0")
- `descriptors`: Ordered list from root to symbol, each with a suffix

**Example FQNs:**
- Rust: `rust/crate mycrate/0.1.0/MyStruct#myMethod(+1).`
- Python: `python/pypi myproject/1.0.0/mymodule/MyClass.my_method`
- TypeScript: `typescript/npm package/1.0.0/MyClass.myMethod`

#### Current Magellan State (v1.0)

**SymbolFact structure:**
```rust
pub struct SymbolFact {
    pub name: Option<String>,        // Simple symbol name only
    pub fqn: Option<String>,         // Currently set to name (v1 compatibility)
    // ... other fields
}
```

**Problem:** Multiple symbols with same name in different scopes collide:
- `crate_a::utils::parse`
- `crate_b::utils::parse`
- Both have `fqn = "parse"` -> same symbol_id -> incorrect merge

**Symbol ID generation:**
```rust
pub fn generate_symbol_id(language: &str, fqn: &str, span_id: &str) -> String {
    // SHA256(language:fqn:span_id)[0..16]
}
```

With simple-name FQN, this produces collisions for same-named symbols.

#### Recommended Approach for v1.1

**Option A: SCIP-Compatible FQN**
- Adopt SCIP descriptor grammar format
- Example: `rust/local magellan//mod_a/parse#().`
- Pros: Export-ready, interoperable, well-specified
- Cons: Complex string parsing, slightly verbose

**Option B: Language-Specific FQN Simplified**
- Use `::` (Rust), `.` (Python/TS/Java) as separators
- Example: `magellan::mod_a::parse`
- Pros: Familiar to developers, simpler parsing
- Cons: Not directly SCIP-exportable, ambiguous across languages

**Recommendation:** Option B for v1.1 (simpler, faster to implement)
- Maintain backward compatibility by using `fqn` field
- Add `scip_symbol` field for future SCIP compatibility
- Format: `{crate}::{module_path}::{name}` for Rust
- Format: `{package}.{module}.{name}` for Python/TS

#### Table Stakes for FQN

| Feature | Required | Notes |
|---------|----------|-------|
| Scope-aware symbol naming | YES | Must distinguish `a::foo` from `b::foo` |
| Module path tracking | YES | Parser must capture full module ancestry |
| Disambiguation for overloads | OPTIONAL | SCIP uses `disambiguator` field; v1.1 can use (file, byte_start) tuple |
| Language-specific syntax | YES | Rust `::`, Python `.`, Java `.`, etc. |
| SCIP-compatible export | DEFER | Prepare data structure but defer SCIP serialization |

### 2. Path Traversal Security in File-Watching Tools

#### Security Context

**Recent Vulnerabilities:**
- **RustFS Path Traversal (GHSA-pq29-69jg-9mxc, CVE-2025-68705):** Unauthorized file access through RPC endpoints due to insufficient path validation
- **Attack vector:** Paths containing `../` or symlinks to escape project root

**OWASP Definition:**
- Path Traversal: "Improper limitation of a pathname to a restricted directory ('path traversal')"
- Occurs when user input controls file paths without proper validation

#### Best Practices (HIGH confidence - 2025 sources)

**1. Canonicalize Before Validate**
```rust
// WRONG: validate before canonicalizing
if !path.starts_with(&root) { return Err(...); }  // Bypassable with ../
let canonical = std::fs::canonicalize(&path)?;

// RIGHT: canonicalize first, then validate
let canonical = std::fs::canonicalize(&path)?;
if !canonical.starts_with(&root_canonical) { return Err(...); }
```

**2. Use Dedicated Crates (2025 updates)**
- **`path-security`** (crates.io, updated Oct 2025): Comprehensive path validation
- **`soft-canonicalize`** (Dec 2025): Canonicalization without file existence requirement
- **`dunce`**: Removes Windows UNC prefix (`\\?\`) safely

**3. Boundary Enforcement**
- Always validate against canonicalized project root
- Reject paths containing `..` components after canonicalization
- Handle symlinks by resolving them before validation

#### Current Magellan State

**watcher.rs (lines 305-351):**
```rust
fn extract_dirty_paths(events: &[DebouncedEvent]) -> BTreeSet<PathBuf> {
    let mut dirty_paths = BTreeSet::new();
    for event in events {
        let path = &event.path;
        // No path validation - just filtering by file type
        if path.is_dir() { continue; }
        if is_database_file(...) { continue; }
        dirty_paths.insert(path.clone());
    }
}
```

**Problem:** No validation that paths are within project root.

**ops.rs (lines 247-334):**
```rust
pub fn reconcile_file_path(graph: &mut CodeGraph, path: &Path, path_key: &str) -> Result<ReconcileOutcome> {
    // No path validation before filesystem access
    if !path.exists() { ... }  // Could escape with symlink
    let source = fs::read(path)?;  // Unchecked read
}
```

**Problem:** No boundary checking before `fs::read()`.

#### Recommended Approach for v1.1

**Phase 1: Add Path Validation Module**
```rust
// graph/path_validation.rs
pub struct PathValidator {
    project_root: PathBuf,
    canonical_root: PathBuf,
}

impl PathValidator {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let canonical_root = std::fs::canonicalize(&project_root)
            .map_err(|e| anyhow!("Failed to canonicalize project root: {}", e))?;
        Ok(Self { project_root, canonical_root })
    }

    pub fn validate(&self, path: &Path) -> Result<PathBuf> {
        let canonical = std::fs::canonicalize(path)
            .map_err(|e| anyhow!("Failed to canonicalize path: {}", e))?;

        if !canonical.starts_with(&self.canonical_root) {
            bail!("Path '{}' escapes project root", path.display());
        }

        Ok(canonical)
    }
}
```

**Phase 2: Integrate into Watcher and Reconcile**
- Validate all paths in `WatcherBatch` before processing
- Validate in `reconcile_file_path` before filesystem access
- Fail fast with clear error message on boundary violation

#### Table Stakes for Path Security

| Feature | Required | Notes |
|---------|----------|-------|
| Canonicalize-before-validate | YES | Core security pattern |
| Project root enforcement | YES | Prevent directory escape |
| Symlink resolution | YES | Follow symlinks then validate against root |
| Windows path handling | YES | Use `dunce` for UNC compatibility |
| Non-existent file handling | OPTIONAL | `soft-canonicalize` for paths that don't exist yet |

### 3. Delete Operation Safety in Graph Databases

#### SQLite Graph Integrity Patterns

**Foreign Key Cascade (HIGH confidence - official SQLite docs)**
- `ON DELETE CASCADE`: Automatically deletes child rows when parent is deleted
- `ON DELETE SET NULL`: Sets foreign key to NULL when parent deleted
- Critical for referential integrity in graph structures

**From SQLite Foreign Key Support:**
> "CASCADE actions propagate delete operations from parent keys to dependent child keys"

**Graph-Specific Challenges (SQLite forum, 2025):**
- Cascade paths can create cycles
- Multiple cascade paths to same entity can cause conflicts
- For code graphs: File -> Symbol -> Reference -> File creates potential cycles

#### Current Magellan State

**schema.rs (lines 73-115):**
```rust
pub fn delete_edges_touching_entities(
    conn: &rusqlite::Connection,
    entity_ids_sorted: &[i64],
) -> Result<usize> {
    // Build IN placeholders and delete edges where from_id OR to_id matches
    // Determinism: IDs must be pre-sorted by caller
}
```

**ops.rs (lines 167-245):**
```rust
pub fn delete_file_facts(graph: &mut CodeGraph, path: &str) -> Result<()> {
    // 1) Delete symbols (File -> DEFINES -> Symbol)
    // 2) Delete references in file
    // 3) Delete calls in file
    // 4) Delete code chunks
    // 5) Delete File node
    // 6) Explicit edge cleanup for deleted IDs
}
```

**Current Issues:**
1. No transaction wrapping multiple deletes
2. Manual edge cleanup (relies on `delete_entity` internal cascade)
3. No explicit foreign key constraints defined in schema

#### Recommended Approach for v1.1

**Phase 1: Transaction Wrapping**
```rust
pub fn delete_file_facts(graph: &mut CodeGraph, path: &str) -> Result<()> {
    let conn = graph.chunks.connect()?;
    let tx = conn.unchecked_transaction()?;

    // All delete operations within transaction
    // ...

    tx.commit()?;
}
```

**Phase 2: Explicit Entity Collection + Sorted Delete**
```rust
// Current approach already does this (GOOD)
let mut deleted_entity_ids: Vec<i64> = Vec::new();
// ... collect all IDs to delete
deleted_entity_ids.sort_unstable();  // Deterministic ordering
for id in &deleted_entity_ids {
    graph.backend.graph().delete_entity(*id)?;
}
```

**Phase 3: Edge Cleanup Verification**
```rust
// After all deletes, verify no orphan edges remain
let orphans = detect_orphan_edges(&conn)?;
if !orphans.is_empty() {
    eprintln!("Warning: {} orphan edges detected after delete", orphans.len());
}
```

#### Deterministic Delete Ordering

**Why it matters:**
- Reproducible database states
- Testable delete behavior
- Debuggable when things go wrong

**Current status:** GOOD - `sort_unstable()` is already used in `delete_file_facts()`

**Recommendation:** Keep and document this behavior as a contract.

#### Table Stakes for Delete Safety

| Feature | Required | Notes |
|---------|----------|-------|
| Transaction wrapping | YES | Atomic all-or-nothing deletes |
| Cascade edge cleanup | YES | No orphan edges after delete |
| Deterministic ordering | YES | Sort IDs before delete |
| Delete all derived data | YES | Symbols, references, calls, chunks |
| Verification mode | OPTIONAL | Assert no orphans in tests |

---

## MVP Recommendation for v1.1

### Must Have (Blocker for Correctness)

1. **FQN Extraction**
   - Implement scope-aware naming in each parser
   - Use language-specific separators (`::`, `.`)
   - Store in existing `fqn` field

2. **Path Validation**
   - Create `PathValidator` module
   - Validate all paths before filesystem access
   - Fail on boundary violation

3. **Transactional Deletes**
   - Wrap delete operations in transactions
   - Verify no orphan edges after delete
   - Keep deterministic ordering

### Should Have (Important for Safety)

1. **FQN Collision Detection**
   - Emit warning when two symbols would have same FQN
   - Include file location in warning

2. **Path Canonicalization**
   - Use `dunce` for Windows compatibility
   - Store canonical paths in File nodes

3. **Delete Verification**
   - Add optional verification mode for testing
   - Count and report orphaned edges

### Can Defer (v1.2+)

1. **SCIP Export Format**
   - Prepare data structure but defer full SCIP serialization
   - Add `scip_symbol` field to SymbolNode

2. **Soft Canonicalization**
   - For validation of paths that don't exist yet
   - Use `soft-canonicalize` crate

3. **Advanced Disambiguation**
   - SCIP-style `disambiguator` field for overloads
   - Use `(file, byte_start)` tuple for now

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| FQN extraction (scope-aware naming) | HIGH | MEDIUM | P1 |
| Path validation (boundary enforcement) | HIGH | LOW-MEDIUM | P1 |
| Transactional deletes | HIGH | LOW | P1 |
| FQN collision detection | MEDIUM | LOW | P2 |
| Windows path compatibility | MEDIUM | LOW | P2 |
| Delete verification mode | MEDIUM | LOW | P2 |
| SCIP export format | MEDIUM | HIGH | P3 |
| Soft canonicalization | LOW | MEDIUM | P3 |

---

## Competitor Analysis

| Feature | SCIP | ripgrep | gh | Magellan v1.0 | Magellan v1.1 (planned) |
|---------|------|---------|-----|---------------|------------------------|
| FQN support | Full symbol grammar | N/A (text search) | Delegates to servers | Simple name only | Language-specific FQN |
| Path validation | Assumes trusted indexer | User-controlled | Server-controlled | None | Project-root boundary |
| Transactional deletes | N/A (immutable index) | N/A | Database-dependent | Partial (no explicit tx) | Full transactional |
| SCIP compatibility | Native | N/A | N/A | None | FQN-compatible (defer export) |

---

## Sources

### FQN and SCIP
- [Sourcegraph SCIP Protocol Buffer Definition](https://github.com/sourcegraph/scip/blob/main/scip.proto) - Official SCIP spec with symbol grammar (HIGH confidence)
- [SCIP - a better code indexing format than LSIF](https://sourcegraph.com/blog/announcing-scip) - SCIP announcement blog
- [Sourcegraph SCIP GitHub Repository](https://github.com/sourcegraph/scip) - Main SCIP project

### Path Security
- [RustFS Path Traversal Vulnerability (GHSA-pq29-69jg-9mxc)](https://github.com/rustfs/rustfs/security/advisories/GHSA-pq29-69jg-9mxc) - Recent CVE (MEDIUM confidence)
- [Rust Path Traversal Guide: Example and Prevention](https://www.stackhawk.com/blog/rust-path-traversal-guide-example-and-prevention/) - Prevention strategies (MEDIUM confidence)
- [path-security crate (crates.io)](https://crates.io/crates/path-security) - Updated Oct 2025 (MEDIUM confidence)
- [soft-canonicalize crate (crates.io)](https://crates.io/crates/soft-canonicalize) - Dec 2025 release (LOW-MEDIUM confidence)
- [OWASP Path Traversal](https://owasp.org/www-community/attacks/Path_Traversal) - OWASP definition

### SQLite Graph Integrity
- [SQLite Foreign Key Support](https://sqlite.org/foreignkeys.html) - Official SQLite docs (HIGH confidence)
- [How Primary and Foreign Keys Enhance Database Integrity](https://chat2db.ai/resources/blog/primary-and-foreign-keys) - Apr 2025 (LOW-MEDIUM confidence)
- [Avoiding Data Anomalies with SQLite Foreign Keys](https://moldstud.com/articles/a-avoiding-data-anomalies-in-sqlite-best-practices-for-foreign-key-implementation) - Aug 2025 (LOW-MEDIUM confidence)
- [Introducing FOREIGN KEY constraint may cause cycles](https://sqlite.org/forum/info/8db9d3d17a127bf081bb0bfbc0ea51bdcdee5d696cd376e93f80c9af6d005daa) - SQLite forum on cascade cycles (MEDIUM confidence)

### General Code Intelligence
- [CodePathFinder MCP Tools Reference](https://codepathfinder.dev/docs/mcp/tools-reference) - MCP-based code intelligence (2026) (LOW confidence)
- [KCL New Semantic Model](https://www.kcl-lang.io/blog/2023-12-09-kcl-new-semantic-model) - FQN indexing example (LOW confidence)

---

## Open Questions

1. **FQN Format Standardization**
   - Should we adopt SCIP descriptor grammar now or defer?
   - Recommendation: Defer full SCIP; use simplified language-specific format

2. **Transaction Granularity**
   - Should batch operations (watcher updates) use single transaction or per-file?
   - Recommendation: Per-file transaction for now; batch transactions in v1.2

3. **Path Validation Failure Mode**
   - Should invalid paths be silently skipped or cause hard failure?
   - Recommendation: Hard failure with clear error message (security-critical)

4. **Delete Verification in Production**
   - Should orphan detection run on every delete or only in debug/test?
   - Recommendation: Test-only by default; opt-in for production

---
*Feature research for Magellan v1.1 (Correctness + Safety milestone)*
*Researched: 2026-01-19*

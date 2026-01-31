# Symbol Identity Design: Stable IDs and Explicit Ambiguity

**Date:** 2026-01-22
**Status:** Design Proposal
**Author:** Internal Discussion

---

## Abstract

This document proposes a fundamental shift in how Magellan identifies and tracks symbols: from name-based FQN (Fully Qualified Name) to stable, hash-based Symbol IDs with explicit ambiguity handling.

**Current state:** FQN collisions occur at 3-5% rate in large codebases, causing false positives in cross-file resolution.
**Proposed state:** Unambiguous symbol identity with visible, not silent, ambiguity.

---

## Part 1: The Problem Analysis

### 1.1 Current FQN Implementation

Magellan currently constructs FQNs like:
```
ChunkStore::new
CodeChunk::new
backend::hip::cache::get_or_init_cache
tests::test_byte_spans_within_bounds
```

The extraction logic (simplified):
1. Start with symbol name
2. Add enclosing scopes (impl block, module, function)
3. Join with scope separators (`::`)

### 1.2 Why Collisions Occur

**Root cause:** The FQN is derived from *semantic* context that doesn't always include *disambiguating* context.

| Collision Type | Example | Why It Happens |
|----------------|---------|----------------|
| `main` functions | 5 duplicates in ROCmForge | No crate/module prefix in binary entry points |
| Test functions | `tests::test_*` duplicates | Test modules flattened across files |
| Impl methods | `MatMulKernel::dequantize` | Same struct name in different impl blocks |
| Generic helpers | `get_or_init_cache` | Common pattern names without module path |

### 1.3 The Impact

**ROCmForge analysis (3,144 symbols):**
- Unique FQNs: 2,994
- Colliding symbols: 150 (4.8%)
- These 150 symbols represent **silent data corruption** in:
  - Cross-file call resolution (wrong edges)
  - Find operations (multiple candidates, one returned)
  - Refactoring workflows (wrong symbol targeted)

**Magellan self-analysis (895 symbols):**
- 18 unique FQNs with collisions
- 44 total duplicate entries
- Mostly test functions (expected) but also `new`, `default`, etc.

### 1.4 Why "Better FQN Extraction" Won't Fix It

Attempting to improve FQN extraction alone faces fundamental issues:

1. **Rust macros** - Expanded code doesn't map cleanly to source paths
2. **Conditional compilation** - `#[cfg(test)]`, `#[cfg(feature)]` create parallel symbol universes
3. **Generics** - `impl<T> Trait for Type` - how to qualify?
4. **Re-exports** - Symbol defined in one place, used from another
5. **Macro-generated code** - No stable source path to extract

Even perfect FQN extraction would hit edge cases. The problem is **philosophical**, not technical:

> **We're trying to derive identity from names, but identity should come from *what something is*, not *what it's called*.**

---

## Part 2: The Proposed Solution

### 2.1 Stable Symbol ID

Every symbol gets a **hash-based identifier** that incorporates all disambiguating context:

```
SymbolId = hash(
    crate_name +           // e.g., "magellan"
    file_path +            // e.g., "src/generation/mod.rs"
    enclosing_items +      // e.g., "impl ChunkStore"
    symbol_kind +          // e.g., "Function"
    symbol_name            // e.g., "new"
)
```

**Properties:**
- **Deterministic:** Same input → same hash
- **Collision-resistant:** SHA-256 makes accidental collision virtually impossible
- **Stable across refactors:** Code movement doesn't change identity if semantic context preserved
- **File-aware:** Path is baked in, eliminating file-level collisions

### 2.2 Display FQN vs Canonical FQN

**Canonical FQN** (internal, full identity):
```
magellan::src/generation/mod.rs::impl ChunkStore::Function new
```

**Display FQN** (user-facing, shortened):
```
ChunkStore::new
```

**Database stores both:**
```json
{
  "symbol_id": "a3f2e8b9c4d1...",    // Stable ID
  "canonical_fqn": "magellan::src/generation/mod.rs::impl ChunkStore::Function new",
  "display_fqn": "ChunkStore::new",
  "name": "new",
  "kind": "Function",
  "file_path": "/path/to/src/generation/mod.rs"
}
```

### 2.3 File Path as First-Class Metadata

File path is not a workaround—it's **essential disambiguating context**.

```rust
pub struct Symbol {
    pub id: SymbolId,           // Primary key
    pub name: String,            // Short name
    pub kind: SymbolKind,        // Function, Method, etc.
    pub canonical_fqn: String,   // Full identity
    pub display_fqn: String,     // Human-readable
    pub crate_name: String,      // "magellan"
    pub module_path: String,    // "generation::mod"
    pub file_path: String,      // Full path to source file
    pub span: Span,              // Byte/line/column range
}
```

**Why file path must be included:**

1. **Same struct name in different files** → Different symbols
2. **Test modules** → File is the only distinguishing context
3. **Generated code** → No stable module hierarchy
4. **Multiple binaries** → `main` distinguished by file

### 2.4 Explicit Ambiguity Modeling

Instead of **hiding** collisions (current behavior), we **model** them explicitly in the graph:

```
(DisplayName: "get_or_init_cache") ──alias_of──> (SymbolId_A: cache.rs::get_or_init_cache)
                                        │
                                        └──alias_of──> (SymbolId_B: kernel.rs::get_or_init_cache)
```

**Benefits:**
- Ambiguity is **visible**, not silent
- Tools can present candidates for human/LLM selection
- Refactors require explicit SymbolId specification
- Queries can rank/filter by file, module, usage frequency

---

## Part 3: Integration with Existing Architecture

### 3.1 sqlitegraph Compatibility

**Current schema uses:**
- `graph_entities` table with `kind`, `name`, `file_path`, `data` JSON

**Proposed additions:**
```sql
ALTER TABLE graph_entities ADD COLUMN symbol_id TEXT UNIQUE;
ALTER TABLE graph_entities ADD COLUMN canonical_fqn TEXT;
ALTER TABLE graph_entities ADD COLUMN display_fqn TEXT;
```

**Migration strategy:**
1. Add columns (nullable initially)
2. Backfill existing symbols on next re-index
3. Make `symbol_id` UNIQUE constraint
4. Future symbols require symbol_id

### 3.2 Native V2 Backend Synergies

The proposal mentions Native V2 should be leveraged:

**1. SymbolId as node identity:**
- Current: Node identified by auto-increment ID
- Proposed: Node identified by SymbolId (hash)
- Collisions disappear at graph level

**2. Ambiguity as graph structure:**
- DisplayName becomes a node type
- `alias_of` edges connect DisplayName → SymbolId
- Traversals resolve candidates

**3. Native V2 for resolution paths:**
- "Find all call sites of this name in this module" becomes graph traversal
- "Disambiguate by nearest enclosing scope" becomes path query
- "Restrict to file/crate/test" becomes subgraph filtering

**4. Scalability for future features:**
- Confidence edges (ML/LLM-assisted disambiguation)
- Frequency edges (usage-based ranking)
- Refactor safety edges (pre/post conditions)

### 3.3 Separation of Concerns

| Concern | Technology | Role |
|----------|------------|------|
| Symbol identity & semantics | Magellan | Defines what symbols *are* |
| Graph relationships & truth | sqlitegraph | Stores how symbols *relate* |
| Performance & traversal | Native V2 | Enables *scaling* |

This is clean separation, not duplication.

---

## Part 4: Migration Strategy

### 4.1 Non-Breaking API Change

**Current API:**
```rust
magellan find --name get_or_init_cache
```

**Proposed API (backward compatible):**
```rust
# Old behavior (returns first match)
magellan find --name get_or_init_cache

# New behavior (explicit)
magellan find --name get_or_init_cache --ambiguous
# Returns multiple candidates with SymbolIds

# New behavior (precise)
magellan find --symbol-id a3f2e8b9c4d1...
# Exact match, no ambiguity
```

### 4.2 Database Migration

**Phase 1: Add columns (non-breaking)**
```sql
ALTER TABLE graph_entities ADD COLUMN symbol_id TEXT;
ALTER TABLE graph_entities ADD COLUMN canonical_fqn TEXT;
ALTER TABLE graph_entities ADD COLUMN display_fqn TEXT;
```

**Phase 2: Backfill on re-index**
- Existing databases work as-is
- Next `magellan watch --scan-initial` populates new fields
- No downtime, no forced migration

**Phase 3: Make symbol_id required**
- New symbols require symbol_id
- Queries can filter by `symbol_id IS NOT NULL`
- Gradual rollout

### 4.3 Code Changes Required

**Modules affected:**
1. `src/ingest/*.rs` - All language parsers must emit SymbolId
2. `src/graph/symbols.rs` - Symbol insertion logic
3. `src/graph/query.rs` - Query by SymbolId
4. `src/find_cmd.rs` - Find with ambiguity handling
5. `src/refs_cmd.rs` - References by SymbolId
6. `src/export.rs` - Include SymbolId in exports

**Estimated effort:** 2-3 phases, ~15-20 plans

---

## Part 5: Open Questions and Decisions Needed

### 5.1 Hash Algorithm

**Question:** Which hash function for SymbolId?

**Options:**
- SHA-256 (crypto, overkill but standard)
- BLAKE3 (fast, designed for this use case)
- xxHash (very fast, non-cryptographic)

**Recommendation:** BLAKE3
- Designed for hashing structured data
- Faster than SHA-256
- Built into Rust via `blake3` crate

### 5.2 SymbolId Format

**Question:** Hex string vs bytes vs base64?

**Options:**
- Hex: `a3f2e8b9c4d1...` (64 chars, readable)
- Base64: `w/PjDi5Q/w==...` (shorter, opaque)
- UUID v5: `3f2e8b9c-4d1a-...` (standard format)

**Recommendation:** Hex string, truncated to 16 chars
- Readable in logs/DB
- Collision still astronomically unlikely
- Easy to work with in CLIs

### 5.3 Backfill Strategy

**Question:** How to handle existing databases?

**Options:**
1. **Flag day:** Everyone re-indexes
2. **Gradual:** New databases use SymbolId, old work as-is
3. **Migration tool:** Offline script to backfill SymbolIds

**Recommendation:** Gradual with migration tool
- No forced re-index for users
- Migration tool available for those who want it
- New databases automatically use SymbolId

### 5.4 Display FQN Generation

**Question:** How to generate human-readable Display FQN?

**Options:**
1. Strip crate path: `magellan::src::foo::bar` → `foo::bar`
2. Heuristic shortening: Show only "relevant" parts
3. User-configurable format string

**Recommendation:** Option 1 with fallback to name
- Simple, deterministic
- Falls back to `name` if FQN is too long
- Example: `backend::hip::cache::get_or_init_cache`

### 5.5 Ambiguity in Exports

**Question:** How does SCIP export handle this?

**Current SCIP:** Uses FQNs, would have same collision issue

**Options:**
1. Keep current SCIP (has same limitation)
2. Extend SCIP with custom fields for SymbolId
3. Note limitation in documentation

**Recommendation:** Option 3 for now, Option 2 later
- SCIP is a standard, changing it has costs
- Document the limitation
- Future work: custom symbol vocabularies

---

## Part 6: Trade-offs and Alternatives

### 6.1 Alternative: File-Relative IDs

**Idea:** Instead of global SymbolId, use `(file_path, symbol_name)` pairs.

**Pros:**
- Simpler, no hashing
- Naturally file-scoped
- Easy to understand

**Cons:**
- Requires composite key everywhere in API
- Doesn't solve cross-file reference ambiguity
- Harder to use in CLIs

**Verdict:** SymbolId is better for global reference resolution.

### 6.2 Alternative: Improve FQN Extraction Only

**Idea:** Fix the FQN extraction to include more context.

**Pros:**
- Smaller change
- Maintains current mental model

**Cons:**
- Still vulnerable to edge cases (macros, cfgs, generics)
- Doesn't make ambiguity explicit
- Doesn't provide stable identity for refactoring

**Verdict:** Worth doing as **Display FQN** improvement, but not as sole solution.

### 6.3 Alternative: Usage-Based Disambiguation

**Idea:** Use cross-reference frequency to pick the "most likely" symbol.

**Pros:**
- Works automatically
- Better UX for simple cases

**Cons:**
- **Silent failure mode** - dangerous for refactoring
- Doesn't scale (long tail of rarely-used symbols)
- Requires existing usage data

**Verdict:** Use as **ranking** signal, not as identity.

---

## Part 7: Implementation Phases (Proposal)

### Phase 1: Foundation (Database + Types)
- Add `SymbolId` type
- Add database columns
- Implement hash function
- Add tests

### Phase 2: Parser Updates
- Modify all language parsers to emit SymbolId
- Compute CanonicalFQN and DisplayFQN
- Update symbol metadata structure

### Phase 3: Query Updates
- Add `--symbol-id` flag to find/refs
- Add `--ambiguous` flag to show all candidates
- Update cross-file resolution to use SymbolId

### Phase 4: Export Updates
- Include SymbolId in JSON exports
- Update SCIP export notes
- Document new export format

### Phase 5: Tooling Updates
- Migration tool for existing databases
- Documentation updates
- Performance testing with Native V2

**Estimated timeline:** 5 phases, ~25 plans, ~2-3 weeks of focused work

---

## Part 8: Philosophical Alignment

### 8.1 "Truth-First" Principle

> **"Names become labels. IDs become truth."**

This aligns with Magellan's core value:

> **"Produce correct, deterministic symbol + reference + call graph data"**

Current: Deterministic output, but **ambiguous identity**
Proposed: Deterministic output **with unambiguous identity**

### 8.2 Ecosystem Alignment

| Component | Role | Benefit |
|------------|------|---------|
| **Magellan** | Produces SymbolId + facts | Stable identifiers |
| **Splice** | Patches by SymbolId | Safe refactors |
| **LLMs** | Reason over candidates | No hallucination |
| **sqlitegraph** | Stores identity | Clean relationships |
| **Native V2** | Traverses relationships | Scales |

This creates a **virtuous cycle**:
1. Magellan assigns stable IDs
2. Splice uses IDs for safe edits
3. LLMs reason over multiple candidates
4. sqlitegraph tracks relationships
5. Native V2 enables complex traversals

### 8.3 Ambiguity as Feature, Not Bug

**Current mindset:** "Collisions are bad, we need to eliminate them"

**Proposed mindset:** "Collisions are real, we need to make them visible"

This mirrors how real-world tools work:
- Compilers show "ambiguous reference" errors
- IDEs show multiple candidates
- Humans ask for clarification

---

## Part 9: Risks and Mitigations

### 9.1 Performance Risk

**Risk:** Hash computation on every symbol adds overhead

**Mitigation:**
- BLAKE3 is fast (~1GB/s on modern CPUs)
- Computed once per symbol during indexing
- No per-query overhead

### 9.2 Storage Risk

**Risk:** Larger database with more fields per symbol

**Mitigation:**
- SymbolId is just 16 bytes (hex string)
- CanonicalFQN replaces current FQN (no net change)
- DisplayFQN is new but optional

### 9.3 Compatibility Risk

**Risk:** Breaking changes for existing users

**Mitigation:**
- Gradual migration (no flag day)
- Backward-compatible CLI flags
- Old databases continue to work

### 9.4 Complexity Risk

**Risk:** More complex mental model for users

**Mitigation:**
- DisplayFQN hides complexity for most use cases
- Ambiguity only surfaced when it exists
- SymbolId only needed for precise operations

---

## Part 10: Recommendation

### 10.1 Should We Do This?

**Yes.** The design addresses a fundamental correctness issue while:

1. **Maintaining backward compatibility** - Gradual migration
2. **Improving correctness** - Unambiguous symbol identity
3. **Enabling future features** - Safe refactoring, LLM integration
4. **Aligning with ecosystem** - Works with sqlitegraph, Native V2, Splice

### 10.2 When Should We Start?

**After v1.5** (whatever that contains)

This is a significant change that deserves its own milestone:
- Database schema change
- Multiple parser modifications
- CLI API additions
- Documentation updates

### 10.3 What Should We Do First?

1. **Design spike** - Implement for one language parser (e.g., Rust only)
2. **Measure impact** - Verify performance, check collision resolution
3. **Validate design** - Test with ROCmForge-like real codebase
4. **Get feedback** - Test with users/LLM integrators

---

## Appendix A: Example Data Structures

### A.1 Symbol Entity (Proposed)

```rust
pub struct Symbol {
    // Primary identity
    pub id: SymbolId,           // Stable hash-based ID

    // Names (for display/search)
    pub name: String,            // Short name: "new"
    pub display_fqn: String,    // Human-readable: "ChunkStore::new"
    pub canonical_fqn: String,  // Full identity: "magellan::src/generation/mod.rs::impl ChunkStore::Function new"

    // Classification
    pub kind: SymbolKind,        // Function, Method, Struct, etc.

    // Context
    pub crate_name: String,     // "magellan"
    pub module_path: String,    // "generation::mod"
    pub file_path: String,      // Absolute path to source file

    // Location
    pub span: Span,              // Byte range, line/column
}
```

### A.2 SymbolId Type

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(String);

impl SymbolId {
    pub fn new(
        crate_name: &str,
        file_path: &str,
        enclosing_items: &[&str],
        kind: &str,
        name: &str,
    ) -> Self {
        use blake3::hash;
        let input = format!(
            "{}:{}:{}:{}:{}",
            crate_name,
            file_path,
            enclosing_items.join("::"),
            kind,
            name
        );
        let hash = hash(input.as_bytes());
        Self(hex::encode(hash))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

---

## Appendix B: Example Collision Resolution

### B.1 Before (Current)

```
$ magellan find --name get_or_init_cache
Found "get_or_init_cache":
  File: backend/hip/cache.rs
  Kind: Function

[But which one? There are 3! User doesn't know]
```

### B.2 After (Proposed)

```
$ magellan find --name get_or_init_cache
Ambiguous reference found. 3 candidates:

  1. SymbolId: a3f2e8b9c4d1...
     File: backend/hip/cache.rs:45
     FQN: backend::hip::HipCache::get_or_init_cache

  2. SymbolId: b7g4h2j9k0m5...
     File: kernels/matmul/cache.rs:123
     FQN: kernels::matmul::MatMulCache::get_or_init_cache

  3. SymbolId: c9h6k4l2n1p7...
     File: utils/cache.rs:12
     FQN: utils::Cache::get_or_init_cache

Use --symbol-id <ID> for precise operations.

$ magellan refs --symbol-id a3f2e8b9c4d1... --direction out
Calls from "backend::hip::HipCache::get_or_init_cache":
  To: kernel_launch at backend/hip/kernel.rs:89
  To: validate_cache at backend/hip/validate.rs:23
```

---

## Appendix C: References

- Original issue discussion: FQN collisions in ROCmForge
- sqlitegraph documentation: Graph persistence with explicit node IDs
- Native V2 backend: Performance for graph traversals
- BLAKE3 specification: https://blake3.io

---

**Document Status:** Ready for review
**Next Steps:** Await approval before creating GSD milestone/roadmap

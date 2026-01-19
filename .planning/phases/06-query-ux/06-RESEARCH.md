# Phase 6: Query UX (Definitions, References, Call Graph, File Listing) - Research

**Researched:** 2026-01-19
**Domain:** CLI query commands, JSON output contract, graph query infrastructure
**Confidence:** HIGH (verified against existing codebase)

## Summary

Phase 6 implements query UX improvements to ensure all query commands return span-aware, deterministically-ordered results with stable IDs. Research reveals:

1. **Query commands exist but have gaps**: `query`, `find`, and `refs` commands exist with JSON output, but gaps exist in stable ID propagation, deterministic ordering, and some query types.

2. **symbol_id is available but not consistently propagated**: Phase 5 added `symbol_id` to `SymbolNode` and `SymbolMatch`, but `find` command's JSON output does not include `symbol_id` for glob listings.

3. **Deterministic ordering is inconsistent**: Some queries sort deterministically (file_path, byte_start), but refs command ordering may not be consistent across all result types.

4. **Files command is missing**: The requirements mention `files_cmd.rs`, but no such file exists. File listing exists via `CodeGraph::all_file_nodes()` but has no dedicated CLI command.

5. **JSON output contract is well-defined**: `src/output/command.rs` defines `Span`, `SymbolMatch`, `ReferenceMatch`, and response types with stable identifiers. The contract needs to be fully implemented.

**Primary recommendation:** Enhance existing query commands to consistently include `symbol_id`, ensure deterministic ordering, add missing `files` CLI command, and verify JSON output compliance with Phase 3 contract.

---

## Existing Query Commands

### query (src/query_cmd.rs)

**Status:** EXISTS, needs minor enhancements

**Current capabilities:**
- Lists symbols in a file, optionally filtered by kind and symbol name
- Supports `--file`, `--kind`, `--symbol`, `--show-extent`, `--explain` flags
- JSON output with `QueryResponse` includes `symbol_id`
- Uses `symbol_nodes_in_file_with_ids()` to get symbol_id from graph
- Deterministic ordering: `file_path`, `start_line`, `start_col`, `byte_start`, `name`

**Gaps:**
- None significant - command is well-implemented
- Minor: human mode doesn't show symbol_id (only JSON mode)

**JSON output example:**
```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "data": {
    "symbols": [
      {
        "symbol_id": "...",  // INCLUDED
        "match_id": "...",
        "name": "main",
        "kind": "Function",
        "span": { ... }
      }
    ],
    "file_path": "...",
    "kind_filter": null
  }
}
```

### find (src/find_cmd.rs)

**Status:** EXISTS, needs enhancement for symbol_id propagation

**Current capabilities:**
- Finds symbols by name across all files or in a specific file
- Supports `--name`, `--path`, `--list-glob` flags
- JSON output with `FindResponse`
- Glob pattern matching with `globset`
- Deterministic ordering: `file_path`, `start_line`, `start_col`

**Gaps:**
- **CRITICAL**: Glob listing (`--list-glob`) includes `symbol_id` in JSON output (line 362), but regular find by name does NOT include `symbol_id` (line 289 sets `symbol_id` to `None`)
- This inconsistency means glob returns stable IDs but exact name search doesn't

**Code location of gap:**
```rust
// src/find_cmd.rs:289 - symbol_id set to None for find by name
SymbolMatch::new(s.name, s.kind_normalized, span, None, None)
                                                    ^^^^
                                                    Should be: s.symbol_id

// src/find_cmd.rs:362 - symbol_id included for glob listing
SymbolMatch::new(s.name, s.kind_normalized, span, None, s.symbol_id)
                                                           ^^^^^^^^^^^^
                                                    Already correct here
```

### refs (src/refs_cmd.rs)

**Status:** EXISTS, needs enhancement for stable IDs

**Current capabilities:**
- Shows callers (`--direction in`) or callees (`--direction out`) for a symbol
- Supports `--name`, `--path`, `--direction` flags
- JSON output with `RefsResponse`
- Uses `CallFact` from call graph
- Deterministic ordering: `file_path`, `byte_start` (line 163-167)

**Gaps:**
- **MISSING**: `ReferenceMatch` and `CallFact` do not include stable IDs for the referenced/called symbols
- The `symbol_id` of the referenced symbol is not included in output
- Call results show caller/callee names but not their stable identifiers

**Current JSON output:**
```json
{
  "schema_version": "1.0.0",
  "execution_id": "...",
  "data": {
    "references": [
      {
        "match_id": "...",
        "referenced_symbol": "callee_name",
        "reference_kind": "call",
        "span": { ... }
      }
    ],
    "symbol_name": "...",
    "file_path": "...",
    "direction": "in"
  }
}
```

**Desired enhancement:** Include `symbol_id` for the referenced symbol

### files (DOES NOT EXIST)

**Status:** MISSING - needs to be created

**Current state:**
- No `src/files_cmd.rs` file exists
- `FilesResponse` type exists in `src/output/command.rs` (line 710-716)
- `CodeGraph::all_file_nodes()` method exists to query all files

**Required capabilities:**
- List all indexed files
- Optional: show symbol counts per file
- JSON output with `FilesResponse`
- Deterministic ordering (alphabetical by path)

---

## Graph Query Infrastructure

### Symbol Queries (src/graph/query.rs)

| Method | Returns | Includes symbol_id | Deterministic |
|--------|---------|-------------------|---------------|
| `symbols_in_file()` | `Vec<SymbolFact>` | No | No |
| `symbols_in_file_with_kind()` | `Vec<SymbolFact>` | No | No |
| `symbol_nodes_in_file()` | `Vec<(i64, SymbolFact)>` | No | Yes (line, col, byte) |
| `symbol_nodes_in_file_with_ids()` | `Vec<(i64, SymbolFact, Option<String>)>` | **Yes** | Yes (line, col, byte) |

**Key insight:** `symbol_nodes_in_file_with_ids()` is the correct method for getting symbol_id. It directly accesses SymbolNode data to extract symbol_id.

### Reference Queries (src/graph/references.rs)

| Method | Returns | Includes target IDs |
|--------|---------|-------------------|
| `references_to_symbol()` | `Vec<ReferenceFact>` | No (only name) |
| `index_references()` | `usize` (count) | N/A |

**Gap:** `ReferenceFact` contains only `referenced_symbol: String` (name), not the stable ID of the referenced symbol.

### Call Queries (src/graph/call_ops.rs)

| Method | Returns | Includes IDs |
|--------|---------|-------------|
| `calls_from_symbol()` | `Vec<CallFact>` | No (only names) |
| `callers_of_symbol()` | `Vec<CallFact>` | No (only names) |

**Gap:** `CallFact` contains `caller: String` and `callee: String` (names only), not stable IDs.

---

## Gaps Analysis

### QRY-01: Definition Lookup (by symbol name/kind)

**Requirement:** Provide definition lookup returning span-aware results and stable IDs

**Current status:** PARTIALLY SATISFIED
- `query` command: YES - includes symbol_id
- `find` command by name: NO - symbol_id is None
- `find` command glob listing: YES - includes symbol_id

**Gap:** Find by name doesn't propagate symbol_id

**Fix required:**
```rust
// src/find_cmd.rs:289
// Change:
SymbolMatch::new(s.name, s.kind_normalized, span, None, None)
// To:
SymbolMatch::new(s.name, s.kind_normalized, span, None, s.symbol_id)
```

### QRY-02: Reference Lookup (by symbol)

**Requirement:** Provide reference lookup returning span-aware results and stable IDs

**Current status:** NOT SATISFIED
- `refs` command exists but doesn't include stable IDs for referenced symbols
- `ReferenceFact` only has name, not ID

**Gap:** No mechanism to get symbol_id of referenced symbol

**Fix required:**
1. Add optional `target_symbol_id` field to `ReferenceMatch`
2. Update `ReferenceOps::references_to_symbol()` to return both span and target ID
3. Update refs command to include target ID in JSON output

### QRY-03: Callers/Callees Queries

**Requirement:** Provide callers/callees with stable IDs and deterministic ordering

**Current status:** PARTIALLY SATISFIED
- Deterministic ordering: YES (file_path, byte_start)
- Stable IDs: NO - only names are returned

**Gap:** `CallFact` doesn't include symbol_id for caller/callee

**Fix required:**
1. Extend `CallFact` to include `caller_symbol_id` and `callee_symbol_id`
2. Update call indexing to store these IDs
3. Update call query output to include IDs

### QRY-04: File/Module Listing

**Requirement:** Provide file/module listing with symbols and counts

**Current status:** NOT SATISFIED
- `FilesResponse` type exists but no CLI command
- `CodeGraph::all_file_nodes()` exists but returns only FileNode metadata

**Gap:** No `files` command; no symbol counts per file

**Fix required:**
1. Create `src/files_cmd.rs` with `run_files()` function
2. Add CLI subcommand for `files`
3. Add optional `--symbols` flag to show symbol counts
4. Query symbols per file for counts

---

## JSON Output Contract

### Existing Response Types (src/output/command.rs)

| Response Type | Status | Stable ID Field |
|---------------|--------|-----------------|
| `JsonResponse<T>` | Implemented | `execution_id` |
| `QueryResponse` | Implemented | `symbol_id` in `SymbolMatch` |
| `FindResponse` | Implemented | `symbol_id` partial (missing in find-by-name) |
| `RefsResponse` | Implemented | None for references |
| `FilesResponse` | Defined | N/A (file list) |
| `StatusResponse` | Defined | N/A |
| `ErrorResponse` | Defined | N/A |

### Span Model (Fully Implemented)

The `Span` type (lines 227-389) is complete with:
- `span_id`: SHA-256 based stable ID
- `file_path`, `byte_start`, `byte_end`
- `start_line`, `start_col`, `end_line`, `end_col`

### SymbolMatch (Partially Implemented)

```rust
pub struct SymbolMatch {
    pub match_id: String,           // Stable match ID
    pub span: Span,                  // Span with span_id
    pub name: String,
    pub kind: String,
    pub parent: Option<String>,
    pub symbol_id: Option<String>,   // Stable symbol ID (MISSING in some paths)
}
```

### ReferenceMatch (Needs Enhancement)

```rust
pub struct ReferenceMatch {
    pub match_id: String,
    pub span: Span,
    pub referenced_symbol: String,   // Only name, no ID
    pub reference_kind: Option<String>,
}

// MISSING: target_symbol_id field for the referenced symbol's stable ID
```

---

## Deterministic Ordering

### Current Sorting Patterns

| Command | Sort Order | Location |
|---------|------------|----------|
| `query` JSON | file_path, start_line, start_col, byte_start, name | query_cmd.rs:298-304 |
| `find` JSON | file_path, start_line, start_col | find_cmd.rs:269-274 |
| `refs` JSON | file_path, byte_start | refs_cmd.rs:163-167 |
| `find` glob | file, name, line | find_cmd.rs:341-346 |

**Verification:** All JSON output sorts deterministically. No changes needed for ordering.

**Recommendation:** Keep existing sort patterns. Add documentation comment explaining the deterministic order.

---

## Implementation Strategy

### Plan 6-01: Fix find command symbol_id propagation

**Goal:** Ensure find-by-name includes symbol_id like glob listing does

**Changes:**
1. `src/find_cmd.rs:289` - Pass `s.symbol_id` instead of `None`
2. Test: verify JSON output includes symbol_id for exact name search

**Effort:** 5 minutes (1 line change + test)

### Plan 6-02: Add target_symbol_id to references

**Goal:** Include stable ID of referenced symbol in reference results

**Changes:**
1. Add `target_symbol_id: Option<String>` to `ReferenceMatch` in `src/output/command.rs`
2. Update `ReferenceOps::references_to_symbol()` to fetch and return target IDs
3. Update `refs_cmd.rs` to populate `target_symbol_id` in JSON output
4. Add tests for reference ID propagation

**Effort:** 30 minutes

### Plan 6-03: Add symbol_id to call graph results

**Goal:** Include stable IDs for caller and callee in call results

**Changes:**
1. Extend `CallFact` with `caller_symbol_id` and `callee_symbol_id` fields
2. Update `CallOps::index_calls()` to fetch and store symbol IDs
3. Update `CallOps::call_fact_from_node()` to deserialize IDs
4. Update `refs_cmd.rs` to include IDs in JSON output
5. Add tests for call ID propagation

**Effort:** 1 hour

### Plan 6-04: Create files command

**Goal:** Add CLI command to list indexed files

**Changes:**
1. Create `src/files_cmd.rs` with `run_files()` function
2. Add `--symbols` flag for symbol counts per file
3. Integrate with CLI parser in `src/main.rs`
4. Add tests for files command

**Effort:** 45 minutes

---

## Test Strategy

### Unit Tests

| Test File | Purpose |
|-----------|---------|
| `query_cmd_tests.rs` | Verify JSON output includes symbol_id |
| `find_cmd_tests.rs` | Verify find-by-name includes symbol_id |
| `refs_cmd_tests.rs` | Verify references include target_symbol_id |
| `call_graph_tests.rs` | Verify calls include caller/callee IDs |

### Integration Tests

Create new test file `tests/query_ux_tests.rs`:
1. Test deterministic ordering (same input = same output order)
2. Test stable ID consistency across runs
3. Test JSON output schema validation
4. Test files command with and without --symbols flag

### Test Pattern (from existing tests)

```rust
#[test]
fn test_query_json_includes_symbol_id() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    // Index file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        // ... index content
    }

    // Run query with JSON output
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--format")
        .arg("json")
        // ... args
        .output()
        .unwrap();

    // Parse JSON and verify symbol_id field exists
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let symbols = &json["data"]["symbols"];
    for symbol in symbols.as_array().unwrap() {
        assert!(symbol["symbol_id"].is_string(), "symbol_id must be present");
    }
}
```

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead |
|---------|-------------|-------------|
| Symbol ID generation | Custom hash | `crate::graph::symbols::generate_symbol_id()` (Phase 5) |
| Span ID generation | Custom hash | `Span::generate_id()` (Phase 4) |
| JSON serialization | Manual serde | Existing `JsonResponse<T>` wrapper |
| Deterministic sorting | Custom sort logic | Existing sort patterns in query/find/refs |
| File path resolution | Custom path logic | `resolve_path()` helper in commands |

---

## Common Pitfalls

### Pitfall 1: Inconsistent symbol_id propagation

**What goes wrong:** Symbol ID is fetched from graph but not passed to JSON output type

**Why it happens:** Different code paths (find-by-name vs find-glob) use different constructors

**How to avoid:** Always use `symbol_nodes_in_file_with_ids()` for queries that need symbol_id

**Warning signs:** `SymbolMatch::new(...)` with `None` for symbol_id parameter

### Pitfall 2: Non-deterministic result ordering

**What goes wrong:** Results appear in different order across runs

**Why it happens:** Relying on HashMap iteration order or database insertion order

**How to avoid:** Always sort results before JSON output (existing pattern)

**Warning signs:** No explicit `sort_by()` before collecting results

### Pitfall 3: Missing stable IDs in cross-reference queries

**What goes wrong:** References and calls show names but not IDs, preventing correlation

**Why it happens:** Reference/Call facts only store names, not the graph node IDs

**How to avoid:** Store both name and symbol_id in facts during indexing

**Warning signs:** Using only `String` for references instead of `(String, Option<String>)`

---

## Code Examples

### Verified Pattern: Query with symbol_id

```rust
// Source: src/query_cmd.rs:209-226
// CORRECT: Uses symbol_nodes_in_file_with_ids to get symbol_id
let mut symbols_with_ids = magellan::graph::query::symbol_nodes_in_file_with_ids(&mut graph_mut, &path_str)?;

// Apply filters
if let Some(ref filter_kind) = kind_filter {
    symbols_with_ids.retain(|(_, fact, _)| fact.kind == *filter_kind);
}

// Convert to (SymbolFact, Option<symbol_id>) for output
let symbols_with_ids: Vec<(SymbolFact, Option<String>)> =
    symbols_with_ids.into_iter().map(|(_, fact, symbol_id)| (fact, symbol_id)).collect();

// Pass to output_json_mode which includes symbol_id in SymbolMatch
```

### Verified Pattern: Deterministic sorting

```rust
// Source: src/query_cmd.rs:297-304
// CORRECT: Sort deterministically before JSON output
symbols_with_ids.sort_by(|(a, _), (b, _)| {
    a.file_path
        .cmp(&b.file_path)
        .then_with(|| a.start_line.cmp(&b.start_line))
        .then_with(|| a.start_col.cmp(&b.start_col))
        .then_with(|| a.name.as_deref().cmp(&b.name.as_deref()))
});
```

### Anti-Pattern: Missing symbol_id

```rust
// Source: src/find_cmd.rs:289
// INCORRECT: symbol_id is None despite having symbol_id available
.map(|s| {
    let span = Span::new(...);
    SymbolMatch::new(s.name, s.kind_normalized, span, None, None)
    //                                                        ^^^^
    //                                                   Should be s.symbol_id
})
```

---

## Open Questions

### Question 1: Should ReferenceMatch include target_symbol_id?

**What we know:** `ReferenceMatch` currently only has `referenced_symbol: String`

**What's unclear:** Should we add a new field or change existing field to include ID?

**Recommendation:** Add optional `target_symbol_id: Option<String>` field. Keep `referenced_symbol` for human readability. This maintains backward compatibility.

### Question 2: Should CallFact be extended with symbol_id fields?

**What we know:** `CallFact` is defined in `src/references.rs` and is `Serialize, Deserialize`

**What's unclear:** Extending it requires database migration for existing Call nodes

**Recommendation:** Add optional fields with `#[serde(default)]` to maintain backward compatibility with existing databases. Old Call nodes will have `None`, new ones will have IDs.

### Question 3: Should files command show symbol counts by default?

**What we know:** `FilesResponse` only contains `Vec<String>` for file paths

**What's unclear:** Should counts be in a separate response type or optional field?

**Recommendation:** Add `--symbols` flag for counts. Use same `FilesResponse` with additional `symbol_counts: HashMap<String, usize>` field that is `skip_serializing_if = Option::is_none`.

---

## Sources

### Primary (HIGH confidence)

- [Existing codebase: src/query_cmd.rs](https://github.com/feanor/magellan) - Verified query command implementation with symbol_id
- [Existing codebase: src/find_cmd.rs](https://github.com/feanor/magellan) - Verified find command, identified symbol_id gap
- [Existing codebase: src/refs_cmd.rs](https://github.com/feanor/magellan) - Verified refs command, identified missing IDs
- [Existing codebase: src/output/command.rs](https://github.com/feanor/magellan) - Verified JSON output contract
- [Existing codebase: src/graph/query.rs](https://github.com/feanor/magellan) - Verified query methods with IDs
- [Existing codebase: src/graph/symbols.rs](https://github.com/feanor/magellan) - Verified symbol_id generation
- [Phase 5 Research](.planning/phases/05-stable-identity/05-RESEARCH.md) - Symbol ID design and implementation

### Secondary (MEDIUM confidence)

- [Phase 3 Research](.planning/phases/03-cli-output-contract/) - JSON output contract requirements
- [Existing tests: tests/cli_query_tests.rs](https://github.com/feanor/magellan) - Test patterns for query commands

### Tertiary (LOW confidence)

- None - all findings verified against existing codebase

---

## Metadata

**Confidence breakdown:**
- Existing Query Commands: HIGH - source code verified
- Graph Query Infrastructure: HIGH - source code verified
- Gaps Analysis: HIGH - specific line numbers identified
- Implementation Strategy: HIGH - based on existing patterns
- Test Strategy: HIGH - follows existing test patterns

**Research date:** 2026-01-19
**Valid until:** 2026-02-19 (query infrastructure stable; only enhancements needed)

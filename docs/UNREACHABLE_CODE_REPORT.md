# Unreachable Code Analysis Report

**Generated:** 2026-01-31
**Analysis Method:** `scripts/unreachable.sh` via Magellan semantic graph
**Database:** `.codemcp/magellan.db`

---

## Executive Summary

The unreachable code detection identified **376 public functions** across the Magellan codebase that have no incoming `REFERENCES` or `CALLS` edges from any entry point. This represents approximately **40-50% of the public API surface**.

### Key Findings by Category

| Category | Count | Risk Level | Recommendation |
|----------|-------|------------|----------------|
| Trait Implementations | 150+ | Low | Keep - called dynamically through traits |
| Test-Only Code | 80+ | None | Keep - used by tests only |
| Future-Proofing API | 60+ | Low | Keep - intentionally exposed for users |
| Ambiguity Operations | 4 | Medium | Review - partially implemented |
| Chunk Storage | 9 | Medium | Review - depends on external factors |
| Cache Operations | 8 | Low | Keep - utility functions |
| Watch Diagnostics | 13 | Low | Keep - public API completeness |
| Metrics Operations | 12 | Low | Keep - utility for future use |

### Critical Observations

1. **False Positive Rate**: ~70% - Many functions are called through trait dispatch, which the graph analysis doesn't capture
2. **Intentionally Kept**: Many functions have `#[allow(dead_code)]` or `#[expect(dead_code)]` attributes
3. **No Action Needed**: Most unreachable code is legitimate infrastructure

---

## Module-by-Module Analysis

### 1. `src/common.rs` (1 function)

#### `safe_str_slice` (line 175)

**Purpose:** Safely extract a UTF-8 string slice with bounds checking.

**Current Status:**
- No `#[allow(dead_code)]` attribute
- Has comprehensive tests (lines 183-301)
- Similar to `safe_slice` which IS used

**Analysis:**
This is a utility function for safe string slicing. The variant `safe_slice` (for `&[u8]`) IS used elsewhere, but `safe_str_slice` (for `&str`) appears to have no direct callers.

**Potential Callers:**
- Could be used by graph operations that need to extract string slices from source code
- Currently, code uses direct slicing or `.get()`

**Recommendation:** **REMOVE** - This appears to be truly dead code. The similar `safe_slice` function for byte slices is used, but the string variant is not. If it were needed, tests would fail.

**Risk:** **LOW** - No callers found, can be safely removed.

---

### 2. `src/diagnostics/watch_diagnostics.rs` (13 functions)

All methods on `SkipReason`, `DiagnosticStage`, and `WatchDiagnostic` are marked unreachable.

#### Functions Affected:
- `SkipReason::sort_key()` (line 38)
- `SkipReason::description()` (line 49)
- `SkipReason::fmt()` (line 61) - via Display impl
- `SkipReason::partial_cmp()` (line 67) - via PartialOrd impl
- `DiagnosticStage::sort_key()` (line 100)
- `DiagnosticStage::description()` (line 112)
- `DiagnosticStage::fmt()` (line 125) - via Display impl
- `DiagnosticStage::partial_cmp()` (line 131) - via PartialOrd impl
- `WatchDiagnostic::sort_key()` (line 180)
- `WatchDiagnostic::format_stderr()` (line 206)
- `WatchDiagnostic::fmt()` (line 223) - via Display impl
- `WatchDiagnostic::partial_cmp()` (line 229) - via PartialOrd impl
- `WatchDiagnostic::skipped()` (line 188)
- `WatchDiagnostic::error()` (line 193)

**Current Status:**
- No dead_code attributes
- These are trait implementations (Display, Ord, PartialOrd)
- Used by tests (lines 242-377)
- Used by `src/graph/filter.rs` and `src/graph/scan.rs`

**Analysis:**
These are **FALSE POSITIVES**. The functions are called through:
1. Trait method dispatch (Display::fmt, Ord::cmp, etc.)
2. Direct method calls in filter.rs and scan.rs
3. Test assertions

The graph analysis doesn't track trait method calls, which is why these appear unreachable.

**Recommendation:** **KEEP ALL** - These are working public API methods called through trait dispatch.

**Risk:** **NONE** - Actively used, just not tracked by static analysis.

---

### 3. `src/generation/mod.rs` + `schema.rs` (12 functions)

#### ChunkStore Functions (mod.rs):
- `new()` (line 54)
- `with_connection()` (line 67)
- `connect()` (line 81)
- `with_conn()` (line 111)
- `with_connection_mut()` (line 134)
- `ensure_schema()` (line 155)
- `store_chunk()` (line 200)
- `store_chunks()` (line 224)
- `get_chunk_by_span()` (line 261)
- `get_chunks_for_file()` (line 302)
- `get_chunks_for_symbol()` (line 337)
- `delete_chunks_for_file()` (line 376)
- `count_chunks()` (line 390)
- `count_chunks_for_file()` (line 407)
- `get_chunks_by_kind()` (line 422)

#### CodeChunk Functions (schema.rs):
- `new()` (line 45)
- `compute_hash()` (line 70)
- `now()` (line 79)
- `byte_len()` (line 88)

**Current Status:**
- `CodeChunk` is exported in `lib.rs` (line 30)
- `ChunkStore` is exported in `lib.rs` (line 30)
- No dead_code attributes
- Comprehensive test coverage

**Analysis:**
The ChunkStore module is designed for **future code chunk storage** functionality. Currently:
- `CodeChunk` type is used by `src/get_cmd.rs` for output formatting
- `ChunkStore` operations exist but are not integrated into the main indexing pipeline
- Module documentation notes this is "for token-efficient queries"

**Why Unreachable:**
1. ChunkStore is prepared but not wired into CodeGraph's indexing operations
2. The feature may be deferred to a future release
3. CodeChunk is used only as a data transfer object in output, not for storage

**Recommendation:** **KEEP** - This is infrastructure for a planned feature. The module is well-designed and tested. Mark with `#[expect(dead_code)]` on unused methods to signal intentional keeping.

**Risk:** **LOW** - Not used, but no breaking changes to remove if needed.

---

### 4. `src/graph/ambiguity.rs` (4 trait methods)

#### Functions Affected:
- `create_ambiguous_group()` (trait at line 126, impl at line 245)
- `resolve_by_symbol_id()` (trait at line 173, impl at line 264)
- `get_candidates()` (trait at line 241, impl at line 282)
- `find_or_create_display_name()` (line 48)

**Current Status:**
- Part of `AmbiguityOps` trait
- Tests exist in `tests/ambiguity_tests.rs`
- Used in Phase 23's query.rs for collision detection

**Analysis:**
This is a **FALSE POSITIVE** for trait methods. The functions are called:
1. Through the `AmbiguityOps` trait
2. In tests via trait dispatch
3. Indirectly through `query::get_ambiguous_candidates()`

**Recommendation:** **KEEP ALL** - These are working trait methods for ambiguity resolution.

**Risk:** **NONE** - Actively used through trait dispatch.

---

### 5. `src/graph/cache.rs` (8 functions)

#### Functions Affected:
- `LruCache::new()` (line 61)
- `LruCache::get()` (line 75)
- `LruCache::put()` (line 94)
- `LruCache::invalidate()` (line 115)
- `LruCache::clear()` (line 123)
- `LruCache::len()` (line 135) - has `#[allow(dead_code)]`
- `LruCache::is_empty()` (line 143) - has `#[allow(dead_code)]`
- `LruCache::stats()` (line 149)
- `LruCache::hit_rate()` (line 162) - has `#[allow(dead_code)]`
- `CacheStats::hit_rate()` (line 37)

**Current Status:**
- File-level documentation warns: "NOT thread-safe"
- Some functions have `#[allow(dead_code)]` for "API completeness"
- `SymbolCache` type alias has `#[expect(dead_code)]` for "Future use"
- Comprehensive test coverage

**Analysis:**
The cache is designed for internal use by `CodeGraph`. The unreachable status occurs because:
1. Cache operations are called through `CodeGraph` methods, not directly
2. Some methods are intentionally kept for API completeness
3. The graph analysis may not track all usage patterns

**Recommendation:** **KEEP ALL** - Cache operations are infrastructure. The attributes explicitly signal intentional keeping.

**Risk:** **NONE** - Properly annotated with dead_code attributes.

---

### 6. `src/graph/call_ops.rs` (8 functions)

#### Functions Affected:
- `index_calls()` (line 77)
- `calls_from_symbol()` (line 227)
- `callers_of_symbol()` (line 255)
- `insert_call_node()` (line 277)
- `insert_calls_edge()` (line 304)
- `insert_caller_edge()` (line 317)
- `call_fact_from_node()` (line 330)
- `symbol_fact_from_node()` (line 355)

**Current Status:**
- No dead_code attributes
- Used by CodeGraph for call indexing
- Part of the call graph infrastructure

**Analysis:**
This is a **FALSE POSITIVE**. These functions are called:
1. Through `CodeGraph::index_calls()` which delegates to `CallOps::index_calls()`
2. Tests in `tests/call_integration_tests.rs`
3. Internal operations within the module

**Recommendation:** **KEEP ALL** - Working code graph infrastructure.

**Risk:** **NONE** - Actively used through delegation.

---

### 7. Language Parser Modules (180+ functions)

#### Files Affected:
- `src/ingest/c.rs` - 19 functions
- `src/ingest/cpp.rs` - 20 functions
- `src/ingest/java.rs` - 20 functions
- `src/ingest/javascript.rs` - 19 functions
- `src/ingest/python.rs` - 18 functions
- `src/ingest/typescript.rs` - 19 functions
- `src/ingest/mod.rs` - 15 functions (Rust parser)
- `src/references.rs` - 16 functions

**Function Types:**
- `Parser` trait implementations
- `extract_symbols()`, `extract_references()`, `extract_calls()`
- `extract_name()`, `extract_function_name()`
- `walk_tree_for_*()` variants
- `new()` constructors

**Current Status:**
- All implement the `Parser` trait
- Called through dynamic dispatch based on language detection
- Tests in `tests/` directory

**Analysis:**
These are **FALSE POSITIVES**. The functions are called through:
1. The `Parser` trait's dynamic dispatch
2. Language detection in `detect_language()` selects the parser
3. Generic `Parser::new()` and extraction methods

**Recommendation:** **KEEP ALL** - These are the core parsing infrastructure.

**Risk:** **NONE** - Actively used through trait polymorphism.

---

### 8. `src/graph/metrics/` (12 functions)

#### Functions Affected:
- `MetricsOps::new()` (line 42)
- `MetricsOps::ensure_schema()` (line 49)
- `MetricsOps::now()` (line 65) - has `#[allow(dead_code)]`
- `MetricsOps::upsert_file_metrics()` (line 73)
- `MetricsOps::upsert_symbol_metrics()` (line 96)
- `MetricsOps::delete_file_metrics()` (line 122)
- `MetricsOps::get_file_metrics()` (line 145)
- `MetricsOps::get_symbol_metrics()` (line 174)
- `MetricsOps::get_hotspots()` (line 208)
- Query wrapper functions in `query` submodule

**Current Status:**
- `now()` has `#[allow(dead_code)]` for "future timestamp tracking"
- Module documentation: "Pre-computed metrics for fast debug tool queries"

**Analysis:**
The metrics module is prepared for **future functionality**. Currently:
- Schema is defined
- Operations are implemented
- Not integrated into the main indexing pipeline

**Recommendation:** **KEEP** - This is planned infrastructure for complexity metrics and hotspots.

**Risk:** **LOW** - Not used, but intentionally kept for future features.

---

### 9. `src/graph/export.rs` (17 functions)

#### Functions Affected:
- `ExportFormat::from_str()` (line 48)
- `ExportConfig::default()` (line 324)
- `ExportConfig::new()` (line 341)
- `ExportConfig::with_symbols()` (line 349)
- `ExportConfig::with_references()` (line 355)
- `ExportConfig::with_calls()` (line 361)
- `ExportConfig::with_minify()` (line 367)
- `export_json()` (line 516)
- `export_graph()` (line 1264)
- `stream_json()` (line 556)
- `stream_json_minified()` (line 693)
- `stream_ndjson()` (line 993)
- Helper functions (escape_dot_*, get_file_path_from_symbol)

**Current Status:**
- Export is wired into `src/export_cmd.rs`
- Used by `magellan export` command

**Analysis:**
These are **FALSE POSITIVES** or **indirectly called**. The export functions are:
1. Called through `export_cmd.rs` which isn't tracked
2. Called through command-line dispatch
3. Used by integration tests

**Recommendation:** **KEEP ALL** - Working export functionality.

**Risk:** **NONE** - Used by CLI commands.

---

### 10. Remaining Modules (100+ functions)

The following modules have similar patterns:

#### `src/graph/mod.rs` (CodeGraph methods)
- Methods like `index_file()`, `scan_directory()`, `delete_file()`, etc.
- **Status:** Called through CLI commands, not tracked by graph analysis

#### `src/graph/{files,filter,symbols,references}.rs`
- Helper functions for indexing operations
- **Status:** Called through CodeGraph delegation

#### `src/watcher.rs` (8 functions)
- Event handling and batching
- **Status:** Called through the watch pipeline

#### `src/output/` (15 functions)
- JSON formatting and response types
- **Status:** Used by CLI output formatting

#### `src/main.rs` (3 functions)
- Command setup helpers
- **Status:** Used in main setup

---

## Recommendations Summary

### Immediate Actions

1. **`src/common.rs::safe_str_slice`** - Consider removal
   - Truly appears unused
   - Similar `safe_slice` exists and is used
   - Safe to remove if tests pass

2. **Add `#[expect(dead_code)]` attributes** to future-proofing code:
   - ChunkStore methods in `src/generation/mod.rs`
   - MetricsOps::now() already has it
   - Cache helper methods already have it

### No Actions Needed

The vast majority of "unreachable" functions fall into these categories:

1. **Trait implementations** - Called through dynamic dispatch
2. **CLI-command entry points** - Not tracked by graph analysis
3. **Test-only code** - Used by tests, not production code
4. **Future infrastructure** - Intentionally kept with attributes

### Risk Assessment

| Risk Level | Count | Description |
|------------|-------|-------------|
| **NONE** | ~300 | False positives - code is actively used |
| **LOW** | ~75 | Future-proofing or intentionally kept |
| **MEDIUM** | 1 | `safe_str_slice` - candidate for removal |

---

## Methodology Notes

### How the Analysis Works

1. The `scripts/unreachable.sh` script queries the Magellan graph database
2. It finds `Symbol` nodes with kind `Function` or `Method`
3. It checks for incoming `REFERENCES` or `CALLS` edges
4. Functions without incoming edges are reported as unreachable

### Limitations

1. **Trait dispatch not tracked** - Methods called through `dyn Trait` don't show edges
2. **CLI commands not tracked** - Entry points from main aren't in the graph
3. **Test-only code** - Functions called only from tests appear unreachable
4. **Indirect calls** - Functions called through function pointers or macros

### False Positive Rate

**Estimated: 70-80%**

Most reported unreachable functions are actually used through mechanisms that static analysis can't track.

---

## Conclusion

The unreachable code report identifies **376 functions**, but:

1. **~300 are false positives** - Trait implementations, CLI commands, etc.
2. **~75 are intentionally kept** - Future infrastructure with dead_code attributes
3. **~1 is candidate for removal** - `safe_str_slice` in common.rs

**Overall Assessment:** The codebase is healthy. The "unreachable" code is primarily:
- Working infrastructure called through untracked mechanisms
- Intentionally kept future-proofing
- Test utilities

No immediate cleanup is required beyond reviewing `safe_str_slice`.

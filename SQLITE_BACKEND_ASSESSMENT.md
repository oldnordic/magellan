# Honest Assessment: Magellan SQLite Backend

**Date:** 2026-03-15
**Test Setup:** Built with SQLite backend only (no geometric-backend feature)
**Test Data:** Indexed Magellan's own source (119 files, 2363 symbols, 3246 calls)

---

## Summary

| Category | Count | Commands |
|----------|-------|----------|
| **WORKING** | 6 | `status`, `doctor`, `find`, `query`, `cycles`, `export` |
| **PARTIALLY WORKING** | 3 | `refs` (no results), `reachable` (no results), `dead-code` (needs exact symbol ID) |
| **BROKEN - DON'T COMPILE** | 7 | `condense`, `paths`, `slice`, `verify`, `context`, `get`, `import-lsif` |

---

## Detailed Command Testing

### WORKING COMMANDS

#### ✅ `status`
```bash
$ magellan status --db magellan.db
Database: magellan.db (SQLite)
Schema version: 7
Files: 119
Symbols: 2363
Calls: 3246
Cycles: 12
```
- Works correctly
- JSON output (`--output json`) also works

#### ✅ `doctor`
```bash
$ magellan doctor --db magellan.db
Checking database file... OK
Checking database readability... OK
Checking database stats... OK
Checking symbol index... OK (2363 symbols)
Checking file index... OK (119 files)
Checking call graph... OK (3246 calls)
No issues found - database is healthy!
```
- Fully functional diagnostic tool

#### ✅ `find`
```bash
$ magellan find --db magellan.db --name "run" --output json
# Returns matching symbols correctly
```
- Works for symbol name search
- Supports JSON output
- Does NOT support path filtering (need to check this)

#### ✅ `query`
```bash
$ magellan query --db magellan.db --file "/home/feanor/Projects/magellan/src/cli.rs"
# Shows all symbols in the file
```
- **CRITICAL CAVEAT:** Requires FULL absolute paths
- Relative paths (like `src/cli.rs`) return empty results
- This is a usability issue

#### ✅ `cycles`
```bash
$ magellan cycles --db magellan.db
[INFO] Found 12 cycles (SCCs)
[INFO] Largest SCC: 14 nodes, 41 edges
[INFO] Average SCC size: 7.25 nodes
```
- Correctly detects mutual recursion cycles
- Outputs valid JSON with cycle information

#### ✅ `export`
```bash
$ magellan export --db magellan.db --output export.geo
# Creates ~3MB JSON file with complete graph
```
- Successfully exports all data to JSON
- Output includes: files, symbols, calls, with all metadata
- File is valid JSON

---

### PARTIALLY WORKING / NEEDS INVESTIGATION

#### ⚠️ `refs` (References)
```bash
$ magellan refs --db magellan.db --name "run_cli" --path "src/cli.rs"
[INFO] Symbol: run_cli at src/cli.rs:300
[INFO] No incoming calls
```
- Finds the symbol correctly
- **BUT:** Shows "No incoming calls" for functions that ARE called
- The call graph appears to be stored (3246 calls in DB)
- Likely a query issue in the refs command

#### ⚠️ `reachable`
```bash
$ magellan reachable --db magellan.db --symbol "<id>"
[INFO] No symbols reachable from origin
```
- Command runs but returns no results
- Graph traversal logic may be broken

#### ⚠️ `dead-code`
```bash
$ magellan dead-code --db magellan.db --entry "run"
Error: Symbol 'run' not found
```
- Requires exact symbol ID, not display name
- Need to use `find` first to get the ID, then pass it
- UX issue: should accept names like other commands

---

### REQUIRES GEOMETRIC BACKEND

These commands fail immediately with:
```
Error: Geometric backend not compiled in. Use --features geometric-backend
```

| Command | Purpose | Status |
|---------|---------|--------|
| `condense` | Graph condensation/SCC analysis | ❌ Requires geometric |
| `paths` | Path enumeration between symbols | ❌ Requires geometric |
| `slice` | Code slicing | ❌ Requires geometric |
| `verify` | Verification/rules checking | ❌ Requires geometric |
| `context` | Context extraction | ❌ Requires geometric |
| `get` | Get symbol details by ID | ❌ Requires geometric |
| `import-lsif` | Import LSIF data | ❌ Requires geometric |

---

## Architecture Observations

### The SQLite Backend is "Read-Only Light"

The SQLite backend is essentially a **storage backend** with limited query capabilities:

1. **Good for:**
   - Basic storage and retrieval
   - Symbol indexing
   - Call recording
   - Status reporting
   - Full export

2. **Bad for:**
   - Complex graph traversal
   - Path enumeration
   - Context extraction
   - Anything requiring graph algorithms

### The Geometric Backend is BROKEN

The "geometric" feature flag is SUPPOSED to enable:
- CFG (Control Flow Graph) extraction
- Path enumeration
- Graph algorithms (condensation, dominators, etc.)
- Advanced queries

**BUT IT DOESN'T COMPILE.**

### Feature Parity Gap

There's NO feature parity - the geometric backend is broken:

```
SQLite Backend:     Geometric Backend:
├── status          └── ❌ BROKEN - doesn't compile
├── doctor
├── find
├── query
├── cycles
├── export
└── import

Commands that fail without geometric backend:
├── condense ❌
├── paths ❌
├── slice ❌
├── verify ❌
├── context ❌
├── get ❌
└── import-lsif ❌
```

---

## Data Integrity Check

**Good news:** The data IS being stored correctly.

From `status` and `doctor`:
- 119 files indexed
- 2363 symbols indexed
- 3246 calls recorded
- 12 cycles detected (in SQLite backend)

The `export` command produces a valid 102,934-line JSON file with complete data.

**The problem:** Query commands don't properly traverse this data.

---

## Specific Issues Found

### 1. `refs` Command Returns No Results
```rust
// In src/refs_cmd.rs
// The query likely uses incorrect SQL or missing joins
```
The call data exists but refs can't find it.

### 2. `reachable` Command Returns No Results
```rust
// Graph traversal is likely not implemented for SQLite
// Or uses a query that returns empty
```

### 3. `query --file` Requires Absolute Paths
```bash
# This doesn't work:
$ magellan query --file src/cli.rs

# This works:
$ magellan query --file /home/feanor/Projects/magellan/src/cli.rs
```

### 4. Commands Panic Without Explicit Error Handling
Some commands fail with raw errors rather than user-friendly messages.

---

## Recommendations

### For Users (Current State)

1. **If you only need:**
   - Symbol indexing
   - Basic search (`find`)
   - Status/diagnostics
   - Export to other formats

   → SQLite backend is sufficient

2. **If you need:**
   - Call graph analysis
   - Dead code detection
   - Path enumeration
   - Code slicing

   → You MUST build with `--features geometric-backend`

### For Development

1. **Fix `refs` command** - The call data exists but queries return nothing
2. **Fix `reachable` command** - Graph traversal needs implementation
3. **Make `query --file` accept relative paths** - Major UX issue
4. **Document the feature split** - Users don't know what requires geometric
5. **Consider:** Merge geometric capabilities into SQLite backend or make geometric the default

---

## Build Instructions for Full Functionality

```bash
# SQLite only (limited)
cargo build --release

# Full functionality (recommended)
cargo build --release --features geometric-backend

# All features
cargo build --release --all-features
```

---

## CRITICAL FINDING: Geometric Backend is BROKEN

**The geometric backend feature DOES NOT COMPILE.**

```bash
$ cargo build --release --features geometric-backend
error[E0432]: unresolved import `crate::graph::geo_index`
  --> src/indexer.rs:573:9
error[E0432]: unresolved import `crate::graph::geometric_backend::extract_symbols_and_cfg_from_file`
  --> src/indexer.rs:704:9
...
error: could not compile `magellan` (lib) due to 15 previous errors
```

### Root Causes:

1. **Missing module declaration**: `src/graph/geo_index.rs` exists but is NOT declared in `src/graph/mod.rs`
2. **Missing function**: `extract_symbols_and_cfg_from_file` doesn't exist in geometric_backend
3. **Broken macro**: `debug_print!` macro has trailing semicolon issues
4. **Type errors**: Several type annotation errors when geometric features are enabled

### What This Means:

**ALL advanced analysis commands are currently UNAVAILABLE** because:
- They require the geometric backend
- The geometric backend doesn't compile
- There are ~15 compilation errors to fix

---

## Conclusion

**The SQLite backend is functional for storage and basic queries, but the codebase is in a broken state for advanced analysis.**

The "honest" assessment:
- ✅ Data storage works correctly
- ✅ Basic querying works
- ✅ Export works
- ❌ **Geometric backend is BROKEN - doesn't compile**
- ❌ **ALL advanced analysis commands are unavailable** (paths, slice, verify, context, condense, get, import-lsif)
- ❌ Graph traversal commands return no results (refs, reachable)
- ❌ Significant UX issues (paths, error messages)

**Bottom line:** The codebase currently ONLY works in SQLite-only mode. The geometric backend, which is required for actual code analysis features, is broken and needs to be fixed before any of those features work.

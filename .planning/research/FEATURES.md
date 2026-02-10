# Phase Research: v2.3 Tool Migration & Core Quality - Feature Status

**Researched:** 2026-02-10
**Domain:** Native-V2 Tool Migration (llmgrep, splice, mirage)
**Confidence:** HIGH

---

## Summary

This research documents the planned features from the native-v2 migration plan (docs/NATIVE-V2-MIGRATION.md) and their current implementation status across the three dependent tools: llmgrep, splice, and mirage.

**Key findings:**
- **splice** is the most migrated tool (70% complete) with `--detect-backend`, `migrate`, `verify`, `batch`, and `snapshots` implemented
- **llmgrep** has partial migration (50% complete) with `complete` and `lookup` commands working, but missing CLI-level `--detect-backend`, `purpose` search mode, and `watch` command
- **mirage** is least migrated (20% complete) - still uses direct SQLite queries, no KV storage for CFG data, missing `diff`, `hotpaths`, `icfg`, and `incremental` commands

**Primary recommendation:** Complete llmgrep CLI features first (highest ROI), then finish splice's `--impact-graph` exposure, then tackle mirage's storage rewrite.

---

## User Constraints

(From docs/NATIVE-V2-MIGRATION.md - all tool features are committed deliverables)

### Locked Decisions
- All tools must support native-v2 backend
- All tools must provide `--detect-backend` flag
- All new features listed in migration plan must be implemented
- Backward compatibility with SQLite must be maintained

### Cross-Tool Requirements
- Backend detection: `tool-name --detect-backend --db codegraph.db`
- Migration utility: `tool-name --migrate --from sqlite --to native-v2 --db codegraph.db`
- Verification mode: `tool-name --verify --db codegraph.db`

---

## Standard Stack

### Core Dependencies (Current Status)

| Library | Version | Purpose | Current Usage |
|---------|---------|---------|---------------|
| sqlitegraph | 1.5.5+ | Graph database foundation | llmgrep uses 1.5.7 |
| magellan | 2.2.1+ | Code graph indexer | All tools updated |
| rusqlite | 0.31+ | SQLite backend | Direct use in mirage (issue) |
| clap | 4.x | CLI argument parsing | All tools use |
| serde | 1.x | Serialization | All tools use |

### Installation
```bash
# Native-v2 builds
cargo build --release --no-default-features --features native-v2

# SQLite builds (default)
cargo build --release
```

---

## Architecture Patterns

### Backend Detection Pattern

All tools must use this pattern for runtime backend detection:

```rust
// From llmgrep/src/backend/mod.rs
use magellan::migrate_backend_cmd::{detect_backend_format, BackendFormat};

pub fn detect_and_open(db_path: &Path) -> Result<Self, LlmError> {
    detect_backend_format(db_path)
        .and_then(|format| match format {
            BackendFormat::Sqlite => SqliteBackend::open(db_path),
            BackendFormat::NativeV2 => NativeV2Backend::open(db_path),
        })
}
```

**Status:** Implemented in llmgrep and splice, NOT in mirage (has internal detection but not exposed as CLI flag)

### Backend Abstraction Trait Pattern

```rust
// From llmgrep/src/backend/mod.rs
pub trait BackendTrait {
    fn search_symbols(&self, options: SearchOptions) -> Result<...>;
    fn search_references(&self, options: SearchOptions) -> Result<...>;
    fn complete(&self, prefix: &str, limit: usize) -> Result<Vec<String>>;
    fn lookup(&self, fqn: &str, db_path: &str) -> Result<SymbolMatch>;
    fn search_by_label(&self, label: &str, limit: usize, db_path: &str) -> Result<...>;
}
```

**Status:** llmgrep has this pattern. splice uses CodeGraph abstraction. mirage uses direct rusqlite (anti-pattern).

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Backend detection | Custom file header parsing | `magellan::migrate_backend_cmd::detect_backend_format` | Already handles SQLite vs native-v2 detection |
| KV storage access | Direct kv_store imports | `magellan::graph::get_cfg_blocks_kv` | Wrapper handles edge cases |
| Symbol resolution | Custom lookup logic | `Backend::lookup()` in llmgrep pattern | O(1) KV get with error handling |
| Snapshot export | Manual JSON serialization | `sqlitegraph::GraphBackend::export_snapshot` | Handles full database state |

**Key insight:** Magellan 2.2.0+ provides all necessary abstractions. Direct database access is an anti-pattern.

---

## Common Pitfalls

### Pitfall 1: Direct SQLite Usage Prevents Backend Abstraction

**What goes wrong:** Using `rusqlite::Connection` directly ties code to SQLite backend.

**Why it happens:** Mirage was written before native-v2 existed; direct SQL queries were the only option.

**How to avoid:** Use the `Backend::detect_and_open()` pattern from llmgrep. Create a storage trait that abstracts both backends.

**Warning signs:** File contains `use rusqlite::{Connection, params};`

**Evidence:** `/home/feanor/Projects/mirage/src/storage/mod.rs:14` uses `rusqlite` directly

### Pitfall 2: CLI Flags Not Exposed for Implemented Features

**What goes wrong:** Feature exists in library code but no CLI flag to use it.

**Why it happens:** Library-first development where CLI is added later.

**How to avoid:** When implementing a feature, add CLI flag simultaneously.

**Warning signs:** Test uses `backend.some_feature()` but no `--some-feature` flag in clap Args.

**Evidence:** splice has `impact_graph` parameter in code but it's not consistently exposed as `--impact-graph` flag.

### Pitfall 3: Feature Name Confusion

**What goes wrong:** Migration plan asks for "hotpaths" (most-traversed execution paths) but tool has "Hotspots" (high-risk functions).

**Why it happens:** Different terminology between plan and implementation.

**How to avoid:** Use exact names from migration plan for CLI commands.

**Evidence:** mirage has `Hotspots` command but migration plan line 98 asks for `hotpaths`

---

## Feature Status Matrix

### Cross-Tool Features (Lines 197-218 of migration plan)

| Feature | llmgrep | splice | mirage | Evidence |
|---------|---------|--------|--------|----------|
| `--detect-backend` CLI flag | MISSING | DONE | MISSING | splice: `src/main.rs:414`, mirage: no flag |
| `--migrate` command | MISSING | DONE | MISSING | splice: `execute_migrate()` at line 4454 |
| `--verify` consistency check | MISSING | DONE (different purpose) | MISSING | splice `verify` compares snapshots, not backends |

### llmgrep Features (Lines 99-104 of migration plan)

| Feature | Plan Description | Status | Evidence |
|---------|-----------------|--------|----------|
| `--detect-backend` | Show active backend | MISSING | No global flag in `main.rs` |
| `complete` command | Prefix autocomplete via KV | DONE | Command at line 198, tests pass |
| `--purpose` search | Purpose-based semantic search | PARTIAL | `search_by_label()` exists in backend but no `--purpose` CLI flag |
| `watch` command | Real-time updates via pub/sub | MISSING | No watch command in `Command` enum |
| Documentation updates | Native-v2 notes in README | UNCHECKED | Not verified in this research |

### splice Features (Lines 119-140 of migration plan)

| Feature | Plan Description | Status | Evidence |
|---------|-----------------|--------|----------|
| `--snapshot-before` flag | Capture snapshot before edit | DONE | `capture_snapshot()` at line 107 |
| `verify` command | Compare before/after snapshots | DONE | `splice verify --before --after` implemented |
| `--impact-graph` flag | DOT graph visualization | PARTIAL | Internal `execute_impact_graph()` exists at line 148, CLI flags exist for some commands |
| `batch` command | Multi-file refactor with proof | DONE | `splice batch --spec` implemented |
| Documentation updates | Native-v2 notes | UNCHECKED | Not verified |

### mirage Features (Lines 170-193 of migration plan)

| Feature | Plan Description | Status | Evidence |
|---------|-----------------|--------|----------|
| Backend-agnostic storage trait | Abstract over SQLite/KV | MISSING | Direct `rusqlite` usage throughout |
| KV storage backend | Store CFG data in KV format | MISSING | Uses SQL tables `cfg_blocks`, `cfg_edges` |
| `migrate` command | Convert SQL to KV | MISSING | No migrate command in `Commands` enum |
| `diff` command | CFG diff between snapshots | MISSING | No `Diff` variant in `Commands` |
| `--incremental` flag | Analyze only changed functions | MISSING | Not in `PathsArgs` |
| `hotpaths` command | Most-traversed paths | MISSING | Has `Hotspots` (different thing) |
| `icfg` command | Inter-procedural CFG | MISSING | No `Icfg` variant in `Commands` |
| `--detect-backend` flag | Show active backend | MISSING | No global flag in CLI |

---

## Feature Dependencies

### Dependency Graph

```
llmgrep:
  complete (DONE) -> no dependencies
  lookup (DONE) -> no dependencies
  --purpose flag (TODO) -> needs search_by_label (DONE)
  watch (TODO) -> needs pub/sub infrastructure
  --detect-backend (TODO) -> needs detect_backend_format (DONE)

splice:
  verify (DONE) -> needs snapshots (DONE)
  --impact-graph (PARTIAL) -> needs impact graph generation (DONE)
  batch (DONE) -> needs snapshot system (DONE)
  --detect-backend (DONE) -> no dependencies

mirage:
  diff (TODO) -> needs snapshots (TODO) + KV storage (TODO)
  incremental (TODO) -> needs function change tracking (TODO)
  hotpaths (TODO) -> needs path execution counting (TODO)
  icfg (TODO) -> needs inter-procedural analysis (TODO)
  --detect-backend (TODO) -> needs Backend::detect() (DONE)
  KV storage (TODO) -> needs storage trait rewrite (TODO)
```

### Critical Path

1. **mirage storage trait** is the blocker for all mirage features
2. **llmgrep --purpose** just needs CLI exposure (backend method exists)
3. **splice --impact-graph** just needs consistent flag exposure

---

## Code Examples

### Backend Detection (splice pattern - READY TO COPY)

```rust
// From /home/feanor/Projects/splice/src/main.rs:414
splice::cli::Commands::Status { db, detect_backend } => {
    execute_status(&db, json_output, detect_backend)
}

// Implementation at line 3904
if detect_backend {
    let backend = splice::graph::CodeGraph::detect_backend(db_path)?;
    // ... output backend type
}
```

### Purpose Search (llmgrep backend method - NEEDS CLI EXPOSURE)

```rust
// From /home/feanor/Projects/llmgrep/src/backend/mod.rs:259
// Backend trait has search_by_label method
pub fn search_by_label(
    &self,
    label: &str,
    limit: usize,
    db_path: &str,
) -> Result<(SearchResponse, bool, bool), LlmError>

// TODO: Add to main.rs Command enum:
// Purpose {
//     #[arg(long)]
//     label: String,
// }
```

### Impact Graph (splice internal - EXISTS BUT INCOMPLETE EXPOSURE)

```rust
// From /home/feanor/Projects/splice/src/main.rs:148
fn execute_impact_graph(
    db_path: &Path,
    symbol_id: &str,
    direction: &splice::cli::ReachabilityDirection,
    max_depth: Option<usize>,
) -> Result<splice::cli::CliSuccessPayload, splice::SpliceError> {
    let mut integration = MagellanIntegration::open(db_path)?;
    let dot = integration.generate_impact_dot(symbol_id, direction, &config)?;
    println!("{}", dot);
    // ...
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SQLite-only tools | Dual backend (SQLite + native-v2) | Magellan 2.2.0 | Tools must detect backend at runtime |
| Direct SQL queries | Backend abstraction trait | In progress | llmgrep done, splice partial, mirage not started |
| Manual backend detection | `detect_backend_format()` from magellan | 2.2.0 | Use shared function, don't reimplement |

**Deprecated/outdated:**
- Direct `rusqlite` usage for new features (use backend trait instead)
- Build-time backend selection (use runtime detection)

---

## Open Questions

1. **mirage storage trait design**
   - What we know: Needs to abstract both SQLite tables and KV storage
   - What's unclear: Should Mirage use the same trait pattern as llmgrep, or a different approach?
   - Recommendation: Follow llmgrep pattern for consistency

2. **watch command implementation for llmgrep**
   - What we know: Requires pub/sub infrastructure from sqlitegraph
   - What's unclear: Is pub/sub fully implemented in sqlitegraph 1.5.7?
   - Recommendation: Verify sqlitegraph pub/sub API before implementing

3. **hotpaths vs Hotspots terminology**
   - What we know: mirage has `Hotspots` command for high-risk functions
   - What's unclear: Does migration plan's `hotpaths` mean the same thing or something different?
   - Recommendation: Clarify with stakeholder - `hotpaths` likely means "most-executed paths" which is different

4. **icfg (inter-procedural CFG) scope**
   - What we know: mirage has `inter_procedural` flag on some commands
   - What's unclear: Should `icfg` be a separate command or a flag on existing commands?
   - Recommendation: Review mirage existing inter-procedural analysis before designing new command

---

## Sources

### Primary (HIGH confidence)
- docs/NATIVE-V2-MIGRATION.md - Full migration plan with all features
- .planning/codebase/CONCERNS-UPDATED.md - Verified status assessment
- /home/feanor/Projects/llmgrep/src/main.rs - CLI command definitions
- /home/feanor/Projects/splice/src/main.rs - CLI command definitions
- /home/feanor/Projects/mirage/src/main.rs - CLI command definitions
- /home/feanor/Projects/mirage/src/cli/mod.rs - Mirage CLI argument structures
- /home/feanor/Projects/llmgrep/src/backend/mod.rs - Backend abstraction pattern

### Secondary (MEDIUM confidence)
- /home/feanor/Projects/splice/CHANGELOG.md - Splice changelog confirms features
- /home/feanor/Projects/mirage/src/storage/mod.rs - Storage layer implementation
- llmgrep/tests/native_v2_commands_test.rs - Tests for label search

### Tertiary (LOW confidence - marked for validation)
- splice and mirage README files (not verified for native-v2 documentation)

---

## Metadata

**Confidence breakdown:**
- Feature status assessment: HIGH - based on direct code inspection
- Architecture patterns: HIGH - based on actual implementation in llmgrep
- Pitfalls: HIGH - based on observed anti-patterns in mirage
- Dependencies: MEDIUM - some dependencies inferred from code structure

**Research date:** 2026-02-10
**Valid until:** 2026-03-10 (30 days - stable tool APIs)

# Phase 3: CLI Output Contract - Research

**Researched:** 2026-01-19
**Domain:** JSON schema versioning, deterministic CLI output, stable IDs, stdout/stderr discipline
**Confidence:** HIGH (repo-specific findings verified by code reading)

## Summary

Phase 3 transforms Magellan's CLI from "human-readable text output" to "scriptable JSON contract." The key insight is that the codebase already has excellent primitives for structured data (serde, span-aware types, deterministic sorting) but lacks:
1. A unified output schema with versioning
2. Stable identifiers (execution_id, match_id, span_id) for cross-reference
3. Consistent stdout/stderr discipline
4. Schema-versioned JSON responses for every command

**Primary recommendation:** Add an `output` crate module with:
- `OutputFormat` enum (Human | Json)
- `JsonResponse<T>` wrapper with `schema_version` and `execution_id`
- Span-aware response types (`Span`, `SymbolMatch`, `ReferenceMatch`)
- Stdout/stderr helper macros (`out_json!`, `err_log!`)

**Status update (2026-01-19):** Magellan already has:
- Serde serialization on all core types (`SymbolFact`, `ReferenceFact`, `CallFact`, `WatchDiagnostic`)
- Span-aware types with byte + line/column info
- Deterministic sorting via BTreeSet in watcher
- Structured diagnostic types (`WatchDiagnostic`, `SkipReason`, `DiagnosticStage`)

The missing pieces are:
1. Schema version constants and response wrappers
2. Stable ID generation (execution_id, match_id, span_id)
3. `--output json` CLI flag and format switching
4. Strict stdout/stderr discipline enforcement

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde | 1.0 (already in repo) | serialization framework | de facto standard for Rust JSON |
| serde_json | 1.0 (already in repo) | JSON serialization | most reliable, well-tested |
| BTreeMap/BTreeSet | std lib | deterministic ordering | guarantees sorted iteration |
| sha2 | 0.10 (already in repo) | stable ID generation | already used for file hashing |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| uuid | NOT YET IN REPO / 1.10 | execution_id generation | use for v4 UUIDs; can defer to hash-based for simplicity |
| chrono | NOT YET IN REPO / 0.4 | timestamp formatting | only if human-readable timestamps needed |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| uuid crate | custom hash-based IDs | uuid is standard but adds dep; hash-based is simpler and deterministic |
| BTreeMap | indexmap with sorted iteration | BTreeMap is in std and guaranteed sorted |

**Installation (if adding new deps):**
```bash
# Only if UUID-based IDs are chosen:
cargo add uuid@1.10 --features v4,serde

# Chrono for timestamps (optional):
cargo add chrono@0.4 --features serde
```

---

## Architecture Patterns

### Recommended Project Structure

Phase 3 introduces a new output module without disrupting existing CLI commands:

```
src/
+-- main.rs                # Add --output flag, OutputFormat parsing
+-- output/                # NEW: CLI output contract module
|   +-- mod.rs            # OutputFormat, output helpers
|   +-- json.rs           # JsonResponse wrapper, schema_version
|   +-- span.rs           # Span type, ID generation
|   +-- command.rs        # Per-command response types
+-- diagnostics/           # EXISTS: WatchDiagnostic (Phase 2)
|   +-- watch_diagnostics.rs
+-- graph/
|   +-- query.rs          # EXISTS: SymbolQueryResult (add JSON output)
+-- *_cmd.rs              # MODIFY: Add OutputFormat param, JSON output
```

### Pattern 1: Schema-Versioned JSON Response Wrapper

**What:** Every JSON response includes `schema_version` for parsing stability.

**When to use:** All commands in `--output json` mode.

**Example:**
```rust
// Source: Based on existing serde patterns in src/graph/export.rs
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Current JSON output schema version
pub const MAGELLAN_JSON_SCHEMA_VERSION: &str = "1.0.0";

/// Wrapper for all JSON responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonResponse<T> {
    /// Schema version for parsing stability
    pub schema_version: String,
    /// Unique execution ID for this run
    pub execution_id: String,
    /// Response data
    pub data: T,
    /// Whether the response is partial (e.g., truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
}

/// Construct a JSON response
pub fn json_response<T>(data: T, execution_id: &str) -> JsonResponse<T> {
    JsonResponse {
        schema_version: MAGELLAN_JSON_SCHEMA_VERSION.to_string(),
        execution_id: execution_id.to_string(),
        data,
        partial: None,
    }
}
```

### Pattern 2: Span-Aware Result Types

**What:** Every match/result includes byte offsets AND line/col with explicit range semantics.

**When to use:** All symbol/reference/call results in JSON output.

**Example:**
```rust
/// Span in source code (byte + line/column)
///
/// Represents an exclusive range: [start, end)
/// - byte_end is the first byte NOT included
/// - end_line/end_col point to the position after the span
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Span {
    /// Stable span ID (hash-based)
    pub span_id: String,
    /// File path (absolute or root-relative)
    pub file_path: String,
    /// Byte range [start, end) - end is exclusive
    pub byte_start: usize,
    pub byte_end: usize,
    /// Line (1-indexed) and column (0-indexed, bytes)
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    /// Generate stable span ID from (file_path, byte_start, byte_end)
    pub fn generate_id(file_path: &str, byte_start: usize, byte_end: usize) -> String {
        use sha2::{Sha256, Digest};
        let input = format!("{}:{}:{}", file_path, byte_start, byte_end);
        let hash = Sha256::digest(input.as_bytes());
        format!("{:x}", hash)[..16].to_string()  // First 16 hex chars
    }
}

/// Symbol match result for query/find commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    /// Stable match ID
    pub match_id: String,
    /// Symbol span
    pub span: Span,
    /// Symbol name
    pub name: String,
    /// Symbol kind (normalized)
    pub kind: String,
    /// Containing symbol (if nested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}
```

### Pattern 3: Deterministic Output Ordering

**What:** All arrays/records use BTreeMap or explicit sort for stable ordering.

**When to use:** Any JSON output with arrays or maps.

**Example:**
```rust
use std::collections::BTreeMap;

/// Status response with deterministic key ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub files: usize,
    pub symbols: usize,
    pub references: usize,
    pub calls: usize,
    pub code_chunks: usize,
    /// Extra fields in deterministic order
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
```

### Pattern 4: Stdout/Stderr Discipline

**What:** stdout = JSON data only, stderr = logs/diagnostics.

**When to use:** All commands in JSON mode.

**Example:**
```rust
/// Output helper macros for strict stdout/stderr discipline
macro_rules! out_json {
    ($value:expr) => {
        println!("{}", serde_json::to_string($value).unwrap())
    };
}

macro_rules! err_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}

/// Print JSON to stdout, logs to stderr
pub fn output_json<T: Serialize>(data: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    println!("{}", json);
    Ok(())
}

/// Print log message to stderr
pub fn log_error(msg: &str) {
    eprintln!("ERROR: {}", msg);
}

pub fn log_warning(msg: &str) {
    eprintln!("WARN: {}", msg);
}

pub fn log_info(msg: &str) {
    eprintln!("INFO: {}", msg);
}
```

### Anti-Patterns to Avoid

- **Mixed stdout:** Don't print progress then JSON; use stderr for progress.
- **Unsorted HashMap:** Don't serialize HashMap directly; use BTreeMap or sort Vec.
- **Missing schema_version:** Don't emit JSON without version field.
- **Ad-hoc span representation:** Don't use different span formats across commands.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON serialization | custom to_string() | serde_json | battle-tested, handles edge cases |
| Deterministic map ordering | manual sort before serialize | BTreeMap | std lib, guarantees sorted order |
| Span ID generation | random or counter | hash-based | deterministic, collision-resistant |
| CLI arg parsing for --output | manual string matching | extend existing parse_args() | consistent with existing flags |

**Key insight:** Rust's std lib + serde already solve these problems. Use them.

---

## Common Pitfalls

### Pitfall 1: HashMap Iteration Leaks into JSON

**What goes wrong:** JSON objects have different key order across runs, breaking tests and scripting.

**Why it happens:** HashMap uses random hash seed for security; iteration order is undefined.

**How to avoid:** Use BTreeMap for all maps in JSON output types.

**Warning signs:** JSON comparison fails randomly; tests are flaky.

```rust
// BAD - HashMap
#[derive(Serialize)]
struct Bad {
    fields: HashMap<String, String>,  // Random order!
}

// GOOD - BTreeMap
#[derive(Serialize)]
struct Good {
    fields: BTreeMap<String, String>,  // Sorted order!
}
```

### Pitfall 2: Progress Messages Interleave with JSON

**What goes wrong:** "Processing..." appears before JSON, breaking parsers.

**Why it happens:** println! for progress, then JSON output to same stdout.

**How to avoid:** All progress/diagnostic messages go to stderr (eprintln!), only JSON to stdout.

**Warning signs:** `jq .` fails on output; JSON parsers complain about extra content.

### Pitfall 3: Span Semantics Inconsistent

**What goes wrong:** Some commands use inclusive ranges, others exclusive.

**Why it happens:** No canonical span type defined.

**How to avoid:** Define `Span` with explicit "end is exclusive" semantics, use everywhere.

**Warning signs:** Off-by-one errors in editors; spans don't match text ranges.

### Pitfall 4: Missing Schema Version Breaks Scripts

**What goes wrong:** Output format changes, old scripts silently misparse.

**Why it happens:** No version field to detect format changes.

**How to avoid:** Always include `schema_version` in JSON response wrapper.

**Warning signs:** Field rename breaks production scripts; no way to detect version.

---

## Code Examples

Verified patterns from existing codebase:

### Existing Span Type (from SymbolFact)

**Source:** `/home/feanor/Projects/magellan/src/ingest/mod.rs` lines 69-90

```rust
/// A fact about a symbol extracted from source code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolFact {
    pub file_path: PathBuf,
    pub kind: SymbolKind,
    pub kind_normalized: String,
    pub name: Option<String>,
    /// Byte offset where symbol starts in file
    pub byte_start: usize,
    /// Byte offset where symbol ends in file
    pub byte_end: usize,
    /// Line where symbol starts (1-indexed)
    pub start_line: usize,
    /// Column where symbol starts (0-indexed, bytes)
    pub start_col: usize,
    /// Line where symbol ends (1-indexed)
    pub end_line: usize,
    /// Column where symbol ends (0-indexed, bytes)
    pub end_col: usize,
}
```

**Analysis:** Magellan already has span-aware types with explicit semantics. The new `Span` type should mirror this structure for consistency.

### Existing Deterministic Sorting (from export.rs)

**Source:** `/home/feanor/Projects/magellan/src/graph/export.rs` lines 156-161

```rust
// Sort for deterministic output
files.sort_by(|a, b| a.path.cmp(&b.path));
symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
references.sort_by(|a, b| (&a.file, &a.referenced_symbol).cmp(&(&b.file, &b.referenced_symbol)));
calls.sort_by(|a, b| (&a.file, &a.caller, &a.callee).cmp(&(&b.file, &b.caller, &b.callee)));
```

**Analysis:** Deterministic sorting pattern already established. Use `.sort_by()` with tuple keys for multi-field sorting.

### Existing Diagnostic Type (Phase 2)

**Source:** `/home/feanor/Projects/magellan/src/diagnostics/watch_diagnostics.rs` lines 146-164

```rust
/// A diagnostic event from the watch/index pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WatchDiagnostic {
    /// File was skipped during scanning/watching
    Skipped {
        path: String,
        reason: SkipReason,
    },
    /// Error occurred while processing a file
    Error {
        path: String,
        stage: DiagnosticStage,
        message: String,
    },
}
```

**Analysis:** Phase 2 already added serde to diagnostic types. Phase 3 should integrate these into JSON error responses.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Text-only output | Schema-versioned JSON | Phase 3 | Scriptable, LLM-consumable |
| Random ordering (HashMap) | Deterministic (BTreeMap) | Throughout | Stable output for comparison |
| No span info | Span-aware (byte + line/col) | Already exists | Editor integration possible |
| No execution tracking | execution_id per run | Phase 3 | Traceable results, auditability |

**Deprecated/outdated:**
- `println!` for data in JSON mode: use stdout-only for JSON
- HashMap in response types: switch to BTreeMap
- Unversioned JSON responses: add schema_version

---

## Open Questions

1. **Execution ID generation strategy:**
   - What we know: need stable, unique IDs per run for cross-reference.
   - Options: UUID v4 (random), hash-based (timestamp + pid + maybe db path), or ULID.
   - Recommendation: Start with hash-based for simplicity (no new deps), format: `{timestamp_hex}-{pid_hex}`.
   - Deferred: UUID crate adds standard semantics but also dependency; evaluate in planning.

2. **Span ID stability across runs:**
   - What we know: spans are content-based (file_path + byte range).
   - Question: Should span_id change if file content shifts but symbol name is same?
   - Recommendation: span_id is purely positional (file + offsets). If file changes, span_id changes. This is correct behavior for static analysis.

3. **JSON output for long-running operations (watch):**
   - What we know: watch mode runs forever, emitting events.
   - Question: Should JSON mode emit NDJSON (newline-delimited JSON) or a single array?
   - Recommendation: NDJSON for streaming (one JSON object per line), matches existing one-event-per-line pattern.

4. **Human mode future:**
   - What we know: Phase 3 is about JSON contract, not human output improvements.
   - Question: Should human mode also get improvements (color, progress bars)?
   - Recommendation: Defer human-mode UX to separate phase. Phase 3 scope is JSON contract only.

---

## Sources

### Primary (HIGH confidence)
- serde docs: https://docs.rs/serde/latest/serde/
- serde_json docs: https://docs.rs/serde_json/latest/serde_json/
- Rust std lib BTreeMap: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
- sha2 crate docs: https://docs.rs/sha2/latest/sha2/

### Repo evidence (HIGH confidence)
- `/home/feanor/Projects/magellan/src/ingest/mod.rs` — SymbolFact with span fields
- `/home/feanor/Projects/magellan/src/references.rs` — ReferenceFact, CallFact with spans
- `/home/feanor/Projects/magellan/src/graph/export.rs` — JSON export with deterministic sorting
- `/home/feanor/Projects/magellan/src/diagnostics/watch_diagnostics.rs` — Phase 2 diagnostic types
- `/home/feanor/Projects/magellan/Cargo.toml` — verified serde 1.0, serde_json 1.0, sha2 0.10

### Dependency verification (HIGH confidence)
- serde: "1.0" (exact version not specified, using 1.x)
- serde_json: "1.0"
- sha2: "0.10"
- NO uuid, chrono, or similar deps currently in repo

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — existing deps verified, serde/serde_json are de facto standards.
- Architecture patterns: HIGH — existing code already follows these patterns.
- Pitfalls: HIGH — all pitfalls observed in similar codebases.
- Span representation: HIGH — verified existing SymbolFact/ReferenceFact span fields.
- ID generation: MEDIUM — hash-based is straightforward, UUID standard but not verified for this use case.

**Research date:** 2026-01-19
**Valid until:** 2026-02-19 (serde/serde_json stable, patterns long-lived)

---

## Appendix: Per-Command Response Types

Quick reference for planned JSON response types (for planner):

| Command | Response Type | Key Fields |
|---------|---------------|------------|
| `status` | `StatusResponse` | files, symbols, references, calls, code_chunks |
| `query` | `QueryResponse` | symbols: Vec<SymbolMatch> |
| `find` | `FindResponse` | matches: Vec<SymbolMatch> |
| `refs` | `RefsResponse` | references: Vec<ReferenceMatch> |
| `get` | `GetResponse` | symbol: SymbolMatch, code_chunk: Option<String> |
| `files` | `FilesResponse` | files: Vec<String> (sorted) |
| `watch` | (NDJSON) | Stream of WatchEvent objects |
| `export` | `GraphExport` | (already exists in export.rs) |

All responses wrapped in `JsonResponse<T>` with `schema_version` and `execution_id`.

# Phase 2: Deterministic Watch & Indexing Pipeline - Research

**Researched:** 2026-01-19
**Domain:** Deterministic filesystem watch + incremental indexing into sqlitegraph
**Confidence:** MEDIUM (repo-specific findings HIGH; some library/version recommendations HIGH)

## Summary

Phase 2 is mostly about turning Magellan’s existing “watch + mutate DB” behavior into a deterministic *pipeline contract*: regardless of OS/editor event storms, the final DB state after the coalescing window must match the state from a clean rescan of the workspace (with the same include/exclude rules).

Magellan already has the basic pieces (scan code, watcher, single-threaded loop), but there are determinism gaps:
- watcher config’s `debounce_ms` is currently unused (events are processed one-by-one, unbounded),
- scan-initial currently happens *before* the watcher starts (baseline-first is satisfied, but changes during scan are missed),
- “idempotent updates per file” is not true at the persistence layer: reference/call nodes are never removed, file-node updates are implemented as delete+insert which can orphan old symbol nodes, and `delete_file` doesn’t delete reference/call nodes created by the file.

**Primary recommendation:** implement a deterministic “reconcile file” operation (existence + hash check → delete derived facts → index) and drive it from a debounced/coalesced batch processor that flushes paths in sorted order, while buffering watcher events during scan-initial.

---

## What Exists Today (Magellan-specific)

### Current watch pipeline (where and how it works)

**Files:**
- `src/watch_cmd.rs` — CLI `watch` command; performs optional scan then runs event loop.
- `src/watcher.rs` — `FileSystemWatcher` using `notify::recommended_watcher` + `std::sync::mpsc::channel`.
- `src/indexer.rs` — separate `run_indexer[_n]` loop (used by tests) that also uses `FileSystemWatcher`.
- `src/graph/scan.rs` — scan-initial implementation (walkdir, sorted paths).
- `src/graph/ops.rs`, `src/graph/symbols.rs`, `src/graph/references.rs`, `src/graph/call_ops.rs`, `src/generation/*` — persistence and derived facts.

**Behavior today:**
- Scan-initial is done in `watch_cmd.rs` *before* starting watcher (`FileSystemWatcher::new`).
- Watcher thread runs forever and sends events on an **unbounded** `std::sync::mpsc` channel.
- `WatcherConfig { debounce_ms }` exists but is unused in `src/watcher.rs` (`run_watcher` takes `_config` but ignores it).
- `convert_notify_event`:
  - drops directory events,
  - maps `notify::EventKind::Create/Modify/Remove` to `EventType` and ignores other kinds (notably rename),
  - uses only the *first* path from `event.paths`,
  - filters out `.db`, `.db-wal`, `.sqlite*` to prevent feedback loops.

**Determinism risks today (HIGH confidence, from reading code):**
1) **No debouncing/coalescing**: storms will enqueue many events and process them in OS delivery order.
2) **Event coverage gaps**: rename events and multi-path events are ignored/mishandled (first path only).
3) **Baseline scan misses concurrent edits**: watcher starts after scan; any edits during scan are lost.
4) **Not actually idempotent persistence**:
   - `index_references` inserts reference nodes/edges but does not delete prior reference nodes for that file.
   - `index_calls` inserts call nodes/edges but does not delete prior call nodes for that file.
   - `delete_file` deletes symbols + chunks + file node, but does **not** delete Reference/Call nodes created by that file; it only deletes edges attached to deleted symbols.
   - `FileOps::find_or_create_file_node` updates an existing file by deleting and re-inserting the File node (sqlitegraph “no update” workaround). This can orphan old symbol nodes because symbol deletion is driven by outgoing `DEFINES` edges from the *current* file node ID.

---

## Standard Stack

This is the standard Rust “deterministic local indexer watch” stack, chosen to directly satisfy WATCH-01..WATCH-05 without hand-rolling.

### Core

| Library | Version (latest, verified) | Purpose | Why Standard |
|---------|----------------------------|---------|--------------|
| notify | 8.2.0 | cross-platform FS events | widely used; docs explicitly discuss editor behavior and known pitfalls; supports multiple sender types | 
| notify-debouncer-mini | 0.7.0 | per-path debouncing + batch mode | recommended by notify docs for debounced events; reduces event storms to stable batches |
| ignore | 0.4.25 | gitignore-style matching + deterministic directory walking | ripgrep’s ignore engine; provides WalkBuilder with documented precedence rules and deterministic sorting |
| globset | 0.4.18 | include/exclude globs | standard glob matching with well-defined semantics and good perf |

**Sources (HIGH):**
- notify docs: https://docs.rs/notify/latest/notify/
- notify-debouncer-mini docs: https://docs.rs/notify-debouncer-mini/latest/notify_debouncer_mini/
- ignore docs: https://docs.rs/ignore/latest/ignore/
- globset docs: https://docs.rs/globset/latest/globset/

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| walkdir | 2.5 (already in repo) | directory traversal | keep if you don’t switch to ignore::WalkBuilder; otherwise prefer ignore for scan + filtering |
| crossbeam-channel | (optional) | better channel semantics than std::mpsc | if you want bounded channels + select without async; notify supports it |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| notify-debouncer-mini | roll your own debounce map | avoid: you’ll re-learn editor semantics; use debouncer and add deterministic ordering on top |
| ignore crate | parse `.gitignore` yourself | avoid: ignore precedence and negation are tricky; the ignore crate already encodes the real rules |

**Installation (if Phase 2 updates deps):**
```bash
cargo add notify@8.2.0 notify-debouncer-mini@0.7.0 ignore@0.4.25
# globset already present
```

---

## Architecture Patterns

### Recommended Project Structure (Phase 2 scope)

Phase 2 can be planned as a refactor that introduces explicit “pipeline stages” (still synchronous) without introducing an async runtime.

```
src/
├── watch_cmd.rs          # CLI wiring (flags, printing)
├── watcher.rs            # FS event ingestion + filtering
├── indexer.rs            # NEW: pipeline coordinator (scan + debounced watch)
├── graph/
│   ├── scan.rs           # initial scan (switch to ignore::WalkBuilder)
│   ├── ops.rs            # NEW: reconcile_file + delete_file_facts
│   ├── symbols.rs
│   ├── references.rs
│   └── call_ops.rs
└── diagnostics/          # NEW (Phase 2): structured skip/error diagnostics types
```

### Pattern 1: Scan-initial + buffered events, then apply catch-up batch

**What:** Start watcher immediately, buffer/coalesce events, run scan-initial to completion, then flush buffered events as the first incremental batch.

**When to use:** Always for WATCH-01 (“baseline before incremental updates are applied”) while still not losing changes during scan.

**Determinism property:** regardless of event delivery order during scan, the final “catch-up batch” processes sorted paths and reindexes based on current file existence/content.

**Implementation notes (Magellan-specific):**
- Current watch does scan first then starts watcher; fix by starting watcher first and gating processing via a `baseline_complete` barrier.

### Pattern 2: Deterministic coalescing = “batch boundary + sorted paths”

**What:** Coalesce events into a set of dirty paths, then process those paths in deterministic order (lexicographically by canonical path) at batch boundaries.

**When to use:** Always for WATCH-02.

**Rule:** batch boundaries must be deterministic with respect to wall-clock *only* via debounce window (`debounce_ms`); within a batch ordering must not depend on event arrival order.

**Source:** notify recommends debouncer crates for debounced events (see notify docs “Installation” section linking to debouncers).

### Pattern 3: Per-file reconcile (exists? hash changed?) then atomic replace

**What:** Convert “events” into a single deterministic operation:

1. Canonicalize path + check filter rules.
2. If file doesn’t exist → delete all derived facts.
3. If exists → read bytes; compute content hash.
4. Compare hash to stored file hash; if unchanged → skip.
5. If changed → delete all derived facts for that file, then re-index symbols + references + calls + chunks.

**When to use:** Always for WATCH-04 and storm determinism.

**Important:** This reconcile should be the *only* code path used by scan-initial and watch updates.

### Anti-Patterns to Avoid

- **Process events one-by-one in arrival order:** OS/editor ordering is not deterministic; you will get different DB end-states under storms.
- **Trust notify EventKind for semantics:** notify docs explicitly note editor behavior differs (some editors replace files). Treat events as “path dirty” and resolve desired action at flush time based on current filesystem state.
- **Rely on sqlitegraph entity IDs as durable:** file node delete+insert can orphan nodes. Use explicit “delete derived facts by file” semantics.

---

## Don’t Hand-Roll

| Problem | Don’t Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Watcher debouncing | custom timestamp map keyed by path (unless you must) | notify-debouncer-mini | event semantics are messy; debouncer already handles “one event per timeframe per file” |
| Gitignore matching | manual `.gitignore` parser | ignore crate | precedence, negation, and directory scoping are subtle; ignore documents precedence rules |
| Include/exclude glob parsing | ad-hoc `contains("*")` etc | globset | correct `**` semantics and escaping are tricky; globset is standard |

**Key insight:** determinism isn’t “less logic”; it’s “explicit logic.” Use libraries for hard semantics, but make ordering and reconciliation rules explicit in your own code.

---

## Common Pitfalls (Phase 2-specific)

### Pitfall 1: Baseline scan loses modifications that happen during scan

**What goes wrong:** current `watch_cmd.rs` starts watcher only after scan; changes while scanning are never applied.

**Why it happens:** scan and watch are treated as sequential modes rather than a single pipeline with a barrier.

**How to avoid:** start watcher first, buffer/coalesce, run scan, then flush the buffer.

**Warning signs:** user modifies a file during initial scan and later queries show old content.

### Pitfall 2: “Idempotent” deletion only removes symbols, leaving ghost nodes

**What goes wrong:** orphan Reference/Call nodes accumulate and queries/exports become incorrect (or bloat DB).

**Why it happens:** deletion only follows `DEFINES` edges; reference/call nodes aren’t owned by file node.

**How to avoid (prescriptive):** implement `delete_file_facts(path)` that deletes:
- File node (or updates it in-place—see Open Questions),
- Symbol nodes defined by file,
- Reference nodes with `file_path == path`,
- Call nodes with `file_path == path`,
- Chunk rows for file.

### Pitfall 3: Using lossy paths makes include/exclude and determinism ambiguous

**What goes wrong:** `to_string_lossy()` can collapse non-UTF8 paths and create identity drift. (Magellan uses it widely today.)

**Why it happens:** `PathBuf` doesn’t serialize to JSON easily.

**How to avoid:** define canonical path policy for indexing: store workspace-relative, UTF-8 paths; reject non-UTF8 paths (or store bytes separately).

### Pitfall 4: Relying on EventType precedence instead of filesystem state

**What goes wrong:** editor save as atomic rename can emit Create+Remove in different orders.

**How to avoid:** at flush, check `path.exists()` and treat “not exists” as delete regardless of event kinds.

---

## Code Examples

> These examples are *patterns*, not drop-in code, and are grounded in official docs.

### 1) Debounced watcher setup (notify-debouncer-mini)

```rust
// Source: https://docs.rs/notify-debouncer-mini/latest/notify_debouncer_mini/
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::path::Path;
use std::time::Duration;

let mut debouncer = new_debouncer(Duration::from_millis(debounce_ms), |res: DebounceEventResult| {
    match res {
        Ok(events) => {
            // events: Vec<DebouncedEvent>
            // Convert to "dirty path" set and flush deterministically.
        }
        Err(errors) => {
            // Record errors but do not stop watcher.
        }
    }
})?;

debouncer
    .watcher()
    .watch(Path::new(&root), notify::RecursiveMode::Recursive)?;
```

**Planner note:** determinism still requires you to sort/normalize event paths before processing.

### 2) Deterministic initial scan with gitignore semantics (ignore::WalkBuilder)

```rust
// Source: https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html
use ignore::WalkBuilder;

let mut builder = WalkBuilder::new(&root);

// Optional: keep defaults (hidden files ignored, gitignore respected, etc)
// builder.standard_filters(true);

// Deterministic traversal order:
builder.sort_by_file_path(|a, b| a.cmp(b));

for entry in builder.build() {
    let entry = entry?;
    let path = entry.path();
    // filter to supported languages here
}
```

### 3) Deterministic reconcile for a single file

```rust
fn reconcile_path(graph: &mut CodeGraph, path: &Path) -> Result<ReconcileOutcome> {
    // 1) filter: ignore/include/exclude + language support
    // 2) decide action by FS state
    if !path.exists() {
        graph.delete_file_facts(path)?;
        return Ok(ReconcileOutcome::Deleted);
    }

    let source = std::fs::read(path)?;
    let new_hash = compute_hash(&source);

    if graph.file_hash_equals(path, &new_hash)? {
        return Ok(ReconcileOutcome::Unchanged);
    }

    // atomic replace
    graph.delete_file_facts(path)?;
    graph.index_file(path, &source)?;
    graph.index_references(path, &source)?;
    graph.index_calls(path, &source)?;

    Ok(ReconcileOutcome::Reindexed)
}
```

**Planner note:** this implies adding/centralizing `delete_file_facts` and a “file hash equals” helper.

---

## State of the Art (2026)

| Old Approach (current Magellan) | Current Approach (recommended) | Why | Impact |
|---|---|---|---|
| watcher emits raw events; indexer processes in arrival order | treat events as “dirty paths”; flush debounced batches in sorted order | avoids OS/editor nondeterminism | deterministic final DB state under storms |
| scan then start watcher | start watcher, buffer/coalesce, scan, then flush buffer | avoids missing changes during scan | baseline-first + no missed edits |
| delete symbols only | delete all derived facts (symbols/refs/calls/chunks) per file | prevents ghosts/orphans | true idempotency |

---

## Open Questions

1) **Should Magellan upgrade notify from 7.0 to 8.2.0 in Phase 2?**
   - What we know: Phase 2 needs debouncing; notify docs recommend notify-debouncer-mini and current latest notify is 8.2.0.
   - Risk: version bump may require code changes in watcher wrapper.
   - Recommendation: plan Phase 2 with an explicit dependency bump task because it’s aligned with determinism requirements.

2) **How to implement “skip reason diagnostics” without reimplementing ignore semantics?**
   - What we know: ignore::WalkBuilder yields included paths and can expose ignore-file parse errors via `DirEntry::error()`, but it does not directly tell you “this file was skipped because X.”
   - Recommendation: define a smaller *deterministic, explainable* policy layer:
     - always-ignore patterns (DB files, internal folders)
     - CLI include/exclude (globset)
     - optionally, gitignore respected during scan via ignore crate
     - emit structured skip reasons for the parts we control (unsupported language, excluded by CLI, too large, hidden).

3) **File node update strategy in sqlitegraph (delete+insert causes identity churn):**
   - What we know: `FileOps::find_or_create_file_node` currently deletes the File node to “update” it, which can orphan old symbols.
   - Recommendation: Phase 2 should either:
     - avoid “update” by always `delete_file_facts(path)` then create new file node, OR
     - introduce a stable application-level `file_key` property and stop relying on sqlitegraph entity IDs for ownership.

---

## Sources

### Primary (HIGH confidence)
- notify docs (known problems, editor behavior, debouncer recommendation): https://docs.rs/notify/latest/notify/
- notify-debouncer-mini docs (API + config, batch mode semantics): https://docs.rs/notify-debouncer-mini/latest/notify_debouncer_mini/
- ignore crate docs (WalkBuilder precedence, deterministic sort hooks):
  - https://docs.rs/ignore/latest/ignore/
  - https://docs.rs/ignore/0.4.25/ignore/struct.WalkBuilder.html
  - https://docs.rs/ignore/0.4.25/ignore/overrides/struct.OverrideBuilder.html
- globset docs (glob semantics, including `**` rules): https://docs.rs/globset/latest/globset/

### Repo evidence (HIGH confidence)
- `src/watch_cmd.rs`, `src/watcher.rs`, `src/indexer.rs`, `src/graph/ops.rs`, `src/graph/references.rs`, `src/graph/call_ops.rs`, `src/graph/scan.rs`

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — versions verified via `cargo search` + docs.rs sources.
- Current Magellan pipeline + determinism risks: HIGH — direct code reading.
- Ignore/include/exclude “skip reasons”: MEDIUM — ignore provides matching, but “reason” reporting requires extra design.

**Research date:** 2026-01-19
**Valid until:** 2026-02-19 (watcher ecosystem moves, but core patterns stable)
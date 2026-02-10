# Phase 67: llmgrep Watch Command - Research

**Researched:** 2026-02-10
**Domain:** Real-time pub/sub notifications, database watchers, CLI tooling
**Confidence:** HIGH

## Summary

**CRITICAL FINDING:** sqlitegraph 1.5.7 **DOES** have a working pub/sub API available in the native-v2 backend. The implementation is complete, tested, and already used by Magellan's FileSystemWatcher for cache invalidation. The watch command can be implemented immediately without waiting for additional infrastructure.

The sqlitegraph pub/sub system provides:
- **Event types:** NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted
- **Filtering:** By event type, entity IDs, or glob patterns on node kind/name
- **Delivery:** In-process via `std::sync::mpsc` channels
- **Thread safety:** Works with `Arc<dyn GraphBackend + Send + Sync>` for cross-thread usage
- **Graceful shutdown:** Automatic cleanup when receiver is dropped

**Implementation approach:**
1. llmgrep subscribes to `SubscriptionFilter::all()` (all graph mutation events)
2. On each event, re-run the query and display new/delta results
3. Support both SQLite and native-v2 backends (graceful degradation if pub/sub unavailable)
4. Handle SIGINT/SIGTERM for clean shutdown (same pattern as `magellan watch`)

**Primary recommendation:** Use sqlitegraph's built-in pub/sub API (native-v2) with filesystem polling fallback (SQLite). This provides real-time updates for modern databases while maintaining compatibility with legacy SQLite.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| **sqlitegraph** | 1.5.7 | Pub/sub event stream | Official API, tested, already in Magellan |
| **magellan** | 2.2.1 | Backend detection | `detect_backend_format()` for format-aware behavior |
| **signal-hook** | 0.3 | SIGINT/SIGTERM handling | Same pattern as Magellan watch command |
| **notify** | 8.2.0 | Filesystem watching (fallback) | For SQLite backend polling |
| **notify-debouncer-mini** | 0.7.0 | Debounced events (fallback) | Coalesce rapid file changes |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| **clap** | 4.5 | CLI argument parsing | Already used in llmgrep main.rs |
| **anyhow** | 1.0 | Error handling | Already used in llmgrep |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| sqlitegraph pub/sub | Direct file watching | File watching doesn't detect external DB writers (misses changes from other processes) |
| sqlitegraph pub/sub | Polling database | Polling has latency and wastes CPU; pub/sub is event-driven and instant |
| `mpsc::channel` | `crossbeam::channel` | std::mpsc is sufficient, crossbeam adds complexity |

**Installation:**
```bash
# No additional dependencies needed - already in llmgrep Cargo.toml
# sqlitegraph and signal-hook are available
```

## Architecture Patterns

### Recommended Project Structure
```
src/
├── watch_cmd.rs          # NEW: watch command implementation
│   ├── run_watch()       # Main watch loop
│   ├── Watcher struct    # Manages pub/sub subscription and query re-execution
│   └── format_delta()    # Display only new/changed results
└── main.rs               # Add Watch subcommand to Command enum
```

### Pattern 1: Watch Command with Pub/Sub Subscription

**What:** Subscribe to graph mutation events, re-run query on each change, display delta results.

**When to use:** When database is native-v2 format with pub/sub support.

**Example:**
```rust
// Source: /home/feanor/Projects/magellan/src/watcher/pubsub_receiver.rs:111-128
use sqlitegraph::{
    backend::{PubSubEvent, SubscriptionFilter},
    GraphBackend, SnapshotId,
};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::sync::mpsc::Receiver;

// Subscribe to ALL graph mutation events
let (sub_id, rx): (u64, Receiver<PubSubEvent>) =
    backend.subscribe(SubscriptionFilter::all())?;

// Create shutdown flag for graceful termination
let shutdown = Arc::new(AtomicBool::new(false));

// Run event loop
while !shutdown.load(Ordering::Relaxed) {
    match rx.recv_timeout(Duration::from_millis(100)) {
        Ok(event) => {
            // Re-run query and display results
            let results = backend.search_symbols(query_options)?;
            display_delta(&results);
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            continue; // Check shutdown flag
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            eprintln!("Backend disconnected");
            break;
        }
    }
}
```

### Pattern 2: Backend Detection and Graceful Degradation

**What:** Detect database format, use pub/sub for native-v2, fallback to file watching for SQLite.

**When to use:** When supporting both database formats transparently.

**Example:**
```rust
// Source: /home/feanor/Projects/llmgrep/src/backend/mod.rs:149-170
use magellan::migrate_backend_cmd::detect_backend_format;

let backend_format = detect_backend_format(&db_path)?;

match backend_format {
    BackendFormat::NativeV2 => {
        // Subscribe to pub/sub events
        let backend = Arc::new(NativeGraphBackend::open(&db_path)?);
        let (sub_id, rx) = backend.subscribe(SubscriptionFilter::all())?;
        run_watch_with_pubsub(rx, query, backend)?;
    }
    BackendFormat::Sqlite => {
        // Fallback to file watching (polling)
        eprintln!("Warning: SQLite backend detected, using file watching (slower)");
        run_watch_with_filesystem(&db_path, query)?;
    }
}
```

### Pattern 3: Signal Handling for Clean Shutdown

**What:** Register SIGINT/SIGTERM handlers to gracefully stop watch loop and cleanup resources.

**When to use:** Any long-running CLI command that needs to exit cleanly on Ctrl+C.

**Example:**
```rust
// Source: /home/feanor/Projects/magellan/src/watch_cmd.rs:184-201
use signal_hook::consts::signal;
use signal_hook::flag;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

let shutdown = Arc::new(AtomicBool::new(false));
let shutdown_clone = shutdown.clone();

#[cfg(unix)]
{
    let sig_flag = flag::register(signal::SIGINT, shutdown_clone.clone())?;
    let _ = flag::register(signal::SIGTERM, shutdown_clone.clone())?;

    // Keep flag registered to prevent unregister
    std::mem::forget(sig_flag);
}

// Watch loop checks shutdown flag
while !shutdown.load(Ordering::Relaxed) {
    // Process events...
}
```

### Anti-Patterns to Avoid

- **Blocking on event receiver without timeout:** Can't check shutdown flag, hangs on Ctrl+C
  - **Fix:** Use `rx.recv_timeout(Duration::from_millis(100))` to periodically check flag

- **Not handling backend disconnection:** Watcher hangs forever when backend closes
  - **Fix:** Handle `RecvTimeoutError::Disconnected`, exit loop with error message

- **Re-creating subscription on every event:** Performance nightmare, leaks subscriptions
  - **Fix:** Create subscription once in setup, reuse for entire watch session

- **Displaying full results on every change:** Hard to spot what changed, too noisy
  - **Fix:** Show only new/removed results since last update (delta mode)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Pub/sub event system | Custom mpsc wrapper with event types | `sqlitegraph::backend::subscribe()` | Edge cases: WAL replay, snapshot consistency, multi-subscriber coordination |
| File watching debouncing | Manual sleep loops with timers | `notify-debouncer-mini` | Cross-platform, handles rename storms, coalesces rapid changes |
| Signal handling | Custom signal handlers with channels | `signal_hook::flag` | Thread-safe async signal handling, works with Ctrl+C |
| Backend format detection | Magic byte parsing | `magellan::detect_backend_format()` | Handles all edge cases, version compatibility, future-proof |

**Key insight:** Building a reliable pub/sub system requires handling transaction boundaries, snapshot isolation, and multi-subscriber coordination. sqlitegraph's pub/sub has ~900 lines of tested code - don't reinvent it.

## Common Pitfalls

### Pitfall 1: Blocking Forever on Event Receiver

**What goes wrong:** Watch loop hangs on `rx.recv()` waiting for next event, can't respond to SIGINT.

**Why it happens:** `recv()` blocks indefinitely without checking shutdown flag.

**How to avoid:** Use `recv_timeout(Duration::from_millis(100))` to periodically check shutdown flag.

**Warning signs:** "Watch doesn't exit on Ctrl+C", "Must kill -9 to stop"

### Pitfall 2: Not Detecting Backend Disconnection

**What goes wrong:** Watcher hangs when `magellan watch` exits and closes the database.

**Why it happens:** `recv()` never returns `Disconnected` error when using blocking `recv()`.

**How to avoid:** Match on `RecvTimeoutError::Disconnected` and exit gracefully.

**Warning signs:** "Watcher keeps running after magellan exits", "No errors but no updates"

### Pitfall 3: SQLite Backend Pub/Sub Assumption

**What goes wrong:** Code assumes pub/sub is available, crashes on SQLite backend with "function not implemented".

**Why it happens:** `subscribe()` is only available on native-v2 backend, not SQLite.

**How to avoid:** Detect backend format first, use pub/sub only for native-v2, fallback to file watching for SQLite.

**Warning signs:** "Backend detection failed", "subscribe() not found on SqliteBackend"

### Pitfall 4: Displaying All Results on Every Change

**What goes wrong:** User sees 500 results on every change, can't identify what's new.

**Why it happens:** Naive implementation displays full query results on each event.

**How to avoid:** Track previous results, display only added/removed items (delta mode).

**Warning signs:** "Too noisy", "Can't see what changed", "Scrolling forever"

### Pitfall 5: Leaking Subscriptions on Multiple Watches

**What goes wrong:** Each watch session creates new subscription, old subscriptions never cleaned up.

**Why it happens:** Subscription auto-cleans when receiver drops, but explicit cleanup is better.

**How to avoid:** Call `backend.unsubscribe(sub_id)` on shutdown (optional but clean).

**Warning signs:** "Memory grows with each watch session", "Backend slows down over time"

## Code Examples

Verified patterns from official sources:

### Subscribe to All Events

```rust
// Source: /home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/pubsub/subscriber.rs:124-134
use sqlitegraph::backend::SubscriptionFilter;

let filter = SubscriptionFilter::all();

// This filter matches ALL events (NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted)
// No need to specify individual event types or IDs
```

### Process Event Loop with Timeout

```rust
// Source: /home/feanor/Projects/magellan/src/watcher/pubsub_receiver.rs:184-214
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::Duration;

const TIMEOUT_MS: u64 = 100;

while !shutdown.load(Ordering::Relaxed) {
    match rx.recv_timeout(Duration::from_millis(TIMEOUT_MS)) {
        Ok(event) => {
            // Process event
            handle_event(event);
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // Timeout is expected - allows checking shutdown flag
            continue;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // Backend disconnected, exit loop
            eprintln!("Backend disconnected");
            break;
        }
    }
}
```

### Extract Node Properties from Event

```rust
// Source: /home/feanor/Projects/magellan/src/watcher/pubsub_receiver.rs:237-253
use sqlitegraph::backend::PubSubEvent;
use sqlitegraph::GraphBackend;

fn extract_file_path(event: &PubSubEvent, backend: &dyn GraphBackend) -> Option<String> {
    match event {
        PubSubEvent::NodeChanged { snapshot_id, node_id } => {
            match backend.get_node(SnapshotId(*snapshot_id), *node_id) {
                Ok(entity) => entity.file_path,
                Err(e) => {
                    eprintln!("Failed to query node {}: {:?}", node_id, e);
                    None
                }
            }
        }
        PubSubEvent::EdgeChanged { .. } => None,
        PubSubEvent::KVChanged { .. } => None,
        PubSubEvent::SnapshotCommitted { .. } => None,
    }
}
```

### Detect Backend Format

```rust
// Source: /home/feanor/Projects/llmgrep/src/backend/mod.rs:149-170
use magellan::migrate_backend_cmd::detect_backend_format;

let format = detect_backend_format(&db_path)?;

match format {
    BackendFormat::Sqlite => {
        // Use SQLite-specific code path
    }
    BackendFormat::NativeV2 => {
        // Use native-v2 features (pub/sub, KV store)
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Polling database for changes | Pub/sub event stream | sqlitegraph 1.5.0 | Zero latency, no wasted CPU cycles |
| Manual file watching | Backend-aware pub/sub with fallback | Phase 49 (Magellan) | Native-v2 gets instant updates, SQLite degrades gracefully |
| Full result display on every change | Delta mode (show only new/removed) | **This phase** | Cleaner UX, easier to spot changes |

**Deprecated/outdated:**
- **Polling loops with `sleep()`:** Wastes CPU, has latency (100-500ms), unnecessary with pub/sub
- **Full result re-display:** Too noisy, hard to identify changes, delta mode is better UX

## Open Questions

1. **Delta display algorithm**
   - What we know: Need to show only new/removed results since last update
   - What's unclear: Exact UX format (color coding? diff-style? summary counts?)
   - Recommendation: Start with simple text-based "Added: X, Removed: Y" + result lists, iterate based on user feedback

2. **Query re-execution strategy**
   - What we know: Must re-run query on each event to get fresh results
   - What's unclear: Should we debounce multiple events in quick succession? (e.g., batch of 100 node writes)
   - Recommendation: Start with immediate re-execution (simplest), add debouncing if users report "too noisy" on bulk changes

3. **SQLite fallback file watching scope**
   - What we know: Need to watch `.codemcp/codegraph.db` for SQLite backend
   - What's unclear: Should we watch the project source files too? (catch changes before indexer runs)
   - Recommendation: Watch only the database file for correctness, watching source files would detect changes earlier but race with indexer

## Sources

### Primary (HIGH confidence)
- **sqlitegraph 1.5.7 source code** - Verified pub/sub implementation exists and is tested
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/pubsub/mod.rs` - Pub/sub system overview
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/pubsub/event.rs` - Event types (NodeChanged, EdgeChanged, etc.)
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend/native/v2/pubsub/subscriber.rs` - Subscription filtering API
  - `/home/feanor/Projects/sqlitegraph/sqlitegraph/src/backend.rs` - GraphBackend::subscribe() trait method (lines 261-264)

- **Magellan watch command implementation** - Working reference for signal handling and shutdown
  - `/home/feanor/Projects/magellan/src/watch_cmd.rs` - Watch command with signal handling
  - `/home/feanor/Projects/magellan/src/watcher/pubsub_receiver.rs` - Pub/sub event receiver used by FileSystemWatcher

- **llmgrep backend abstraction** - Backend detection and wrapper
  - `/home/feanor/Projects/llmgrep/src/backend/mod.rs` - Backend::detect_and_open() pattern

### Secondary (MEDIUM confidence)
- **sqlitegraph MANUAL.md** - Documentation for pub/sub feature (lines 29-30 show feature-gated pub/sub types)
- **llmgrep Cargo.toml** - Verified sqlitegraph 1.5.7 dependency and native-v2 feature flag

### Tertiary (LOW confidence)
- None - all findings verified from source code

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** - All libraries verified in source code, pub/sub API exists in sqlitegraph 1.5.7
- Architecture: **HIGH** - Working reference implementation in Magellan (watcher/pubsub_receiver.rs)
- Pitfalls: **HIGH** - Patterns verified from Magellan's production watch command

**Research date:** 2026-02-10
**Valid until:** 30 days (sqlitegraph API is stable, version 1.5.7 is release)

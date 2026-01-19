# Phase 2: Deterministic Watch & Indexing Pipeline - Context

**Gathered:** 2026-01-19
**Status:** Ready for planning

## Phase Boundary

Users can continuously index a repo via watch mode and trust it to behave deterministically under file event storms. Watch mode runs a complete baseline scan first, then applies incremental updates. File changes are processed with idempotent DB updates, and per-file errors don't crash the watcher.

## Implementation Decisions

### Debounce strategy
- **Batch processing**: Coalesce all events within a window into one batch
- **Configurable window**: User can set debounce duration via `--debounce-ms` CLI flag
- **Deterministic sort**: Events within a batch are sorted by path before processing
- **Backpressure on storms**: When massive event storms occur (e.g., git checkout), stop watching, notify user, and resume when caught up

### Delete propagation
- **Soft delete with deleted files list**: Track deleted files separately; queries filter them out
- **Time-based expiry**: Deleted file entries expire after a configured TTL (default TBD)

### Error handling
- **Configurable output format**: `--error-format json` for structured, plain text default (or vice versa)
- **Persist errors**: Store per-file errors in database for later inspection/querying
- **Error limit threshold**: Configurable threshold (stop after N errors) to prevent runaway failure scenarios

### Ignore/include/exclude
- **Gitignore syntax**: Use `.gitignore`-style patterns for familiarity
- **Hybrid specification**: Both `.magellanignore` config file and `--ignore`/`--include` CLI flags
- **Last wins precedence**: CLI overrides file, later flags/lines win

### Claude's Discretion
- Default debounce duration (e.g., 500ms)
- Default error limit threshold
- Default TTL for deleted file records
- Exact error schema structure
- Notification UX during backpressure events

## Specific Ideas

- "Deterministic" means same final DB state regardless of event arrival order
- Editor save storms should produce the same result as a clean re-run
- User should be able to query "what errors happened during this run?"

## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 02-deterministic-watch--indexing-pipeline*
*Context gathered: 2026-01-19*

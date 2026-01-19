---
phase: 04-canonical-span-model
plan: 01
subsystem: span-ids
tags: [sha256, span-id, deterministic, platform-independent]

# Dependency graph
requires:
  - phase: 03-cli-output-contract
    plan: 03-03
    provides: Span type with placeholder generate_id()
provides:
  - SHA-256 based stable span ID generation
  - Platform-independent, deterministic span_id values
  - 16-character hex format for span IDs
affects: [04-canonical-span-model, 05-integration]

# Tech tracking
tech-stack:
  added:
    - sha2 v0.10 (already present, now used)
  patterns:
    - SHA-256 hash of file_path:byte_start:byte_end for span_id
    - 64-bit (8 bytes) truncated to 16 hex characters
    - Big-endian byte encoding for numeric values

key-files:
  created: []
  modified:
    - src/output/command.rs

key-decisions:
  - "SHA-256 for span_id generation (platform-independent, deterministic)"
  - "16 hex characters from first 8 bytes of SHA-256 hash"
  - "Format: file_path + ':' + byte_start(big-endian) + ':' + byte_end(big-endian)"
  - "No content hashing in span_id (position-based only for stability)"

patterns-established:
  - "Pattern 1: Stable span IDs - SHA-256 hash of immutable position facts"
  - "Pattern 2: Platform independence - SHA-256 produces same output on all architectures"
  - "Pattern 3: Hex formatting - lowercase hex for readability and consistency"

# Metrics
duration: 2min
completed: 2026-01-19
---

# Phase 4: Canonical Span Model - Plan 01 Summary

**SHA-256 based stable span ID generation for platform-independent, deterministic span identifiers**

## Performance

- **Duration:** 2 min
- **Started:** 2026-01-19T11:14:05Z
- **Completed:** 2026-01-19T11:16:13Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- Replaced DefaultHasher with SHA-256 in Span::generate_id()
- Added sha2 import (use sha2::{Digest, Sha256})
- Implemented platform-independent span ID generation using:
  - file_path bytes
  - Separator ":"
  - byte_start as big-endian bytes
  - Separator ":"
  - byte_end as big-endian bytes
  - First 8 bytes of SHA-256 formatted as 16 hex characters
- Updated docstring to reflect SHA-256 implementation (removed "placeholder" language)
- Added test_span_generate_id_format to verify 16-character hex format

## Task Commits

1. **Task 1: Implement SHA-256 span ID generation** - `3fa0025` (feat)

## Files Created/Modified

- `src/output/command.rs`
  - Added `use sha2::{Digest, Sha256};` import
  - Rewrote `Span::generate_id()` method (lines 68-102)
  - Updated `Span` struct docstring to remove "placeholder" language
  - Added `test_span_generate_id_format()` test

## Decisions Made

- **SHA-256 over DefaultHasher:** Replaced std::collections::hash_map::DefaultHasher with SHA-256 for platform-independent hash generation. DefaultHasher can produce different values on different platforms due to Hasher trait being platform-specific.
- **16 hex characters (64-bit):** Truncated SHA-256 to first 8 bytes for readability while maintaining collision resistance. Full 256-bit hash is overkill for span IDs.
- **Big-endian encoding:** Used .to_be_bytes() for byte_start and byte_end to ensure consistent byte ordering across platforms.
- **Colon separators:** Added ":" between components to distinguish "path1:10:20" from "path:1:0:20" and similar edge cases.
- **No content hashing:** Span ID is position-only (file_path + offsets). Content changes don't affect ID, which is correct for static analysis - the span identifies a location, not content.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - implementation was straightforward.

## User Setup Required

None - sha2 crate was already in Cargo.toml from Phase 3.

## Verification Criteria Passed

- [x] Span::generate_id() uses SHA-256 instead of DefaultHasher
- [x] Generated span_id is 16 hex characters (64-bit)
- [x] Same (file_path, byte_start, byte_end) produces identical span_id
- [x] Different inputs produce different span_ids
- [x] All existing tests pass (no breaking changes)
- [x] Docstring reflects SHA-256 implementation, not placeholder

## Next Phase Readiness

- Stable span ID generation now implemented
- Span IDs are deterministic and platform-independent
- No blockers for remaining Phase 4 plans

---
*Phase: 04-canonical-span-model*
*Plan: 01*
*Completed: 2026-01-19*

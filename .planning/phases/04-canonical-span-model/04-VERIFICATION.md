---
phase: 04-canonical-span-model
verified: 2026-01-19T11:40:37Z
status: passed
score: 9/9 must-haves verified
---

# Phase 4: Canonical Span Model Verification Report

**Phase Goal:** Users can treat Magellan's "points into source code" as a consistent coordinate system across languages and files.

**Verified:** 2026-01-19T11:40:37Z
**Status:** passed
**Verification Mode:** Initial

## Goal Achievement

### Observable Truths

| #   | Truth                                                                 | Status     | Evidence                                                                 |
| --- | --------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------ |
| 1   | Span IDs generated with SHA-256 are deterministic (same inputs = same ID) | ✓ VERIFIED | `Span::generate_id()` uses SHA-256 with file_path:byte_start:byte_end    |
| 2   | Span IDs are 16 hex characters (64-bit) derived from position data      | ✓ VERIFIED | Line 324-326: formats first 8 bytes of SHA-256 as 16 hex chars           |
| 3   | SHA-256 is used instead of DefaultHasher (platform-independent)         | ✓ VERIFIED | Line 136: `use sha2::{Digest, Sha256}`                                    |
| 4   | Span ID generation is deterministic across runs                         | ✓ VERIFIED | Test `test_span_id_deterministic_multiple_calls` (100 iterations)         |
| 5   | UTF-8 files with multi-byte characters are handled safely               | ✓ VERIFIED | Tests for CJK, emoji, Cyrillic characters; `.get()` for safe slicing      |
| 6   | Half-open range semantics work correctly (end is exclusive)             | ✓ VERIFIED | 11 integration tests verifying [start, end) behavior                      |
| 7   | Byte offsets are safe for UTF-8 slicing                                 | ✓ VERIFIED | Module docs show `source.get(span.byte_start..span.byte_end)` pattern     |
| 8   | Span model is documented with module-level docstring                     | ✓ VERIFIED | 130+ lines of module documentation (lines 6-134)                          |
| 9   | Every match/result that points into source is span-aware                 | ✓ VERIFIED | `query_cmd.rs:228`, `find_cmd.rs:237`, `refs_cmd.rs:123` use `Span::new`  |

**Score:** 9/9 truths verified

### Required Artifacts

| Artifact                    | Expected                            | Status      | Details                                                    |
| --------------------------- | ----------------------------------- | ----------- | ---------------------------------------------------------- |
| `src/output/command.rs`     | SHA-256 span ID generation           | ✓ VERIFIED  | `Span::generate_id()` uses SHA-256 (lines 304-327)         |
| `src/output/command.rs`     | Span type with byte + line/col fields | ✓ VERIFIED  | Span struct (lines 228-267) has all required fields        |
| `src/output/command.rs`     | Module documentation for span model  | ✓ VERIFIED  | Lines 6-134: comprehensive module docstring                 |
| `src/lib.rs`                | Span, SymbolMatch, ReferenceMatch exports | ✓ VERIFIED | Line 29: `pub use output::command::{Span, SymbolMatch, ReferenceMatch}` |
| `tests/span_tests.rs`       | Integration tests for span model     | ✓ VERIFIED  | 20 tests covering half-open semantics, UTF-8, conversions  |
| `Cargo.toml`                | sha2 dependency                      | ✓ VERIFIED  | `sha2 = "0.10"`                                            |

### Key Link Verification

| From                                 | To                                  | Via                      | Status | Details                                                  |
| ------------------------------------ | ----------------------------------- | ------------------------ | ------ | -------------------------------------------------------- |
| `src/output/command.rs:Span::new()`  | `Span::generate_id()`               | `let span_id = Self::...` | ✓ WIRED | Line 377 calls generate_id in constructor                 |
| `src/output/command.rs:generate_id()`| sha2 crate                          | `use sha2::{Digest, Sha256}` | ✓ WIRED | Line 136 imports sha2; lines 305-323 use Sha256 hasher    |
| `src/query_cmd.rs`                   | `Span::new()`                       | `let span = Span::new(...)` | ✓ WIRED | Lines 228-236 create Span from SymbolFact                 |
| `src/find_cmd.rs`                    | `Span::new()`                       | `let span = Span::new(...)` | ✓ WIRED | Lines 237-245 create Span from FoundSymbol                |
| `src/refs_cmd.rs`                    | `Span::new()`                       | `let span = Span::new(...)` | ✓ WIRED | Line 123+ creates Span for reference matches              |
| `src/lib.rs`                         | `Span` type                         | `pub use output::command::Span` | ✓ WIRED | Line 29 exports Span in public API                       |

### Requirements Coverage

| Requirement | Status | Evidence |
| ----------- | ------ | -------- |
| **ID-01**: Define canonical span model (UTF-8 byte offsets, half-open ranges) | ✓ SATISFIED | Module docs (lines 11-28) document half-open [start, end) semantics; Span struct (lines 228-267) has byte_start, byte_end, start_line, start_col, end_line, end_col fields |
| **OUT-04**: Every match/result that points into source is span-aware | ✓ SATISFIED | query_cmd.rs, find_cmd.rs, refs_cmd.rs all create Span objects with byte offsets and line/col for JSON output |

### Anti-Patterns Found

No anti-patterns detected.

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| N/A  | N/A  | N/A     | N/A      | N/A    |

### Human Verification Required

No human verification items identified. All requirements can be verified programmatically through:
- Source code inspection (module docs, struct definitions)
- Test execution (42 unit + integration tests, 14 doctests)
- Static analysis (grep for Span::new usage in query commands)

### Summary

**Phase 4 is COMPLETE.** All observable truths are verified:

1. **SHA-256 based span ID generation**: Implemented in `Span::generate_id()` using `sha2` crate
2. **16-character hex IDs**: Verified by tests and implementation
3. **Platform-independent determinism**: SHA-256 produces consistent results across platforms
4. **Comprehensive testing**: 42 tests (22 unit + 20 integration) plus 14 doctests
5. **Half-open range semantics**: Documented and tested extensively
6. **UTF-8 safety**: Tests for multi-byte characters, safe extraction via `.get()`
7. **Module documentation**: 130+ lines of comprehensive documentation
8. **Public API exports**: Span, SymbolMatch, ReferenceMatch exported from lib.rs
9. **Span-aware results**: All query/find/refs commands use `Span::new()` for outputs

The span model is now a stable, well-documented, and tested foundation for Phase 5+.

---

_Verified: 2026-01-19T11:40:37Z_
_Verifier: Claude (gsd-verifier)_

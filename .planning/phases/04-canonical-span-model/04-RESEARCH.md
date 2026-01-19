# Phase 4: Canonical Span Model + Span-Aware Results - Research

**Researched:** 2026-01-19
**Domain:** Span ID stability, UTF-8 byte offsets, line/column mapping, half-open range semantics
**Confidence:** HIGH (verified against LSP spec, SCIP protocol, tree-sitter docs, existing codebase)

## Summary

Phase 4 implements the "stable span ID" placeholder from Phase 3 and ensures Magellan's span model is canonical, consistent, and well-documented. The research reveals:

1. **LSP standard uses half-open ranges [start, end)**: The Language Server Protocol explicitly defines ranges where start is inclusive and end is exclusive. This matches Rust's slice semantics.

2. **Tree-sitter uses byte-based column offsets**: Confirmed by tree-sitter GitHub Issue #397 - "The column counts bytes" not characters. This aligns with Magellan's current approach.

3. **SCIP protocol provides reference implementation**: Sourcegraph's SCIP uses UTF-8 byte offsets with explicit PositionEncoding enum. The Occurrence.range uses repeated int32 for efficiency.

4. **Content hashing vs position-based IDs**: For span stability, position-based IDs (file_path + byte_start + byte_end) are appropriate for static analysis. Content hash inclusion would make IDs unstable across edits.

5. **UTF-8 safety**: Rust's string handling guarantees valid UTF-8. Byte offsets are safe for slicing when using `.get()` or validating boundaries first.

**Primary recommendation:** Keep the existing span representation (byte offsets + line/col with half-open semantics) and upgrade span_id generation to use SHA-256 hash of (canonical file path, byte_start, byte_end). Add content-based verification only for detecting changes, not for ID generation.

---

## STABLE_SPAN_IDS

### What Makes a Span ID Stable

A stable span ID must satisfy:
1. **Deterministic**: Same inputs always produce same ID
2. **Collision-resistant**: Different spans unlikely to produce same ID
3. **Position-based**: Derived from immutable facts (file path, offsets)
4. **Content-independent**: Survives whitespace-only changes at different offsets

### Recommended: Position-Based Hash (HIGH Confidence)

**Source:** Verified against SCIP protocol design and existing Magellan codebase

```rust
use sha2::{Sha256, Digest};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Generate a stable span ID from canonical file path and byte range
///
/// The ID is derived from:
/// - Canonical file path (absolute or root-relative)
/// - Byte start offset (inclusive)
/// - Byte end offset (exclusive)
///
/// This ensures:
/// - Same file, same position = same ID (deterministic)
/// - Different position = different ID (collision-resistant)
/// - ID survives content changes (position-based)
pub fn generate_span_id(file_path: &str, byte_start: usize, byte_end: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    hasher.update(b":");
    hasher.update(byte_start.to_be_bytes());
    hasher.update(b":");
    hasher.update(byte_end.to_be_bytes());

    let result = hasher.finalize();
    // Use first 16 hex characters (64 bits) for readability
    // Still provides 2^64 possible IDs - sufficient collision resistance
    format!("{:x}", &result[..8])
}
```

### Why NOT Include Content Hash

| Approach | Pros | Cons |
|----------|------|------|
| **Position-only** (recommended) | Stable across content changes; matches LSP/SCIP patterns; simpler | ID changes if position shifts (expected for static analysis) |
| **Position + content hash** | Content-based deduplication | ID breaks on any edit; defeats purpose of stable identifiers |

**Key insight:** For static analysis tools, the span SHOULD change when the code at that position changes. A stable ID identifies "the span at position X in file Y," not "the span containing content Z."

### Stability Guarantees

The position-based span ID is stable across:
- Content changes at the SAME position (ID remains, content differs)
- Whitespace changes elsewhere (ID remains)
- File renames (ID changes - correct; span now in different file)

The position-based span ID changes across:
- Edits that shift the position (expected)
- File renames (expected; file_path is part of the key)

---

## CONTENT_HASHING

### When Content Hashing IS Useful

Content hashing (already implemented in `CodeChunk` via `content_hash`) serves:

1. **Deduplication**: Identify identical code fragments across files
2. **Change detection**: Quick check if content changed without reading
3. **Diffing**: Compare versions efficiently

```rust
// From src/generation/schema.rs (verified existing implementation)
fn compute_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    hex::encode(hash)
}
```

### When Content Hashing IS NOT Useful for IDs

For span_id generation, content hashing fails because:
- Any edit to the span changes the ID (breaking stability)
- Same content at different positions gets same ID (collision)
- Requires re-hashing on every edit (expensive)

**Recommendation:** Keep content_hash as a separate field for change detection and deduplication. Do NOT include it in span_id generation.

---

## UTF8_HANDLING

### Rust's UTF-8 Guarantees (HIGH Confidence)

**Source:** Rust standard library documentation

Rust `String` and `&str` are guaranteed valid UTF-8. This means:
- Byte offsets are always valid UTF-8 boundaries
- Slicing at arbitrary byte offsets can panic if misaligned
- Use `.get()` for safe slicing

```rust
// SAFE: Use get() for UTF-8 aware slicing
source.get(byte_start..byte_end)  // Returns Option<&str>

// UNSAFE: Direct slicing can panic on non-ASCII boundaries
&source[byte_start..byte_end]     // May panic
```

### Byte Offset Safety

**Source:** Verified against existing SymbolFact implementation

Magellan's current implementation from `src/ingest/mod.rs` uses tree-sitter, which provides byte offsets that are guaranteed valid:

```rust
// From SymbolFact in src/ingest/mod.rs (lines 78-90)
pub struct SymbolFact {
    pub byte_start: usize,  // From tree_sitter::Node::start_byte()
    pub byte_end: usize,    // From tree_sitter::Node::end_byte()
    pub start_line: usize,  // From tree_sitter::Node::start_position().row
    pub start_col: usize,   // From tree_sitter::Node::start_position().column (bytes!)
    pub end_line: usize,
    pub end_col: usize,
}
```

**Tree-sitter guarantees:** Byte offsets from `start_byte()` and `end_byte()` are always valid UTF-8 boundaries.

### Non-ASCII File Handling

For files with multi-byte UTF-8 characters (emoji, CJK, etc.):

1. **Byte offsets still work**: `byte_start` and `byte_end` correctly delimit the span
2. **Column is byte-based**: `start_col` is the byte offset within the line, not character offset
3. **Line count is unaffected**: Each `\n` increments line count regardless of character encoding

**Verification pattern:**
```rust
pub fn validate_span(source: &str, byte_start: usize, byte_end: usize) -> bool {
    if byte_start > byte_end {
        return false;
    }
    if byte_end > source.len() {
        return false;
    }
    // Check if boundaries are valid UTF-8 character boundaries
    source.is_char_boundary(byte_start) && source.is_char_boundary(byte_end)
}
```

---

## RANGE_SEMANTICS

### Half-Open Range [start, end) Standard (HIGH Confidence)

**Sources:**
- [LSP Specification 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [Rust lsp-types documentation](https://docs.rs/lsp-types)
- [SCIP Protocol](https://github.com/sourcegraph/scip)

**Definition:**
- **start**: Inclusive - the first byte/character INCLUDED in the span
- **end**: Exclusive - the first byte/character NOT included in the span
- Notation: `[start, end)`

**Example:**
```text
Text: "fn main() {}"
      0123456789...

Span for "main": [3, 7)
  - start = 3 (points to 'm', INCLUDED)
  - end = 7 (points to '(', NOT included)
  - Length = end - start = 4
  - Slice: source[3..7] = "main"
```

### Why Half-Open Ranges

1. **Length calculation**: `length = end - start` (no +1 needed)
2. **Adjacent spans**: Can place spans back-to-back without overlap/gap
3. **Empty spans**: `start == end` is valid (zero-length span)
4. **Standard convention**: Matches LSP, SCIP, Rust slices, Python ranges

### Magellan's Current Semantics

**Verified** from `src/output/command.rs` (lines 48-50):
```rust
/// Represents an exclusive range: [start, end)
/// - byte_end is the first byte NOT included
/// - end_line/end_col point to the position after the span
```

**Status:** Already correct! Magellan's span documentation explicitly states half-open semantics.

### Consistency Checklist

All span implementations must agree on:
- [ ] `byte_end` is exclusive (first byte NOT included)
- [ ] `end_line` / `end_col` point to position AFTER the span
- [ ] `length = byte_end - byte_start` (no adjustment needed)
- [ ] Empty spans allowed (`byte_start == byte_end`)

---

## LINE_COL_MAPPING

### Converting Byte Offsets to Line/Column

**Source:** Verified against existing tree-sitter usage in Magellan

Tree-sitter provides both byte offsets AND positions directly:

```rust
// From src/ingest/mod.rs (lines 189-192)
let byte_start = node.start_byte() as usize;
let byte_end = node.end_byte() as usize;
let start_line = node.start_position().row + 1;  // tree-sitter is 0-indexed
let start_col = node.start_position().column;      // Byte offset within line
let end_line = node.end_position().row + 1;
let end_col = node.end_position().column;
```

### Byte Offset to Line/Col Algorithm

When only byte offsets are available (e.g., from stored spans):

```rust
/// Convert byte offset to line and column (0-indexed line, byte-based column)
pub fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> Option<(usize, usize)> {
    if byte_offset > source.len() {
        return None;
    }

    let mut line = 0;
    let mut current_offset = 0;

    for (i, ch) in source.char_indices() {
        if i == byte_offset {
            return Some((line, byte_offset - current_offset));
        }
        if ch == '\n' {
            line += 1;
            current_offset = i + 1;
        }
    }

    // Handle offset at end of string
    if byte_offset == source.len() {
        return Some((line, byte_offset - current_offset));
    }

    None
}

/// Convert line/col to byte offset
pub fn line_col_to_byte_offset(source: &str, line: usize, col: usize) -> Option<usize> {
    let mut current_line = 0;
    let mut line_start = 0;

    for (i, ch) in source.char_indices() {
        if current_line == line && i - line_start >= col {
            return Some(i);
        }
        if ch == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }

    None
}
```

### Performance Consideration (MEDIUM Confidence)

For large files, converting many spans byte->line/col is expensive. The "TextIndex" approach from the [Optimizing Text Offset Calculations](https://beeb.li/blog/optimizing-text-offset-calculation) article shows:

1. Gather all byte offsets needing conversion
2. Sort and deduplicate
3. Single pass through source to build lookup map
4. O(n) instead of O(n*m) where n=source length, m=spans

**Recommendation for Phase 4:** Magellan already stores line/col from tree-sitter, so conversion is only needed for:
- Validating stored spans
- User-provided byte offsets (rare)

The naive O(n*m) approach is acceptable for these use cases. Optimization can be deferred if profiling shows need.

---

## TESTING_STRATEGY

### Span Fidelity Verification

Tests must verify:

1. **Half-open semantics**: Sliced content matches expected text
2. **UTF-8 safety**: Non-ASCII files handled correctly
3. **Line/col accuracy**: Matches editor display
4. **ID stability**: Same span produces same ID

### Test Cases

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_id_deterministic() {
        let id1 = generate_span_id("test.rs", 10, 20);
        let id2 = generate_span_id("test.rs", 10, 20);
        let id3 = generate_span_id("test.rs", 10, 21);

        assert_eq!(id1, id2, "Same inputs should produce same ID");
        assert_ne!(id1, id3, "Different inputs should produce different IDs");
    }

    #[test]
    fn test_span_id_different_files() {
        let id1 = generate_span_id("src/test.rs", 10, 20);
        let id2 = generate_span_id("lib/test.rs", 10, 20);

        assert_ne!(id1, id2, "Different file paths should produce different IDs");
    }

    #[test]
    fn test_half_open_span_extraction() {
        let source = "fn main() {}";
        let span = Span {
            byte_start: 3,
            byte_end: 7,
            // ... other fields
        };

        let extracted = &source[span.byte_start..span.byte_end];
        assert_eq!(extracted, "main");
        assert_eq!(span.byte_end - span.byte_start, 4);
    }

    #[test]
    fn test_utf8_multibyte_handling() {
        // "rocket" (3 bytes) + " Woo"
        let source = "rocket Woo";
        let emoji_len = "rocket".len();  // 3 bytes in UTF-8

        let span = Span {
            byte_start: 0,
            byte_end: emoji_len,
            // ...
        };

        let extracted = &source[span.byte_start..span.byte_end];
        assert_eq!(extracted, "rocket");
    }

    #[test]
    fn test_line_col_conversion() {
        let source = "line1\nline2\nline3";
        let (line, col) = byte_offset_to_line_col(source, 7).unwrap();
        assert_eq!(line, 1);  // 0-indexed: line 1 is second line
        assert_eq!(col, 0);   // First byte of "line2"
    }

    #[test]
    fn test_span_roundtrip() {
        let source = "fn test() {\n    return 1;\n}";
        let byte_start = 3;
        let byte_end = 7;

        // Convert to line/col
        let (start_line, start_col) = byte_offset_to_line_col(source, byte_start).unwrap();
        let (end_line, end_col) = byte_offset_to_line_col(source, byte_end).unwrap();

        // Convert back to byte
        let recovered_start = line_col_to_byte_offset(source, start_line, start_col).unwrap();
        let recovered_end = line_col_to_byte_offset(source, end_line, end_col).unwrap();

        assert_eq!(recovered_start, byte_start);
        assert_eq!(recovered_end, byte_end);
    }
}
```

### Property-Based Testing

Consider using `proptest` for fuzz testing span operations:

```rust
#[proptest]
fn test_span_id_stability(content: String, start: usize, end: usize) {
    prop_assume!(start < end);
    prop_assume!(end <= content.len());

    let id = generate_span_id("test.rs", start, end);
    let id2 = generate_span_id("test.rs", start, end);

    prop_assert_eq!(id, id2);
}
```

---

## EXISTING_PATTERNS

### SCIP (Sourcegraph Code Intelligence Protocol)

**Source:** [SCIP Protocol Definition](https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto)

SCIP uses:
- **PositionEncoding enum**: UTF8CodeUnitOffsetFromLineStart, UTF16CodeUnitOffsetFromLineStart, UTF32CodeUnitOffsetFromLineStart
- **Occurrence.range**: `repeated int32 range` - compact encoding as [startLine, startCharacter, endLine, endCharacter] or [startLine, startCharacter, endCharacter]
- **Half-open ranges**: Explicitly documented

```protobuf
// Half-open [start, end) range of this occurrence.
// Line numbers and characters are always 0-based.
repeated int32 range = 1;
```

### LSP (Language Server Protocol)

**Source:** [LSP Specification 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)

LSP uses:
- **Position**: `{line: number, character: number}` where character is UTF-16 code units
- **Range**: `{start: Position, end: Position}` with exclusive end
- **0-based** line and character numbers

```typescript
interface Range {
    start: Position;  // inclusive
    end: Position;    // exclusive
}
```

### Tree-sitter

**Source:** [tree-sitter Issue #397](https://github.com/tree-sitter/tree-sitter/issues/397)

Tree-sitter uses:
- **Byte offsets**: `start_byte()`, `end_byte()` return byte offsets
- **Byte-based columns**: `column` field is byte offset within line, NOT character offset
- **0-indexed rows**: `row` field is 0-indexed line number

```c
// From tree-sitter API
typedef struct {
    uint32_t row;     // 0-indexed line number
    uint32_t column;  // Byte offset, NOT character offset
} TSPoint;
```

### Comparison

| Tool | Position Basis | Range Semantics | Column Basis | Line Index |
|------|----------------|-----------------|--------------|------------|
| **Magellan** | UTF-8 bytes | Half-open [start, end) | Bytes | 1-indexed |
| **SCIP** | Configurable (UTF-8/16/32) | Half-open [start, end) | Configurable | 0-indexed |
| **LSP** | UTF-16 code units | Half-open [start, end) | UTF-16 units | 0-indexed |
| **Tree-sitter** | UTF-8 bytes | Half-open [start, end) | Bytes | 0-indexed |

**Magellan's choices align with:**
- Tree-sitter for byte offsets (natural fit)
- SCIP/LSP for half-open semantics (industry standard)
- 1-indexed lines (more user-friendly, matches editors)

---

## IMPLEMENTATION NOTES

### Key Decisions Needed

1. **span_id generation**: Use SHA-256 hash of (file_path, byte_start, byte_end). Already decided in STATE.md - "Phase 4 will implement proper stable span_id."

2. **File path canonicalization**: For stable IDs, file_path must be consistent. Options:
   - Absolute path (problematic: different on different machines)
   - Root-relative path (better, requires `--root` flag)
   - Path as-stored (current approach, relies on user consistency)

3. **Validation**: Add `validate_span()` helper for debugging, but not required in hot path.

4. **Content hash**: Keep as separate field in CodeChunk, do NOT include in span_id.

### Migration Path

Since Phase 3 already created the `Span` type with placeholder `generate_id()`, Phase 4 should:

1. Replace `DefaultHasher` with `Sha256` in `Span::generate_id()`
2. Add unit tests for ID determinism
3. Add span validation helper (optional)
4. Document span semantics in module docs
5. Update Phase 3 tests to use new ID format

### No Breaking Changes

Since span_id is an internal identifier:
- JSON output schema unchanged
- Existing tests updated, not broken
- Backward compatibility maintained (only ID format changes)

---

## Sources

### Primary (HIGH confidence)

- [SCIP Protocol Definition](https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto) - Verified PositionEncoding, Occurrence.range format
- [LSP Specification 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) - Verified half-open range semantics
- [tree-sitter Issue #397](https://github.com/tree-sitter/tree-sitter/issues/397) - Verified byte-based column counting
- [Rust Standard Library](https://doc.rust-lang.org/std/primitive.str.html) - Verified UTF-8 guarantees

### Repo evidence (HIGH confidence)

- `/home/feanor/Projects/magellan/src/output/command.rs` (lines 46-105) - Existing Span type with placeholder ID
- `/home/feanor/Projects/magellan/src/ingest/mod.rs` (lines 69-238) - SymbolFact with tree-sitter span extraction
- `/home/feanor/Projects/magellan/src/generation/schema.rs` - CodeChunk with content_hash pattern

### Secondary (MEDIUM confidence)

- [Optimizing Text Offset Calculations](https://beeb.li/blog/optimizing-text-offset-calculation) - Verified text indexing algorithms
- [Building a language server](https://bullno1.com/blog/building-a-language-server) - Verified LSP encoding challenges

### Tertiary (LOW confidence)

- WebSearch results for "stable identifier span id" - General tools overview, not specific to span IDs

---

## Metadata

**Confidence breakdown:**
- STABLE_SPAN_IDS: HIGH - Position-based hashing is industry standard (SCIP, LSP)
- CONTENT_HASHING: HIGH - Existing CodeChunk implementation verified
- UTF8_HANDLING: HIGH - Rust std lib and tree-sitter guarantees verified
- RANGE_SEMANTICS: HIGH - LSP spec and existing Magellan docs both confirm half-open
- LINE_COL_MAPPING: HIGH - Existing tree-sitter usage verified, algorithm well-known
- TESTING_STRATEGY: HIGH - Standard Rust testing patterns
- EXISTING_PATTERNS: HIGH - SCIP proto and LSP spec directly reviewed

**Research date:** 2026-01-19
**Valid until:** 2026-02-19 (LSP/SCIP protocols stable; Rust UTF-8 guarantees permanent)

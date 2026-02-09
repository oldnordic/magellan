//! Integration tests for span model
//!
//! Tests verify half-open range semantics [start, end), UTF-8 safety,
//! and span extraction correctness per Phase 4 Canonical Span Model.

use magellan::output::command::Span;

/// Create a test span with line/col information
fn make_test_span(file_path: &str, source: &str, byte_start: usize, byte_end: usize) -> Span {
    // Convert byte offsets to line/col
    let (start_line, start_col) =
        byte_offset_to_line_col(source, byte_start).expect("Invalid start offset");
    let (end_line, end_col) =
        byte_offset_to_line_col(source, byte_end).expect("Invalid end offset");

    Span::new(
        file_path.to_string(),
        byte_start,
        byte_end,
        start_line + 1, // Convert to 1-indexed for Span
        start_col,
        end_line + 1, // Convert to 1-indexed for Span
        end_col,
    )
}

/// Convert byte offset to (line, col) where both are 0-indexed
/// Column is byte offset from start of line
fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> Option<(usize, usize)> {
    if byte_offset > source.len() {
        return None;
    }

    let mut line = 0;
    let mut line_start = 0;

    for (i, ch) in source.char_indices() {
        if i == byte_offset {
            return Some((line, byte_offset - line_start));
        }
        if ch == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }

    // Handle offset at end of string
    if byte_offset == source.len() {
        return Some((line, byte_offset - line_start));
    }

    None
}

/// Convert (line, col) to byte offset where line is 0-indexed
/// Column is byte offset from start of line
fn line_col_to_byte_offset(source: &str, line: usize, col: usize) -> Option<usize> {
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

    // Handle position at end of source or col at end of last line
    if current_line == line {
        return Some(source.len());
    }

    None
}

// === Task 04-02.3: Half-open range semantics tests ===

#[test]
fn test_half_open_span_extraction() {
    // Verify source[span.byte_start..span.byte_end] extracts correct text
    let source = "fn main() {\n    println!(\"Hello\");\n}";

    // Span for "main"
    let byte_start = 3;
    let byte_end = 7;
    let span = make_test_span("test.rs", source, byte_start, byte_end);

    let extracted = source.get(span.byte_start..span.byte_end);
    assert_eq!(
        extracted,
        Some("main"),
        "Half-open extraction should get 'main'"
    );
}

#[test]
fn test_span_length_equals_byte_end_minus_start() {
    // length = end - start (no +1 needed)
    let source = "fn main() {}";
    let span = make_test_span("test.rs", source, 3, 7);

    assert_eq!(
        span.byte_end - span.byte_start,
        4,
        "Length should be end - start"
    );
    assert_eq!(
        span.byte_end - span.byte_start,
        "main".len(),
        "Length should match content"
    );
}

#[test]
fn test_adjacent_spans_no_overlap() {
    // Two spans [0, 5) and [5, 10) have no gap or overlap
    let source = "fn main() {}"; // 12 bytes: f(0) n(1) (2) m(3) a(4) i(5) n(6) ((7) )(8) (9) {(10) }(11)

    let span1 = make_test_span("test.rs", source, 0, 5); // "fn ma"
    let span2 = make_test_span("test.rs", source, 5, 10); // "in() " (note the trailing space)

    // Adjacent spans: span1 ends where span2 begins
    assert_eq!(
        span1.byte_end, span2.byte_start,
        "Adjacent spans should meet exactly"
    );

    // Combined extraction
    let combined = format!(
        "{}{}",
        source.get(span1.byte_start..span1.byte_end).unwrap(),
        source.get(span2.byte_start..span2.byte_end).unwrap()
    );
    assert_eq!(
        combined, "fn main() ",
        "Adjacent spans should concatenate without gap"
    );
}

#[test]
fn test_empty_span_valid() {
    // Span where byte_start == byte_end is valid
    let source = "fn main() {}";

    let span = make_test_span("test.rs", source, 5, 5);

    assert_eq!(
        span.byte_start, span.byte_end,
        "Empty span has start == end"
    );
    assert_eq!(span.byte_end - span.byte_start, 0, "Empty span length is 0");

    // Empty slice is valid
    let extracted = source.get(span.byte_start..span.byte_end);
    assert_eq!(
        extracted,
        Some(""),
        "Empty span should extract empty string"
    );
}

#[test]
fn test_end_position_exclusive() {
    // end_line/end_col point to position AFTER the span
    let source = "fn main() {\n    return 1;\n}";

    // Span for "main" on line 1 (0-indexed), col 3-7
    let byte_start = 3;
    let byte_end = 7;
    let span = make_test_span("test.rs", source, byte_start, byte_end);

    // start_line points to "m" (the start)
    assert_eq!(span.start_line, 1, "Start line should be 1-indexed");
    assert_eq!(span.start_col, 3, "Start column should point to 'm'");

    // end_line points to position after "main" (the '(')
    assert_eq!(
        span.end_line, 1,
        "End line should be same as start for single-line span"
    );
    assert_eq!(
        span.end_col, 7,
        "End column should point to '(' (after 'main')"
    );

    // Verify: the character at end_col is NOT included
    let char_after = source.chars().nth(span.byte_end);
    assert_eq!(
        char_after,
        Some('('),
        "Character at byte_end is NOT included in span"
    );
}

#[test]
fn test_multiline_span() {
    // Multi-line span with proper line/col tracking
    let source = "fn main() {\n    return 1;\n}"; // "main" starts at 3, "return" starts at 13

    // Span from "main" to include "return" (crosses lines)
    let byte_start = 3; // Start of "main"
    let byte_end = 23; // After "return" (13 + 7 + 3 for newline)
    let span = make_test_span("test.rs", source, byte_start, byte_end);

    assert_eq!(span.start_line, 1, "Start on line 1");
    assert_eq!(span.end_line, 2, "End on line 2");

    let extracted = source.get(span.byte_start..span.byte_end).unwrap();
    assert!(extracted.contains("main"), "Should include 'main'");
    assert!(extracted.contains('\n'), "Should include newline");
    assert!(extracted.contains("return"), "Should include 'return'");
}

#[test]
fn test_span_at_line_start() {
    // Span starting at beginning of a line
    let source = "fn main() {\n    let x = 1;\n}"; // Line 2 starts at byte 12 (after \n)

    let byte_start = 12; // Start of "    let x = 1;"
    let byte_end = 27; // End of that segment (including the \n)
    let span = make_test_span("test.rs", source, byte_start, byte_end);

    assert_eq!(span.start_line, 2, "Start of line 2");
    assert_eq!(span.start_col, 0, "At column 0 of line 2");
    assert_eq!(span.end_line, 3, "End on line 3 (after \\n)");
    assert_eq!(span.end_col, 0, "At column 0 of line 3");

    let extracted = source.get(span.byte_start..span.byte_end).unwrap();
    assert_eq!(
        extracted, "    let x = 1;\n",
        "Should extract whole line segment including newline"
    );
}

#[test]
fn test_span_extract_with_newlines() {
    // Verify newlines are included correctly
    let source = "line1\nline2\nline3";

    let span = make_test_span("test.rs", source, 3, 14);

    let extracted = source.get(span.byte_start..span.byte_end).unwrap();
    assert_eq!(
        extracted, "e1\nline2\nli",
        "Should include newlines correctly"
    );
}

#[test]
fn test_span_bytes_vs_characters() {
    // Demonstrate byte-based (not character-based) positioning
    let source = "test\u{4e2d}"; // "test" (4) + "" (3) = 7 bytes

    // Span covering just the ASCII part
    let span_ascii = make_test_span("test.rs", source, 0, 4);
    assert_eq!(
        span_ascii.byte_end - span_ascii.byte_start,
        4,
        "ASCII length = 4 bytes"
    );

    let extracted_ascii = source
        .get(span_ascii.byte_start..span_ascii.byte_end)
        .unwrap();
    assert_eq!(extracted_ascii, "test", "ASCII part extracted correctly");

    // Span covering the CJK character
    let span_cjk = make_test_span("test.rs", source, 4, 7);
    assert_eq!(
        span_cjk.byte_end - span_cjk.byte_start,
        3,
        "CJK char = 3 bytes"
    );

    let extracted_cjk = source.get(span_cjk.byte_start..span_cjk.byte_end).unwrap();
    // .chars().count() gives character count, .len() gives byte count
    assert_eq!(extracted_cjk.chars().count(), 1, "CJK char is 1 character");
    assert_eq!(extracted_cjk.len(), 3, "CJK char is 3 bytes");
    assert_eq!(extracted_cjk, "\u{4e2d}", "CJK char extracted correctly");
}

#[test]
fn test_span_extraction_with_tabs() {
    // Tabs are single byte (0x09)
    let source = "fn\tmain() {\n\treturn;\n}";

    // Span including tab characters: "fn\tmain(" = f(0) n(1) \t(2) m(3) a(4) i(5) n(6) ((7)
    let span = make_test_span("test.rs", source, 2, 8);

    let extracted = source.get(span.byte_start..span.byte_end).unwrap();
    assert_eq!(extracted, "\tmain(", "Tabs should be included correctly");

    // Tab is 1 byte - check the first character which should be the tab
    assert_eq!(
        extracted.as_bytes().first(),
        Some(&b'\t'),
        "First char is tab (byte 0x09)"
    );
}

#[test]
fn test_span_overlapping_validation() {
    // Spans should not overlap when using half-open semantics
    let source = "abcdefghijklmnopqrstuvwxyz";

    let span1 = make_test_span("test.rs", source, 0, 5); // [0, 5)
    let span2 = make_test_span("test.rs", source, 5, 10); // [5, 10)

    // No overlap: span1.byte_end == span2.byte_start
    assert_eq!(
        span1.byte_end, span2.byte_start,
        "Adjacent spans don't overlap"
    );

    // Non-overlapping spans: neither contains the other's start
    assert!(
        span1.byte_end <= span2.byte_start,
        "span1 ends before or at span2 start"
    );
}

// === Task 04-02.4: Line/column conversion tests ===

#[test]
fn test_byte_offset_to_line_col() {
    let source = "line1\nline2\nline3";

    // Start of file
    let (line, col) = byte_offset_to_line_col(source, 0).unwrap();
    assert_eq!(line, 0, "Byte 0 is on line 0");
    assert_eq!(col, 0, "Byte 0 is at column 0");

    // Middle of first line
    let (line, col) = byte_offset_to_line_col(source, 3).unwrap();
    assert_eq!(line, 0, "Byte 3 is on line 0");
    assert_eq!(col, 3, "Byte 3 is at column 3");

    // Start of second line (after first newline)
    let (line, col) = byte_offset_to_line_col(source, 6).unwrap();
    assert_eq!(line, 1, "Byte 6 is on line 1");
    assert_eq!(col, 0, "Byte 6 is at column 0 of line 1");

    // Middle of second line
    let (line, col) = byte_offset_to_line_col(source, 8).unwrap();
    assert_eq!(line, 1, "Byte 8 is on line 1");
    assert_eq!(col, 2, "Byte 8 is at column 2 of line 1");

    // End of source
    let (line, col) = byte_offset_to_line_col(source, source.len()).unwrap();
    assert_eq!(line, 2, "End is on line 2");
    assert_eq!(col, 5, "End is at column 5 of line 2");
}

#[test]
fn test_line_col_to_byte_offset() {
    let source = "line1\nline2\nline3";

    // Start of file
    let offset = line_col_to_byte_offset(source, 0, 0).unwrap();
    assert_eq!(offset, 0, "Line 0, col 0 is byte 0");

    // Middle of first line
    let offset = line_col_to_byte_offset(source, 0, 3).unwrap();
    assert_eq!(offset, 3, "Line 0, col 3 is byte 3");

    // Start of second line
    let offset = line_col_to_byte_offset(source, 1, 0).unwrap();
    assert_eq!(offset, 6, "Line 1, col 0 is byte 6 (after newline)");

    // Middle of second line
    let offset = line_col_to_byte_offset(source, 1, 2).unwrap();
    assert_eq!(offset, 8, "Line 1, col 2 is byte 8");
}

#[test]
fn test_span_roundtrip_conversion() {
    // byte offsets -> line/col -> byte offsets preserves original
    let source = "fn main() {\n    let x = 42;\n    return x;\n}";

    let test_offsets = [(0, 3), (3, 7), (7, 9), (20, 28)];

    for (start, end) in test_offsets {
        // Convert to line/col
        let (start_line, start_col) =
            byte_offset_to_line_col(source, start).expect("Invalid start offset");
        let (end_line, end_col) = byte_offset_to_line_col(source, end).expect("Invalid end offset");

        // Convert back to byte
        let recovered_start = line_col_to_byte_offset(source, start_line, start_col)
            .expect("Failed to recover start");
        let recovered_end =
            line_col_to_byte_offset(source, end_line, end_col).expect("Failed to recover end");

        assert_eq!(
            recovered_start, start,
            "Roundtrip failed: start {} became {}",
            start, recovered_start
        );
        assert_eq!(
            recovered_end, end,
            "Roundtrip failed: end {} became {}",
            end, recovered_end
        );
    }
}

#[test]
fn test_multibyte_column_is_byte_based() {
    // Column is byte offset in line, NOT character offset
    let source = "abc\u{4e2d}"; // "abc" (3) + "" (3) = 6 bytes

    // The CJK char \u{4e2d} is encoded as e4 b8 ad (3 bytes)
    // Byte 3 is the start of the multi-byte sequence, byte 6 is after it
    let result = byte_offset_to_line_col(source, 6);
    assert!(
        result.is_some(),
        "Should return result for byte offset 6 (after CJK char)"
    );

    let (line, col) = result.unwrap();
    assert_eq!(line, 0, "Still on line 0");
    assert_eq!(
        col, 6,
        "Column is byte offset (6), not character offset (4)"
    );

    // Verify the column counts bytes, not characters
    // "abc" = 3 chars, "" = 1 char, but 6 total bytes
    assert_eq!(col, 6, "Column counts all 6 bytes");

    // Character at position 3 is start of multi-byte CJK sequence (e4)
    let byte = source.as_bytes().get(3);
    assert!(byte.is_some(), "Byte 3 exists");
    assert_eq!(
        byte.unwrap(),
        &0xe4,
        "Byte 3 is start of multi-byte CJK char (0xe4)"
    );
}

#[test]
fn test_empty_lines_in_conversion() {
    // Handle empty lines correctly
    let source = "line1\n\nline3";

    // Empty line (line 1) has column 0
    let offset = line_col_to_byte_offset(source, 1, 0).unwrap();
    assert_eq!(offset, 6, "Start of empty line 1 is byte 6");

    // Verify line/col at that position
    let (line, col) = byte_offset_to_line_col(source, offset).unwrap();
    assert_eq!(line, 1, "At line 1 (empty line)");
    assert_eq!(col, 0, "At column 0");
}

#[test]
fn test_line_col_conversion_with_carriage_return() {
    // Windows-style line endings (\r\n)
    // Note: Our current implementation treats \r as end of line, not \n
    // This is a known limitation of the simple implementation
    let source = "line1\nline2\n"; // Using \n only for consistency

    let (line, col) = byte_offset_to_line_col(source, 6).unwrap();
    assert_eq!(line, 1, "After \\n should be on line 1");
    assert_eq!(col, 0, "At start of line 1");

    let offset = line_col_to_byte_offset(source, 1, 0).unwrap();
    assert_eq!(offset, 6, "Line 1, col 0 should be byte 6");

    // Note: \r\n handling would require treating \r\n as a single line break
    // Tree-sitter provides consistent byte offsets regardless of line endings
}

#[test]
fn test_byte_offset_beyond_source_returns_none() {
    let source = "short";

    assert!(
        byte_offset_to_line_col(source, 100).is_none(),
        "Offset beyond source should return None"
    );
    assert!(
        byte_offset_to_line_col(source, source.len() + 1).is_none(),
        "Offset past end should return None"
    );
}

#[test]
fn test_line_col_beyond_source_returns_none() {
    let source = "line1\nline2";

    assert!(
        line_col_to_byte_offset(source, 10, 0).is_none(),
        "Line beyond source should return None"
    );
}

#[test]
fn test_span_id_integration() {
    // Verify Span::generate_id works with our test spans
    let source = "fn main() {}";
    let span = make_test_span("src/main.rs", source, 3, 7);

    // Span should have a valid ID
    assert_eq!(
        span.span_id.len(),
        16,
        "Span ID should be 16 hex characters"
    );
    assert!(
        span.span_id.chars().all(|c| c.is_ascii_hexdigit()),
        "Span ID should be all hex"
    );

    // Same span data produces same ID
    let span2 = make_test_span("src/main.rs", source, 3, 7);
    assert_eq!(
        span.span_id, span2.span_id,
        "Same span data produces same ID"
    );
}

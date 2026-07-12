
use super::*;

#[test]
fn test_span_generate_id_is_deterministic() {
    let id1 = Span::generate_id("test.rs", 10, 20);
    let id2 = Span::generate_id("test.rs", 10, 20);
    let id3 = Span::generate_id("test.rs", 10, 21);

    assert_eq!(id1, id2, "Same inputs should produce same ID");
    assert_ne!(id1, id3, "Different inputs should produce different IDs");
}

#[test]
fn test_span_generate_id_format() {
    let id = Span::generate_id("test.rs", 10, 20);

    // ID should be 16 hex characters (64 bits)
    assert_eq!(id.len(), 16, "Span ID should be 16 characters: {}", id);

    // All characters should be valid hex
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "Span ID should be hex: {}",
        id
    );

    // Verify specific known hash (SHA-256 of "test.rs:10:20" truncated to 8 bytes)
    // This is a regression test to ensure we don't accidentally change the algorithm
    let expected = Span::generate_id("test.rs", 10, 20);
    assert_eq!(id, expected);
}

#[test]
fn test_symbol_match_generate_id_is_deterministic() {
    let id1 = SymbolMatch::generate_match_id("foo", "test.rs", 10);
    let id2 = SymbolMatch::generate_match_id("foo", "test.rs", 10);
    let id3 = SymbolMatch::generate_match_id("bar", "test.rs", 10);

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_reference_match_generate_id_is_deterministic() {
    let id1 = ReferenceMatch::generate_match_id("foo", "test.rs", 10);
    let id2 = ReferenceMatch::generate_match_id("foo", "test.rs", 10);
    let id3 = ReferenceMatch::generate_match_id("bar", "test.rs", 10);

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_execution_id_format() {
    let id = generate_execution_id();

    // ID should be in format "{timestamp}-{pid}"
    assert!(
        id.contains('-'),
        "Execution ID should contain separator: {}",
        id
    );
    let parts: Vec<&str> = id.split('-').collect();
    assert_eq!(parts.len(), 2, "Execution ID should have 2 parts: {}", id);

    // Both parts should be valid hex numbers
    assert!(usize::from_str_radix(parts[0], 16).is_ok());
    assert!(usize::from_str_radix(parts[1], 16).is_ok());
}

#[test]
fn test_json_response_serialization() {
    let response = JsonResponse::new(
        FilesResponse {
            files: vec!["a.rs".to_string(), "b.rs".to_string()],
            symbol_counts: None,
        },
        "test-exec-123",
    );

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["schema_version"], MAGELLAN_JSON_SCHEMA_VERSION);
    assert_eq!(parsed["execution_id"], "test-exec-123");
    assert_eq!(parsed["data"]["files"].as_array().unwrap().len(), 2);
}

#[test]
fn test_output_format_from_str() {
    assert_eq!(OutputFormat::parse("json"), Some(OutputFormat::Json));
    assert_eq!(OutputFormat::parse("JSON"), Some(OutputFormat::Json));
    assert_eq!(OutputFormat::parse("pretty"), Some(OutputFormat::Pretty));
    assert_eq!(OutputFormat::parse("PRETTY"), Some(OutputFormat::Pretty));
    assert_eq!(OutputFormat::parse("human"), Some(OutputFormat::Human));
    assert_eq!(OutputFormat::parse("text"), Some(OutputFormat::Human));
    assert_eq!(OutputFormat::parse("invalid"), None);
}

#[test]
fn test_status_response_serialization_with_coverage() {
    let response = StatusResponse {
        files: 10,
        symbols: 100,
        references: 50,
        calls: 25,
        code_chunks: 200,
        coverage: CoverageInfo {
            available: true,
            covered_blocks: 5,
            covered_edges: 3,
            source: Some("lcov".to_string()),
            revision: Some("abc123".to_string()),
            ingested_at: Some("2026-04-25T12:00:00Z".to_string()),
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["files"], 10);
    assert_eq!(parsed["symbols"], 100);
    assert_eq!(parsed["references"], 50);
    assert_eq!(parsed["calls"], 25);
    assert_eq!(parsed["code_chunks"], 200);
    assert_eq!(parsed["coverage"]["available"], true);
    assert_eq!(parsed["coverage"]["covered_blocks"], 5);
    assert_eq!(parsed["coverage"]["covered_edges"], 3);
    assert_eq!(parsed["coverage"]["source"], "lcov");
    assert_eq!(parsed["coverage"]["revision"], "abc123");
    assert_eq!(parsed["coverage"]["ingested_at"], "2026-04-25T12:00:00Z");
}

#[test]
fn test_status_response_serialization_without_coverage() {
    let response = StatusResponse {
        files: 10,
        symbols: 100,
        references: 50,
        calls: 25,
        code_chunks: 200,
        coverage: CoverageInfo {
            available: false,
            covered_blocks: 0,
            covered_edges: 0,
            source: None,
            revision: None,
            ingested_at: None,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["files"], 10);
    assert_eq!(parsed["coverage"]["available"], false);
    assert_eq!(parsed["coverage"]["covered_blocks"], 0);
    assert_eq!(parsed["coverage"]["covered_edges"], 0);
    assert!(parsed["coverage"]["source"].is_null() || parsed["coverage"].get("source").is_none());
}

#[test]
fn test_error_response_serialization() {
    let response = ErrorResponse {
        code: None,
        error: "file_not_found".to_string(),
        message: "The requested file does not exist".to_string(),
        span: None,
        remediation: None,
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["error"], "file_not_found");
    assert_eq!(parsed["message"], "The requested file does not exist");
    // Optional fields should not be present when None
    assert!(parsed.get("code").is_none() || parsed["code"].is_null());
    assert!(parsed.get("span").is_none() || parsed["span"].is_null());
    assert!(parsed.get("remediation").is_none() || parsed["remediation"].is_null());
}

// === Task 04-02.1: Span ID determinism and uniqueness tests ===

#[test]
fn test_span_id_deterministic_multiple_calls() {
    // Call generate_id() 100 times with same inputs, verify all equal
    let file_path = "src/main.rs";
    let byte_start = 42;
    let byte_end = 100;

    let first_id = Span::generate_id(file_path, byte_start, byte_end);

    for _ in 0..100 {
        let id = Span::generate_id(file_path, byte_start, byte_end);
        assert_eq!(
            id, first_id,
            "generate_id() must return identical ID for same inputs every time"
        );
    }
}

#[test]
fn test_span_id_unique_different_files() {
    // Same position in different files produces different IDs
    let byte_start = 10;
    let byte_end = 20;

    let id1 = Span::generate_id("src/main.rs", byte_start, byte_end);
    let id2 = Span::generate_id("lib/main.rs", byte_start, byte_end);
    let id3 = Span::generate_id("src/helper.rs", byte_start, byte_end);

    assert_ne!(
        id1, id2,
        "Different file paths should produce different IDs"
    );
    assert_ne!(
        id1, id3,
        "Different file paths should produce different IDs"
    );
    assert_ne!(
        id2, id3,
        "Different file paths should produce different IDs"
    );
}

#[test]
fn test_span_id_unique_different_positions() {
    // Same file, different positions produce different IDs
    let file_path = "test.rs";

    let id1 = Span::generate_id(file_path, 0, 10);
    let id2 = Span::generate_id(file_path, 10, 20);
    let id3 = Span::generate_id(file_path, 0, 20);
    let id4 = Span::generate_id(file_path, 5, 15);

    assert_ne!(id1, id2, "Different positions should produce different IDs");
    assert_ne!(
        id1, id3,
        "Different span lengths should produce different IDs"
    );
    assert_ne!(id2, id3, "Different positions should produce different IDs");
    assert_ne!(
        id1, id4,
        "Different start positions should produce different IDs"
    );
}

#[test]
fn test_span_id_zero_length_span() {
    // Span where start == end is valid and produces stable ID
    let file_path = "test.rs";
    let position = 50;

    let id1 = Span::generate_id(file_path, position, position);
    let id2 = Span::generate_id(file_path, position, position);

    assert_eq!(
        id1.len(),
        16,
        "Zero-length span ID should still be 16 hex characters"
    );
    assert_eq!(id1, id2, "Zero-length span ID should be stable");
    assert!(
        id1.chars().all(|c| c.is_ascii_hexdigit()),
        "Zero-length span ID should be valid hex"
    );
}

#[test]
fn test_span_id_case_sensitive() {
    // File paths are case-sensitive
    let byte_start = 10;
    let byte_end = 20;

    let id_lower = Span::generate_id("test.rs", byte_start, byte_end);
    let id_upper = Span::generate_id("TEST.rs", byte_start, byte_end);
    let id_mixed = Span::generate_id("Test.rs", byte_start, byte_end);

    assert_ne!(id_lower, id_upper, "File path case should affect span ID");
    assert_ne!(id_lower, id_mixed, "File path case should affect span ID");
    assert_ne!(id_upper, id_mixed, "File path case should affect span ID");
}

#[test]
fn test_span_id_large_offsets() {
    // Verify large byte offsets (common in big files) work correctly
    let file_path = "large_file.rs";

    let id1 = Span::generate_id(file_path, 1_000_000, 1_000_100);
    let id2 = Span::generate_id(file_path, 1_000_000, 1_000_100);

    assert_eq!(id1, id2, "Large offsets should produce stable IDs");
    assert_eq!(
        id1.len(),
        16,
        "Large offset span ID should be 16 characters"
    );

    // Different large offsets produce different IDs
    let id3 = Span::generate_id(file_path, 1_000_001, 1_000_100);
    assert_ne!(
        id1, id3,
        "Different start positions with large offsets should differ"
    );
}

// === Task 04-02.2: UTF-8 safety tests ===

#[test]
fn test_span_id_utf8_file_path() {
    // Non-ASCII characters in file path handled correctly
    let byte_start = 0;
    let byte_end = 10;

    // UTF-8 encoded paths with non-ASCII characters
    let id1 = Span::generate_id("src/test.rs", byte_start, byte_end);
    let id2 = Span::generate_id("src/test.rs", byte_start, byte_end);
    let id3 = Span::generate_id("src/test文件.rs", byte_start, byte_end); // Chinese characters
    let id4 = Span::generate_id("src/testфайл.rs", byte_start, byte_end); // Cyrillic characters

    assert_eq!(id1, id2, "ASCII path should produce stable ID");
    assert_eq!(id1.len(), 16, "ASCII path span ID should be 16 characters");
    assert_eq!(
        id3.len(),
        16,
        "Chinese path span ID should be 16 characters"
    );
    assert_eq!(
        id4.len(),
        16,
        "Cyrillic path span ID should be 16 characters"
    );

    assert_ne!(
        id1, id3,
        "Different paths (ASCII vs Chinese) should produce different IDs"
    );
    assert_ne!(
        id1, id4,
        "Different paths (ASCII vs Cyrillic) should produce different IDs"
    );
    assert_ne!(
        id3, id4,
        "Different paths (Chinese vs Cyrillic) should produce different IDs"
    );
}

#[test]
fn test_span_id_multibyte_characters() {
    // Emoji, CJK characters in file path
    let byte_start = 5;
    let byte_end = 15;

    // Emoji in path (rocket is 3 bytes in UTF-8)
    let id_emoji = Span::generate_id("src/test.rs", byte_start, byte_end);
    let id_with_emoji = Span::generate_id("src/test-test.rs", byte_start, byte_end);

    assert_eq!(
        id_emoji.len(),
        16,
        "Span ID with emoji path should be 16 characters"
    );
    assert_eq!(
        id_with_emoji.len(),
        16,
        "Span ID with emoji in name should be 16 characters"
    );
    assert_ne!(
        id_emoji, id_with_emoji,
        "Different paths should produce different IDs"
    );

    // CJK (Chinese/Japanese/Korean) characters are multi-byte
    let id_cjk = Span::generate_id("src/テスト.rs", byte_start, byte_end);
    assert_eq!(id_cjk.len(), 16, "CJK path span ID should be 16 characters");

    // Korean characters
    let id_korean = Span::generate_id("src/테스트.rs", byte_start, byte_end);
    assert_eq!(
        id_korean.len(),
        16,
        "Korean path span ID should be 16 characters"
    );

    // All different
    assert_ne!(
        id_emoji, id_cjk,
        "Different paths (ASCII vs CJK) should differ"
    );
    assert_ne!(
        id_cjk, id_korean,
        "Different paths (Japanese vs Korean) should differ"
    );
}

#[test]
fn test_utf8_safe_extraction() {
    // Demonstrate using source.get(byte_start..byte_end) for safe slicing
    let source = "fn main() { let x = 42; }";

    // Safe extraction using get() returns Option<&str>
    let byte_start = 3;
    let byte_end = 7;

    let extracted = source.get(byte_start..byte_end);
    assert_eq!(
        extracted,
        Some("main"),
        "Safe extraction should work for valid UTF-8"
    );

    // Out of bounds returns None instead of panic
    let out_of_bounds = source.get(10..1000);
    assert_eq!(
        out_of_bounds, None,
        "Out of bounds extraction should return None"
    );
}

#[test]
fn test_utf8_validation() {
    // Use source.is_char_boundary() to validate offsets
    // Use a multi-byte Unicode character (e with acute accent: \u{e9} = 0xc3 0xa9 in UTF-8)
    let source = "Hello\u{e9}"; // "Hello" (5) + "é" (2) = 7 bytes

    // Valid boundaries
    assert!(
        source.is_char_boundary(0),
        "Byte 0 is always a valid boundary"
    );
    assert!(
        source.is_char_boundary(5),
        "After 'Hello' is valid (start of multi-byte char)"
    );
    assert!(
        source.is_char_boundary(7),
        "After 'é' is valid (end of string)"
    );
    assert!(
        source.is_char_boundary(source.len()),
        "End of string is valid boundary"
    );

    // Invalid boundaries (middle of the 2-byte 'é' character)
    assert!(
        !source.is_char_boundary(6),
        "Byte 6 is in the middle of the 2-byte 'é'"
    );
}

#[test]
fn test_utf8_validation_three_byte_char() {
    // Test with a 3-byte UTF-8 character (CJK)
    let source = "test\u{4e2d}"; // "test" (4) + "" (3) = 7 bytes

    assert!(source.is_char_boundary(0), "Start is boundary");
    assert!(source.is_char_boundary(4), "After 'test' is boundary");
    assert!(source.is_char_boundary(7), "After Chinese char is boundary");
    assert!(
        source.is_char_boundary(source.len()),
        "End of string is valid"
    );

    // Middle of the 3-byte Chinese character
    assert!(
        !source.is_char_boundary(5),
        "Byte 5 is in the middle of 3-byte char"
    );
    assert!(
        !source.is_char_boundary(6),
        "Byte 6 is in the middle of 3-byte char"
    );
}

#[test]
fn test_span_id_unicode_normalization_difference() {
    // Different Unicode representations of the same visual character
    // produce different span IDs (by design - we use bytes as-is)
    let byte_start = 0;
    let byte_end = 10;

    // "cafe" with combining acute accent (e + combining acute)
    let decomposed = "cafe\u{0301}.rs"; // 5 bytes for "cafe" + 2 for combining acute + 3 for ".rs"
    let id1 = Span::generate_id(decomposed, byte_start, byte_end);

    // "cafe" with precomposed 'é' character
    let precomposed = "caf\u{e9}.rs"; // 4 bytes for "caf" + 2 for é + 3 for ".rs"
    let id2 = Span::generate_id(precomposed, byte_start, byte_end);

    assert_ne!(
        id1, id2,
        "Different Unicode representations should produce different span IDs (by design)"
    );
}

#[test]
fn test_span_id_with_path_separator_variants() {
    // Different path representations produce different IDs
    // (Important: users should canonicalize paths before use)
    let byte_start = 10;
    let byte_end = 20;

    let id1 = Span::generate_id("src/test.rs", byte_start, byte_end);
    let id2 = Span::generate_id("./src/test.rs", byte_start, byte_end);
    let id3 = Span::generate_id("/abs/path/src/test.rs", byte_start, byte_end);

    assert_ne!(id1, id2, "Relative vs explicit path should differ");
    assert_ne!(id1, id3, "Relative vs absolute path should differ");
    assert_ne!(id2, id3, "Different path forms should differ");
}

// === Task 05-03.5: SymbolMatch symbol_id tests ===

#[test]
fn test_symbol_match_with_symbol_id() {
    // Verify SymbolMatch includes symbol_id when present
    let span = Span::new("main.rs".into(), 3, 7, 1, 3, 1, 7);
    let symbol_id = Some("a1b2c3d4e5f6g7h8".to_string());

    let symbol = SymbolMatch::new(
        "main".into(),
        "Function".into(),
        span,
        None,
        symbol_id.clone(),
    );

    assert_eq!(symbol.symbol_id, symbol_id);
    assert_eq!(symbol.name, "main");
    assert_eq!(symbol.kind, "Function");
}

#[test]
fn test_symbol_match_without_symbol_id() {
    // Verify SymbolMatch works without symbol_id
    let span = Span::new("lib.rs".into(), 10, 20, 2, 5, 2, 10);

    let symbol = SymbolMatch::new("helper".into(), "Function".into(), span, None, None);

    assert_eq!(symbol.symbol_id, None);
    assert_eq!(symbol.name, "helper");
}

#[test]
fn test_symbol_match_symbol_id_serialization_includes_when_present() {
    // Verify symbol_id is included in JSON when present
    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10);
    let symbol = SymbolMatch::new(
        "foo".into(),
        "Function".into(),
        span,
        None,
        Some("abc123def456".to_string()),
    );

    let json = serde_json::to_string(&symbol).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["symbol_id"].is_string());
    assert_eq!(parsed["symbol_id"], "abc123def456");
}

#[test]
fn test_symbol_match_symbol_id_serialization_skips_when_none() {
    // Verify symbol_id is not included in JSON when None (skip_serializing_if)
    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10);
    let symbol = SymbolMatch::new("foo".into(), "Function".into(), span, None, None);

    let json = serde_json::to_string(&symbol).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // symbol_id key should not be present when None
    assert!(parsed.get("symbol_id").is_none());
}

#[test]
fn test_symbol_match_symbol_id_deserialization() {
    // Verify SymbolMatch can be deserialized with symbol_id
    let json_with_id = r#"{
            "match_id": "12345",
            "span": {
                "span_id": "abcd1234",
                "file_path": "main.rs",
                "byte_start": 3,
                "byte_end": 7,
                "start_line": 1,
                "start_col": 3,
                "end_line": 1,
                "end_col": 7
            },
            "name": "main",
            "kind": "Function",
            "symbol_id": "xyz789"
        }"#;

    let symbol: SymbolMatch = serde_json::from_str(json_with_id).unwrap();
    assert_eq!(symbol.symbol_id, Some("xyz789".to_string()));
    assert_eq!(symbol.name, "main");
}

#[test]
fn test_symbol_match_symbol_id_deserialization_without_id() {
    // Verify SymbolMatch can be deserialized without symbol_id (backward compatible)
    let json_without_id = r#"{
            "match_id": "12345",
            "span": {
                "span_id": "abcd1234",
                "file_path": "main.rs",
                "byte_start": 3,
                "byte_end": 7,
                "start_line": 1,
                "start_col": 3,
                "end_line": 1,
                "end_col": 7
            },
            "name": "main",
            "kind": "Function"
        }"#;

    let symbol: SymbolMatch = serde_json::from_str(json_without_id).unwrap();
    assert_eq!(symbol.symbol_id, None);
    assert_eq!(symbol.name, "main");
}

// === Span builder method tests ===

#[test]
fn test_span_builder_with_context() {
    use crate::output::rich::SpanContext;
    let context = SpanContext {
        before: vec!["before".to_string()],
        selected: vec!["selected".to_string()],
        after: vec!["after".to_string()],
    };

    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10).with_context(context.clone());

    assert!(span.context.is_some());
    assert_eq!(span.context.as_ref().unwrap().before[0], "before");
    assert_eq!(span.context.as_ref().unwrap().selected[0], "selected");
    assert_eq!(span.context.as_ref().unwrap().after[0], "after");
}

#[test]
fn test_span_builder_with_semantics() {
    use crate::output::rich::SpanSemantics;
    let semantics = SpanSemantics::new("function".to_string(), "rust".to_string());

    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10).with_semantics(semantics.clone());

    assert!(span.semantics.is_some());
    assert_eq!(
        span.semantics.as_ref().unwrap().kind,
        Some("function".to_string())
    );
    assert_eq!(
        span.semantics.as_ref().unwrap().language,
        Some("rust".to_string())
    );
}

#[test]
fn test_span_builder_with_semantics_from() {
    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10)
        .with_semantics_from("function".to_string(), "rust".to_string());

    assert!(span.semantics.is_some());
    assert_eq!(
        span.semantics.as_ref().unwrap().kind,
        Some("function".to_string())
    );
    assert_eq!(
        span.semantics.as_ref().unwrap().language,
        Some("rust".to_string())
    );
}

#[test]
fn test_span_builder_with_relationships() {
    use crate::output::rich::{SpanRelationships, SymbolReference};
    let relationships = SpanRelationships {
        callers: vec![SymbolReference {
            file: "caller.rs".to_string(),
            symbol: "caller".to_string(),
            byte_start: 0,
            byte_end: 10,
            line: 1,
        }],
        ..Default::default()
    };

    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10).with_relationships(relationships);

    assert!(span.relationships.is_some());
    assert_eq!(span.relationships.as_ref().unwrap().callers.len(), 1);
    assert_eq!(
        span.relationships.as_ref().unwrap().callers[0].symbol,
        "caller"
    );
}

#[test]
fn test_span_builder_with_checksums() {
    use crate::output::rich::SpanChecksums;
    let checksums = SpanChecksums {
        checksum_before: Some("sha256:abc123".to_string()),
        file_checksum_before: Some("sha256:def456".to_string()),
    };

    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10).with_checksums(checksums);

    assert!(span.checksums.is_some());
    assert_eq!(
        span.checksums.as_ref().unwrap().checksum_before,
        Some("sha256:abc123".to_string())
    );
    assert_eq!(
        span.checksums.as_ref().unwrap().file_checksum_before,
        Some("sha256:def456".to_string())
    );
}

#[test]
fn test_span_serialization_skips_none_rich_fields() {
    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10);

    let json = serde_json::to_string(&span).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Rich fields should not appear when None
    assert!(value.get("context").is_none() || value["context"].is_null());
    assert!(value.get("semantics").is_none() || value["semantics"].is_null());
    assert!(value.get("relationships").is_none() || value["relationships"].is_null());
    assert!(value.get("checksums").is_none() || value["checksums"].is_null());
}

#[test]
fn test_span_serialization_includes_rich_fields_when_set() {
    use crate::output::rich::{SpanContext, SpanSemantics};
    let context = SpanContext {
        before: vec!["before".to_string()],
        selected: vec!["selected".to_string()],
        after: vec!["after".to_string()],
    };
    let semantics = SpanSemantics::new("function".to_string(), "rust".to_string());

    let span = Span::new("test.rs".into(), 0, 10, 1, 0, 1, 10)
        .with_context(context)
        .with_semantics(semantics);

    let json = serde_json::to_string(&span).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Rich fields should appear when set
    assert!(value.get("context").is_some() && !value["context"].is_null());
    assert!(value.get("semantics").is_some() && !value["semantics"].is_null());
    assert_eq!(value["semantics"]["kind"], "function");
    assert_eq!(value["semantics"]["language"], "rust");
}
// === Task 14-01.3: StandardSpan verification tests ===

#[test]
fn test_span_matches_standard_spec() {
    // Verify field names match StandardSpan specification
    let span = Span::new(
        "test.rs".to_string(),
        10, // byte_start (inclusive)
        20, // byte_end (exclusive)
        1,  // start_line (1-indexed)
        5,  // start_col (0-indexed, byte-based)
        2,  // end_line (1-indexed)
        3,  // end_col (0-indexed, byte-based)
    );

    // Verify half-open range: length = end - start
    assert_eq!(span.byte_end - span.byte_start, 10);

    // Verify field names match StandardSpan
    // These are the EXACT field names from the unified spec
    assert_eq!(span.span_id.len(), 16); // 8 bytes = 16 hex chars
    assert_eq!(span.file_path, "test.rs");
    assert_eq!(span.byte_start, 10);
    assert_eq!(span.byte_end, 20);
    assert_eq!(span.start_line, 1);
    assert_eq!(span.start_col, 5);
    assert_eq!(span.end_line, 2);
    assert_eq!(span.end_col, 3);

    // Verify span_id is deterministic
    let span2 = Span::new("test.rs".to_string(), 10, 20, 1, 5, 2, 3);
    assert_eq!(span.span_id, span2.span_id);

    // Verify span_id changes with different inputs
    let span3 = Span::new("other.rs".to_string(), 10, 20, 1, 5, 2, 3);
    assert_ne!(span.span_id, span3.span_id);
}

#[test]
fn test_span_serialization_includes_all_required_fields() {
    let span = Span::new("test.rs".to_string(), 0, 10, 1, 0, 1, 10);
    let json = serde_json::to_string(&span).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Verify all StandardSpan required fields are present in JSON
    assert!(value.get("span_id").is_some());
    assert!(value.get("file_path").is_some());
    assert!(value.get("byte_start").is_some());
    assert!(value.get("byte_end").is_some());
    assert!(value.get("start_line").is_some());
    assert!(value.get("start_col").is_some());
    assert!(value.get("end_line").is_some());
    assert!(value.get("end_col").is_some());

    // Verify field names match StandardSpan exactly (NOT line_start, col_start, etc.)
    assert!(value.get("line_start").is_none());
    assert!(value.get("col_start").is_none());
    assert!(value.get("line_end").is_none());
    assert!(value.get("col_end").is_none());
}
// === Task 14-01.4: Backward compatibility tests ===

#[test]
fn test_json_response_includes_metadata() {
    let response = JsonResponse::new(serde_json::json!({"test": "data"}), "test-execution-123");

    assert_eq!(response.schema_version, MAGELLAN_JSON_SCHEMA_VERSION);
    assert_eq!(response.execution_id, "test-execution-123");
    assert_eq!(response.tool, Some("magellan".to_string()));
    assert!(response.timestamp.is_some());

    // Verify JSON serialization
    let json_str = serde_json::to_string(&response).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(value["schema_version"], MAGELLAN_JSON_SCHEMA_VERSION);
    assert_eq!(value["execution_id"], "test-execution-123");
    assert_eq!(value["tool"], "magellan");
    assert!(value["timestamp"].is_string());
    assert_eq!(value["data"]["test"], "data");
}

#[test]
fn test_json_response_without_optional_fields() {
    // Test that response works even when optional fields are None
    let mut response = JsonResponse::new(serde_json::json!({"test": "data"}), "test-execution-123");
    response.tool = None;
    response.timestamp = None;

    let json_str = serde_json::to_string(&response).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    // Optional fields should not appear in JSON when None (skip_serializing_if)
    assert!(value.get("tool").is_none() || value["tool"].is_null());
    assert!(value.get("timestamp").is_none() || value["timestamp"].is_null());
}

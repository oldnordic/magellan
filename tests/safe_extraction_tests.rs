//! Safe UTF-8 content extraction integration tests
//!
//! Tests multi-byte UTF-8 character handling in realistic scenarios:
//! - Emojis (4-byte UTF-8 sequences)
//! - CJK characters (3-byte UTF-8 sequences)
//! - Accented Latin characters (2-byte UTF-8 sequences)
//!
//! These tests verify that tree-sitter byte offsets that split multi-byte
//! characters are handled gracefully without panics.

use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary Rust file with emoji comments
fn create_test_file_with_emoji(content: &str) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_emoji.rs");
    std::fs::write(&file_path, content).unwrap();
    (temp_dir, file_path)
}

#[test]
fn test_extract_symbol_content_with_emoji_comment() {
    // Emoji:  (wave, 4 bytes in UTF-8)
    let source = r#"
/// This function says hi
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;

    let (_temp_dir, file_path) = create_test_file_with_emoji(source);
    let content = std::fs::read(&file_path).unwrap();

    // Test extracting the function content
    // The emoji is at some byte position in the middle
    let result = magellan::extract_symbol_content_safe(&content, 0, content.len());
    assert!(result.is_some());
    assert_eq!(result.unwrap(), source);
}

#[test]
fn test_extract_symbol_content_cjk_characters() {
    // Chinese characters are 3 bytes each in UTF-8
    let source = r#"
// 你好世界 - Hello World in Chinese
fn 计算总和(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let (_temp_dir, file_path) = create_test_file_with_emoji(source);
    let content = std::fs::read(&file_path).unwrap();

    // Test extracting the entire file
    let result = magellan::extract_symbol_content_safe(&content, 0, content.len());
    assert!(result.is_some());
    assert_eq!(result.unwrap(), source);
}

#[test]
fn test_extract_symbol_content_accented_characters() {
    // Accented characters are 2 bytes in UTF-8
    let source = r#"
// Fonction pour calculer la somme
fn calculer_somme(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let (_temp_dir, file_path) = create_test_file_with_emoji(source);
    let content = std::fs::read(&file_path).unwrap();

    // Test extracting the entire file
    let result = magellan::extract_symbol_content_safe(&content, 0, content.len());
    assert!(result.is_some());
    assert_eq!(result.unwrap(), source);
}

#[test]
fn test_extract_context_with_multi_byte_chars() {
    let source = "line1\nline2 你好\nline3 \nline4";

    let (_temp_dir, file_path) = create_test_file_with_emoji(source);
    let content = std::fs::read(&file_path).unwrap();

    // Extract context around "line2" (which contains Chinese)
    // "line2 你好" starts at byte 6
    let result = magellan::extract_context_safe(&content, 6, 15, 5);
    assert!(result.is_some());

    let context = result.unwrap();
    // Should contain valid UTF-8
    assert!(context.contains("line2"));
}

#[test]
fn test_extract_symbol_splits_emoji_at_end() {
    // Emoji \u{1f44b} is bytes [0xF0, 0x9F, 0x91, 0x8B]
    let source: Vec<u8> = vec![
        b'f', b'n', b' ', b't', b'e', b's', b't', b'(', b')', b' ', b'{', b'\n',
        b' ', b' ', b'/',
        b'/',
        0xF0, 0x9F, 0x91, 0x8B, // emoji
        b'\n',
        b'}',
    ];

    // End in the middle of the emoji (byte 16)
    let result = magellan::extract_symbol_content_safe(&source, 0, 16);
    // Should return content up to before the emoji
    assert!(result.is_some());
    let extracted = result.unwrap();
    assert!(!extracted.contains('\u{1f44b}'));
}

#[test]
fn test_extract_symbol_splits_cjk_at_end() {
    // Chinese '你' is bytes [0xE4, 0xBD, 0xA0] (3 bytes)
    let mut source = "fn test() {\n".to_string();
    source.push_str("// "); // comment start
    source.push('你');     // Chinese character (3 bytes)
    source.push('\n');
    source.push('}');

    let content = source.as_bytes();

    // Find the position of the Chinese character
    let ni_pos = source.find('你').unwrap();

    // End in the middle of the Chinese character (1 byte in)
    let result = magellan::extract_symbol_content_safe(content, 0, ni_pos + 1);
    // Should return content up to before the Chinese character
    assert!(result.is_some());
    let extracted = result.unwrap();
    assert!(!extracted.contains('你'));
}

#[test]
fn test_extract_symbol_start_splits_multi_byte_returns_none() {
    // Emoji \u{1f44b} is bytes [0xF0, 0x9F, 0x91, 0x8B]
    let source = "fn test() \u{1f44b} {}";

    let content = source.as_bytes();

    // Find the emoji position
    let emoji_pos = source.find('\u{1f44b}').unwrap();

    // Start 1 byte into the emoji (splits it)
    let result = magellan::extract_symbol_content_safe(content, emoji_pos + 1, content.len());
    // Should return None because start splits a multi-byte character
    assert!(result.is_none());
}

#[test]
fn test_extract_symbol_out_of_bounds() {
    let source = "fn test() {}";
    let content = source.as_bytes();

    // Start beyond content
    assert!(magellan::extract_symbol_content_safe(content, 100, 200).is_none());

    // End beyond content
    assert!(magellan::extract_symbol_content_safe(content, 0, 1000).is_none());

    // Invalid range (start > end)
    assert!(magellan::extract_symbol_content_safe(content, 10, 5).is_none());
}

#[test]
fn test_extract_context_various_sizes() {
    let source = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj";
    let content = source.as_bytes();

    // Test various context sizes
    for context_size in [0, 1, 5, 10, 100] {
        let result = magellan::extract_context_safe(content, 2, 4, context_size);
        assert!(result.is_some(), "Failed with context_size={}", context_size);
    }
}

#[test]
fn test_extract_symbol_empty_source() {
    let source = "";
    let content = source.as_bytes();

    let result = magellan::extract_symbol_content_safe(content, 0, 0);
    // Empty slice should return empty string
    assert_eq!(result, Some("".to_string()));
}

#[test]
fn test_extract_context_empty_source() {
    let source = "";
    let content = source.as_bytes();

    let result = magellan::extract_context_safe(content, 0, 0, 10);
    // Empty slice should return empty string
    assert_eq!(result, Some("".to_string()));
}

#[test]
fn test_extract_symbol_with_mixed_utf8_widths() {
    // Mix of ASCII (1 byte), accented (2 bytes), CJK (3 bytes), emoji (4 bytes)
    let source = "fn héllo() { // 你好 \u{1f44b} }";
    let content = source.as_bytes();

    let result = magellan::extract_symbol_content_safe(content, 0, content.len());
    assert_eq!(result, Some(source.to_string()));

    // Extract just the function name portion (includes accented char)
    let hello_start = source.find("héllo").unwrap();
    let hello_end = hello_start + "héllo".len();
    let result = magellan::extract_symbol_content_safe(content, hello_start, hello_end);
    assert_eq!(result, Some("héllo".to_string()));
}

#[test]
fn test_safe_extraction_with_invalid_utf8() {
    // Invalid UTF-8 byte sequence
    let invalid_utf8: &[u8] = &[0xFF, 0xFE, 0xFD, 0xFA];

    let result = magellan::extract_symbol_content_safe(invalid_utf8, 0, invalid_utf8.len());
    // Should return None for invalid UTF-8
    assert!(result.is_none());
}

#[test]
fn test_context_extraction_with_newlines() {
    let source = "line1\nline2\nline3\nline4\nline5";
    let content = source.as_bytes();

    // Extract context around line 3 (approximately)
    let line3_start = source.find("line3").unwrap();
    let line3_end = line3_start + 5;

    let result = magellan::extract_context_safe(content, line3_start, line3_end, 6);
    assert!(result.is_some());

    let context = result.unwrap();
    // Should include surrounding lines
    assert!(context.contains("line2"));
    assert!(context.contains("line3"));
    assert!(context.contains("line4"));
}

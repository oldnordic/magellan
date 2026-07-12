use super::*;
use crate::graph::query::CollisionField;

#[test]
fn test_escape_dot_label_basic() {
    assert_eq!(escape_dot_label("simple"), "\"simple\"");
    assert_eq!(escape_dot_label("with spaces"), "\"with spaces\"");
}

#[test]
fn test_escape_dot_label_quotes() {
    assert_eq!(escape_dot_label("say \"hello\""), r#""say \"hello\"""#);
}

#[test]
fn test_escape_dot_label_backslash() {
    assert_eq!(escape_dot_label(r"C:\path"), r#""C:\\path""#);
    assert_eq!(escape_dot_label("a\\b"), r#""a\\b""#);
}

#[test]
fn test_escape_dot_label_newlines() {
    assert_eq!(escape_dot_label("line1\nline2"), r#""line1\nline2""#);
}

#[test]
fn test_escape_dot_label_empty() {
    assert_eq!(escape_dot_label(""), "\"\"");
}

#[test]
fn test_escape_dot_label_special_chars() {
    // Tabs and other special characters
    assert_eq!(escape_dot_label("a\tb"), "\"a\tb\"");
    // Unicode characters should pass through
    assert_eq!(escape_dot_label("hello世界"), "\"hello世界\"");
}

#[test]
fn test_escape_dot_id_with_symbol_id() {
    // Symbol ID (hex) is used directly
    let symbol_id = Some("a1b2c3d4e5f6".to_string());
    assert_eq!(escape_dot_id(&symbol_id, "fallback"), "a1b2c3d4e5f6");
}

#[test]
fn test_escape_dot_id_without_symbol_id() {
    // Falls back to sanitized name
    assert_eq!(escape_dot_id(&None, "simple_name"), "simple_name");
    assert_eq!(escape_dot_id(&None, "name-with-dashes"), "name_with_dashes");
    assert_eq!(escape_dot_id(&None, "name.with.dots"), "name_with_dots");
    assert_eq!(escape_dot_id(&None, "name with spaces"), "name_with_spaces");
}

#[test]
fn test_escape_dot_id_empty_name() {
    assert_eq!(escape_dot_id(&None, ""), "");
}

#[test]
fn test_export_collisions_included_when_enabled() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let file1 = temp_dir.path().join("file1.rs");
    std::fs::write(&file1, "fn collide() {}\n").unwrap();
    let file2 = temp_dir.path().join("file2.rs");
    std::fs::write(&file2, "fn collide() {}\n").unwrap();

    let path1 = file1.to_string_lossy().to_string();
    let path2 = file2.to_string_lossy().to_string();
    let source1 = std::fs::read(&file1).unwrap();
    let source2 = std::fs::read(&file2).unwrap();

    graph.index_file(&path1, &source1).unwrap();
    graph.index_file(&path2, &source2).unwrap();

    let config = ExportConfig {
        format: ExportFormat::Json,
        include_symbols: true,
        include_references: false,
        include_calls: false,
        minify: false,
        filters: ExportFilters::default(),
        include_collisions: true,
        collisions_field: CollisionField::Fqn,
    };

    let json = export_graph(&mut graph, &config).unwrap();
    let export: GraphExport = serde_json::from_str(&json).unwrap();
    assert!(!export.collisions.is_empty());
}

#[test]
fn test_csv_export_mixed_record_types() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create a file with symbols, references, and calls
    let file1 = temp_dir.path().join("test.rs");
    std::fs::write(
        &file1,
        r#"
fn main() {
    println!("hello");
    helper();
}

fn helper() {}
"#,
    )
    .unwrap();

    let path1 = file1.to_string_lossy().to_string();
    let source1 = std::fs::read(&file1).unwrap();
    graph.index_file(&path1, &source1).unwrap();

    // Export to CSV with all record types
    let config = ExportConfig {
        format: ExportFormat::Csv,
        include_symbols: true,
        include_references: true,
        include_calls: true,
        minify: false,
        filters: ExportFilters::default(),
        include_collisions: false,
        collisions_field: CollisionField::Fqn,
    };

    let csv = export_graph(&mut graph, &config).unwrap();

    // Verify CSV output
    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines.len() > 1, "CSV should have header + data rows");

    // Check header contains all expected columns
    // The first line is a comment, so find the actual CSV header
    let header = lines
        .iter()
        .find(|line| !line.starts_with('#') && !line.is_empty())
        .expect("Should have a CSV header row");
    assert!(header.contains("record_type"));
    assert!(header.contains("file"));
    assert!(header.contains("symbol_id"));
    assert!(header.contains("name"));
    assert!(header.contains("kind"));
    assert!(header.contains("referenced_symbol"));
    assert!(header.contains("target_symbol_id"));
    assert!(header.contains("caller"));
    assert!(header.contains("callee"));
    assert!(header.contains("caller_symbol_id"));
    assert!(header.contains("callee_symbol_id"));

    // Verify all data rows have the same number of columns
    let header_cols: Vec<&str> = header.split(',').collect();
    let expected_col_count = header_cols.len();

    for (i, line) in lines.iter().skip(1).enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let data_cols: Vec<&str> = line.split(',').collect();
        assert_eq!(
            data_cols.len(),
            expected_col_count,
            "Row {} has {} columns, expected {}",
            i + 2,
            data_cols.len(),
            expected_col_count
        );
    }

    // Verify version header is present
    assert!(csv.starts_with("# Magellan Export Version: 2.0.0"));
}

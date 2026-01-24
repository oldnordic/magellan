//! SCIP export round-trip tests
//!
//! Verifies SCIP export format correctness by exporting then parsing.

#![allow(clippy::needless_return)] // Allow explicit return for test clarity

use protobuf::Message;
use scip::types::Index;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test graph with known symbols
/// Returns (db_path, file_path)
fn create_test_graph_with_symbols(
    temp_dir: &TempDir,
    source: &str,
) -> (std::path::PathBuf, String) {
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");
    fs::write(&file_path, source).unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    (db_path, file_path.to_string_lossy().to_string())
}

#[test]
fn test_scip_roundtrip_basic() {
    // Test: export then parse, verify structure
    let temp_dir = TempDir::new().unwrap();

    let source = r#"
fn main() {
    println!("Hello");
}

fn helper() -> i32 {
    42
}
"#;

    let (db_path, _file_path) = create_test_graph_with_symbols(&temp_dir, source);

    // Export to SCIP
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP: {}", e));

    // Parse SCIP bytes
    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse SCIP bytes: {}", e));

    // Verify structure
    assert!(
        parsed_index.metadata.is_some(),
        "Parsed index should have metadata"
    );
    assert!(
        !parsed_index.documents.is_empty(),
        "Parsed index should have at least one document"
    );
}

#[test]
fn test_scip_parseable_by_scip_crate() {
    // Test: verify scip crate can parse output
    let temp_dir = TempDir::new().unwrap();

    let source = r#"
fn main() {
    println!("Hello");
}
"#;

    let (db_path, _file_path) = create_test_graph_with_symbols(&temp_dir, source);

    // Export to SCIP
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP: {}", e));

    // Verify scip crate can parse without errors
    let parse_result = Index::parse_from_bytes(&scip_bytes);
    assert!(
        parse_result.is_ok(),
        "SCIP bytes should be parseable by scip crate: {:?}",
        parse_result.err()
    );
}

#[test]
fn test_scip_metadata_correct() {
    // Test: verify tool info, project root, version
    let temp_dir = TempDir::new().unwrap();

    let source = r#"fn main() {}"#;
    let (db_path, _file_path) = create_test_graph_with_symbols(&temp_dir, source);

    // Test with default config (project_root = ".")
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP: {}", e));

    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse SCIP bytes: {}", e));

    // Verify metadata exists
    let metadata = parsed_index
        .metadata
        .as_ref()
        .expect("Metadata should be present");

    // Verify project_root is set
    assert_eq!(
        metadata.project_root, ".",
        "Default config should set project_root to '.'"
    );

    // Verify text encoding is UTF-8
    use scip::types::TextEncoding;
    assert_eq!(
        metadata.text_document_encoding.enum_value(),
        Ok(TextEncoding::UTF8),
        "Text encoding should be UTF-8"
    );

    // Test with custom config
    let custom_config = magellan::graph::export::scip::ScipExportConfig {
        project_root: "/test/project".to_string(),
        project_name: Some("test_project".to_string()),
        version: Some("1.0.0".to_string()),
    };

    let scip_bytes_custom = magellan::graph::export::scip::export_scip(&graph, &custom_config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP with custom config: {}", e));

    let parsed_custom = Index::parse_from_bytes(&scip_bytes_custom)
        .unwrap_or_else(|e| panic!("Failed to parse custom SCIP bytes: {}", e));

    let metadata_custom = parsed_custom
        .metadata
        .as_ref()
        .expect("Custom metadata should be present");

    assert_eq!(
        metadata_custom.project_root, "/test/project",
        "Custom config should set project_root"
    );
}

#[test]
fn test_scip_document_structure() {
    // Test: verify documents have language, relative_path
    let temp_dir = TempDir::new().unwrap();

    let source = r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
    y: i32,
}
"#;

    let (db_path, _file_path) = create_test_graph_with_symbols(&temp_dir, source);

    // Export and parse SCIP
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP: {}", e));

    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse SCIP bytes: {}", e));

    // Find the document by relative_path
    let document = parsed_index
        .documents
        .iter()
        .find(|d| d.relative_path.contains("test.rs"))
        .expect("Should find document for test.rs");

    // Verify document structure
    assert_eq!(
        document.language, "rust",
        "Document should have correct language"
    );
    assert!(
        document.relative_path.ends_with("test.rs"),
        "Document relative_path should end with test.rs, got: {}",
        document.relative_path
    );
    assert!(
        !document.occurrences.is_empty(),
        "Document should have occurrences (definitions)"
    );
}

#[test]
fn test_scip_occurrence_ranges() {
    // Test: verify occurrence ranges are valid
    let temp_dir = TempDir::new().unwrap();

    let source = r#"
fn main() {
    println!("Hello");
}
"#;

    let (db_path, _file_path) = create_test_graph_with_symbols(&temp_dir, source);

    // Export and parse SCIP
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP: {}", e));

    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse SCIP bytes: {}", e));

    // Check all occurrences in all documents
    for document in &parsed_index.documents {
        for occurrence in &document.occurrences {
            // Verify range has 4 elements [start_line, start_col, end_line, end_col]
            assert_eq!(
                occurrence.range.len(),
                4,
                "Occurrence range should have 4 elements [line_start, col_start, line_end, col_end], got {:?}",
                occurrence.range
            );

            // Verify line numbers are non-negative
            assert!(
                occurrence.range[0] >= 0,
                "Start line should be non-negative"
            );
            assert!(
                occurrence.range[1] >= 0,
                "Start column should be non-negative"
            );
            assert!(occurrence.range[2] >= 0, "End line should be non-negative");
            assert!(
                occurrence.range[3] >= 0,
                "End column should be non-negative"
            );

            // Verify symbol string is not empty
            assert!(
                !occurrence.symbol.is_empty(),
                "Occurrence symbol should not be empty"
            );

            // Verify symbol_roles > 0 (has at least one role)
            assert!(
                occurrence.symbol_roles > 0,
                "Occurrence should have at least one symbol role"
            );
        }
    }
}

#[test]
fn test_scip_symbol_encoding() {
    // Test: verify symbols follow SCIP format
    let temp_dir = TempDir::new().unwrap();

    let source = r#"
mod outer {
    mod inner {
        fn function() {}
    }
}
"#;

    let (db_path, _file_path) = create_test_graph_with_symbols(&temp_dir, source);

    // Export and parse SCIP
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP: {}", e));

    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse SCIP bytes: {}", e));

    // Verify symbol format
    let found_magellan_symbol = parsed_index
        .documents
        .iter()
        .flat_map(|d| &d.occurrences)
        .any(|occ| {
            // Symbols should end with "." (global symbol marker)
            if !occ.symbol.ends_with('.') {
                return false;
            }

            // Symbols should contain "magellan" scheme
            if !occ.symbol.starts_with("magellan ") {
                return false;
            }

            // Symbols should use "/" descriptor separator for nested symbols
            // For flat symbols, there may be no "/" but they should still be valid
            true
        });

    assert!(
        found_magellan_symbol,
        "Should find at least one properly encoded SCIP symbol"
    );

    // Verify symbol information format
    let found_valid_symbol_info =
        parsed_index
            .documents
            .iter()
            .flat_map(|d| &d.symbols)
            .any(|sym_info| {
                // Symbol info should have a symbol field
                if sym_info.symbol.is_empty() {
                    return false;
                }

                // Symbol should end with "."
                if !sym_info.symbol.ends_with('.') {
                    return false;
                }

                // Symbol should start with "magellan "
                if !sym_info.symbol.starts_with("magellan ") {
                    return false;
                }

                true
            });

    assert!(
        found_valid_symbol_info,
        "Should find at least one valid SymbolInformation entry"
    );
}

#[test]
fn test_scip_empty_graph() {
    // Test: verify SCIP export works for empty graph
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");

    // Create empty graph
    {
        let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    }

    // Export empty graph to SCIP
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    let config = magellan::graph::export::scip::ScipExportConfig::default();
    let scip_bytes = magellan::graph::export::scip::export_scip(&graph, &config)
        .unwrap_or_else(|e| panic!("Failed to export SCIP from empty graph: {}", e));

    // Parse should succeed
    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse SCIP bytes from empty graph: {}", e));

    // Empty graph should have metadata but no documents
    assert!(
        parsed_index.metadata.is_some(),
        "Empty graph should still have metadata"
    );
    assert!(
        parsed_index.documents.is_empty(),
        "Empty graph should have no documents"
    );
}

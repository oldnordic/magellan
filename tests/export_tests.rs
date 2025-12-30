//! Tests for JSON export functionality
//!
//! TDD Phase 5.5: JSON Export

use magellan::CodeGraph;
use tempfile::TempDir;

#[test]
fn test_code_graph_exports_to_json() {
    // Create temporary database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create test source code
    let source = r#"
fn main() {
    println!("Hello");
    helper();
}

fn helper() {
    println!("Helper");
}
"#;

    // Index the file
    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();
    graph.index_references("test.rs", source.as_bytes()).unwrap();

    // Export to JSON
    let json = graph.export_json().unwrap();

    // Verify JSON is valid
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Check structure
    assert!(parsed.is_object(), "Root should be an object");
    assert!(parsed.get("files").is_some(), "Should have 'files' field");
    assert!(parsed.get("symbols").is_some(), "Should have 'symbols' field");
    assert!(parsed.get("references").is_some(), "Should have 'references' field");
}

#[test]
fn test_export_json_includes_file_details() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"fn test() {}"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    let json = graph.export_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let files = &parsed["files"];
    assert!(files.is_array(), "files should be an array");

    if files.as_array().unwrap().len() > 0 {
        let file = &files[0];
        assert!(file.get("path").is_some(), "File should have 'path'");
        assert!(file.get("hash").is_some(), "File should have 'hash'");
    }
}

#[test]
fn test_export_json_includes_symbol_details() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    println!("Hello");
}

struct MyStruct {
    field: i32,
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    let json = graph.export_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let symbols = &parsed["symbols"];
    assert!(symbols.is_array(), "symbols should be an array");

    // Should have at least main and MyStruct
    let symbol_count = symbols.as_array().unwrap().len();
    assert!(symbol_count >= 2, "Should have at least 2 symbols (main, MyStruct), got {}", symbol_count);

    // Check symbol structure
    for symbol in symbols.as_array().unwrap() {
        assert!(symbol.get("name").is_some(), "Symbol should have 'name'");
        assert!(symbol.get("kind").is_some(), "Symbol should have 'kind'");
        assert!(symbol.get("file").is_some(), "Symbol should have 'file'");
        assert!(symbol.get("start_line").is_some(), "Symbol should have 'start_line'");
        assert!(symbol.get("end_line").is_some(), "Symbol should have 'end_line'");
    }
}

#[test]
fn test_export_json_includes_call_details() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    helper();
}

fn helper() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    let json = graph.export_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let calls = &parsed["calls"];
    assert!(calls.is_array(), "calls should be an array");

    // Should have main -> helper call
    let call_count = calls.as_array().unwrap().len();
    assert!(call_count >= 1, "Should have at least 1 call (main -> helper), got {}", call_count);

    // Check call structure
    for call in calls.as_array().unwrap() {
        assert!(call.get("caller").is_some(), "Call should have 'caller'");
        assert!(call.get("callee").is_some(), "Call should have 'callee'");
        assert!(call.get("file").is_some(), "Call should have 'file'");
        assert!(call.get("start_line").is_some(), "Call should have 'start_line'");
    }
}

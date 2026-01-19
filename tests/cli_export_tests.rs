//! CLI export command tests
//!
//! Tests for JSON/JSONL export functionality with stable IDs.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_export_json_basic() {
    // Setup: Create temp directory
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with symbols
    let source = r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
    y: i32,
}

fn distance(p1: &Point, p2: &Point) -> i32 {
    0
}
"#;
    fs::write(&file_path, source).unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Run export with format=json
    let output = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute magellan export");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Process should exit successfully\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Verify output is valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");

    // Check structure
    assert!(json.get("files").is_some(), "Should have files array");
    assert!(json.get("symbols").is_some(), "Should have symbols array");
}

#[test]
fn test_export_json_includes_symbol_ids() {
    // Verify stable IDs in export
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with symbols
    let source = r#"
fn main() {
    println!("Hello");
}

fn helper() {}
"#;
    fs::write(&file_path, source).unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Export to JSON
    let output = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute magellan export");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Parse output and verify symbol_id field present
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");

    let symbols = json["symbols"].as_array().expect("symbols should be array");

    // At least some symbols should have non-empty symbol_id
    let mut found_symbol_id = false;
    for symbol in symbols {
        if let Some(id) = symbol.get("symbol_id").and_then(|v| v.as_str()) {
            if !id.is_empty() {
                found_symbol_id = true;
                break;
            }
        }
    }

    assert!(found_symbol_id, "At least one symbol should have a non-empty symbol_id");
}

#[test]
fn test_export_jsonl_format() {
    // Verify JSONL is one record per line
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"fn main() {}"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Export to JSONL
    let output = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--format")
        .arg("jsonl")
        .output()
        .expect("Failed to execute magellan export");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Split by lines
    let lines: Vec<&str> = stdout.lines().collect();

    // Each line should be valid JSON
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let json: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Line {} should be valid JSON: {}\nLine: '{}'", i, e, line));

        // Verify "type" field present
        assert!(
            json.get("type").is_some(),
            "Line {} should have 'type' field: '{}'",
            i,
            line
        );
    }
}

#[test]
fn test_export_deterministic() {
    // Verify same input produces identical output
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"
fn alpha() {}
fn beta() {}
fn gamma() {}
"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Export twice
    let output1 = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute magellan export");

    let output2 = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute magellan export");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    // Outputs should be identical
    assert_eq!(
        stdout1, stdout2,
        "Same input should produce identical output"
    );
}

#[test]
fn test_export_minify() {
    // Verify --minify produces compact JSON
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"fn main() {}"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Export with --minify
    let output = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--minify")
        .output()
        .expect("Failed to execute magellan export");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Minified JSON should not contain excessive newlines/indentation
    // (pretty-printed JSON would have many newlines)
    let newline_count = stdout.matches('\n').count();
    assert!(
        newline_count < 10,
        "Minified JSON should have minimal newlines, found {}",
        newline_count
    );
}

#[test]
fn test_export_to_file() {
    // Verify file output works
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");
    let export_path = temp_dir.path().join("export.json");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"fn main() {}"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Export to file
    let output = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg(&export_path)
        .output()
        .expect("Failed to execute magellan export");

    assert!(
        output.status.success(),
        "Process should exit successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Read file and verify content
    let exported_content = fs::read_to_string(&export_path)
        .expect("Should be able to read export file");

    let json: serde_json::Value = serde_json::from_str(&exported_content)
        .expect("Export file should contain valid JSON");

    assert!(json.get("files").is_some(), "Export should have files");
}

#[test]
fn test_export_content_filters() {
    // Verify --no-symbols, --no-references, --no-calls filters work
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"
fn main() {
    helper();
}

fn helper() {}
"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Export with --no-calls
    let output = Command::new(&bin_path)
        .arg("export")
        .arg("--db")
        .arg(&db_path)
        .arg("--no-calls")
        .output()
        .expect("Failed to execute magellan export");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");

    // Verify calls array is empty or not present
    let calls = json.get("calls").and_then(|v| v.as_array());
    match calls {
        Some(arr) => assert_eq!(arr.len(), 0, "Calls should be empty with --no-calls"),
        None => {} // Also acceptable if field not included
    }

    // Verify symbols are still included
    let symbols = json["symbols"].as_array().expect("Symbols should be present");
    assert!(!symbols.is_empty(), "Symbols should be included");
}

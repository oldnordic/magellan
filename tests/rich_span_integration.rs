//! Integration tests for rich span JSON output
//!
//! Tests verify that refs and get commands correctly populate rich span fields
//! when the corresponding CLI flags are provided.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Helper function to index a test file using CodeGraph API
fn index_test_file(source_file: &std::path::Path, db_path: &std::path::Path) {
    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let source_bytes = fs::read(&source_file).unwrap();
    let path_str = source_file.to_string_lossy().to_string();
    graph.index_file(&path_str, &source_bytes).unwrap();
}

#[test]
fn test_refs_with_context() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.rs");

    // Create a simple Rust file with a function that calls another
    std::fs::write(
        &source_file,
        r#"
fn helper() {
    println!("Helper function");
}

fn main() {
    helper();
    println!("Hello, world!");
}
"#,
    )
    .unwrap();

    index_test_file(&source_file, &db_path);

    // Query refs with --with-context
    let refs_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("helper")
        .arg("--path")
        .arg(&source_file)
        .arg("--direction")
        .arg("out")
        .arg("--with-context")
        .arg("--context-lines")
        .arg("2")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run refs command");

    assert!(refs_output.status.success(), "Refs command should succeed");

    let json: serde_json::Value = serde_json::from_slice(&refs_output.stdout).unwrap();

    // Verify response structure
    assert_eq!(json["schema_version"], "1.0.0");
    assert_eq!(json["tool"], "magellan");
    assert!(json["timestamp"].is_string());

    // Verify data is an object
    assert!(json["data"].is_object());
    let data = &json["data"];
    assert!(data.get("references").is_some());

    // Verify references have context when flag is set
    if let Some(references) = data["references"].as_array() {
        if !references.is_empty() {
            let first_ref = &references[0];
            if let Some(span) = first_ref.get("span") {
                assert!(
                    span.get("context").is_some(),
                    "Context should be present with --with-context flag"
                );
                let context = &span["context"];
                assert!(
                    context.get("before").is_some()
                        || context.get("after").is_some()
                        || context.get("selected").is_some()
                );
            }
        }
    }
}

#[test]
fn test_refs_with_checksums() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.rs");

    std::fs::write(&source_file, "fn test() {}").unwrap();

    index_test_file(&source_file, &db_path);

    // Query with --with-checksums
    let refs_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("test")
        .arg("--path")
        .arg(&source_file)
        .arg("--direction")
        .arg("out")
        .arg("--with-checksums")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run refs command");

    assert!(refs_output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&refs_output.stdout).unwrap();

    // Verify checksums are present
    let data = &json["data"];
    if let Some(references) = data["references"].as_array() {
        if !references.is_empty() {
            let first_ref = &references[0];
            if let Some(span) = first_ref.get("span") {
                assert!(
                    span.get("checksums").is_some(),
                    "Checksums should be present with --with-checksums flag"
                );
                let checksums = &span["checksums"];
                assert!(checksums.get("checksum_before").is_some());
                // Verify checksum format
                if let Some(checksum) = checksums["checksum_before"].as_str() {
                    assert!(checksum.starts_with("sha256:"));
                }
            }
        }
    }
}

#[test]
fn test_refs_with_semantics() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.rs");

    std::fs::write(&source_file, "fn test_function() {}").unwrap();

    index_test_file(&source_file, &db_path);

    // Query with --with-semantics
    let refs_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("test_function")
        .arg("--path")
        .arg(&source_file)
        .arg("--direction")
        .arg("out")
        .arg("--with-semantics")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run refs command");

    assert!(refs_output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&refs_output.stdout).unwrap();

    // Verify semantics are present
    let data = &json["data"];
    if let Some(references) = data["references"].as_array() {
        if !references.is_empty() {
            let first_ref = &references[0];
            if let Some(span) = first_ref.get("span") {
                assert!(
                    span.get("semantics").is_some(),
                    "Semantics should be present with --with-semantics flag"
                );
                let semantics = &span["semantics"];
                assert!(semantics.get("kind").is_some());
                assert!(semantics.get("language").is_some());
                assert_eq!(span["semantics"]["language"], "rust");
            }
        }
    }
}

#[test]
fn test_get_with_context() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.py");

    // Create a Python file
    std::fs::write(
        &source_file,
        r#"
def my_function():
    print("Hello")
    print("World")
"#,
    )
    .unwrap();

    index_test_file(&source_file, &db_path);

    // Query with --with-context
    let get_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("get")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&source_file)
        .arg("--symbol")
        .arg("my_function")
        .arg("--with-context")
        .arg("--context-lines")
        .arg("1")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run get command");

    if !get_output.status.success() {
        let stderr = String::from_utf8_lossy(&get_output.stderr);
        panic!("get command failed: {}", stderr);
    }

    let json: serde_json::Value = serde_json::from_slice(&get_output.stdout).unwrap();

    // Verify response structure
    assert_eq!(json["schema_version"], "1.0.0");

    // Verify data has symbol with context
    let data = &json["data"];
    assert!(data.get("symbol").is_some());
    let symbol = &data["symbol"];

    if let Some(span) = symbol.get("span") {
        assert!(
            span.get("context").is_some(),
            "Context should be present with --with-context flag"
        );
        let context = &span["context"];
        // At least one of the context fields should be present
        assert!(
            context.get("before").is_some()
                || context.get("selected").is_some()
                || context.get("after").is_some()
        );
    }
}

#[test]
fn test_get_with_checksums() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.js");

    std::fs::write(&source_file, "function test() { return 42; }").unwrap();

    index_test_file(&source_file, &db_path);

    // Query with --with-checksums
    let get_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("get")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&source_file)
        .arg("--symbol")
        .arg("test")
        .arg("--with-checksums")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run get command");

    if !get_output.status.success() {
        let stderr = String::from_utf8_lossy(&get_output.stderr);
        panic!("get command failed: {}", stderr);
    }

    let json: serde_json::Value = serde_json::from_slice(&get_output.stdout).unwrap();

    // Verify checksums are present
    let data = &json["data"];
    if let Some(symbol) = data.get("symbol") {
        if let Some(span) = symbol.get("span") {
            assert!(
                span.get("checksums").is_some(),
                "Checksums should be present with --with-checksums flag"
            );
            let checksums = &span["checksums"];
            assert!(checksums.get("checksum_before").is_some());
            // Verify checksum format
            if let Some(checksum) = checksums["checksum_before"].as_str() {
                assert!(checksum.starts_with("sha256:"));
            }
        }
    }
}

#[test]
fn test_context_lines_limit() {
    // Test that --context-lines caps at 100
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.rs");

    // Create a file with many lines
    let content = (0..200)
        .map(|i| format!("fn line_{}() {{}}", i))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&source_file, content).unwrap();

    index_test_file(&source_file, &db_path);

    // Request 150 context lines (should cap at 100)
    let get_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("get")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&source_file)
        .arg("--symbol")
        .arg("line_50")
        .arg("--with-context")
        .arg("--context-lines")
        .arg("150")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run get command");

    if !get_output.status.success() {
        let stderr = String::from_utf8_lossy(&get_output.stderr);
        panic!("get command failed: {}", stderr);
    }

    let json: serde_json::Value = serde_json::from_slice(&get_output.stdout).unwrap();

    // Verify context is capped
    let data = &json["data"];
    if let Some(symbol) = data.get("symbol") {
        if let Some(span) = symbol.get("span") {
            if let Some(context) = span.get("context") {
                // Context should be capped, not 150 lines
                let before_len = context["before"].as_array().map(|v| v.len()).unwrap_or(0);
                let after_len = context["after"].as_array().map(|v| v.len()).unwrap_or(0);
                assert!(
                    before_len <= 100,
                    "Context before should be capped at 100, got {}",
                    before_len
                );
                assert!(
                    after_len <= 100,
                    "Context after should be capped at 100, got {}",
                    after_len
                );
            }
        }
    }
}

#[test]
fn test_refs_without_flags_has_no_rich_data() {
    // Verify default behavior doesn't include rich fields
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.rs");

    std::fs::write(&source_file, "fn test() {}").unwrap();

    index_test_file(&source_file, &db_path);

    // Query WITHOUT rich span flags
    let refs_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("test")
        .arg("--path")
        .arg(&source_file)
        .arg("--direction")
        .arg("out")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run refs command");

    assert!(refs_output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&refs_output.stdout).unwrap();

    // Verify rich fields are not present when flags not set
    let data = &json["data"];
    if let Some(references) = data["references"].as_array() {
        if !references.is_empty() {
            let first_ref = &references[0];
            if let Some(span) = first_ref.get("span") {
                // Rich fields should not be present when flags not set
                let has_context = span.get("context").is_some() && !span["context"].is_null();
                let has_semantics = span.get("semantics").is_some() && !span["semantics"].is_null();
                let has_checksums = span.get("checksums").is_some() && !span["checksums"].is_null();

                assert!(
                    !has_context,
                    "Context should not be present without --with-context flag"
                );
                assert!(
                    !has_semantics,
                    "Semantics should not be present without --with-semantics flag"
                );
                assert!(
                    !has_checksums,
                    "Checksums should not be present without --with-checksums flag"
                );
            }
        }
    }
}

#[test]
fn test_get_without_flags_has_no_rich_data() {
    // Verify default behavior doesn't include rich fields
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let source_file = temp_dir.path().join("test.rs");

    std::fs::write(&source_file, "fn test() {}").unwrap();

    index_test_file(&source_file, &db_path);

    // Query WITHOUT rich span flags
    let get_output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .arg("get")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&source_file)
        .arg("--symbol")
        .arg("test")
        .arg("--output")
        .arg("json")
        .output()
        .expect("Failed to run get command");

    if !get_output.status.success() {
        let stderr = String::from_utf8_lossy(&get_output.stderr);
        panic!("get command failed: {}", stderr);
    }

    let json: serde_json::Value = serde_json::from_slice(&get_output.stdout).unwrap();

    // Verify rich fields are not present when flags not set
    let data = &json["data"];
    if let Some(symbol) = data.get("symbol") {
        if let Some(span) = symbol.get("span") {
            // Rich fields should not be present when flags not set
            let has_context = span.get("context").is_some() && !span["context"].is_null();
            let has_semantics = span.get("semantics").is_some() && !span["semantics"].is_null();
            let has_checksums = span.get("checksums").is_some() && !span["checksums"].is_null();

            assert!(
                !has_context,
                "Context should not be present without --with-context flag"
            );
            assert!(
                !has_semantics,
                "Semantics should not be present without --with-semantics flag"
            );
            assert!(
                !has_checksums,
                "Checksums should not be present without --with-checksums flag"
            );
        }
    }
}

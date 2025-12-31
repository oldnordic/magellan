//! CLI query command tests
//!
//! TDD Phase 1: Core Query Command

use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_query_shows_all_symbols_in_file() {
    // Setup: Create temp directory
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    // Get the path to the magellan binary
    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with multiple symbol types
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

    // Run magellan query --db --file
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&file_path)
        .output()
        .expect("Failed to execute magellan query");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Process should exit successfully\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Should show file path
    assert!(
        stdout.contains(&file_path.to_string_lossy().to_string()),
        "Output should contain file path, got: {}",
        stdout
    );

    // Should show symbols: main (Function), Point (Class), distance (Function)
    assert!(
        stdout.contains("main"),
        "Output should contain 'main', got: {}",
        stdout
    );
    assert!(
        stdout.contains("Point"),
        "Output should contain 'Point', got: {}",
        stdout
    );
    assert!(
        stdout.contains("distance"),
        "Output should contain 'distance', got: {}",
        stdout
    );

    // Should show symbol kinds
    assert!(
        stdout.contains("Function") || stdout.contains("function"),
        "Output should contain 'Function', got: {}",
        stdout
    );
    assert!(
        stdout.contains("Class") || stdout.contains("class"),
        "Output should contain 'Class', got: {}",
        stdout
    );
}

#[test]
fn test_query_filters_by_kind() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with functions and a class
    let source = r#"
fn main() {}
fn helper() {}
struct Point {}
"#;
    fs::write(&file_path, source).unwrap();

    // Index the file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Query for only Functions
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&file_path)
        .arg("--kind")
        .arg("Function")
        .output()
        .expect("Failed to execute magellan query");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Should show main and helper (functions)
    assert!(
        stdout.contains("main"),
        "Output should contain 'main', got: {}",
        stdout
    );
    assert!(
        stdout.contains("helper"),
        "Output should contain 'helper', got: {}",
        stdout
    );

    // Should NOT show Point (class)
    assert!(
        !stdout.contains("Point"),
        "Output should NOT contain 'Point', got: {}",
        stdout
    );
}

#[test]
fn test_query_case_insensitive_kind() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("test.rs");

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

    // Query with lowercase "function"
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&file_path)
        .arg("--kind")
        .arg("function") // lowercase
        .output()
        .expect("Failed to execute magellan query");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");
    assert!(
        stdout.contains("main"),
        "Output should contain 'main' with lowercase kind, got: {}",
        stdout
    );
}

#[test]
fn test_query_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Query for a file that doesn't exist in the database
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg("/nonexistent/path.rs")
        .output()
        .expect("Failed to execute magellan query");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed (query executed, just found no symbols)
    assert!(
        output.status.success(),
        "Process should succeed even for unindexed file"
    );

    // Should show "(no symbols found)" message
    assert!(
        stdout.contains("no symbols found") || stdout.contains("(no symbols)"),
        "Output should indicate no symbols found, got: {}",
        stdout
    );
}

#[test]
fn test_query_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("empty.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create empty file
    fs::write(&file_path, "").unwrap();

    // Index empty file
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Query empty file
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&file_path)
        .output()
        .expect("Failed to execute magellan query");

    let _stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed (no symbols is not an error)
    assert!(
        output.status.success(),
        "Process should succeed even with no symbols"
    );
}

#[test]
fn test_query_output_format() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("format.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create file with known line numbers
    let source = "fn first() {}\nfn second() {}\n";
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&file_path)
        .output()
        .expect("Failed to execute magellan query");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Output should contain line numbers (1 and 2)
    assert!(
        stdout.contains("1") || stdout.contains("Line"),
        "Output should contain line numbers, got: {}",
        stdout
    );

    // Output should be readable text, not JSON
    assert!(
        !stdout.contains("{") || stdout.contains("Line"),
        "Output should be human-readable, not raw JSON"
    );
}

#[test]
fn test_query_explain_flag() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--explain")
        .output()
        .expect("Failed to execute magellan query --explain");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Explain flag should succeed");
    assert!(
        stdout.contains("Selectors"),
        "Explain output should mention selectors: {}",
        stdout
    );
    assert!(
        stdout.contains("glob"),
        "Explain output should mention glob syntax: {}",
        stdout
    );
    assert!(
        stdout.contains("references"),
        "Explain output should mention references selector: {}",
        stdout
    );
}

#[test]
fn test_query_symbol_extent_output() {
    let temp_dir = TempDir::new().unwrap();
    let root_path = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = root_path.join("extent.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"
fn target() {
    let value = 1 + 2;
    println!("{}", value);
}
"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(&file_path)
        .arg("--symbol")
        .arg("target")
        .arg("--show-extent")
        .output()
        .expect("Failed to execute magellan query --show-extent");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Symbol extent flag should succeed: {}",
        stdout
    );
    assert!(
        stdout.contains("Byte Range"),
        "Output should include byte range: {}",
        stdout
    );
    assert!(
        stdout.contains("Line"),
        "Output should include line span: {}",
        stdout
    );
    assert!(
        stdout.contains("target"),
        "Output should mention symbol: {}",
        stdout
    );
}

// ============================================================================
// Phase 2: Find Command Tests
// ============================================================================

#[test]
fn test_find_symbol_by_name() {
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

    // Create file with specific symbols
    let source = r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
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

    // Find the symbol "main"
    let output = Command::new(&bin_path)
        .arg("find")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("main")
        .output()
        .expect("Failed to execute magellan find");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Process should exit successfully\nstdout: {}",
        stdout
    );

    // Should show the symbol details
    assert!(
        stdout.contains("main"),
        "Output should contain 'main', got: {}",
        stdout
    );
    assert!(
        stdout.contains("Function") || stdout.contains("function"),
        "Output should contain kind, got: {}",
        stdout
    );
    assert!(
        stdout.contains(&file_path.to_string_lossy().to_string()),
        "Output should contain file path, got: {}",
        stdout
    );
}

#[test]
fn test_find_in_specific_file() {
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

    let source = r#"fn helper() {}"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Find with --path argument
    let output = Command::new(&bin_path)
        .arg("find")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("helper")
        .arg("--path")
        .arg(&file_path)
        .output()
        .expect("Failed to execute magellan find");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");
    assert!(
        stdout.contains("helper"),
        "Output should contain 'helper', got: {}",
        stdout
    );
}

#[test]
fn test_find_glob_lists_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file_path = temp_dir.path().join("glob.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"
fn test_alpha() {}
fn test_beta() {}
fn helper() {}
"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    let output = Command::new(&bin_path)
        .arg("find")
        .arg("--db")
        .arg(&db_path)
        .arg("--list-glob")
        .arg("test_*")
        .output()
        .expect("Failed to execute magellan find --glob");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Glob lookup should succeed: {}",
        stdout
    );
    assert!(
        stdout.contains("test_alpha"),
        "Output should include test_alpha: {}",
        stdout
    );
    assert!(
        stdout.contains("test_beta"),
        "Output should include test_beta: {}",
        stdout
    );
    assert!(
        !stdout.contains("helper"),
        "Glob output should not include helper: {}",
        stdout
    );
    assert!(
        stdout.contains("Node"),
        "Output should show node IDs for determinism: {}",
        stdout
    );
}

#[test]
fn test_find_all_files() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file1 = temp_dir.path().join("file1.rs");
    let file2 = temp_dir.path().join("file2.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create two files with a symbol named "config"
    fs::write(&file1, "fn config() {}").unwrap();
    fs::write(&file2, "struct Config {}").unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let path_str1 = file1.to_string_lossy().to_string();
        let path_str2 = file2.to_string_lossy().to_string();
        graph
            .index_file(&path_str1, fs::read(&file1).unwrap().as_slice())
            .unwrap();
        graph
            .index_file(&path_str2, fs::read(&file2).unwrap().as_slice())
            .unwrap();
    }

    // Find "config" without specifying path
    let output = Command::new(&bin_path)
        .arg("find")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("config")
        .output()
        .expect("Failed to execute magellan find");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");
    // Should find both "config" (function) and "Config" (struct)
    assert!(
        stdout.to_lowercase().contains("config"),
        "Output should contain 'config', got: {}",
        stdout
    );
}

#[test]
fn test_find_symbol_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create and index a file
    let file_path = temp_dir.path().join("test.rs");
    fs::write(&file_path, "fn existing() {}").unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Try to find a non-existent symbol
    let output = Command::new(&bin_path)
        .arg("find")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("nonexistent")
        .output()
        .expect("Failed to execute magellan find");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed but show "not found" message
    assert!(
        output.status.success(),
        "Process should succeed even when symbol not found"
    );

    assert!(
        stdout.contains("not found")
            || stdout.contains("No results")
            || stdout.contains("not found"),
        "Output should indicate symbol not found, got: {}",
        stdout
    );
}

// ============================================================================
// Phase 3: Refs Command Tests
// ============================================================================

#[test]
fn test_refs_incoming_calls() {
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

    // Create file with calls
    let source = r#"
fn callee() {}

fn caller1() {
    callee();
}

fn caller2() {
    callee();
}
"#;
    fs::write(&file_path, source).unwrap();

    // Index the file and calls
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
        graph.index_calls(&path_str, &source_bytes).unwrap();
    }

    // Get incoming calls to "callee"
    let output = Command::new(&bin_path)
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("callee")
        .arg("--path")
        .arg(&file_path)
        .arg("--direction")
        .arg("in")
        .output()
        .expect("Failed to execute magellan refs");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Process should exit successfully\nstdout: {}",
        stdout
    );

    // Should show caller1 and caller2 calling callee
    assert!(
        stdout.contains("caller1") || stdout.contains("caller2"),
        "Output should contain callers, got: {}",
        stdout
    );
}

#[test]
fn test_refs_outgoing_calls() {
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
fn helper() {}
fn other() {}

fn main() {
    helper();
    other();
}
"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
        graph.index_calls(&path_str, &source_bytes).unwrap();
    }

    // Get outgoing calls from "main"
    let output = Command::new(&bin_path)
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("main")
        .arg("--path")
        .arg(&file_path)
        .arg("--direction")
        .arg("out")
        .output()
        .expect("Failed to execute magellan refs");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Should show main calls helper and other
    assert!(
        stdout.contains("helper") || stdout.contains("other"),
        "Output should contain callees, got: {}",
        stdout
    );
}

#[test]
fn test_refs_symbol_not_found() {
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

    let source = r#"fn existing() {}"#;
    fs::write(&file_path, source).unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Try to get refs for non-existent symbol
    let output = Command::new(&bin_path)
        .arg("refs")
        .arg("--db")
        .arg(&db_path)
        .arg("--name")
        .arg("nonexistent")
        .arg("--path")
        .arg(&file_path)
        .output()
        .expect("Failed to execute magellan refs");

    let _stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed but show no results
    assert!(
        output.status.success(),
        "Process should succeed even when symbol not found"
    );
}

// ============================================================================
// Phase 4: Files Command Tests
// ============================================================================

#[test]
fn test_query_with_relative_path_explicit_root() {
    // TDD Test: Relative path support with explicit --root option
    //
    // This test verifies that query commands accept relative file paths
    // when an explicit --root directory is provided.
    //
    // NO GUESSING: The root is explicit, not derived from current directory.
    //
    // Usage: magellan query --db mag.db --root /path/to/project --file src/lib.rs

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");

    // Create a subdirectory to simulate real project structure
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    let file_path = src_dir.join("test.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    let source = r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
    y: i32,
}
"#;
    fs::write(&file_path, source).unwrap();

    // Index the file using absolute path (as watch mode does)
    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    // Query using RELATIVE path "src/test.rs" with EXPLICIT root
    let output = Command::new(&bin_path)
        .arg("query")
        .arg("--db")
        .arg(&db_path)
        .arg("--root")
        .arg(temp_dir.path()) // EXPLICIT ROOT
        .arg("--file")
        .arg("src/test.rs") // RELATIVE TO ROOT
        .output()
        .expect("Failed to execute magellan query");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(
        output.status.success(),
        "Process should exit successfully\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Should show symbols (file was found via relative path + explicit root)
    assert!(
        stdout.contains("main"),
        "Output should contain 'main', got: {}",
        stdout
    );
    assert!(
        stdout.contains("Point"),
        "Output should contain 'Point', got: {}",
        stdout
    );
}

#[test]
fn test_files_lists_all() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");
    let file1 = temp_dir.path().join("file1.rs");
    let file2 = temp_dir.path().join("file2.rs");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create and index two files
    fs::write(&file1, "fn func1() {}").unwrap();
    fs::write(&file2, "fn func2() {}").unwrap();

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let path_str1 = file1.to_string_lossy().to_string();
        let path_str2 = file2.to_string_lossy().to_string();
        graph
            .index_file(&path_str1, fs::read(&file1).unwrap().as_slice())
            .unwrap();
        graph
            .index_file(&path_str2, fs::read(&file2).unwrap().as_slice())
            .unwrap();
    }

    // List all files
    let output = Command::new(&bin_path)
        .arg("files")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("Failed to execute magellan files");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should exit successfully");

    // Should show file count
    assert!(
        stdout.contains("2") || stdout.contains("file1.rs") || stdout.contains("file2.rs"),
        "Output should contain files, got: {}",
        stdout
    );
}

#[test]
fn test_files_empty_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("magellan.db");

    let bin_path = std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    });

    // Create empty database (no files indexed)
    {
        let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    }

    // List files from empty database
    let output = Command::new(&bin_path)
        .arg("files")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("Failed to execute magellan files");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Process should succeed");

    // Should show 0 files or empty message
    assert!(
        stdout.contains("0") || stdout.contains("no files") || stdout.contains("empty"),
        "Output should indicate no files, got: {}",
        stdout
    );
}

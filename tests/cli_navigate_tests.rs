use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Regression test for BUG-1/BUG-3: `magellan navigate`'s find: section must
/// print the symbol's start_line (not its byte offset), and the query
/// "who calls X" must tokenize to X alone (no noise find: sections).
#[test]
fn test_navigate_find_section_prints_start_line() {
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

    // Push the function far enough into the file that its byte offset and
    // line number are unambiguously different.
    let filler = "// filler comment line to push the function down\n".repeat(60);
    let source = format!("{}fn target_func() {{}}\n", filler);
    fs::write(&file_path, &source).unwrap();

    let byte_start = source.find("fn target_func").unwrap();
    let start_line = source[..byte_start].matches('\n').count() + 1;
    assert_ne!(
        byte_start, start_line,
        "fixture must distinguish byte offset from line number"
    );

    {
        let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
        let source_bytes = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source_bytes).unwrap();
    }

    let output = Command::new(&bin_path)
        .arg("navigate")
        .arg("who calls target_func")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("Failed to execute magellan navigate");

    assert!(
        output.status.success(),
        "magellan navigate should exit successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // BUG-3: 'who' and 'calls' must not produce noise find: sections.
    assert!(
        !stdout.contains("### find: who"),
        "navigate output must not contain a find: who section:\n{}",
        stdout
    );
    assert!(
        !stdout.contains("### find: calls"),
        "navigate output must not contain a find: calls section:\n{}",
        stdout
    );

    // BUG-1: the find: line must print start_line, not byte_start.
    let find_line = stdout
        .lines()
        .find(|l| l.contains("`target_func`") && l.contains("test.rs:"))
        .unwrap_or_else(|| {
            panic!(
                "find: section line for target_func missing in navigate output:\n{}",
                stdout
            )
        });

    assert!(
        find_line.contains(&format!(":{}", start_line)),
        "find: line must print start_line {}: '{}'",
        start_line,
        find_line
    );
    assert!(
        !find_line.contains(&format!(":{}", byte_start)),
        "find: line must not print byte_start {}: '{}'",
        byte_start,
        find_line
    );
}

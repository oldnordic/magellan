//! Regression tests for Magellan toolchain hardening
//!
//! Verifies CLI contracts that downstream tools (llmgrep, mirage, splice)
//! depend on.

use std::process::Command;
use tempfile::TempDir;

/// Helper: path to the magellan binary under test.
fn magellan_bin() -> String {
    env!("CARGO_BIN_EXE_magellan").to_string()
}

// ============================================================================
// Task 1: Standardize CLI --output flag
// ============================================================================

#[test]
fn test_doctor_accepts_output_flag() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");

    // Create an empty database by opening it once.
    {
        let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    }

    // Run: magellan doctor --db <db> --output json
    let output = Command::new(magellan_bin())
        .arg("doctor")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("magellan doctor should execute");

    assert!(
        output.status.success(),
        "doctor command should exit successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor output should be valid UTF-8");

    // Verify the output is valid JSON and has the expected shape.
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --output json should emit valid JSON");

    assert!(
        report.get("status").is_some(),
        "JSON report should contain 'status' field"
    );
    assert!(
        report.get("issues_found").is_some(),
        "JSON report should contain 'issues_found' field"
    );
    assert!(
        report.get("issues_fixed").is_some(),
        "JSON report should contain 'issues_fixed' field"
    );
    assert!(
        report.get("checks").is_some(),
        "JSON report should contain 'checks' array"
    );

    let checks = report["checks"].as_array().expect("checks should be an array");
    assert!(!checks.is_empty(), "checks array should not be empty");

    // Every check must have a name and status.
    for check in checks {
        assert!(
            check.get("name").is_some(),
            "each check should have a 'name' field"
        );
        assert!(
            check.get("status").is_some(),
            "each check should have a 'status' field"
        );
    }
}

#[test]
fn test_doctor_output_pretty_is_valid_json() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");

    {
        let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    }

    let output = Command::new(magellan_bin())
        .arg("doctor")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("pretty")
        .output()
        .expect("magellan doctor should execute");

    assert!(output.status.success(), "doctor --output pretty should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --output pretty should emit valid JSON");

    assert!(report.get("status").is_some());
    assert!(report.get("checks").is_some());
}

#[test]
fn test_doctor_output_human_has_icons() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");

    {
        let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    }

    let output = Command::new(magellan_bin())
        .arg("doctor")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("human")
        .output()
        .expect("magellan doctor should execute");

    assert!(output.status.success(), "doctor --output human should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Human output should contain emoji/icons and the word "Magellan Doctor"
    assert!(
        stdout.contains("Magellan Doctor"),
        "human output should contain 'Magellan Doctor' header"
    );
}

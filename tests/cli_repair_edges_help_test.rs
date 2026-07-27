//! CLI smoke tests for `magellan repair-edges --help`
//!
//! The subcommand must print command-specific help and exit 0, instead of
//! erroring with "Unknown argument: --help" and dumping the general usage.

use std::process::Command;

fn magellan_bin() -> String {
    std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_str().unwrap().to_string()
    })
}

#[test]
fn test_repair_edges_help_exits_zero_with_usage() {
    let output = Command::new(magellan_bin())
        .args(["repair-edges", "--help"])
        .output()
        .expect("Failed to run magellan repair-edges --help");

    assert!(
        output.status.success(),
        "repair-edges --help should exit 0, got {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage: magellan repair-edges"),
        "help output should contain repair-edges usage line, got:\n{stdout}"
    );
    assert!(
        stdout.contains("--apply"),
        "help output should document --apply, got:\n{stdout}"
    );
    assert!(
        stdout.contains("--dry-run"),
        "help output should document --dry-run, got:\n{stdout}"
    );
}

#[test]
fn test_repair_edges_short_help_exits_zero() {
    let output = Command::new(magellan_bin())
        .args(["repair-edges", "-h"])
        .output()
        .expect("Failed to run magellan repair-edges -h");

    assert!(
        output.status.success(),
        "repair-edges -h should exit 0, got {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage: magellan repair-edges"),
        "-h output should contain repair-edges usage line, got:\n{stdout}"
    );
}

#[test]
fn test_repair_edges_unknown_argument_still_fails() {
    let output = Command::new(magellan_bin())
        .args(["repair-edges", "--bogus"])
        .output()
        .expect("Failed to run magellan repair-edges --bogus");

    assert!(
        !output.status.success(),
        "repair-edges --bogus should exit non-zero"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown argument: --bogus"),
        "unknown argument should still be reported, got:\n{stderr}"
    );
}

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn bin_path() -> String {
    std::env::var("CARGO_BIN_EXE_magellan").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.pop();
        path.push("magellan");
        path.to_string_lossy().to_string()
    })
}

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup_temporal_db() -> (TempDir, PathBuf, PathBuf, String) {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let db_path = temp_dir.path().join("magellan.db");
    fs::create_dir_all(repo_root.join("src")).unwrap();

    run_git(temp_dir.path(), &["init", repo_root.to_str().unwrap()]);
    run_git(&repo_root, &["config", "user.name", "Temporal Query Test"]);
    run_git(
        &repo_root,
        &["config", "user.email", "temporal-query@example.com"],
    );

    fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname = \"temporal-query-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        repo_root.join("src/lib.rs"),
        "pub fn helper() -> u32 {\n    1\n}\n\npub fn wrapper() -> u32 {\n    helper()\n}\n",
    )
    .unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "initial snapshot"]);

    fs::write(
        repo_root.join("src/lib.rs"),
        "\npub fn helper() -> u32 {\n    1\n}\n\npub fn wrapper() -> u32 {\n    helper()\n}\n",
    )
    .unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "line shift"]);

    let head_output = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("failed to read HEAD");
    assert!(head_output.status.success());
    let head_commit = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_string();

    let sweep_output = Command::new(bin_path())
        .arg("temporal-sweep")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute magellan temporal-sweep");
    assert!(
        sweep_output.status.success(),
        "temporal-sweep should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&sweep_output.stdout),
        String::from_utf8_lossy(&sweep_output.stderr)
    );

    (temp_dir, repo_root, db_path, head_commit)
}

fn setup_temporal_cycle_db() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let db_path = temp_dir.path().join("magellan.db");
    fs::create_dir_all(repo_root.join("src")).unwrap();

    run_git(temp_dir.path(), &["init", repo_root.to_str().unwrap()]);
    run_git(&repo_root, &["config", "user.name", "Temporal SCC Test"]);
    run_git(
        &repo_root,
        &["config", "user.email", "temporal-scc@example.com"],
    );

    fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname = \"temporal-cycle-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        repo_root.join("src/lib.rs"),
        "pub fn a() -> u32 {\n    b()\n}\n\npub fn b() -> u32 {\n    a()\n}\n",
    )
    .unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "initial cycle"]);

    fs::write(
        repo_root.join("src/lib.rs"),
        "\npub fn a() -> u32 {\n    b()\n}\n\npub fn b() -> u32 {\n    a()\n}\n",
    )
    .unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "line shift cycle"]);

    let sweep_output = Command::new(bin_path())
        .arg("temporal-sweep")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute magellan temporal-sweep");
    assert!(
        sweep_output.status.success(),
        "temporal-sweep should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&sweep_output.stdout),
        String::from_utf8_lossy(&sweep_output.stderr)
    );

    (temp_dir, repo_root, db_path)
}

#[test]
fn test_temporal_status_reports_snapshot_counts() {
    let (_temp_dir, _repo_root, db_path, _head_commit) = setup_temporal_db();

    let output = Command::new(bin_path())
        .arg("temporal-status")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute temporal-status");

    assert!(
        output.status.success(),
        "temporal-status should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(data["snapshot_count"].as_u64(), Some(2));
    assert_eq!(data["file_version_count"].as_u64(), Some(2));
    assert!(data["symbol_version_count"].as_u64().unwrap_or(0) >= 4);
    assert!(data["edge_version_count"].as_u64().unwrap_or(0) >= 2);
    assert!(data["latest_commit_oid"].as_str().unwrap_or("").len() >= 7);
}

#[test]
fn test_temporal_barcode_reports_symbol_lifetime() {
    let (_temp_dir, _repo_root, db_path, _head_commit) = setup_temporal_db();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let stable_id: String = conn
        .query_row(
            "SELECT stable_id FROM symbol_versions WHERE name = 'wrapper' ORDER BY snapshot_id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let output = Command::new(bin_path())
        .arg("temporal-barcode")
        .arg("--db")
        .arg(&db_path)
        .arg("--symbol")
        .arg(&stable_id)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute temporal-barcode");

    assert!(
        output.status.success(),
        "temporal-barcode should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(data["stable_id"].as_str(), Some(stable_id.as_str()));
    assert_eq!(data["snapshot_count"].as_u64(), Some(2));
    assert!(data["first_commit_oid"].as_str().unwrap_or("").len() >= 7);
    assert!(data["last_commit_oid"].as_str().unwrap_or("").len() >= 7);
}

#[test]
fn test_temporal_barcode_reports_edge_lifetime() {
    let (_temp_dir, _repo_root, db_path, _head_commit) = setup_temporal_db();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let (source_stable_id, target_stable_id): (String, String) = conn
        .query_row(
            "SELECT source_stable_id, target_stable_id
             FROM edge_versions
             WHERE kind = 'CALLS'
             ORDER BY snapshot_id
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    let output = Command::new(bin_path())
        .arg("temporal-barcode")
        .arg("--db")
        .arg(&db_path)
        .arg("--edge-source")
        .arg(&source_stable_id)
        .arg("--edge-target")
        .arg(&target_stable_id)
        .arg("--kind")
        .arg("CALLS")
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute temporal-barcode edge");

    assert!(
        output.status.success(),
        "temporal-barcode edge should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(
        data["source_stable_id"].as_str(),
        Some(source_stable_id.as_str())
    );
    assert_eq!(
        data["target_stable_id"].as_str(),
        Some(target_stable_id.as_str())
    );
    assert_eq!(data["kind"].as_str(), Some("CALLS"));
    assert_eq!(data["snapshot_count"].as_u64(), Some(2));
}

#[test]
fn test_as_of_symbol_lookup_reads_snapshot_state() {
    let (_temp_dir, _repo_root, db_path, head_commit) = setup_temporal_db();

    let output = Command::new(bin_path())
        .arg("as-of")
        .arg("--db")
        .arg(&db_path)
        .arg("--commit")
        .arg(&head_commit)
        .arg("--symbol")
        .arg("wrapper")
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute as-of");

    assert!(
        output.status.success(),
        "as-of should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(data["commit_oid"].as_str(), Some(head_commit.as_str()));
    assert_eq!(data["count"].as_u64(), Some(1));
    assert_eq!(data["matches"][0]["name"].as_str(), Some("wrapper"));
    assert_eq!(data["matches"][0]["file_path"].as_str(), Some("src/lib.rs"));
}

#[test]
fn test_temporal_barcode_reports_scc_lifetime() {
    let (_temp_dir, _repo_root, db_path) = setup_temporal_cycle_db();

    let output = Command::new(bin_path())
        .arg("temporal-barcode")
        .arg("--db")
        .arg(&db_path)
        .arg("--scc")
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute temporal-barcode --scc");

    assert!(
        output.status.success(),
        "temporal-barcode --scc should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(data["count"].as_u64(), Some(1));
    assert_eq!(data["sccs"][0]["snapshot_count"].as_u64(), Some(2));
    assert_eq!(data["sccs"][0]["member_count"].as_u64(), Some(2));
    assert_eq!(data["sccs"][0]["lifetime_length"].as_u64(), Some(2));
    assert_eq!(data["sccs"][0]["churn_count"].as_u64(), Some(0));
}

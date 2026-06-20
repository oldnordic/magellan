use serde_json::Value;
use std::fs;
use std::path::Path;
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

#[test]
fn test_temporal_sweep_ingests_every_commit_into_snapshot_tables() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let db_path = temp_dir.path().join("magellan.db");
    fs::create_dir_all(repo_root.join("src")).unwrap();

    run_git(temp_dir.path(), &["init", repo_root.to_str().unwrap()]);
    run_git(&repo_root, &["config", "user.name", "Temporal Sweep Test"]);
    run_git(
        &repo_root,
        &["config", "user.email", "temporal-sweep@example.com"],
    );

    fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname = \"temporal-sweep-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
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

    let output = Command::new(bin_path())
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
        output.status.success(),
        "temporal-sweep should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(data["sampled_commits"].as_u64(), Some(2));
    assert_eq!(data["snapshots_ingested"].as_u64(), Some(2));
    assert_eq!(data["files_total"].as_u64(), Some(2));
    assert_eq!(data["files_indexed"].as_u64(), Some(2));
    assert_eq!(data["files_skipped"].as_u64(), Some(0));
    assert!(data["symbol_versions"].as_u64().unwrap_or(0) >= 4);
    assert!(data["edge_versions"].as_u64().unwrap_or(0) >= 2);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let snapshot_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM repo_snapshots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(snapshot_count, 2);

    let file_version_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_versions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(file_version_count, 2);

    let symbol_version_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbol_versions", [], |row| row.get(0))
        .unwrap();
    assert!(symbol_version_count >= 4);

    let edge_version_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM edge_versions", [], |row| row.get(0))
        .unwrap();
    assert!(edge_version_count >= 2);
}

#[test]
fn test_temporal_sweep_every_n_samples_commits() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let db_path = temp_dir.path().join("magellan.db");
    fs::create_dir_all(repo_root.join("src")).unwrap();

    run_git(temp_dir.path(), &["init", repo_root.to_str().unwrap()]);
    run_git(
        &repo_root,
        &["config", "user.name", "Temporal Sweep Sample Test"],
    );
    run_git(
        &repo_root,
        &["config", "user.email", "temporal-sweep-sample@example.com"],
    );

    fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname = \"temporal-sweep-sample-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    for idx in 0..3 {
        fs::write(
            repo_root.join("src/lib.rs"),
            format!("pub fn helper() -> u32 {{\n    {}\n}}\n", idx + 1),
        )
        .unwrap();
        run_git(&repo_root, &["add", "."]);
        run_git(
            &repo_root,
            &["commit", "-m", &format!("snapshot {}", idx + 1)],
        );
    }

    let output = Command::new(bin_path())
        .arg("temporal-sweep")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .arg("--every")
        .arg("2")
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute magellan temporal-sweep");

    assert!(
        output.status.success(),
        "temporal-sweep should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert_eq!(data["sampled_commits"].as_u64(), Some(2));
    assert_eq!(data["snapshots_ingested"].as_u64(), Some(2));

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let snapshot_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM repo_snapshots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(snapshot_count, 2);
}

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

fn setup_orient_db() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let db_path = temp_dir.path().join("magellan.db");
    fs::create_dir_all(repo_root.join("src")).unwrap();

    run_git(temp_dir.path(), &["init", repo_root.to_str().unwrap()]);
    run_git(&repo_root, &["config", "user.name", "Orient Test"]);
    run_git(&repo_root, &["config", "user.email", "orient@example.com"]);

    fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname = \"orient-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        repo_root.join("src/lib.rs"),
        "pub fn helper() -> u32 {\n    1\n}\n\npub fn wrapper() -> u32 {\n    helper()\n}\n",
    )
    .unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "initial"]);

    fs::write(
        repo_root.join("src/lib.rs"),
        "pub fn helper() -> u32 {\n    2\n}\n\npub fn wrapper() -> u32 {\n    helper()\n}\n",
    )
    .unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "second"]);

    // Index the repo (single-file, exits immediately)
    let index_output = Command::new(bin_path())
        .arg("index")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(repo_root.join("src/lib.rs"))
        .arg("--root")
        .arg(repo_root.join("src"))
        .output()
        .expect("failed to execute magellan index");
    assert!(
        index_output.status.success(),
        "index failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&index_output.stdout),
        String::from_utf8_lossy(&index_output.stderr)
    );

    // Sweep for temporal data
    let sweep_output = Command::new(bin_path())
        .arg("temporal-sweep")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .output()
        .expect("failed to execute magellan temporal-sweep");
    assert!(
        sweep_output.status.success(),
        "temporal-sweep failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&sweep_output.stdout),
        String::from_utf8_lossy(&sweep_output.stderr)
    );

    (temp_dir, repo_root, db_path)
}

#[test]
fn test_orient_human_output_has_all_sections() {
    let (_temp_dir, repo_root, db_path) = setup_orient_db();

    let output = Command::new(bin_path())
        .arg("orient")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .arg("--top")
        .arg("5")
        .output()
        .expect("failed to execute magellan orient");

    assert!(
        output.status.success(),
        "orient should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Orient"), "missing Orient header");
    assert!(stdout.contains("symbols"), "missing symbols count");
    assert!(stdout.contains("Temporal"), "missing Temporal section");
    assert!(stdout.contains("snapshots"), "missing snapshots");
    assert!(stdout.contains("churn"), "missing churn section");
    assert!(
        stdout.contains("wrapper") || stdout.contains("helper"),
        "missing symbol names"
    );
    assert!(
        stdout.contains("Contributors"),
        "missing Contributors section"
    );
    assert!(stdout.contains("Orient Test"), "missing git author name");
}

#[test]
fn test_orient_json_output_has_required_fields() {
    let (_temp_dir, repo_root, db_path) = setup_orient_db();

    let output = Command::new(bin_path())
        .arg("orient")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute magellan orient --output json");

    assert!(
        output.status.success(),
        "orient json should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];

    assert!(data["db"]["symbol_count"].as_u64().unwrap_or(0) > 0);
    assert!(data["temporal"]["snapshot_count"].as_u64().unwrap_or(0) >= 2);
    assert!(data["top_churn"].is_array());
    assert!(!data["top_churn"].as_array().unwrap().is_empty());
    assert!(data["contributors"].is_array());
    assert!(!data["contributors"].as_array().unwrap().is_empty());
}

#[test]
fn test_orient_without_repo_skips_contributors() {
    let (_temp_dir, _repo_root, db_path) = setup_orient_db();

    let output = Command::new(bin_path())
        .arg("orient")
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute magellan orient without --repo");

    assert!(
        output.status.success(),
        "orient without --repo should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let data = &json["data"];
    assert!(data["contributors"].as_array().unwrap().is_empty());
}

#[test]
fn test_orient_without_temporal_data_still_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let db_path = temp_dir.path().join("magellan.db");
    fs::create_dir_all(repo_root.join("src")).unwrap();

    run_git(temp_dir.path(), &["init", repo_root.to_str().unwrap()]);
    run_git(&repo_root, &["config", "user.name", "No Temporal"]);
    run_git(&repo_root, &["config", "user.email", "no@temporal.com"]);
    fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname = \"no-temporal\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(repo_root.join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-m", "init"]);

    // Index but NO temporal-sweep
    let index_output = Command::new(bin_path())
        .arg("index")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg(repo_root.join("src/lib.rs"))
        .arg("--root")
        .arg(repo_root.join("src"))
        .output()
        .expect("failed to index");
    assert!(index_output.status.success());

    let output = Command::new(bin_path())
        .arg("orient")
        .arg("--db")
        .arg(&db_path)
        .arg("--repo")
        .arg(&repo_root)
        .arg("--output")
        .arg("json")
        .output()
        .expect("failed to execute orient");

    assert!(
        output.status.success(),
        "orient should succeed without temporal data\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert_eq!(json["data"]["temporal"]["snapshot_count"].as_u64(), Some(0));
    assert!(json["data"]["top_churn"].as_array().unwrap().is_empty());
}

//! Candidate patch verification: temp worktree → apply → test

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Result of verifying a candidate patch in a temp worktree.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub passed: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Create a temp copy of `project_root`, apply `patch_diff`, and run tests.
pub fn verify_candidate(project_root: &Path, patch_diff: &str) -> Result<VerifyResult> {
    let temp_dir = tempfile::tempdir()?;
    let worktree = temp_dir.path().join("worktree");
    std::fs::create_dir_all(&worktree)?;

    // Copy project into temp worktree
    copy_dir_all(project_root, &worktree)
        .context("copy project to temp worktree")?;

    // Write patch to temp file inside worktree
    let patch_file = worktree.join("candidate.patch");
    std::fs::write(&patch_file, patch_diff)
        .context("write patch file")?;

    // Apply patch
    let patch_output = Command::new("patch")
        .args(["-p0", "-i", "candidate.patch"])
        .current_dir(&worktree)
        .output()
        .context("execute patch command")?;

    if !patch_output.status.success() {
        let stdout = String::from_utf8_lossy(&patch_output.stdout).into_owned();
        return Ok(VerifyResult {
            passed: false,
            exit_code: patch_output.status.code().unwrap_or(1),
            stdout,
            stderr: String::from_utf8_lossy(&patch_output.stderr).into_owned(),
        });
    }

    // Run tests (default to cargo test for Rust repos)
    let test_cmd = detect_test_command(&worktree);
    let test_output = test_output(&worktree, &test_cmd)?;

    Ok(test_output)
}

fn detect_test_command(worktree: &Path) -> Vec<String> {
    if worktree.join("Cargo.toml").exists() {
        vec!["cargo".to_string(), "test".to_string()]
    } else if worktree.join("pyproject.toml").exists() {
        vec!["pytest".to_string()]
    } else if worktree.join("package.json").exists() {
        vec!["npm".to_string(), "test".to_string()]
    } else {
        vec!["false".to_string()] // no-op if unknown
    }
}

fn test_output(worktree: &Path, cmd: &[String]) -> Result<VerifyResult> {
    if cmd.is_empty() || cmd[0] == "false" {
        return Ok(VerifyResult {
            passed: false,
            exit_code: 1,
            stdout: String::new(),
            stderr: "No test harness detected (missing Cargo.toml / pyproject.toml / package.json)".into(),
        });
    }
    let output = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(worktree)
        .env("CARGO_TERM_COLOR", "never")
        .env("RUST_BACKTRACE", "0")
        .output()
        .with_context(|| format!("execute test command: {:?}", cmd))?;
    Ok(VerifyResult {
        passed: output.status.success(),
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        // Skip destination to avoid infinite recursion
        if std::fs::canonicalize(&src_path).ok() == std::fs::canonicalize(dst).ok() {
            continue;
        }
        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

//! `magellan init` command — creates a `.magellan.toml` in the project root.

use anyhow::{Context, Result};
use magellan::project_config::ProjectConfig;
use std::path::PathBuf;

pub fn run_project_init(path: Option<PathBuf>) -> Result<()> {
    let project_root = match path {
        Some(p) => p,
        None => detect_project_root()
            .context("Could not detect project root. Use --path to specify one.")?,
    };

    let name = project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    ProjectConfig::init(&project_root, &name)?;

    println!("Created .magellan.toml in {}", project_root.display());
    println!("Edit it to customize include/exclude paths and watcher settings.");

    Ok(())
}

/// Walk up from cwd looking for `.git` or `Cargo.toml`.
fn detect_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() || dir.join("Cargo.toml").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

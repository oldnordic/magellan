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

    // Canonicalize so relative paths like "." resolve to the actual directory name.
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.clone());

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn init_with_dot_path_uses_canonical_name() {
        let _guard = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dir_name = dir.path().file_name().unwrap().to_str().unwrap();

        // Simulate `magellan init --path .` by running from inside the dir with "."
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        run_project_init(Some(PathBuf::from("."))).unwrap();
        std::env::set_current_dir(original).unwrap();

        let config = ProjectConfig::load(dir.path()).unwrap();
        assert_eq!(
            config.project.name.as_deref(),
            Some(dir_name),
            "init with '.' should resolve to canonical directory name, not 'project'"
        );
    }
}

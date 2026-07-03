//! Database path resolution helper.
//!
//! Centralizes the "--db is optional" logic so query commands
//! fall back to the project registry, the enclosing git repository name,
//! or `.magellan/magellan.db` in cwd.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::service::registry::Registry;

/// Resolve database path from explicit argument, registry, git repo, or cwd heuristic.
///
/// Resolution order:
/// 1. If `explicit` is `Some`, return it directly.
/// 2. Load the project registry and look for a project whose root matches cwd.
///    If found, return the canonical DB path for that project.
/// 3. If cwd is inside a git repository, derive a project name from the `origin`
///    remote URL or the repository root directory, and return the canonical DB path
///    `~/.magellan/<name>/<name>.db`.
/// 4. Fallback to `.magellan/magellan.db` in the current working directory.
///
/// This is idempotent — repeated calls for the same cwd return the same path.
pub fn resolve_db_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }

    let registry =
        Registry::load().context("Failed to load project registry for default DB resolution")?;

    let cwd = std::env::current_dir().context("Failed to get current working directory")?;

    if let Some(entry) = registry.find_by_root(&cwd) {
        let canon = Registry::canonical_db_path(&entry.name);
        Registry::ensure_db_dir(&entry.name)?;
        return Ok(canon);
    }

    if let Some(name) = detect_project_name_from_git(&cwd) {
        let canon = Registry::canonical_db_path(&name);
        Registry::ensure_db_dir(&name)?;
        return Ok(canon);
    }

    Ok(PathBuf::from(".magellan/magellan.db"))
}

/// Detect a project name from a git repository containing `start`.
///
/// Returns `Some(name)` when `start` is inside a git repository, using:
/// 1. The repository name from the `origin` remote URL (with `.git` stripped).
/// 2. The directory name of the repository root.
fn detect_project_name_from_git(start: &Path) -> Option<String> {
    let repo = git2::Repository::discover(start).ok()?;
    let repo_root = repo
        .workdir()
        .or_else(|| repo.path().parent())?
        .to_path_buf();

    if let Ok(remote) = repo.find_remote("origin") {
        if let Ok(url) = remote.url() {
            if let Some(name) = project_name_from_remote_url(url) {
                return Some(name);
            }
        }
    }

    repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize_project_name)
}

/// Extract a project name from a git remote URL.
///
/// Handles common formats such as:
/// - `https://github.com/user/project.git`
/// - `git@github.com:user/project.git`
/// - `file:///path/to/project.git`
fn project_name_from_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    // Strip an optional scheme (https://, git://, ssh://) so the host isn't
    // mistaken for a project name when the URL has no path beyond the host.
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    // Split on the last path separator ('/' for HTTPS/file URLs, ':' for SCP-style).
    let name = without_scheme
        .rsplit_once('/')
        .or_else(|| without_scheme.rsplit_once(':'))
        .map(|(_, name)| name)?;
    if name.is_empty() {
        return None;
    }
    // Reject a bare host with no path (e.g. "github.com" from "https://github.com/").
    // A real project name has no dot-separated domain suffix.
    if !without_scheme.contains('/') && !without_scheme.contains(':') {
        return None;
    }
    Some(sanitize_project_name(name))
}

/// Replace filesystem-hostile characters with `-`.
fn sanitize_project_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_project_name_from_remote_url_https() {
        assert_eq!(
            project_name_from_remote_url("https://github.com/oldnordic/magellan.git"),
            Some("magellan".to_string())
        );
    }

    #[test]
    fn test_project_name_from_remote_url_ssh() {
        assert_eq!(
            project_name_from_remote_url("git@github.com:oldnordic/magellan.git"),
            Some("magellan".to_string())
        );
    }

    #[test]
    fn test_project_name_from_remote_url_no_suffix() {
        assert_eq!(
            project_name_from_remote_url("https://github.com/oldnordic/magellan"),
            Some("magellan".to_string())
        );
    }

    #[test]
    fn test_project_name_from_remote_url_file() {
        assert_eq!(
            project_name_from_remote_url("file:///home/user/projects/magellan.git"),
            Some("magellan".to_string())
        );
    }

    #[test]
    fn test_project_name_from_remote_url_trailing_slash() {
        assert_eq!(
            project_name_from_remote_url("https://github.com/oldnordic/magellan.git/"),
            Some("magellan".to_string())
        );
    }

    #[test]
    fn test_project_name_from_remote_url_invalid() {
        assert_eq!(project_name_from_remote_url(""), None);
        assert_eq!(project_name_from_remote_url("https://github.com/"), None);
    }

    #[test]
    fn test_sanitize_project_name() {
        assert_eq!(sanitize_project_name("my_project"), "my_project");
        assert_eq!(sanitize_project_name("my/project"), "my-project");
        assert_eq!(sanitize_project_name("my project"), "my-project");
        assert_eq!(sanitize_project_name("foo..bar"), "foo..bar");
    }

    #[test]
    fn test_detect_project_name_from_git_uses_origin_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("my-repo");
        std::fs::create_dir(&repo_dir).unwrap();

        let repo = git2::Repository::init(&repo_dir).unwrap();
        repo.remote("origin", "https://github.com/user/remote-name.git")
            .unwrap();

        // Create the src subdir so git2::Repository::discover can stat the
        // start path (libgit2 requires the path to exist before walking up).
        let src_dir = repo_dir.join("src");
        std::fs::create_dir(&src_dir).unwrap();

        let name = detect_project_name_from_git(&src_dir);
        assert_eq!(name, Some("remote-name".to_string()));
    }

    #[test]
    fn test_detect_project_name_from_git_falls_back_to_dir_name() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("local-name");
        git2::Repository::init(&repo_dir).unwrap();

        let name = detect_project_name_from_git(&repo_dir);
        assert_eq!(name, Some("local-name".to_string()));
    }

    #[test]
    fn test_detect_project_name_from_git_outside_repo() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(detect_project_name_from_git(tmp.path()).is_none());
    }

    #[test]
    fn test_resolve_db_path_explicit() {
        assert_eq!(
            resolve_db_path(Some(PathBuf::from("custom.db"))).unwrap(),
            PathBuf::from("custom.db")
        );
    }
}

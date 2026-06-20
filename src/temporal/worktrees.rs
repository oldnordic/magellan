use anyhow::{Context, Result};
use git2::{Repository, Sort};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::manifest::detect_include_paths_from_root;
use crate::temporal::SnapshotFileInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalCommit {
    pub commit_oid: String,
    pub tree_oid: String,
    pub author_time: i64,
    pub commit_time: i64,
    pub commit_message: String,
    pub parent_oids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TemporalSweepSelection {
    pub every_n: Option<usize>,
    pub tags_only: bool,
    pub merge_commits_only: bool,
    pub since_commit_time: Option<i64>,
    pub until_commit_time: Option<i64>,
}

fn tagged_commit_oids(repo: &Repository) -> Result<BTreeSet<String>> {
    let mut tagged = BTreeSet::new();
    let tag_names = repo.tag_names(None)?;
    for name in tag_names.iter().flatten().flatten() {
        if let Ok(object) = repo.revparse_single(name) {
            if let Ok(commit) = object.peel_to_commit() {
                tagged.insert(commit.id().to_string());
            }
        }
    }
    Ok(tagged)
}

fn apply_selection(
    commits: Vec<TemporalCommit>,
    selection: &TemporalSweepSelection,
    tagged_commits: &BTreeSet<String>,
) -> Vec<TemporalCommit> {
    let mut selected: Vec<TemporalCommit> = commits
        .into_iter()
        .filter(|commit| {
            selection
                .since_commit_time
                .map(|since| commit.commit_time >= since)
                .unwrap_or(true)
        })
        .filter(|commit| {
            selection
                .until_commit_time
                .map(|until| commit.commit_time <= until)
                .unwrap_or(true)
        })
        .filter(|commit| !selection.merge_commits_only || commit.parent_oids.len() > 1)
        .filter(|commit| !selection.tags_only || tagged_commits.contains(&commit.commit_oid))
        .collect();

    if let Some(n) = selection.every_n {
        let stride = n.max(1);
        selected = selected
            .into_iter()
            .enumerate()
            .filter_map(|(idx, commit)| (idx % stride == 0).then_some(commit))
            .collect();
    }

    selected
}

pub fn list_commits(
    repo_root: &Path,
    selection: &TemporalSweepSelection,
) -> Result<Vec<TemporalCommit>> {
    let repo = Repository::open(repo_root)
        .with_context(|| format!("Failed to open git repository at {}", repo_root.display()))?;
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
    revwalk.push_head()?;

    let mut commits = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        commits.push(TemporalCommit {
            commit_oid: oid.to_string(),
            tree_oid: commit.tree_id().to_string(),
            author_time: commit.author().when().seconds(),
            commit_time: commit.committer().when().seconds(),
            commit_message: commit.summary().ok().flatten().unwrap_or("").to_string(),
            parent_oids: commit
                .parent_ids()
                .map(|parent| parent.to_string())
                .collect(),
        });
    }

    let tagged_commits = tagged_commit_oids(&repo)?;
    Ok(apply_selection(commits, selection, &tagged_commits))
}

#[derive(Debug)]
pub struct ManagedWorktree {
    repo_root: PathBuf,
    path: PathBuf,
    _base_dir: TempDir,
}

impl ManagedWorktree {
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup(&self) {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(["worktree", "prune"])
            .output();
    }
}

impl Drop for ManagedWorktree {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub fn create_detached_worktree(repo_root: &Path, commit_oid: &str) -> Result<ManagedWorktree> {
    let base_dir = tempfile::Builder::new()
        .prefix("magellan-temporal-sweep-")
        .tempdir()
        .context("Failed to create worktree directory for git checkout")?;
    let worktree_path = base_dir.path().join("checkout");

    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg(commit_oid)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute git worktree add for '{}' in {}",
                commit_oid,
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        anyhow::bail!(
            "git worktree add failed for '{}': {}",
            commit_oid,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(ManagedWorktree {
        repo_root: repo_root.to_path_buf(),
        path: worktree_path,
        _base_dir: base_dir,
    })
}

pub fn collect_snapshot_files(repo_root: &Path) -> Result<Vec<SnapshotFileInput>> {
    let include_paths = detect_include_paths_from_root(repo_root);
    let mut candidate_paths = BTreeSet::new();

    for include_path in include_paths {
        let include_root = repo_root.join(include_path.trim_end_matches('/'));
        if !include_root.exists() {
            continue;
        }

        for entry in WalkDir::new(&include_root) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            if path
                .components()
                .any(|component| component.as_os_str() == ".git")
            {
                continue;
            }
            candidate_paths.insert(path.to_path_buf());
        }
    }

    let mut files = Vec::with_capacity(candidate_paths.len());
    for path in candidate_paths {
        let source = std::fs::read(&path)
            .with_context(|| format!("Failed to read snapshot file {}", path.display()))?;
        files.push(SnapshotFileInput { path, source });
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_commit(oid: &str, commit_time: i64, parents: usize) -> TemporalCommit {
        TemporalCommit {
            commit_oid: oid.to_string(),
            tree_oid: format!("tree-{oid}"),
            author_time: commit_time,
            commit_time,
            commit_message: oid.to_string(),
            parent_oids: (0..parents).map(|idx| format!("p{idx}")).collect(),
        }
    }

    #[test]
    fn test_apply_selection_every_n() {
        let commits = vec![
            sample_commit("c1", 1, 1),
            sample_commit("c2", 2, 1),
            sample_commit("c3", 3, 1),
            sample_commit("c4", 4, 1),
        ];
        let selected = apply_selection(
            commits,
            &TemporalSweepSelection {
                every_n: Some(2),
                ..TemporalSweepSelection::default()
            },
            &BTreeSet::new(),
        );
        let oids: Vec<_> = selected
            .into_iter()
            .map(|commit| commit.commit_oid)
            .collect();
        assert_eq!(oids, vec!["c1".to_string(), "c3".to_string()]);
    }

    #[test]
    fn test_apply_selection_combined_filters() {
        let commits = vec![
            sample_commit("c1", 10, 1),
            sample_commit("c2", 20, 2),
            sample_commit("c3", 30, 1),
            sample_commit("c4", 40, 2),
        ];
        let tagged: BTreeSet<String> = ["c2".to_string(), "c4".to_string()].into_iter().collect();
        let selected = apply_selection(
            commits,
            &TemporalSweepSelection {
                tags_only: true,
                merge_commits_only: true,
                since_commit_time: Some(15),
                until_commit_time: Some(35),
                ..TemporalSweepSelection::default()
            },
            &tagged,
        );
        let oids: Vec<_> = selected
            .into_iter()
            .map(|commit| commit.commit_oid)
            .collect();
        assert_eq!(oids, vec!["c2".to_string()]);
    }
}

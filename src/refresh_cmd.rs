//! Refresh command implementation for Magellan
//!
//! Synchronizes the graph database with the current git working tree state.
//! Detects modified, deleted, and new files, then updates the database accordingly.

use anyhow::{Context, Result};
use git2::{Repository, StatusOptions};
use magellan::output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
use magellan::{CodeGraph, ReconcileOutcome};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Arguments for the refresh command
#[derive(Debug, Clone)]
pub struct RefreshArgs {
    /// Path to the database file
    pub db_path: PathBuf,
    /// If true, only preview changes without applying them
    pub dry_run: bool,
    /// If true, include untracked files in the refresh
    pub include_untracked: bool,
    /// If true, only process staged changes
    pub staged: bool,
    /// If true, only process unstaged changes
    pub unstaged: bool,
    /// If true, force refresh even if no changes detected.
    /// Force mode re-indexes all tracked files regardless of git status.
    pub force: bool,
    /// Output format (Human, Json, or Pretty)
    pub output_format: OutputFormat,
}

impl Default for RefreshArgs {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from(".magellan/magellan.db"),
            dry_run: false,
            include_untracked: false,
            staged: false,
            unstaged: false,
            force: false,
            output_format: OutputFormat::Human,
        }
    }
}

/// Report of refresh operation results
#[derive(Debug, Clone, Serialize)]
pub struct RefreshReport {
    /// Files that were updated (modified and re-indexed)
    pub updated: Vec<String>,
    /// Files that were deleted from the database
    pub deleted: Vec<String>,
    /// Files that were added to the database
    pub added: Vec<String>,
    /// Number of files that were unchanged
    pub unchanged: usize,
    /// Whether this was a dry run
    pub dry_run: bool,
    /// Duration of the operation in milliseconds
    pub duration_ms: u64,
}

impl RefreshReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self {
            updated: Vec::new(),
            deleted: Vec::new(),
            added: Vec::new(),
            unchanged: 0,
            dry_run: false,
            duration_ms: 0,
        }
    }

    #[allow(dead_code, reason = "used in tests, future public API")]
    /// Total number of changes (updated + deleted + added)
    pub fn total_changes(&self) -> usize {
        self.updated.len() + self.deleted.len() + self.added.len()
    }
}

impl Default for RefreshReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Response structure for JSON output
#[derive(Debug, Clone, Serialize)]
struct RefreshResponse {
    updated: Vec<String>,
    deleted: Vec<String>,
    added: Vec<String>,
    unchanged: usize,
    duration_ms: u64,
    dry_run: bool,
}

impl RefreshResponse {
    fn from_report(report: &RefreshReport, dry_run: bool) -> Self {
        Self {
            updated: report.updated.clone(),
            deleted: report.deleted.clone(),
            added: report.added.clone(),
            unchanged: report.unchanged,
            duration_ms: report.duration_ms,
            dry_run,
        }
    }
}

/// Resolve database path from registry or fallback default
///
/// When `--db` is omitted:
/// 1. Load project registry
/// 2. Find registered project whose root matches current working directory
/// 3. Return that project's canonical DB path
/// 4. If no match, fall back to existing default (`.magellan/magellan.db`)
///
/// This is idempotent — repeated calls return the same DB for the same cwd.
pub use crate::db_resolver::resolve_db_path;

/// Run the refresh command
///
/// Synchronizes the graph database with the current git working tree state.
///
/// # Arguments
/// * `args` - Refresh command arguments
///
/// # Returns
/// Result containing the refresh report or an error
pub fn run_refresh(args: &RefreshArgs) -> Result<RefreshReport> {
    let start_time = Instant::now();
    let exec_id = generate_execution_id();

    // Open the git repository
    let repo = Repository::open(".")
        .context("Failed to open git repository. Are you in a git repository?")?;
    let repo_root = repo
        .workdir()
        .context("Failed to get repository working directory")?;

    // Open the graph database
    let mut graph = CodeGraph::open(&args.db_path)?;

    // Start execution tracking
    graph.execution_log().start_execution(
        &exec_id,
        env!("CARGO_PKG_VERSION"),
        &["refresh".to_string()],
        Some("."),
        &args.db_path.to_string_lossy(),
    )?;

    // Phase: get_git_status
    graph
        .telemetry()
        .record_phase_start(&exec_id, "get_git_status")?;
    let git_status = get_git_status(&repo, args)?;
    graph
        .telemetry()
        .record_phase_end(&exec_id, "get_git_status")?;

    // Phase: compute_delta
    graph
        .telemetry()
        .record_phase_start(&exec_id, "compute_delta")?;
    let db_files = graph.all_file_nodes()?;
    let db_file_paths: HashSet<String> = db_files.keys().cloned().collect();
    let delta = compute_delta(&git_status, &db_file_paths, args, repo_root)?;
    graph
        .telemetry()
        .record_phase_end(&exec_id, "compute_delta")?;

    // Phase: apply_changes
    if !args.dry_run {
        graph
            .telemetry()
            .record_phase_start(&exec_id, "apply_changes")?;
        apply_changes(&mut graph, &delta)?;
        graph
            .telemetry()
            .record_phase_end(&exec_id, "apply_changes")?;

        // Rebuild FTS5 index so symbol search stays synchronized
        if let Err(e) = CodeGraph::rebuild_fts5_index(&args.db_path) {
            eprintln!("  Warning: FTS5 rebuild failed: {}", e);
        }
    }

    // Build report (after apply_changes, moving fields from delta)
    let mut report = RefreshReport::new();
    report.updated = delta.to_update;
    report.deleted = delta.to_delete;
    report.added = delta.to_add;
    report.unchanged = delta.unchanged;
    report.dry_run = args.dry_run;

    // Calculate duration
    report.duration_ms = start_time.elapsed().as_millis() as u64;

    // Output results
    match args.output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = RefreshResponse::from_report(&report, args.dry_run);
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, args.output_format)?;
        }
        OutputFormat::Human => {
            print_human_output(&report, args.dry_run);
        }
    }

    // Finish execution tracking
    let total_files = report.updated.len() + report.added.len();
    graph.execution_log().finish_execution(
        &exec_id,
        "success",
        None,
        total_files,
        0, // Symbol count not tracked here
        0, // Reference count not tracked here
    )?;

    Ok(report)
}

/// Git status information for refresh
#[derive(Debug, Clone)]
struct GitStatus {
    /// Modified files (staged or unstaged)
    modified: Vec<String>,
    /// Deleted files
    deleted: Vec<String>,
    /// Untracked files
    untracked: Vec<String>,
    /// Staged files (for --staged filter)
    staged: Vec<String>,
    /// Unstaged files (for --unstaged filter)
    unstaged: Vec<String>,
}

/// Delta between git and database
#[derive(Debug, Clone)]
struct FileDelta {
    /// Files to update (modified in git, exist in DB)
    to_update: Vec<String>,
    /// Files to delete (in DB but deleted in git)
    to_delete: Vec<String>,
    /// Files to add (new in git, not in DB)
    to_add: Vec<String>,
    /// Files that are unchanged
    unchanged: usize,
}

/// Get git status for the repository
fn get_git_status(repo: &Repository, args: &RefreshArgs) -> Result<GitStatus> {
    let mut status_opts = StatusOptions::new();
    status_opts
        .include_untracked(args.include_untracked)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    // If --staged is specified, only look at staged changes
    if args.staged {
        status_opts.include_untracked(false);
    }

    // If --unstaged is specified, exclude staged changes
    if args.unstaged {
        // StatusOptions doesn't have a direct exclude_staged, but we filter later
    }

    let statuses = repo.statuses(Some(&mut status_opts))?;

    // Get the working directory for converting relative paths to absolute
    let workdir = repo
        .workdir()
        .context("Failed to get repository working directory")?;

    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    let mut untracked = Vec::new();
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();

    for entry in statuses.iter() {
        let rel_path = entry.path().unwrap_or("").to_string();
        // Convert relative path to absolute path to match database
        let path = workdir.join(&rel_path).to_string_lossy().to_string();
        let status = entry.status();

        // Check if staged (in index)
        let is_staged = status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_renamed()
            || status.is_index_typechange();

        // Check if unstaged (in workdir)
        let is_unstaged = status.is_wt_new()
            || status.is_wt_modified()
            || status.is_wt_deleted()
            || status.is_wt_renamed()
            || status.is_wt_typechange();

        if is_staged {
            staged.push(path.clone());
            if status.is_index_modified() || status.is_index_renamed() {
                modified.push(path.clone());
            } else if status.is_index_deleted() {
                deleted.push(path.clone());
            } else if status.is_index_new() {
                untracked.push(path.clone());
            }
        }

        if is_unstaged {
            unstaged.push(path.clone());
            if status.is_wt_modified() || status.is_wt_renamed() {
                modified.push(path.clone());
            } else if status.is_wt_deleted() {
                deleted.push(path.clone());
            } else if status.is_wt_new() {
                untracked.push(path.clone());
            }
        }

        // Handle untracked files (not in index, not in workdir modifications)
        if status.is_wt_new() && !is_staged {
            untracked.push(path);
        }
    }

    // Remove duplicates from modified
    modified.sort();
    modified.dedup();

    Ok(GitStatus {
        modified,
        deleted,
        untracked,
        staged,
        unstaged,
    })
}

/// Compute delta between git status and database files
fn compute_delta(
    git_status: &GitStatus,
    db_files: &HashSet<String>,
    args: &RefreshArgs,
    project_root: &Path,
) -> Result<FileDelta> {
    let mut to_update = Vec::new();
    let mut to_delete = Vec::new();
    let mut to_add = Vec::new();

    // Determine which files to consider based on flags
    let modified_files: HashSet<String> = if args.staged {
        git_status.staged.iter().cloned().collect()
    } else if args.unstaged {
        git_status.unstaged.iter().cloned().collect()
    } else {
        git_status.modified.iter().cloned().collect()
    };

    let deleted_files: HashSet<String> = git_status.deleted.iter().cloned().collect();
    let untracked_files: HashSet<String> = if args.include_untracked {
        git_status.untracked.iter().cloned().collect()
    } else {
        HashSet::new()
    };

    // If force is set, re-index all tracked files regardless of git status
    if args.force {
        let all_tracked: Vec<String> = db_files.iter().cloned().collect();
        let mut sorted = all_tracked;
        sorted.sort();
        return Ok(FileDelta {
            to_update: sorted,
            to_delete: Vec::new(),
            to_add: Vec::new(),
            unchanged: 0,
        });
    }

    // Files to update: modified in git AND exist in database
    for path in &modified_files {
        if db_files.contains(path) {
            to_update.push(path.clone());
        } else if args.include_untracked && untracked_files.contains(path) {
            // New file that should be added
            to_add.push(path.clone());
        }
    }

    // Files to delete: in database but deleted in git
    for path in db_files {
        if deleted_files.contains(path) {
            to_delete.push(path.clone());
        } else if !Path::new(path).exists() {
            // File doesn't exist on filesystem (stale in DB)
            to_delete.push(path.clone());
        }
    }

    // Files to add: untracked in git and not in database
    if args.include_untracked {
        for path in &untracked_files {
            if !db_files.contains(path) && project_root.join(path).exists() {
                to_add.push(path.clone());
            }
        }
    }

    // Calculate unchanged files
    let all_affected: HashSet<String> = to_update
        .iter()
        .chain(to_delete.iter())
        .chain(to_add.iter())
        .cloned()
        .collect();
    let unchanged = db_files.difference(&all_affected).count();

    // Sort for deterministic output
    to_update.sort();
    to_delete.sort();
    to_add.sort();

    Ok(FileDelta {
        to_update,
        to_delete,
        to_add,
        unchanged,
    })
}

/// Apply changes to the graph database
fn apply_changes(graph: &mut CodeGraph, delta: &FileDelta) -> Result<()> {
    // Process updates
    for path_str in &delta.to_update {
        let path = Path::new(path_str);
        match graph.reconcile_file_path(path, path_str) {
            Ok(ReconcileOutcome::Reindexed { symbols, .. }) => {
                eprintln!("  Updated: {} ({} symbols)", path_str, symbols);
            }
            Ok(ReconcileOutcome::Unchanged) => {
                eprintln!("  Unchanged: {}", path_str);
            }
            Ok(ReconcileOutcome::Deleted) => {
                eprintln!("  Deleted during update: {}", path_str);
            }
            Err(e) => {
                eprintln!("  Error updating {}: {}", path_str, e);
            }
        }
    }

    // Process deletes
    for path_str in &delta.to_delete {
        match graph.delete_file_facts(path_str) {
            Ok(result) => {
                eprintln!(
                    "  Deleted: {} ({} symbols, {} refs, {} calls)",
                    path_str,
                    result.symbols_deleted,
                    result.references_deleted,
                    result.calls_deleted
                );
            }
            Err(e) => {
                eprintln!("  Error deleting {}: {}", path_str, e);
            }
        }
    }

    // Process adds
    for path_str in &delta.to_add {
        let path = Path::new(path_str);
        match graph.reconcile_file_path(path, path_str) {
            Ok(ReconcileOutcome::Reindexed { symbols, .. }) => {
                eprintln!("  Added: {} ({} symbols)", path_str, symbols);
            }
            Ok(ReconcileOutcome::Unchanged) => {
                eprintln!("  Skipped (unchanged): {}", path_str);
            }
            Ok(ReconcileOutcome::Deleted) => {
                eprintln!("  Skipped (deleted): {}", path_str);
            }
            Err(e) => {
                eprintln!("  Error adding {}: {}", path_str, e);
            }
        }
    }

    Ok(())
}

/// Print human-readable output
fn print_human_output(report: &RefreshReport, dry_run: bool) {
    let mode = if dry_run { " (dry run)" } else { "" };
    println!("Refresh complete{}:", mode);
    println!();

    if report.updated.is_empty() && report.deleted.is_empty() && report.added.is_empty() {
        println!("  No changes detected.");
        println!("  {} files unchanged", report.unchanged);
    } else {
        if !report.updated.is_empty() {
            println!("  Updated: {} files", report.updated.len());
            for path in &report.updated {
                println!("    - {}", path);
            }
        }

        if !report.deleted.is_empty() {
            println!("  Deleted: {} files", report.deleted.len());
            for path in &report.deleted {
                println!("    - {}", path);
            }
        }

        if !report.added.is_empty() {
            println!("  Added: {} files", report.added.len());
            for path in &report.added {
                println!("    - {}", path);
            }
        }

        println!();
        println!("  {} files unchanged", report.unchanged);
    }

    println!();
    println!("Duration: {}ms", report.duration_ms);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_report_new() {
        let report = RefreshReport::new();
        assert!(report.updated.is_empty());
        assert!(report.deleted.is_empty());
        assert!(report.added.is_empty());
        assert_eq!(report.unchanged, 0);
        assert!(!report.dry_run);
        assert_eq!(report.duration_ms, 0);
    }

    #[test]
    fn test_refresh_report_total_changes() {
        let report = RefreshReport {
            updated: vec!["a.rs".to_string(), "b.rs".to_string()],
            deleted: vec!["c.rs".to_string()],
            added: vec!["d.rs".to_string()],
            unchanged: 5,
            dry_run: false,
            duration_ms: 100,
        };
        assert_eq!(report.total_changes(), 4);
    }

    use std::sync::Mutex;
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_refresh_args_default() {
        let args = RefreshArgs::default();
        assert_eq!(args.db_path, PathBuf::from(".magellan/magellan.db"));
        assert!(!args.dry_run);
        assert!(!args.include_untracked);
        assert!(!args.staged);
        assert!(!args.unstaged);
        assert!(!args.force);
        assert_eq!(args.output_format, OutputFormat::Human);
    }

    #[test]
    fn test_compute_delta_basic() {
        let _guard = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let git_status = GitStatus {
            modified: vec!["src/main.rs".to_string()],
            deleted: vec!["src/old.rs".to_string()],
            untracked: vec![],
            staged: vec!["src/main.rs".to_string()],
            unstaged: vec![],
        };

        let mut db_files = HashSet::new();
        db_files.insert("src/main.rs".to_string());
        db_files.insert("src/old.rs".to_string());

        let args = RefreshArgs::default();
        let delta = compute_delta(&git_status, &db_files, &args, dir.path()).unwrap();

        std::env::set_current_dir(original).unwrap();

        assert_eq!(delta.to_update, vec!["src/main.rs"]);
        assert_eq!(delta.to_delete, vec!["src/old.rs"]);
        assert!(delta.to_add.is_empty());
        assert_eq!(delta.unchanged, 0);
    }

    #[test]
    fn test_compute_delta_with_untracked() {
        let _guard = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/new.rs"), "").unwrap();
        std::fs::write(dir.path().join("src/existing.rs"), "").unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let git_status = GitStatus {
            modified: vec![],
            deleted: vec![],
            untracked: vec!["src/new.rs".to_string()],
            staged: vec![],
            unstaged: vec!["src/new.rs".to_string()],
        };

        let mut db_files = HashSet::new();
        db_files.insert("src/existing.rs".to_string());

        let args = RefreshArgs {
            include_untracked: true,
            ..Default::default()
        };

        let delta = compute_delta(&git_status, &db_files, &args, dir.path()).unwrap();

        std::env::set_current_dir(original).unwrap();

        assert_eq!(delta.to_add, vec!["src/new.rs"]);
    }

    #[test]
    fn test_refresh_response_from_report() {
        let report = RefreshReport {
            updated: vec!["a.rs".to_string()],
            deleted: vec!["b.rs".to_string()],
            added: vec!["c.rs".to_string()],
            unchanged: 2,
            dry_run: true,
            duration_ms: 50,
        };

        let response = RefreshResponse::from_report(&report, true);
        assert_eq!(response.updated, vec!["a.rs"]);
        assert_eq!(response.deleted, vec!["b.rs"]);
        assert_eq!(response.added, vec!["c.rs"]);
        assert_eq!(response.unchanged, 2);
        assert_eq!(response.duration_ms, 50);
        assert!(response.dry_run);
    }
}

#[cfg(test)]
mod resolve_db_tests {
    use super::*;
    use crate::service::registry::Registry;
    use std::path::PathBuf;
    use std::sync::Mutex;

    // Guard to prevent concurrent registry file writes from interfering
    static REG_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_resolve_explicit_path_returns_it() {
        let explicit = Some(PathBuf::from("/custom/code.db"));
        let resolved = resolve_db_path(explicit).unwrap();
        assert_eq!(resolved, PathBuf::from("/custom/code.db"));
    }

    #[test]
    fn test_resolve_canonical_db_path_contains_name() {
        // Verify that Registry::canonical_db_path is wired correctly
        let _guard = REG_LOCK.lock().unwrap();
        let resolved = Registry::canonical_db_path("myproj");
        assert!(resolved.to_string_lossy().contains("myproj"));
    }
}

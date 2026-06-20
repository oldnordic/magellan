use anyhow::{Context, Result};
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::temporal::worktrees::{
    collect_snapshot_files, create_detached_worktree, list_commits, TemporalSweepSelection,
};
use magellan::{CodeGraph, SnapshotSpec};
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize)]
struct TemporalSweepCommitResult {
    commit_oid: String,
    snapshot_id: i64,
    files_total: usize,
    files_indexed: usize,
    files_skipped: usize,
    symbol_versions: usize,
    edge_versions: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TemporalSweepResponse {
    repo_path: String,
    sampled_commits: usize,
    snapshots_ingested: usize,
    files_total: usize,
    files_indexed: usize,
    files_skipped: usize,
    symbol_versions: usize,
    edge_versions: usize,
    commits: Vec<TemporalSweepCommitResult>,
}

pub fn run_temporal_sweep(
    db_path: PathBuf,
    repo_path: PathBuf,
    selection: TemporalSweepSelection,
    output_format: OutputFormat,
) -> Result<()> {
    let commits = list_commits(&repo_path, &selection)?;
    if commits.is_empty() {
        anyhow::bail!("No commits found in repository {}", repo_path.display());
    }

    let graph = CodeGraph::open(&db_path)
        .with_context(|| format!("Failed to open Magellan database {}", db_path.display()))?;
    let exec_id = magellan::output::generate_execution_id();

    let mut response = TemporalSweepResponse {
        repo_path: repo_path.to_string_lossy().to_string(),
        sampled_commits: commits.len(),
        snapshots_ingested: 0,
        files_total: 0,
        files_indexed: 0,
        files_skipped: 0,
        symbol_versions: 0,
        edge_versions: 0,
        commits: Vec::with_capacity(commits.len()),
    };

    for commit in commits {
        let worktree = create_detached_worktree(&repo_path, &commit.commit_oid)?;
        let files = collect_snapshot_files(worktree.path())?;
        let snapshot_id = graph.register_snapshot(&SnapshotSpec {
            repo_root: repo_path.clone(),
            commit_oid: commit.commit_oid.clone(),
            tree_oid: commit.tree_oid.clone(),
            author_time: commit.author_time,
            commit_time: commit.commit_time,
            commit_message: commit.commit_message.clone(),
            parent_oids: commit.parent_oids.clone(),
        })?;
        let stats = graph.ingest_snapshot_sources(snapshot_id, worktree.path(), &files)?;

        response.snapshots_ingested += 1;
        response.files_total += stats.files_total;
        response.files_indexed += stats.files_indexed;
        response.files_skipped += stats.files_skipped;
        response.symbol_versions += stats.symbol_versions;
        response.edge_versions += stats.edge_versions;
        response.commits.push(TemporalSweepCommitResult {
            commit_oid: commit.commit_oid,
            snapshot_id,
            files_total: stats.files_total,
            files_indexed: stats.files_indexed,
            files_skipped: stats.files_skipped,
            symbol_versions: stats.symbol_versions,
            edge_versions: stats.edge_versions,
        });
    }

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(response, &exec_id);
        return output_json(&json_response, output_format);
    }

    println!(
        "Temporal sweep complete for {}",
        repo_path.to_string_lossy()
    );
    println!("  Sampled commits: {}", response.sampled_commits);
    println!("  Snapshots ingested: {}", response.snapshots_ingested);
    println!("  Files seen: {}", response.files_total);
    println!("  Files indexed: {}", response.files_indexed);
    println!("  Files skipped: {}", response.files_skipped);
    println!("  Symbol versions: {}", response.symbol_versions);
    println!("  Edge versions: {}", response.edge_versions);

    Ok(())
}

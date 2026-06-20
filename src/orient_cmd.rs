use anyhow::Result;
use magellan::output::{generate_execution_id, output_json, JsonResponse, OutputFormat};
use magellan::temporal::query::load_temporal_status;
use rusqlite::params;
use serde::Serialize;
use std::cmp::Reverse;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Serialize)]
pub struct OrientDbStats {
    pub symbol_count: u64,
    pub call_count: u64,
    pub file_count: u64,
}

#[derive(Debug, Serialize)]
pub struct OrientTemporal {
    pub snapshot_count: u64,
    pub first_commit: Option<String>,
    pub last_commit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChurnEntry {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub snapshot_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ContributorEntry {
    pub name: String,
    pub commits: u64,
}

#[derive(Debug, Serialize)]
pub struct OrientResponse {
    pub project: String,
    pub db: OrientDbStats,
    pub temporal: OrientTemporal,
    pub top_churn: Vec<ChurnEntry>,
    pub contributors: Vec<ContributorEntry>,
}

pub fn run_orient(
    db_path: PathBuf,
    repo_path: Option<PathBuf>,
    top_n: usize,
    output_format: OutputFormat,
) -> Result<()> {
    let exec_id = generate_execution_id();

    let project = db_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // DB stats
    let conn = rusqlite::Connection::open(&db_path)?;
    let symbol_count: u64 =
        conn.query_row("SELECT COUNT(*) FROM graph_entities", [], |r| r.get(0))?;
    let call_count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'CALLS'",
        [],
        |r| r.get(0),
    )?;
    let file_count: u64 = conn.query_row(
        "SELECT COUNT(DISTINCT file_path) FROM graph_entities WHERE file_path IS NOT NULL",
        [],
        |r| r.get(0),
    )?;

    // Temporal — tables only exist after temporal-sweep; absence is not an error
    let temporal_status = match load_temporal_status(&db_path) {
        Ok(s) => Some(s),
        Err(e) if e.to_string().contains("no such table") => None,
        Err(e) => return Err(e),
    };
    let (snapshot_count, first_commit, last_commit, top_churn) =
        match temporal_status.as_ref().filter(|s| s.snapshot_count > 0) {
            Some(_) => {
                let first: Option<String> = conn
                    .query_row(
                        "SELECT commit_oid FROM repo_snapshots ORDER BY commit_time ASC LIMIT 1",
                        [],
                        |r| r.get(0),
                    )
                    .ok();
                let last: Option<String> = conn
                    .query_row(
                        "SELECT commit_oid FROM repo_snapshots ORDER BY commit_time DESC LIMIT 1",
                        [],
                        |r| r.get(0),
                    )
                    .ok();
                let mut stmt = conn.prepare(
                    "SELECT sv.name, sv.kind, sv.file_path, COUNT(*) as cnt
                     FROM symbol_versions sv
                     GROUP BY sv.stable_id
                     ORDER BY cnt DESC
                     LIMIT ?1",
                )?;
                let churn: Vec<ChurnEntry> = stmt
                    .query_map(params![top_n as i64], |r| {
                        Ok(ChurnEntry {
                            name: r.get(0)?,
                            kind: r.get(1)?,
                            file_path: r.get(2)?,
                            snapshot_count: r.get(3)?,
                        })
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                (
                    temporal_status
                        .as_ref()
                        .map(|s| s.snapshot_count as u64)
                        .unwrap_or(0),
                    first,
                    last,
                    churn,
                )
            }
            None => (0, None, None, vec![]),
        };

    // Contributors from git log
    let contributors = match &repo_path {
        Some(repo) => git_contributors(repo, top_n),
        None => vec![],
    };

    let response = OrientResponse {
        project,
        db: OrientDbStats {
            symbol_count,
            call_count,
            file_count,
        },
        temporal: OrientTemporal {
            snapshot_count,
            first_commit,
            last_commit,
        },
        top_churn,
        contributors,
    };

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(response, &exec_id);
        return output_json(&json_response, output_format);
    }

    print_human(&response);
    Ok(())
}

fn git_contributors(repo: &PathBuf, top_n: usize) -> Vec<ContributorEntry> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "--format=%aN"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for line in stdout.lines() {
        let name = line.trim();
        if !name.is_empty() {
            *counts.entry(name.to_string()).or_insert(0) += 1;
        }
    }

    let mut entries: Vec<ContributorEntry> = counts
        .into_iter()
        .map(|(name, commits)| ContributorEntry { name, commits })
        .collect();
    entries.sort_by_key(|e| Reverse(e.commits));
    entries.truncate(top_n);
    entries
}

fn print_human(r: &OrientResponse) {
    println!("=== Orient: {} ===", r.project);
    println!(
        "DB: {} symbols · {} calls · {} files",
        r.db.symbol_count, r.db.call_count, r.db.file_count
    );

    if r.temporal.snapshot_count > 0 {
        let range = match (&r.temporal.first_commit, &r.temporal.last_commit) {
            (Some(f), Some(l)) => format!("{}..{}", &f[..8.min(f.len())], &l[..8.min(l.len())]),
            _ => String::new(),
        };
        println!(
            "Temporal: {} snapshots {}",
            r.temporal.snapshot_count, range
        );
    } else {
        println!("Temporal: no snapshots (run temporal-sweep to index history)");
    }

    if !r.top_churn.is_empty() {
        println!("\nTop churn symbols (most changed across history):");
        for (i, e) in r.top_churn.iter().enumerate() {
            println!(
                "  {:2}. {:<30} {:<8} {}  ({} snapshots)",
                i + 1,
                e.name,
                e.kind,
                e.file_path,
                e.snapshot_count
            );
        }
    }

    if !r.contributors.is_empty() {
        println!("\nContributors:");
        for e in &r.contributors {
            println!("  {:4} commits  {}", e.commits, e.name);
        }
    }
}
// test

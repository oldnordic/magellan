//! Candidate fact helpers for project DBs
use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A candidate improvement tracked in a project shard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateRecord {
    pub candidate_id: String,
    pub status: String,
    pub properties_json: String,
    pub created_at: i64,
}

/// Insert a new candidate fact into a project DB.
pub fn insert_candidate_fact(
    db_path: &Path,
    candidate_id: &str,
    subject_type: &str,
    subject_key: &str,
    predicate: &str,
    properties_json: &str,
    status: &str,
) -> Result<()> {
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("open project db {}", db_path.display()))?;
    let now = now_secs();
    conn.execute(
        "INSERT INTO candidate_facts
         (candidate_id, source_document_id, subject_type, subject_key, predicate, properties_json, status, created_at)
         VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![candidate_id, subject_type, subject_key, predicate, properties_json, status, now],
    )
    .with_context(|| "insert candidate fact")?;
    Ok(())
}

/// List candidate facts from a project DB, optionally filtered by status.
pub fn list_candidates(
    db_path: &Path,
    status_filter: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<CandidateRecord>> {
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("open project db {}", db_path.display()))?;
    let query = "SELECT candidate_id, status, properties_json, created_at
         FROM candidate_facts
         WHERE (?1 IS NULL OR status = ?1)
         ORDER BY created_at DESC";
    let mut stmt = conn.prepare(query)?;
    let rows = stmt.query_map(params![status_filter], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    if let Some(l) = limit {
        out.truncate(l);
    }
    Ok(out)
}

fn map_row(row: &rusqlite::Row) -> std::result::Result<CandidateRecord, rusqlite::Error> {
    Ok(CandidateRecord {
        candidate_id: row.get(0)?,
        status: row.get(1)?,
        properties_json: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Update the status (and optionally rejection_reason) of an existing candidate.
pub fn update_candidate_status(
    db_path: &Path,
    candidate_id: &str,
    new_status: &str,
    rejection_reason: Option<&str>,
) -> Result<usize> {
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("open project db {}", db_path.display()))?;
    let now = now_secs();
    let rows = if let Some(reason) = rejection_reason {
        conn.execute(
            "UPDATE candidate_facts
             SET status = ?1, rejection_reason = ?2, reviewed_at = ?3
             WHERE candidate_id = ?4",
            params![new_status, reason, now, candidate_id],
        )
    } else {
        conn.execute(
            "UPDATE candidate_facts
             SET status = ?1, reviewed_at = ?2
             WHERE candidate_id = ?3",
            params![new_status, now, candidate_id],
        )
    }
    .with_context(|| "update candidate status")?;
    Ok(rows)
}

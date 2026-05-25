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
         ORDER BY created_at DESC, id DESC";
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

/// Query a single candidate by its candidate_id.
pub fn get_candidate_by_id(
    db_path: &Path,
    candidate_id: &str,
) -> Result<Option<CandidateRecord>> {
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("open project db {}", db_path.display()))?;
    let mut stmt = conn.prepare(
        "SELECT candidate_id, status, properties_json, created_at
         FROM candidate_facts
         WHERE candidate_id = ?1"
    )?;
    let mut rows = stmt.query_map(params![candidate_id], map_row)?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_with_schema() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("test.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS candidate_facts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                candidate_id TEXT UNIQUE NOT NULL,
                source_document_id INTEGER NOT NULL DEFAULT 0,
                subject_type TEXT NOT NULL,
                subject_key TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object_type TEXT,
                object_key TEXT,
                properties_json TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                rejection_reason TEXT,
                created_at INTEGER,
                reviewed_at INTEGER
            );"
        ).unwrap();
        (dir, db)
    }

    #[test]
    fn test_insert_and_list_roundtrip() {
        let (_dir, db) = temp_db_with_schema();
        insert_candidate_fact(
            &db, "c-1", "Symbol", "sym_a", "proposes-improvement",
            r#"{"patch_diff":"@@ -1 +1 @@\n-a\n+b\n"}"#, "pending",
        ).unwrap();
        insert_candidate_fact(
            &db, "c-2", "Symbol", "sym_b", "proposes-improvement",
            r#"{"patch_diff":"@@ -1 +1 @@\n-c\n+d\n"}"#, "promoted",
        ).unwrap();

        let recs = list_candidates(&db, None, None).unwrap();
        assert_eq!(recs.len(), 2);
        // Most recent first (c-2 inserted second)
        assert_eq!(recs[0].candidate_id, "c-2");
        assert_eq!(recs[1].candidate_id, "c-1");

        let pending = list_candidates(&db, Some("pending"), None).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].candidate_id, "c-1");

        let limited = list_candidates(&db, None, Some(1)).unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[test]
    fn test_update_candidate_status_promote_and_reject() {
        let (_dir, db) = temp_db_with_schema();
        insert_candidate_fact(
            &db, "c-10", "Symbol", "sym_x", "proposes-improvement",
            r#"{"patch_diff":"x"}"#, "pending",
        ).unwrap();

        let updated = update_candidate_status(&db, "c-10", "promoted", None).unwrap();
        assert_eq!(updated, 1);

        let recs = list_candidates(&db, Some("promoted"), None).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].status, "promoted");

        let updated = update_candidate_status(&db, "c-10", "rejected", Some("broken tests")).unwrap();
        assert_eq!(updated, 1);

        let recs = list_candidates(&db, Some("rejected"), None).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].status, "rejected");
    }

    #[test]
    fn test_update_missing_candidate_returns_zero() {
        let (_dir, db) = temp_db_with_schema();
        let updated = update_candidate_status(&db, "does-not-exist", "promoted", None
        ).unwrap();
        assert_eq!(updated, 0);
    }

    #[test]
    fn test_get_candidate_by_id_found_and_not_found() {
        let (_dir, db) = temp_db_with_schema();
        insert_candidate_fact(
            &db, "c-99", "Symbol", "sym_z", "proposes-improvement",
            r#"{"patch_diff":"z"}"#, "pending",
        ).unwrap();

        let found = get_candidate_by_id(&db, "c-99").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().candidate_id, "c-99");

        let not_found = get_candidate_by_id(&db, "no-such-id").unwrap();
        assert!(not_found.is_none());
    }
}

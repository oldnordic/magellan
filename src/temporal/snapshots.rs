use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct SnapshotSpec {
    pub repo_root: PathBuf,
    pub commit_oid: String,
    pub tree_oid: String,
    pub author_time: i64,
    pub commit_time: i64,
    pub commit_message: String,
    pub parent_oids: Vec<String>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn register_snapshot(conn: &mut Connection, spec: &SnapshotSpec) -> Result<i64> {
    let tx = conn.transaction()?;
    let existing_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM repo_snapshots WHERE commit_oid = ?1",
            params![spec.commit_oid],
            |row| row.get(0),
        )
        .optional()?;

    let snapshot_id = if let Some(existing_id) = existing_id {
        tx.execute(
            "UPDATE repo_snapshots
             SET repo_root = ?1, tree_oid = ?2, author_time = ?3, commit_time = ?4, commit_message = ?5
             WHERE id = ?6",
            params![
                spec.repo_root.to_string_lossy(),
                spec.tree_oid,
                spec.author_time,
                spec.commit_time,
                spec.commit_message,
                existing_id
            ],
        )?;
        existing_id
    } else {
        tx.execute(
            "INSERT INTO repo_snapshots
             (repo_root, commit_oid, tree_oid, author_time, commit_time, commit_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                spec.repo_root.to_string_lossy(),
                spec.commit_oid,
                spec.tree_oid,
                spec.author_time,
                spec.commit_time,
                spec.commit_message,
                now_secs()
            ],
        )?;
        tx.last_insert_rowid()
    };

    tx.execute(
        "DELETE FROM repo_snapshot_parents WHERE snapshot_id = ?1",
        params![snapshot_id],
    )?;
    for parent_oid in &spec.parent_oids {
        tx.execute(
            "INSERT INTO repo_snapshot_parents (snapshot_id, parent_oid) VALUES (?1, ?2)",
            params![snapshot_id, parent_oid],
        )?;
    }

    tx.commit()?;
    Ok(snapshot_id)
}

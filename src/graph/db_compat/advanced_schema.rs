use rusqlite::Connection;
use std::path::Path;

use super::{map_sqlite_query_err, DbCompatError};

/// Create repository snapshot tables for temporal tracking (v18).
pub fn ensure_temporal_schema(conn: &Connection, db_path: &Path) -> Result<(), DbCompatError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS repo_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_root TEXT NOT NULL,
            commit_oid TEXT NOT NULL UNIQUE,
            tree_oid TEXT NOT NULL,
            author_time INTEGER NOT NULL,
            commit_time INTEGER NOT NULL,
            commit_message TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_repo_snapshots_commit
         ON repo_snapshots(commit_oid)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS repo_snapshot_parents (
            snapshot_id INTEGER NOT NULL,
            parent_oid TEXT NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE,
            PRIMARY KEY (snapshot_id, parent_oid)
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_versions_snapshot
         ON file_versions(snapshot_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_versions_path
         ON file_versions(file_path)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbol_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            stable_id TEXT NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            file_path TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            start_col INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            end_col INTEGER NOT NULL,
            body_hash TEXT,
            FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_versions_snapshot
         ON symbol_versions(snapshot_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_versions_stable
         ON symbol_versions(stable_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_versions_name
         ON symbol_versions(name)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS edge_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            source_stable_id TEXT NOT NULL,
            target_stable_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_edge_versions_snapshot
         ON edge_versions(snapshot_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_edge_versions_source
         ON edge_versions(source_stable_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_edge_versions_target
         ON edge_versions(target_stable_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    Ok(())
}

pub fn ensure_scorer_schema(conn: &Connection, db_path: &Path) -> Result<(), DbCompatError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbol_scores (
            symbol_id INTEGER PRIMARY KEY,
            snapshot_id INTEGER NOT NULL,
            stable_id TEXT NOT NULL,
            score REAL NOT NULL,
            rank INTEGER,
            feature_loc INTEGER NOT NULL DEFAULT 0,
            feature_fan_in INTEGER NOT NULL DEFAULT 0,
            feature_fan_out INTEGER NOT NULL DEFAULT 0,
            feature_complexity INTEGER NOT NULL DEFAULT 0,
            feature_cfg_block_count INTEGER NOT NULL DEFAULT 0,
            feature_cfg_edge_count INTEGER NOT NULL DEFAULT 0,
            feature_conditional_density REAL NOT NULL DEFAULT 0.0,
            feature_lifetime INTEGER NOT NULL DEFAULT 0,
            feature_churn_count INTEGER NOT NULL DEFAULT 0,
            scorer_version TEXT NOT NULL,
            scored_at INTEGER NOT NULL,
            FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE,
            FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_scores_score ON symbol_scores(score DESC)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_scores_stable ON symbol_scores(stable_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS scorer_features (
            name TEXT PRIMARY KEY,
            weight REAL NOT NULL,
            enabled INTEGER NOT NULL,
            description TEXT
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS scorer_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scorer_version TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            symbols_scored INTEGER NOT NULL,
            feature_count INTEGER NOT NULL,
            metadata TEXT
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    Ok(())
}

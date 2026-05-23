//! Global meta-index database (~/.magellan/meta.db)
//!
//! Tracks project health, last reindex times, and daemon-level statistics
//! across all registered project shards.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const META_DB_DIR: &str = "/home/feanor/.magellan";
const META_DB_NAME: &str = "meta.db";

/// Daemon-level meta-index of all registered projects.
pub struct MetaDb {
    conn: Connection,
    path: PathBuf,
}

/// Project health snapshot from meta.db.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectStats {
    pub name: String,
    pub root: String,
    pub db_path: String,
    pub enabled: bool,
    pub last_reindexed: Option<i64>,
    pub file_count: Option<i64>,
    pub symbol_count: Option<i64>,
}

impl MetaDb {
    /// Return the default filesystem path for the global meta.db.
    pub fn default_path() -> PathBuf {
        PathBuf::from(META_DB_DIR).join(META_DB_NAME)
    }

    /// Open (or create) the global meta.db.
    pub fn open() -> Result<Self> {
        let path = PathBuf::from(META_DB_DIR).join(META_DB_NAME);
        std::fs::create_dir_all(META_DB_DIR)
            .with_context(|| format!("Failed to create {}", META_DB_DIR))?;
        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open meta.db at {}", path.display()))?;
        let mut db = Self { conn, path };
        db.ensure_schema()?;
        Ok(db)
    }

    /// Open at a specific path (useful for tests).
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent dir for {}", path.display()))?;
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open meta.db at {}", path.display()))?;
        let mut db = Self { conn, path };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&mut self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS project_registry (
                name TEXT PRIMARY KEY,
                root TEXT NOT NULL,
                db_path TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_reindexed INTEGER,
                file_count INTEGER,
                symbol_count INTEGER
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_project_registry_enabled
             ON project_registry (enabled)",
            [],
        )?;
        Ok(())
    }

    /// Upsert a project entry in the registry.
    pub fn upsert_project(
        &mut self,
        name: &str,
        root: &str,
        db_path: &str,
        enabled: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO project_registry (name, root, db_path, enabled, last_reindexed, file_count, symbol_count)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL)
             ON CONFLICT (name) DO UPDATE SET
               root = excluded.root,
               db_path = excluded.db_path,
               enabled = excluded.enabled",
            params![name, root, db_path, if enabled { 1 } else { 0 }],
        )?;
        Ok(())
    }

    /// Remove a project entry.
    pub fn remove_project(&mut self, name: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM project_registry WHERE name = ?1",
                params![name],
            )
            .with_context(|| format!("Failed to remove project '{}' from meta.db", name))?;
        Ok(())
    }

    /// Update last_reindexed to now.
    pub fn update_last_reindexed(&mut self, name: &str) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.conn.execute(
            "UPDATE project_registry SET last_reindexed = ?1 WHERE name = ?2",
            params![now, name],
        )?;
        Ok(())
    }

    /// Update file and symbol counts for a project.
    pub fn update_counts(&mut self, name: &str, file_count: i64, symbol_count: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE project_registry SET file_count = ?1, symbol_count = ?2 WHERE name = ?3",
            params![file_count, symbol_count, name],
        )?;
        Ok(())
    }

    /// List all projects.
    pub fn list_projects(&self) -> Result<Vec<ProjectStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, root, db_path, enabled, last_reindexed, file_count, symbol_count
             FROM project_registry ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectStats {
                name: row.get(0)?,
                root: row.get(1)?,
                db_path: row.get(2)?,
                enabled: row.get::<_, i32>(3)? != 0,
                last_reindexed: row.get(4)?,
                file_count: row.get(5)?,
                symbol_count: row.get(6)?,
            })
        })?;
        let mut projects = Vec::new();
        for row in rows {
            projects.push(row?);
        }
        Ok(projects)
    }

    /// Get single project stats.
    pub fn get_project(&self, name: &str) -> Result<Option<ProjectStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, root, db_path, enabled, last_reindexed, file_count, symbol_count
             FROM project_registry WHERE name = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(ProjectStats {
                name: row.get(0)?,
                root: row.get(1)?,
                db_path: row.get(2)?,
                enabled: row.get::<_, i32>(3)? != 0,
                last_reindexed: row.get(4)?,
                file_count: row.get(5)?,
                symbol_count: row.get(6)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    /// Close connection gracefully.
    pub fn close(self) -> Result<()> {
        self.conn
            .close()
            .map_err(|e| anyhow::anyhow!("MetaDb close error: {}", e.1))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_db_schema_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("meta.db");
        let mut db = MetaDb::open_at(&db_path).unwrap();

        db.upsert_project("alpha", "/home/alpha", "/home/alpha/db.sqlite", true)
            .unwrap();
        db.upsert_project("beta", "/home/beta", "/home/beta/db.sqlite", false)
            .unwrap();

        let projects = db.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "alpha");
        assert!(projects[0].enabled);
        assert!(!projects[1].enabled);

        db.update_last_reindexed("alpha").unwrap();
        let alpha = db.get_project("alpha").unwrap().unwrap();
        assert!(alpha.last_reindexed.is_some());

        db.update_counts("alpha", 42, 1337).unwrap();
        let alpha2 = db.get_project("alpha").unwrap().unwrap();
        assert_eq!(alpha2.file_count, Some(42));
        assert_eq!(alpha2.symbol_count, Some(1337));

        db.remove_project("beta").unwrap();
        assert!(db.get_project("beta").unwrap().is_none());
    }
}

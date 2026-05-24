//! Global meta-index database (~/.magellan/meta.db)
//!
//! Tracks project health, last reindex times, and daemon-level statistics
//! across all registered project shards.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const META_DB_DIR: &str = "/home/feanor/.magellan";

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Structural embedding record from `concept_embeddings`.
#[derive(Debug, Clone)]
pub struct EmbeddingRecord {
    pub project: String,
    pub symbol: String,
    pub file: String,
    pub hash: String,
    pub vec: Vec<u8>, // packed little-endian f32 bytes
}

/// Cross-project similarity pair from `pattern_cross_refs`.
#[derive(Debug, Clone)]
pub struct CrossRefRecord {
    pub project_a: String,
    pub symbol_a: String,
    pub file_a: String,
    pub project_b: String,
    pub symbol_b: String,
    pub file_b: String,
    pub similarity_score: f64,
}
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
        // Phase 4: structural analogy tables
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS concept_embeddings (
                project    TEXT NOT NULL,
                symbol     TEXT NOT NULL,
                file       TEXT NOT NULL,
                hash       TEXT NOT NULL,
                vec        BLOB NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (project, symbol, file)
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_concept_embeddings_project
             ON concept_embeddings (project)",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS pattern_cross_refs (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                project_a        TEXT NOT NULL,
                symbol_a         TEXT NOT NULL,
                file_a           TEXT NOT NULL,
                project_b        TEXT NOT NULL,
                symbol_b         TEXT NOT NULL,
                file_b           TEXT NOT NULL,
                similarity_score REAL NOT NULL,
                updated_at       INTEGER NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pattern_cross_refs_a
             ON pattern_cross_refs (project_a, symbol_a)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pattern_cross_refs_score
             ON pattern_cross_refs (similarity_score DESC)",
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
        let now = now_secs();
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

    // ── Phase 4: concept_embeddings ──

    /// Upsert a structural embedding for a symbol.
    /// `vec` is stored as packed little-endian f32 bytes.
    pub fn upsert_embedding(
        &mut self,
        project: &str,
        symbol: &str,
        file: &str,
        hash: &str,
        vec: &[f32],
    ) -> Result<()> {
        let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO concept_embeddings (project, symbol, file, hash, vec, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (project, symbol, file) DO UPDATE SET
               hash = excluded.hash,
               vec  = excluded.vec,
               updated_at = excluded.updated_at",
            params![project, symbol, file, hash, blob, now],
        )?;
        Ok(())
    }

    /// List all concept embeddings. `vec` is raw bytes (packed f32 LE).
    pub fn list_embeddings(&self) -> Result<Vec<EmbeddingRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT project, symbol, file, hash, vec FROM concept_embeddings ORDER BY project, symbol",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(EmbeddingRecord {
                project: row.get(0)?,
                symbol: row.get(1)?,
                file: row.get(2)?,
                hash: row.get(3)?,
                vec: row.get(4)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    // ── Phase 4: pattern_cross_refs ──

    /// Insert a cross-project similarity pair.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_cross_ref(
        &mut self,
        project_a: &str,
        symbol_a: &str,
        file_a: &str,
        project_b: &str,
        symbol_b: &str,
        file_b: &str,
        similarity_score: f64,
    ) -> Result<()> {
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO pattern_cross_refs
             (project_a, symbol_a, file_a, project_b, symbol_b, file_b, similarity_score, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![project_a, symbol_a, file_a, project_b, symbol_b, file_b, similarity_score, now],
        )?;
        Ok(())
    }

    /// Query cross-refs where project_a + symbol_a match, ordered by similarity DESC.
    pub fn query_cross_refs_for_symbol(
        &self,
        project: &str,
        symbol: &str,
    ) -> Result<Vec<CrossRefRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT project_a, symbol_a, file_a, project_b, symbol_b, file_b, similarity_score
             FROM pattern_cross_refs
             WHERE project_a = ?1 AND symbol_a = ?2
             ORDER BY similarity_score DESC",
        )?;
        let rows = stmt.query_map(params![project, symbol], |row| {
            Ok(CrossRefRecord {
                project_a: row.get(0)?,
                symbol_a: row.get(1)?,
                file_a: row.get(2)?,
                project_b: row.get(3)?,
                symbol_b: row.get(4)?,
                file_b: row.get(5)?,
                similarity_score: row.get(6)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
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

    // ── concept_embeddings ──

    #[test]
    fn test_upsert_and_list_embeddings() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = MetaDb::open_at(dir.path().join("meta.db")).unwrap();

        let vec_a: Vec<f32> = vec![1.0, 0.0, 0.0];
        db.upsert_embedding("proj_a", "greet", "src/lib.rs", "aabbcc", &vec_a)
            .unwrap();

        let vec_b: Vec<f32> = vec![0.0, 1.0, 0.0];
        db.upsert_embedding("proj_b", "greet", "src/main.rs", "ddeeff", &vec_b)
            .unwrap();

        let rows = db.list_embeddings().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].project, "proj_a");
        assert_eq!(rows[0].symbol, "greet");
        assert_eq!(rows[0].hash, "aabbcc");
        assert_eq!(rows[0].vec.len(), 3 * 4, "3 f32s × 4 bytes");
    }

    #[test]
    fn test_upsert_embedding_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = MetaDb::open_at(dir.path().join("meta.db")).unwrap();

        let v: Vec<f32> = vec![0.5, 0.5];
        db.upsert_embedding("p", "sym", "f.rs", "hash1", &v)
            .unwrap();
        db.upsert_embedding("p", "sym", "f.rs", "hash2", &v)
            .unwrap(); // same PK, updated hash

        let rows = db.list_embeddings().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].hash, "hash2", "second upsert should overwrite hash");
    }

    // ── pattern_cross_refs ──

    #[test]
    fn test_insert_and_query_cross_refs() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = MetaDb::open_at(dir.path().join("meta.db")).unwrap();

        db.insert_cross_ref(
            "proj_a", "greet", "src/a.rs", "proj_b", "hello", "src/b.rs", 0.92,
        )
        .unwrap();
        db.insert_cross_ref(
            "proj_a", "greet", "src/a.rs", "proj_c", "hi", "src/c.rs", 0.75,
        )
        .unwrap();

        let refs = db.query_cross_refs_for_symbol("proj_a", "greet").unwrap();
        assert_eq!(refs.len(), 2);
        // Should be ordered by similarity DESC
        assert!(
            refs[0].similarity_score >= refs[1].similarity_score,
            "expected descending order"
        );
        assert_eq!(refs[0].symbol_b, "hello");
        assert_eq!(refs[0].project_b, "proj_b");
    }

    #[test]
    fn test_query_cross_refs_empty_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let db = MetaDb::open_at(dir.path().join("meta.db")).unwrap();
        let refs = db
            .query_cross_refs_for_symbol("missing_proj", "sym")
            .unwrap();
        assert!(refs.is_empty());
    }
}

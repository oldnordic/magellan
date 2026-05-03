//! Read-only sqlitegraph database compatibility preflight.
//!
//! Phase 1 goal: refuse incompatible existing DBs *before any writes occur*.

use rusqlite::{params, OpenFlags, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn expected_sqlitegraph_schema_version() -> i64 {
    sqlitegraph::schema::SCHEMA_VERSION
}

pub use crate::migrate_cmd::MAGELLAN_SCHEMA_VERSION;

pub fn ensure_magellan_meta(db_path: &Path) -> Result<(), DbCompatError> {
    if is_in_memory_path(db_path) {
        return Ok(());
    }
    let conn = rusqlite::Connection::open(db_path).map_err(|e| map_sqlite_open_err(db_path, e))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    let existing: Option<(i64, i64)> = conn.query_row(
        "SELECT magellan_schema_version, sqlitegraph_schema_version FROM magellan_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(|e| map_sqlite_query_err(db_path, e))?;

    let expected_sqlitegraph = expected_sqlitegraph_schema_version();
    match existing {
        None => {
            let created_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            conn.execute(
                "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
                 VALUES (1, ?1, ?2, ?3)",
                params![MAGELLAN_SCHEMA_VERSION, expected_sqlitegraph, created_at],
            ).map_err(|e| map_sqlite_query_err(db_path, e))?;
            ensure_geo_index_meta_schema(&conn)?;
            ensure_symbol_fts_schema(&conn)?;
            Ok(())
        }
        Some((found_magellan, found_sqlitegraph)) => {
            if found_magellan != MAGELLAN_SCHEMA_VERSION {
                let mut current_version = found_magellan;
                while current_version < MAGELLAN_SCHEMA_VERSION {
                    match current_version {
                        4 => {
                            ensure_ast_schema(&conn)?;
                            current_version = 5;
                        }
                        5 => {
                            ensure_ast_schema(&conn)?;
                            current_version = 6;
                        }
                        6 => {
                            ensure_cfg_schema(&conn)?;
                            current_version = 7;
                        }
                        7 => {
                            ensure_cfg_hash_column(&conn)?;
                            current_version = 8;
                        }
                        8 => {
                            ensure_statements_column(&conn)?;
                            current_version = 9;
                        }
                        9 => {
                            ensure_4d_coordinates_columns(&conn)?;
                            current_version = 10;
                        }
                        10 => {
                            ensure_geo_index_meta_schema(&conn)?;
                            current_version = 11;
                        }
                        11 => {
                            ensure_symbol_fts_schema(&conn)?;
                            current_version = 12;
                        }
                        _ => {
                            return Err(DbCompatError::MagellanSchemaMismatch {
                                path: db_path.to_path_buf(),
                                found: found_magellan,
                                expected: MAGELLAN_SCHEMA_VERSION,
                            });
                        }
                    }
                    conn.execute(
                        "UPDATE magellan_meta SET magellan_schema_version = ?1 WHERE id = 1",
                        params![current_version],
                    )
                    .map_err(|e| map_sqlite_query_err(db_path, e))?;
                }
            }
            if found_sqlitegraph != expected_sqlitegraph {
                return Err(DbCompatError::SqliteGraphSchemaMismatch {
                    path: db_path.to_path_buf(),
                    found: found_sqlitegraph,
                    expected: expected_sqlitegraph,
                });
            }
            Ok(())
        }
    }
}

pub fn ensure_ast_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute("CREATE TABLE IF NOT EXISTS ast_nodes (id INTEGER PRIMARY KEY AUTOINCREMENT, parent_id INTEGER, kind TEXT NOT NULL, byte_start INTEGER NOT NULL, byte_end INTEGER NOT NULL, file_id INTEGER)", []).map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent ON ast_nodes(parent_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span ON ast_nodes(byte_start, byte_end)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_file_id ON ast_nodes(file_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    Ok(())
}

pub fn ensure_metrics_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute("CREATE TABLE IF NOT EXISTS file_metrics (file_path TEXT PRIMARY KEY, symbol_count INTEGER NOT NULL, loc INTEGER NOT NULL, estimated_loc REAL NOT NULL, fan_in INTEGER NOT NULL DEFAULT 0, fan_out INTEGER NOT NULL DEFAULT 0, complexity_score REAL NOT NULL DEFAULT 0.0, last_updated INTEGER NOT NULL)", []).map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute("CREATE TABLE IF NOT EXISTS symbol_metrics (symbol_id INTEGER PRIMARY KEY, symbol_name TEXT NOT NULL, kind TEXT NOT NULL, file_path TEXT NOT NULL, loc INTEGER NOT NULL, estimated_loc REAL NOT NULL, fan_in INTEGER NOT NULL DEFAULT 0, fan_out INTEGER NOT NULL DEFAULT 0, cyclomatic_complexity INTEGER NOT NULL DEFAULT 1, last_updated INTEGER NOT NULL, FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE)", []).map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    Ok(())
}

pub const CFG_EDGE: &str = "CFG_BLOCK";

pub fn ensure_cfg_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_blocks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        function_id INTEGER NOT NULL,
        kind TEXT NOT NULL,
        terminator TEXT NOT NULL,
        byte_start INTEGER NOT NULL,
        byte_end INTEGER NOT NULL,
        start_line INTEGER NOT NULL,
        start_col INTEGER NOT NULL,
        end_line INTEGER NOT NULL,
        end_col INTEGER NOT NULL,
        cfg_hash TEXT,
        statements TEXT,
        coord_x INTEGER DEFAULT 0,
        coord_y INTEGER DEFAULT 0,
        coord_z INTEGER DEFAULT 0,
        coord_t TEXT DEFAULT NULL,
        FOREIGN KEY (function_id) REFERENCES graph_entities(id) ON DELETE CASCADE
    )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function ON cfg_blocks(function_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_hash ON cfg_blocks(cfg_hash)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Check if cfg_edges table already exists (with any schema - legacy or new)
    // If it exists, skip creating tables/indexes to avoid schema conflicts
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cfg_edges' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .unwrap_or(false);

    if table_exists {
        // cfg_edges already exists - don't try to create with potentially incompatible schema
        return Ok(());
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_edges (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        function_id INTEGER NOT NULL,
        source_idx INTEGER NOT NULL,
        target_idx INTEGER NOT NULL,
        edge_type TEXT NOT NULL,
        FOREIGN KEY (function_id) REFERENCES graph_entities(id) ON DELETE CASCADE
    )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_edges_function ON cfg_edges(function_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    Ok(())
}

pub fn ensure_statements_column(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    let has_col: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='statements'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute("ALTER TABLE cfg_blocks ADD COLUMN statements TEXT", [])
            .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }
    Ok(())
}

/// Add 4D spatial-temporal coordinate columns to cfg_blocks table
///
/// This enables the 4D SSoT (Single Source of Truth) feature where:
/// - X (coord_x): Dominator depth (structural hierarchy)
/// - Y (coord_y): Loop nesting level (iterative complexity)
/// - Z (coord_z): Branch count (decision density)
/// - T (coord_t): Git commit hash or trace timestamp
pub fn ensure_4d_coordinates_columns(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    // Check and add coord_x column
    let has_x: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_x'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_x {
        conn.execute(
            "ALTER TABLE cfg_blocks ADD COLUMN coord_x INTEGER DEFAULT 0",
            [],
        )
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }

    // Check and add coord_y column
    let has_y: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_y'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_y {
        conn.execute(
            "ALTER TABLE cfg_blocks ADD COLUMN coord_y INTEGER DEFAULT 0",
            [],
        )
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }

    // Check and add coord_z column
    let has_z: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_z'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_z {
        conn.execute(
            "ALTER TABLE cfg_blocks ADD COLUMN coord_z INTEGER DEFAULT 0",
            [],
        )
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }

    // Check and add coord_t column
    let has_t: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_t'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_t {
        conn.execute(
            "ALTER TABLE cfg_blocks ADD COLUMN coord_t TEXT DEFAULT NULL",
            [],
        )
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }

    Ok(())
}

/// Add geo_index_meta table for lazy geometric index tracking
///
/// This table records when a .geo file was built from the SQLite database
/// and is used for staleness detection.
pub fn ensure_geo_index_meta_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS geo_index_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            geo_path TEXT NOT NULL,
            built_at INTEGER NOT NULL,
            schema_version INTEGER NOT NULL,
            symbol_count INTEGER NOT NULL,
            call_count INTEGER NOT NULL,
            cfg_block_count INTEGER NOT NULL,
            checksum TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    Ok(())
}

/// Add symbol_fts FTS5 virtual table for fast symbol search (v11 -> v12 migration)
///
/// Creates an FTS5 virtual table that indexes symbol names from graph_entities
/// for prefix and full-text search capabilities.
pub fn ensure_symbol_fts_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS symbol_fts USING fts5(
            name,
            content='graph_entities',
            content_rowid='id'
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    Ok(())
}

/// Ensure cfg_hash column exists in cfg_blocks table (v7 -> v8 migration)
///
/// Adds cfg_hash column for cache invalidation in downstream tools.
pub fn ensure_cfg_hash_column(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    // Add cfg_hash column if it doesn't exist
    // SQLite doesn't support IF NOT EXISTS for ADD COLUMN, so we check first
    let has_column: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='cfg_hash'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if !has_column {
        conn.execute("ALTER TABLE cfg_blocks ADD COLUMN cfg_hash TEXT", [])
            .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }

    // Create index for hash-based cache lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_hash
         ON cfg_blocks(cfg_hash)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    Ok(())
}

/// Add coverage side tables for weighted CFG analysis.
///
/// Creates cfg_block_coverage, cfg_edge_coverage, and cfg_coverage_meta.
/// Safe to call repeatedly — uses CREATE TABLE IF NOT EXISTS.
pub fn ensure_coverage_schema(
    conn: &rusqlite::Connection,
    db_path: &std::path::Path,
) -> Result<(), DbCompatError> {
    // Check if cfg_edges has the expected 'id' column before creating coverage tables
    // that reference it. If not, skip coverage schema (this is a legacy database).
    let has_cfg_edges_id: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_edges') WHERE name='id' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .unwrap_or(false);

    if !has_cfg_edges_id {
        // Legacy database with different cfg_edges schema - skip coverage tables
        return Ok(());
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_block_coverage (
            block_id INTEGER PRIMARY KEY,
            hit_count INTEGER NOT NULL DEFAULT 0,
            source_kind TEXT NOT NULL,
            source_revision TEXT,
            ingested_at INTEGER NOT NULL,
            FOREIGN KEY (block_id) REFERENCES cfg_blocks(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_edge_coverage (
            edge_id INTEGER PRIMARY KEY,
            hit_count INTEGER NOT NULL DEFAULT 0,
            source_kind TEXT NOT NULL,
            source_revision TEXT,
            ingested_at INTEGER NOT NULL,
            FOREIGN KEY (edge_id) REFERENCES cfg_edges(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_coverage_meta (
            source_kind TEXT PRIMARY KEY,
            source_revision TEXT,
            ingested_at INTEGER,
            total_blocks INTEGER,
            total_edges INTEGER
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_block_cov_hit ON cfg_block_coverage(block_id, hit_count)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_edge_cov_hit ON cfg_edge_coverage(edge_id, hit_count)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    Ok(())
}

/// Result of a successful preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightOk {
    NewDb,
    CompatibleExisting { found_schema_version: i64 },
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DbCompatError {
    #[error("DB_COMPAT: not a sqlite database: {path}")]
    NotSqlite { path: PathBuf },
    #[error("DB_COMPAT: corrupt sqlite database: {path}")]
    CorruptSqlite { path: PathBuf },
    #[error("DB_COMPAT: sqlitegraph schema mismatch: {path} (found={found}, expected={expected})")]
    SqliteGraphSchemaMismatch {
        path: PathBuf,
        found: i64,
        expected: i64,
    },
    #[error("DB_COMPAT: magellan schema mismatch: {path} (found={found}, expected={expected})")]
    MagellanSchemaMismatch {
        path: PathBuf,
        found: i64,
        expected: i64,
    },
    #[error("DB_COMPAT: expected sqlitegraph database but missing graph_meta table: {path}")]
    MissingGraphMeta { path: PathBuf },
    #[error("DB_COMPAT: graph_meta missing schema_version column: {path}")]
    GraphMetaMissingSchemaVersion { path: PathBuf },
    #[error("DB_COMPAT: graph_meta missing expected row id=1: {path}")]
    MissingGraphMetaRow { path: PathBuf, id: i64 },
    #[error("DB_COMPAT: sqlite preflight failure: {path}")]
    PreflightSqliteFailure { path: PathBuf },
}

pub fn preflight_sqlitegraph_compat(db_path: &Path) -> Result<PreflightOk, DbCompatError> {
    if is_in_memory_path(db_path) || !db_path.exists() {
        return Ok(PreflightOk::NewDb);
    }

    let header = std::fs::read(db_path).map_err(|_| DbCompatError::PreflightSqliteFailure {
        path: db_path.to_path_buf(),
    })?;
    if header.len() < 16 || &header[..16] != b"SQLite format 3\0" {
        return Err(DbCompatError::NotSqlite {
            path: db_path.to_path_buf(),
        });
    }

    let conn = rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| map_sqlite_open_err(db_path, e))?;

    let has_graph_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='graph_meta' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .optional()
        .map_err(|e| map_sqlite_query_err(db_path, e))?
        .unwrap_or(false);
    if !has_graph_meta {
        return Err(DbCompatError::MissingGraphMeta {
            path: db_path.to_path_buf(),
        });
    }

    let has_schema_version: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('graph_meta') WHERE name='schema_version' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .optional()
        .map_err(|e| map_sqlite_query_err(db_path, e))?
        .unwrap_or(false);
    if !has_schema_version {
        return Err(DbCompatError::GraphMetaMissingSchemaVersion {
            path: db_path.to_path_buf(),
        });
    }

    let found: Option<i64> = conn
        .query_row(
            "SELECT schema_version FROM graph_meta WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| map_sqlite_query_err(db_path, e))?;
    let Some(found) = found else {
        return Err(DbCompatError::MissingGraphMetaRow {
            path: db_path.to_path_buf(),
            id: 1,
        });
    };

    let expected = expected_sqlitegraph_schema_version();
    if found != expected {
        return Err(DbCompatError::SqliteGraphSchemaMismatch {
            path: db_path.to_path_buf(),
            found,
            expected,
        });
    }
    Ok(PreflightOk::CompatibleExisting {
        found_schema_version: found,
    })
}

fn is_in_memory_path(db_path: &Path) -> bool {
    db_path == Path::new(":memory:")
}
fn map_sqlite_open_err(path: &Path, _e: rusqlite::Error) -> DbCompatError {
    DbCompatError::PreflightSqliteFailure {
        path: path.to_path_buf(),
    }
}
fn map_sqlite_query_err(path: &Path, _e: rusqlite::Error) -> DbCompatError {
    DbCompatError::PreflightSqliteFailure {
        path: path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfg_edges_schema_created() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_cfg_schema(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_edges'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify index was also created
        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_cfg_edges_function'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 1);
    }

    #[test]
    fn test_coverage_schema_created() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_cfg_schema(&conn).unwrap();
        ensure_coverage_schema(&conn, std::path::Path::new(":memory:")).unwrap();

        let block_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_block_coverage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(block_count, 1);

        let edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_edge_coverage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 1);
    }
}

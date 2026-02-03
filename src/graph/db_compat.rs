//! Read-only sqlitegraph database compatibility preflight.
//!
//! Phase 1 goal: refuse incompatible existing DBs *before any writes occur*.
//!
//! Preflight rules (see `.planning/phases/01-persistence-compatibility-baseline/01-02-PLAN.md`):
//! - `:memory:` is treated as a new DB (compat OK)
//! - Non-existent file path is treated as a new DB (compat OK)
//! - Existing file is opened READ_ONLY via rusqlite and checked for:
//!   - valid sqlite header (query succeeds)
//!   - sqlitegraph meta presence (`graph_meta` table)
//!   - `graph_meta.schema_version` column exists
//!   - `graph_meta` row `id=1` exists
//!   - schema_version matches expected sqlitegraph schema version for this build

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OpenFlags, OptionalExtension};

/// Stable sqlitegraph schema version expected by this Magellan build.
///
/// Hard requirement: sourced from sqlitegraph public API.
pub fn expected_sqlitegraph_schema_version() -> i64 {
    // Preferred path: compile-time constant.
    // If this becomes unavailable due to upstream visibility changes, this function
    // will fail to compile and we should implement the plan's contingency strategy.
    sqlitegraph::schema::SCHEMA_VERSION
}

/// Magellan-owned schema version for side tables (e.g. `magellan_meta`).
///
/// Phase 1 baseline starts at version 1.
/// Phase 5: added execution_log table for tracking Magellan runs.
/// Phase 11: FQN-based symbol_id generation (breaking change).
/// Phase 20: Added canonical_fqn/display_fqn fields, switched to BLAKE3 (breaking change).
/// Phase 36: Added ast_nodes table for AST hierarchy storage.
/// Phase 40: Added file_id column to ast_nodes for per-file tracking.
pub const MAGELLAN_SCHEMA_VERSION: i64 = 6;

/// Ensure Magellan-owned metadata exists and matches expected versions.
///
/// ## Ordering contract
/// This MUST ONLY be called after:
/// 1) sqlitegraph preflight (read-only) succeeded
/// 2) sqlitegraph::SqliteGraph::open succeeded
///
/// This preserves the "no partial mutation" guarantee for incompatible DBs.
pub fn ensure_magellan_meta(db_path: &Path) -> Result<(), DbCompatError> {
    if is_in_memory_path(db_path) {
        // No on-disk metadata for in-memory databases.
        return Ok(());
    }

    // This is a write connection, but it's only reached after sqlitegraph preflight/open.
    let conn = rusqlite::Connection::open(db_path).map_err(|e| map_sqlite_open_err(db_path, e))?;

    // Create table if missing.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (\
            id INTEGER PRIMARY KEY CHECK (id = 1),\
            magellan_schema_version INTEGER NOT NULL,\
            sqlitegraph_schema_version INTEGER NOT NULL,\
            created_at INTEGER NOT NULL\
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(db_path, e))?;

    // Insert row if missing.
    let existing: Option<(i64, i64)> = conn
        .query_row(
            "SELECT magellan_schema_version, sqlitegraph_schema_version \
             FROM magellan_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| map_sqlite_query_err(db_path, e))?;

    let expected_sqlitegraph = expected_sqlitegraph_schema_version();

    match existing {
        None => {
            let created_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)\
                 VALUES (1, ?1, ?2, ?3)",
                params![MAGELLAN_SCHEMA_VERSION, expected_sqlitegraph, created_at],
            )
            .map_err(|e| map_sqlite_query_err(db_path, e))?;

            Ok(())
        }
        Some((found_magellan, found_sqlitegraph)) => {
            // Check if we need to upgrade magellan schema
            if found_magellan != MAGELLAN_SCHEMA_VERSION {
                // For v4 -> v5, we can do a lightweight migration here
                // since this is just adding a table, not changing core schema
                if found_magellan == 4 && MAGELLAN_SCHEMA_VERSION == 5 {
                    // Create ast_nodes table
                    ensure_ast_schema(&conn)?;

                    // Update version
                    conn.execute(
                        "UPDATE magellan_meta SET magellan_schema_version = ?1 WHERE id = 1",
                        params![MAGELLAN_SCHEMA_VERSION],
                    )
                    .map_err(|e| map_sqlite_query_err(db_path, e))?;
                } else if found_magellan == 5 && MAGELLAN_SCHEMA_VERSION == 6 {
                    // For v5 -> v6, add file_id column to ast_nodes
                    ensure_ast_schema(&conn)?;

                    // Update version
                    conn.execute(
                        "UPDATE magellan_meta SET magellan_schema_version = ?1 WHERE id = 1",
                        params![MAGELLAN_SCHEMA_VERSION],
                    )
                    .map_err(|e| map_sqlite_query_err(db_path, e))?;
                } else {
                    return Err(DbCompatError::MagellanSchemaMismatch {
                        path: db_path.to_path_buf(),
                        found: found_magellan,
                        expected: MAGELLAN_SCHEMA_VERSION,
                    });
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

/// Ensure ast_nodes table exists for Phase 36
///
/// Creates ast_nodes table with parent_id for tree structure and indexes
/// for efficient parent-child and span queries.
pub fn ensure_ast_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    // Main ast_nodes table (v5 schema without file_id)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ast_nodes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent_id INTEGER,
            kind TEXT NOT NULL,
            byte_start INTEGER NOT NULL,
            byte_end INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Index for parent-child queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent
         ON ast_nodes(parent_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Index for span-based position queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span
         ON ast_nodes(byte_start, byte_end)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Add file_id column if not exists (v6 upgrade)
    // SQLite doesn't support IF NOT EXISTS for ALTER TABLE, so we check first
    let has_file_id: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('ast_nodes') WHERE name='file_id' LIMIT 1",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?
        .unwrap_or(false);

    if !has_file_id {
        conn.execute(
            "ALTER TABLE ast_nodes ADD COLUMN file_id INTEGER",
            [],
        )
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

        // Create index for efficient per-file queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_file_id
             ON ast_nodes(file_id)",
            [],
        )
        .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;
    }

    Ok(())
}

/// Ensure metrics tables exist for Phase 34
///
/// Creates file_metrics and symbol_metrics tables with indexes if they don't exist.
pub fn ensure_metrics_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    // File-level metrics table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_metrics (
            file_path TEXT PRIMARY KEY,
            symbol_count INTEGER NOT NULL,
            loc INTEGER NOT NULL,
            estimated_loc REAL NOT NULL,
            fan_in INTEGER NOT NULL DEFAULT 0,
            fan_out INTEGER NOT NULL DEFAULT 0,
            complexity_score REAL NOT NULL DEFAULT 0.0,
            last_updated INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Symbol-level metrics table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbol_metrics (
            symbol_id INTEGER PRIMARY KEY,
            symbol_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            file_path TEXT NOT NULL,
            loc INTEGER NOT NULL,
            estimated_loc REAL NOT NULL,
            fan_in INTEGER NOT NULL DEFAULT 0,
            fan_out INTEGER NOT NULL DEFAULT 0,
            cyclomatic_complexity INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Indexes for query performance
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_in
         ON symbol_metrics(fan_in DESC)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_out
         ON symbol_metrics(fan_out DESC)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_metrics_complexity
         ON file_metrics(complexity_score DESC)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    Ok(())
}

/// Edge type constant for CFG edges in graph_edges table
///
/// Used to identify edges that represent control flow between basic blocks.
pub const CFG_EDGE: &str = "CFG_BLOCK";

/// Ensure CFG tables exist for Phase 42
///
/// Creates cfg_blocks table with indexes for efficient CFG queries.
/// Basic blocks are stored as separate entities with CFG_EDGE edges
/// representing control flow between blocks.
pub fn ensure_cfg_schema(conn: &rusqlite::Connection) -> Result<(), DbCompatError> {
    // Main cfg_blocks table
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
            FOREIGN KEY (function_id) REFERENCES graph_entities(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Index for function-based queries (get all blocks for a function)
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function
         ON cfg_blocks(function_id)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Index for span-based position queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_span
         ON cfg_blocks(byte_start, byte_end)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    // Index for terminator kind queries (find all return blocks, etc.)
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_terminator
         ON cfg_blocks(terminator)",
        [],
    )
    .map_err(|e| map_sqlite_query_err(Path::new(":memory:"), e))?;

    Ok(())
}

/// Result of a successful preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightOk {
    /// `:memory:` or missing file path; safe for sqlitegraph to create schema later.
    NewDb,
    /// Existing DB is sqlitegraph and schema matches expected.
    CompatibleExisting { found_schema_version: i64 },
}

/// Deterministic, normalized preflight failure.
///
/// IMPORTANT: user-facing error strings must be stable; do not propagate raw rusqlite messages.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DbCompatError {
    #[error("DB_COMPAT: not a sqlite database: {path}")]
    NotSqlite { path: PathBuf },

    #[error(
        "DB_COMPAT: corrupt sqlite database: {path} (code={code:?}, extended_code={extended_code})"
    )]
    CorruptSqlite {
        path: PathBuf,
        code: rusqlite::ErrorCode,
        extended_code: i32,
    },

    #[error(
        "DB_COMPAT: sqlite preflight failed: {path} (code={code:?}, extended_code={extended_code})"
    )]
    PreflightSqliteFailure {
        path: PathBuf,
        code: rusqlite::ErrorCode,
        extended_code: i32,
    },

    #[error("DB_COMPAT: expected sqlitegraph database but missing graph_meta table: {path}")]
    MissingGraphMeta { path: PathBuf },

    #[error("DB_COMPAT: graph_meta table missing schema_version column: {path}")]
    GraphMetaMissingSchemaVersion { path: PathBuf },

    #[error("DB_COMPAT: graph_meta missing expected row id={id}: {path}")]
    MissingGraphMetaRow { path: PathBuf, id: i64 },

    #[error("DB_COMPAT: sqlitegraph schema mismatch: {path} (found={found}, expected={expected})")]
    SqliteGraphSchemaMismatch {
        path: PathBuf,
        found: i64,
        expected: i64,
    },

    #[error("DB_COMPAT: magellan schema mismatch: {path} (found={found}, expected={expected})\n\nThis database was created by an older version of Magellan.\nTo upgrade, delete the database file and re-index your codebase.\n\n  rm {path}\n  magellan scan --db {path} <your-code-directory>\n\nNote: Symbol IDs have changed to use BLAKE3-based identity with 32-character hex format.")]
    MagellanSchemaMismatch {
        path: PathBuf,
        found: i64,
        expected: i64,
    },
}

/// Read-only preflight for sqlitegraph compatibility.
///
/// This function MUST NOT mutate the on-disk database.
pub fn preflight_sqlitegraph_compat(db_path: &Path) -> Result<PreflightOk, DbCompatError> {
    if is_in_memory_path(db_path) {
        return Ok(PreflightOk::NewDb);
    }

    if !db_path.exists() {
        return Ok(PreflightOk::NewDb);
    }

    let conn = rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| map_sqlite_open_err(db_path, e))?;

    // (a) Not a SQLite database / corrupt file.
    // Perform a trivial query and classify ONLY using structured error variants + error codes.
    verify_can_query(&conn, db_path)?;

    // (b) Missing sqlitegraph meta table.
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

    // (c) Missing required columns.
    let has_schema_version_col: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('graph_meta') WHERE name='schema_version' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .optional()
        .map_err(|e| map_sqlite_query_err(db_path, e))?
        .unwrap_or(false);

    if !has_schema_version_col {
        return Err(DbCompatError::GraphMetaMissingSchemaVersion {
            path: db_path.to_path_buf(),
        });
    }

    // (d) Missing id=1 row.
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

    // (e) Version mismatch.
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

fn verify_can_query(conn: &rusqlite::Connection, db_path: &Path) -> Result<(), DbCompatError> {
    // Querying sqlite_master is sufficient to force sqlite to parse header.
    // Using `SELECT 1` is even simpler.
    let res: Result<i64, rusqlite::Error> = conn.query_row("SELECT 1", [], |row| row.get(0));
    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(map_sqlite_query_err(db_path, e)),
    }
}

fn map_sqlite_open_err(db_path: &Path, err: rusqlite::Error) -> DbCompatError {
    map_sqlite_err(db_path, err)
}

fn map_sqlite_query_err(db_path: &Path, err: rusqlite::Error) -> DbCompatError {
    map_sqlite_err(db_path, err)
}

fn map_sqlite_err(db_path: &Path, err: rusqlite::Error) -> DbCompatError {
    match err {
        rusqlite::Error::SqliteFailure(sql_err, _maybe_msg) => {
            let code = sql_err.code;
            let extended_code = sql_err.extended_code;

            if code == rusqlite::ErrorCode::NotADatabase {
                return DbCompatError::NotSqlite {
                    path: db_path.to_path_buf(),
                };
            }

            if code == rusqlite::ErrorCode::DatabaseCorrupt
                || is_corrupt_extended_code(extended_code)
            {
                return DbCompatError::CorruptSqlite {
                    path: db_path.to_path_buf(),
                    code,
                    extended_code,
                };
            }

            DbCompatError::PreflightSqliteFailure {
                path: db_path.to_path_buf(),
                code,
                extended_code,
            }
        }

        // For deterministic output we still map "other" errors to a stable variant.
        // rusqlite errors without sqlite codes are relatively rare here.
        _ => DbCompatError::PreflightSqliteFailure {
            path: db_path.to_path_buf(),
            code: rusqlite::ErrorCode::Unknown,
            extended_code: 0,
        },
    }
}

fn is_corrupt_extended_code(extended_code: i32) -> bool {
    // SQLite defines extended result codes by OR-ing the primary code into the low bits.
    // Anything whose low byte matches SQLITE_CORRUPT (11) is treated as corruption.
    // This is deterministic and avoids error-message matching.
    (extended_code & 0xFF) == rusqlite::ErrorCode::DatabaseCorrupt as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn preflight_treats_memory_as_new() {
        let ok = preflight_sqlitegraph_compat(Path::new(":memory:")).unwrap();
        assert_eq!(ok, PreflightOk::NewDb);
    }

    #[test]
    fn preflight_treats_missing_file_as_new() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("does-not-exist.db");

        let ok = preflight_sqlitegraph_compat(&db_path).unwrap();
        assert_eq!(ok, PreflightOk::NewDb);
    }

    #[test]
    fn preflight_rejects_non_sqlite_file() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("not-sqlite.db");
        std::fs::write(&db_path, b"hello").unwrap();

        let err = preflight_sqlitegraph_compat(&db_path).unwrap_err();
        assert!(matches!(err, DbCompatError::NotSqlite { .. }), "{err}");
    }

    #[test]
    fn preflight_rejects_sqlite_without_graph_meta() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("plain.db");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE t(x INTEGER)", []).unwrap();
        drop(conn);

        let err = preflight_sqlitegraph_compat(&db_path).unwrap_err();
        assert!(
            matches!(err, DbCompatError::MissingGraphMeta { .. }),
            "{err}"
        );
    }

    #[test]
    fn preflight_rejects_schema_mismatch() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("sqlitegraph.db");

        // Create a real sqlitegraph DB.
        let _ = sqlitegraph::SqliteGraph::open(&db_path).unwrap();

        // Mutate graph_meta schema_version to a mismatching value.
        let conn = Connection::open(&db_path).unwrap();
        let expected = expected_sqlitegraph_schema_version();
        let mismatch = expected + 1;
        conn.execute(
            "UPDATE graph_meta SET schema_version=?1 WHERE id=1",
            [mismatch],
        )
        .unwrap();
        drop(conn);

        let err = preflight_sqlitegraph_compat(&db_path).unwrap_err();
        assert!(
            matches!(
                err,
                DbCompatError::SqliteGraphSchemaMismatch {
                    found,
                    expected,
                    ..
                } if found == mismatch && expected == expected_sqlitegraph_schema_version()
            ),
            "{err}"
        );
    }

    #[test]
    fn preflight_accepts_compatible_sqlitegraph_db() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("sqlitegraph_ok.db");

        let _ = sqlitegraph::SqliteGraph::open(&db_path).unwrap();

        let ok = preflight_sqlitegraph_compat(&db_path).unwrap();
        assert!(matches!(ok, PreflightOk::CompatibleExisting { .. }));
    }
}

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

use rusqlite::{OpenFlags, OptionalExtension};

/// Stable sqlitegraph schema version expected by this Magellan build.
///
/// Hard requirement: sourced from sqlitegraph public API.
pub fn expected_sqlitegraph_schema_version() -> i64 {
    // Preferred path: compile-time constant.
    // If this becomes unavailable due to upstream visibility changes, this function
    // will fail to compile and we should implement the plan's contingency strategy.
    sqlitegraph::schema::SCHEMA_VERSION
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

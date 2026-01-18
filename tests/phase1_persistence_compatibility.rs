//! Phase 1 persistence compatibility regression tests (DB-02).
//!
//! Hard requirements (from `.planning/phases/01-persistence-compatibility-baseline/01-03-PLAN.md`):
//! - New DB open records schema versions in both sqlitegraph's `graph_meta` and Magellan's `magellan_meta`
//! - Incompatible DBs are refused deterministically (stable DB_COMPAT markers)
//! - Refusals do NOT partially mutate the DB (no new tables) and do NOT overwrite non-sqlite files

use std::path::{Path, PathBuf};

use magellan::{CodeGraph, MAGELLAN_SCHEMA_VERSION};
use rusqlite::Connection;
use tempfile::tempdir;

fn sqlite_master_tables(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare_cached("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();

    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn file_sha256(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(path).unwrap();
    let mut h = Sha256::new();
    h.update(&bytes);
    hex::encode(h.finalize())
}

fn assert_magellan_meta_row(db_path: &Path) {
    let conn = Connection::open(db_path).unwrap();

    // Ensure table exists.
    let has_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='magellan_meta' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_table, "magellan_meta table must exist");

    let (magellan_schema_version, sqlitegraph_schema_version): (i64, i64) = conn
        .query_row(
            "SELECT magellan_schema_version, sqlitegraph_schema_version FROM magellan_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(magellan_schema_version, MAGELLAN_SCHEMA_VERSION);
    assert_eq!(
        sqlitegraph_schema_version,
        sqlitegraph::schema::SCHEMA_VERSION,
        "sqlitegraph_schema_version must match sqlitegraph::schema::SCHEMA_VERSION"
    );
}

#[test]
fn new_db_records_schema_versions() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("new.db");

    let _graph = CodeGraph::open(&db_path).unwrap();

    let conn = Connection::open(&db_path).unwrap();

    // graph_meta exists + row id=1 exists
    let has_graph_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='graph_meta' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_graph_meta, "graph_meta table must exist");

    let graph_meta_row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM graph_meta WHERE id=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(graph_meta_row_count, 1, "graph_meta must contain id=1");

    assert_magellan_meta_row(&db_path);
}

#[test]
fn not_a_sqlite_database_is_refused_without_overwrite() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("not-sqlite.db");

    std::fs::write(&db_path, b"not sqlite bytes").unwrap();

    let before_size = std::fs::metadata(&db_path).unwrap().len();
    let before_hash = file_sha256(&db_path);

    let err = match CodeGraph::open(&db_path) {
        Ok(_) => panic!("expected open() to fail"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("DB_COMPAT: not a sqlite database"),
        "expected normalized not-sqlite error, got: {msg}"
    );

    let after_size = std::fs::metadata(&db_path).unwrap().len();
    let after_hash = file_sha256(&db_path);

    assert_eq!(before_size, after_size, "file must not be overwritten");
    assert_eq!(before_hash, after_hash, "file contents must be unchanged");
}

#[test]
fn missing_graph_meta_table_is_refused_without_mutation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("missing_graph_meta.db");

    let conn = Connection::open(&db_path).unwrap();
    conn.execute("CREATE TABLE t(x INTEGER)", []).unwrap();
    let before_tables = sqlite_master_tables(&conn);
    drop(conn);

    let err = match CodeGraph::open(&db_path) {
        Ok(_) => panic!("expected open() to fail"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("DB_COMPAT: expected sqlitegraph database but missing graph_meta table"),
        "expected normalized missing-graph_meta error, got: {msg}"
    );

    let conn_after = Connection::open(&db_path).unwrap();
    let after_tables = sqlite_master_tables(&conn_after);

    assert_eq!(before_tables, after_tables, "no tables should be created");
}

#[test]
fn missing_graph_meta_row_id_1_is_refused_without_mutation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("missing_graph_meta_row.db");

    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE graph_meta(id INTEGER PRIMARY KEY CHECK (id = 1), schema_version INTEGER NOT NULL)",
        [],
    )
    .unwrap();
    let before_tables = sqlite_master_tables(&conn);
    drop(conn);

    let err = match CodeGraph::open(&db_path) {
        Ok(_) => panic!("expected open() to fail"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("DB_COMPAT: graph_meta missing expected row id=1"),
        "expected normalized missing id=1 error, got: {msg}"
    );

    let conn_after = Connection::open(&db_path).unwrap();
    let after_tables = sqlite_master_tables(&conn_after);

    assert_eq!(before_tables, after_tables, "no tables should be created");
}

#[test]
fn sqlitegraph_schema_version_mismatch_older_newer_is_refused_without_mutation() {
    let dir = tempdir().unwrap();

    let expected = sqlitegraph::schema::SCHEMA_VERSION;
    let cases = [expected - 1, expected + 1];

    for wrong in cases {
        let db_path = dir.path().join(format!("schema_mismatch_{wrong}.db"));

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE graph_meta(id INTEGER PRIMARY KEY CHECK (id = 1), schema_version INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO graph_meta (id, schema_version) VALUES (1, ?1)",
            [wrong],
        )
        .unwrap();
        let before_tables = sqlite_master_tables(&conn);
        drop(conn);

        let err = match CodeGraph::open(&db_path) {
            Ok(_) => panic!("expected open() to fail"),
            Err(e) => e,
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("DB_COMPAT: sqlitegraph schema mismatch"),
            "expected schema mismatch marker, got: {msg}"
        );
        assert!(
            msg.contains(&format!("found={wrong}"))
                && msg.contains(&format!("expected={expected}")),
            "expected found/expected values in message, got: {msg}"
        );

        let conn_after = Connection::open(&db_path).unwrap();
        let after_tables = sqlite_master_tables(&conn_after);
        assert_eq!(before_tables, after_tables, "no tables should be created");
    }
}

#[test]
fn cli_status_refuses_incompatible_db_deterministically() {
    // Uses assert_cmd-like behavior implemented manually with std::process::Command
    // to avoid introducing a new dev-dependency.

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("not-sqlite-cli.db");
    std::fs::write(&db_path, b"not sqlite bytes").unwrap();

    let exe = PathBuf::from(env!("CARGO_BIN_EXE_magellan"));

    let output = std::process::Command::new(exe)
        .arg("status")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for incompatible DB"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DB_COMPAT:"),
        "stderr must include deterministic DB_COMPAT prefix; got: {stderr}"
    );
}

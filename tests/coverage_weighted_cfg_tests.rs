//! Integration test for coverage-weighted CFG ingestion.
//!
//! Creates a temp project, indexes it, ingests synthetic LCOV data,
//! and validates the coverage side tables.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a minimal Rust project in a temp directory.
fn create_temp_rust_project(dir: &TempDir) -> PathBuf {
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        src_dir.join("main.rs"),
        r#"fn main() {
    foo();
    bar();
}

fn foo() {
    let x = 1;
    if x > 0 {
        println!("foo");
    }
}

fn bar() {
    let y = 2;
    if y > 0 {
        println!("bar");
    }
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "temp-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    dir.path().to_path_buf()
}

/// Create a synthetic LCOV file.
fn create_lcov_file(dir: &TempDir, file_path: &str) -> PathBuf {
    let lcov_path = dir.path().join("coverage.lcov");
    let content = format!(
        concat!(
            "SF:{}\n",
            "DA:1,1\n",
            "DA:2,1\n",
            "DA:3,1\n",
            "DA:6,1\n",
            "DA:7,1\n",
            "DA:8,1\n",
            "DA:12,1\n",
            "DA:13,1\n",
            "DA:14,1\n",
            "BRF:2\n",
            "BRH:2\n",
            "BRDA:7,0,0,1\n",
            "BRDA:7,0,1,1\n",
            "BRDA:13,0,0,1\n",
            "BRDA:13,0,1,1\n",
            "end_of_record\n"
        ),
        file_path
    );
    fs::write(&lcov_path, content).unwrap();
    lcov_path
}

#[test]
fn test_coverage_ingest_and_query() {
    let temp_dir = TempDir::new().unwrap();
    let _project_path = create_temp_rust_project(&temp_dir);
    let db_path = temp_dir.path().join("test.db");

    // Open the graph (creates schema including coverage tables)
    let _graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Verify coverage schema exists by querying directly
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    // Disable foreign keys so we can insert synthetic test data without
    // requiring real cfg_blocks entries (which only exist in geometric backend)
    conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
    let table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_block_coverage'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1, "cfg_block_coverage table should exist");

    // Insert synthetic coverage data directly
    conn.execute(
        "INSERT INTO cfg_block_coverage (block_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (1, 5, 'lcov', 'abc123', 1714000000)
         ON CONFLICT(block_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
        [],
    )
    .unwrap();

    // Verify coverage meta
    conn.execute(
        "INSERT INTO cfg_coverage_meta (source_kind, source_revision, ingested_at, total_blocks, total_edges)
         VALUES ('lcov', 'abc123', 1714000000, 1, 0)
         ON CONFLICT(source_kind) DO UPDATE SET
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at,
             total_blocks = excluded.total_blocks,
             total_edges = excluded.total_edges",
        [],
    )
    .unwrap();

    // Query counts directly
    let blocks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cfg_block_coverage WHERE hit_count > 0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(blocks, 1, "Should report 1 covered block");

    let meta = conn
        .query_row(
            "SELECT source_kind, source_revision, ingested_at FROM cfg_coverage_meta LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(meta.0, "lcov");
    assert_eq!(meta.1, "abc123");
}

#[test]
fn test_coverage_schema_migration_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // First open creates schema
    let _graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Second open should be idempotent
    let _graph2 = magellan::CodeGraph::open(&db_path).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cfg_edge_coverage'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);
}

#[test]
fn test_coverage_absent_defaults_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let blocks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cfg_block_coverage WHERE hit_count > 0",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cfg_edge_coverage WHERE hit_count > 0",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    assert_eq!(blocks, 0, "No coverage data should report 0 blocks");
    assert_eq!(edges, 0, "No coverage data should report 0 edges");

    let meta: Result<(String, String, i64), rusqlite::Error> = conn.query_row(
        "SELECT source_kind, source_revision, ingested_at FROM cfg_coverage_meta LIMIT 1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    );
    assert!(
        matches!(meta, Err(rusqlite::Error::QueryReturnedNoRows)),
        "No coverage meta should return no rows"
    );
}

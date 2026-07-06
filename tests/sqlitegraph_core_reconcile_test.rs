//! Regression harness for repeated reconcile over sqlitegraph-core.
//!
//! This exercises the same large Rust tree that exposed daemon-side reconcile
//! failures in local service runs. The test is ignored by default because it
//! depends on the developer workspace layout and performs a full scan + replay.

use magellan::CodeGraph;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use walkdir::WalkDir;

const SQLITEGRAPH_CORE_SRC: &str = "/home/feanor/Projects/sqlitegraph/sqlitegraph-core/src";
const SQLITEGRAPH_CORE_DB: &str = "/home/feanor/.magellan/sqlitegraph/sqlitegraph-core.db";

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .collect();
    files.sort();
    files
}

fn copy_sqlite_family(src_db: &Path, dst_db: &Path) {
    fs::copy(src_db, dst_db).expect("copy main db");
    for suffix in ["-wal", "-shm"] {
        let src = PathBuf::from(format!("{}{}", src_db.display(), suffix));
        if src.exists() {
            let dst = PathBuf::from(format!("{}{}", dst_db.display(), suffix));
            fs::copy(src, dst).expect("copy sqlite sidecar");
        }
    }
}

#[test]
#[ignore = "local regression harness for sqlitegraph-core reconcile failures"]
fn sqlitegraph_core_reconcile_passes_without_db_errors() {
    let root = PathBuf::from(SQLITEGRAPH_CORE_SRC);
    if !root.exists() {
        eprintln!("Skipping: {} not present", root.display());
        return;
    }

    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("sqlitegraph-core-reconcile.db");
    let mut graph = CodeGraph::open(&db_path).expect("open graph");

    graph
        .scan_directory(&root, None)
        .expect("initial scan of sqlitegraph-core/src");

    for path in rust_files(&root) {
        let path_key =
            magellan::normalize_path(&path).unwrap_or_else(|_| path.to_string_lossy().to_string());
        graph
            .reconcile_file_path(&path, &path_key)
            .unwrap_or_else(|err| panic!("reconcile failed for {}: {err:#}", path.display()));
    }

    graph.checkpoint_wal().expect("checkpoint after replay");

    let conn = rusqlite::Connection::open(&db_path).expect("open db for integrity check");
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity_check");
    assert_eq!(integrity, "ok", "database integrity check failed");
}

#[test]
#[ignore = "local reproduction for delete_file_facts failure on canonical sqlitegraph-core db"]
fn sqlitegraph_core_delete_edge_compat_on_canonical_copy() {
    let src_db = PathBuf::from(SQLITEGRAPH_CORE_DB);
    if !src_db.exists() {
        eprintln!("Skipping: {} not present", src_db.display());
        return;
    }

    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("sqlitegraph-core-canonical-copy.db");
    copy_sqlite_family(&src_db, &db_path);

    let mut graph = CodeGraph::open(&db_path).expect("open copied graph");
    let path =
        "/home/feanor/Projects/sqlitegraph/sqlitegraph-core/src/backend/native/v3/edge_compat.rs";

    graph
        .delete_file_facts(path)
        .unwrap_or_else(|err| panic!("delete_file_facts failed for {path}: {err:#}"));
}

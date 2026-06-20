use magellan::{CodeGraph, SnapshotFileInput, SnapshotSpec};
use tempfile::tempdir;

#[test]
fn test_stable_symbol_id_survives_line_shift() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("graph.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path = "src/lib.rs";
    let source_v1 = b"fn compute() -> u32 {\n    1\n}\n";
    graph.index_file(path, source_v1).unwrap();
    let stable_v1 = graph
        .stable_symbol_id_by_name(path, "compute")
        .unwrap()
        .unwrap();

    let source_v2 = b"\n\nfn compute() -> u32 {\n    1\n}\n";
    graph.index_file(path, source_v2).unwrap();
    let stable_v2 = graph
        .stable_symbol_id_by_name(path, "compute")
        .unwrap()
        .unwrap();

    assert_eq!(stable_v1, stable_v2);
}

#[test]
fn test_snapshot_ingest_persists_versions_and_skips_unchanged_files() {
    let dir = tempdir().unwrap();
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(repo_root.join("src")).unwrap();

    let db_path = dir.path().join("graph.db");
    let graph = CodeGraph::open(&db_path).unwrap();

    let helper_path = repo_root.join("src/helper.rs");
    let consts_path = repo_root.join("src/consts.rs");

    let helper_v1 =
        b"pub fn helper() -> u32 {\n    1\n}\n\npub fn wrapper() -> u32 {\n    helper()\n}\n"
            .to_vec();
    let consts_v1 = b"pub const VALUE: u32 = 7;\n".to_vec();

    let snapshot1 = graph
        .register_snapshot(&SnapshotSpec {
            repo_root: repo_root.clone(),
            commit_oid: "commit-1".to_string(),
            tree_oid: "tree-1".to_string(),
            author_time: 1,
            commit_time: 1,
            commit_message: "initial".to_string(),
            parent_oids: Vec::new(),
        })
        .unwrap();

    let stats1 = graph
        .ingest_snapshot_sources(
            snapshot1,
            &repo_root,
            &[
                SnapshotFileInput {
                    path: helper_path.clone(),
                    source: helper_v1.clone(),
                },
                SnapshotFileInput {
                    path: consts_path.clone(),
                    source: consts_v1.clone(),
                },
            ],
        )
        .unwrap();

    assert_eq!(stats1.files_total, 2);
    assert_eq!(stats1.files_indexed, 2);
    assert_eq!(stats1.files_skipped, 0);
    assert!(stats1.symbol_versions >= 2);
    assert!(stats1.edge_versions >= 1);

    let helper_v2 =
        b"\npub fn helper() -> u32 {\n    1\n}\n\npub fn wrapper() -> u32 {\n    helper()\n}\n"
            .to_vec();

    let snapshot2 = graph
        .register_snapshot(&SnapshotSpec {
            repo_root: repo_root.clone(),
            commit_oid: "commit-2".to_string(),
            tree_oid: "tree-2".to_string(),
            author_time: 2,
            commit_time: 2,
            commit_message: "line shift".to_string(),
            parent_oids: vec!["commit-1".to_string()],
        })
        .unwrap();

    let stats2 = graph
        .ingest_snapshot_sources(
            snapshot2,
            &repo_root,
            &[
                SnapshotFileInput {
                    path: helper_path.clone(),
                    source: helper_v2,
                },
                SnapshotFileInput {
                    path: consts_path.clone(),
                    source: consts_v1,
                },
            ],
        )
        .unwrap();

    assert_eq!(stats2.files_total, 2);
    assert_eq!(stats2.files_indexed, 1);
    assert_eq!(stats2.files_skipped, 1);
    assert!(stats2.symbol_versions >= 2);
    assert!(stats2.edge_versions >= 1);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let helper_id_v1: String = conn
        .query_row(
            "SELECT stable_id FROM symbol_versions WHERE snapshot_id = ?1 AND file_path = 'src/helper.rs' AND name = 'helper'",
            [snapshot1],
            |row| row.get(0),
        )
        .unwrap();
    let helper_id_v2: String = conn
        .query_row(
            "SELECT stable_id FROM symbol_versions WHERE snapshot_id = ?1 AND file_path = 'src/helper.rs' AND name = 'helper'",
            [snapshot2],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(helper_id_v1, helper_id_v2);

    let snapshot2_files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_versions WHERE snapshot_id = ?1",
            [snapshot2],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(snapshot2_files, 2);
}

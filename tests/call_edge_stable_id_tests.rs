//! Regression tests for BUG-2: CALLER/CALLS edges mis-wired at ingest.
//!
//! Root cause: edge construction looked up the caller/callee SIMPLE name in an
//! FQN-keyed map (always missing for methods), then fell back to `.first()` of
//! all same-named symbols DB-wide in HashMap order — while the correct,
//! file-scoped stable symbol ID was already persisted on the Call node.
//!
//! These tests prove that for same-named methods in different files, the CALLER
//! edge of a call node points at the same-file impl, and the CALLS edge points
//! at the same-file callee.

use rusqlite::Connection;

/// Fetch (from_id, to_id) pairs for edges of a given type touching a node.
fn edge_pairs(conn: &Connection, edge_type: &str, call_node_id: i64) -> Vec<(i64, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT from_id, to_id FROM graph_edges
             WHERE edge_type = ?1 AND (from_id = ?2 OR to_id = ?2)
             ORDER BY from_id, to_id",
        )
        .unwrap();
    stmt.query_map(rusqlite::params![edge_type, call_node_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

/// Look up the Symbol entity id for (file_path, name).
fn symbol_entity_id(conn: &Connection, file_path: &str, name: &str) -> i64 {
    conn.query_row(
        "SELECT id FROM graph_entities
         WHERE kind = 'Symbol' AND file_path = ?1
           AND json_extract(data, '$.name') = ?2",
        rusqlite::params![file_path, name],
        |row| row.get(0),
    )
    .unwrap()
}

/// Entity ids of all Call nodes recorded in a given file.
fn call_node_ids_in_file(conn: &Connection, file_path: &str) -> Vec<i64> {
    let mut stmt = conn
        .prepare("SELECT id FROM graph_entities WHERE kind = 'Call' AND file_path = ?1 ORDER BY id")
        .unwrap();
    stmt.query_map(rusqlite::params![file_path], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

const SAME_NAME_FIXTURE_A: &str = r#"
pub struct Worker;

impl Worker {
    pub fn same_name(&self) {}

    pub fn boot(&self) {
        self.same_name();
    }
}
"#;

#[test]
#[allow(deprecated)]
fn test_caller_edge_points_to_same_file_impl() {
    // Two files each defining fn same_name + one calling it. The CALLER edge of
    // each call node must point at the same-file `boot` method, and the CALLS
    // edge at the same-file `same_name` method — never the other file's instance.
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path_a = temp_dir.path().join("a.rs").to_string_lossy().to_string();
    let path_b = temp_dir.path().join("b.rs").to_string_lossy().to_string();

    graph
        .index_file(&path_a, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();
    graph
        .index_file(&path_b, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();
    drop(graph);

    let conn = Connection::open(&db_path).unwrap();

    for file in [&path_a, &path_b] {
        let call_ids = call_node_ids_in_file(&conn, file);
        assert_eq!(
            call_ids.len(),
            1,
            "{} should have exactly one call node (boot -> same_name)",
            file
        );
        let call_id = call_ids[0];

        let expected_caller = symbol_entity_id(&conn, file, "boot");
        let expected_callee = symbol_entity_id(&conn, file, "same_name");

        let caller_edges = edge_pairs(&conn, "CALLER", call_id);
        assert_eq!(
            caller_edges,
            vec![(expected_caller, call_id)],
            "CALLER edge for call in {file} must originate from the same-file `boot`"
        );

        let calls_edges = edge_pairs(&conn, "CALLS", call_id);
        assert_eq!(
            calls_edges,
            vec![(call_id, expected_callee)],
            "CALLS edge for call in {file} must target the same-file `same_name`"
        );
    }
}

#[test]
#[allow(deprecated)]
fn test_repair_call_edges_rewires_miswired_db() {
    // Simulate a BUG-2-corrupted database: correct stable IDs persisted on the
    // Call node, but the CALLER/CALLS edges manually rewired to the WRONG
    // same-named symbols in the other file. The repair pass must detect and
    // rewire them from the persisted stable IDs alone (no re-parse).
    use magellan::{repair_call_edges, CodeGraph};
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path_a = temp_dir.path().join("a.rs").to_string_lossy().to_string();
    let path_b = temp_dir.path().join("b.rs").to_string_lossy().to_string();

    graph
        .index_file(&path_a, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();
    graph
        .index_file(&path_b, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();
    drop(graph);

    // Corrupt: point b.rs's CALLER/CALLS edges at a.rs's symbols.
    {
        let conn = Connection::open(&db_path).unwrap();
        let b_call = call_node_ids_in_file(&conn, &path_b)[0];
        let a_boot = symbol_entity_id(&conn, &path_a, "boot");
        let a_same_name = symbol_entity_id(&conn, &path_a, "same_name");
        conn.execute(
            "UPDATE graph_edges SET from_id = ?1 WHERE to_id = ?2 AND edge_type = 'CALLER'",
            rusqlite::params![a_boot, b_call],
        )
        .unwrap();
        conn.execute(
            "UPDATE graph_edges SET to_id = ?1 WHERE from_id = ?2 AND edge_type = 'CALLS'",
            rusqlite::params![a_same_name, b_call],
        )
        .unwrap();
    }

    // Dry-run: reports the corruption, writes nothing.
    let dry = repair_call_edges(&db_path, false).unwrap();
    assert!(!dry.applied);
    assert_eq!(dry.call_nodes_total, 2);
    assert_eq!(dry.call_nodes_with_stable_ids, 2);
    assert_eq!(dry.caller_edges_miswired, 1);
    assert_eq!(dry.calls_edges_miswired, 1);
    assert_eq!(dry.caller_edges_missing, 0);
    assert_eq!(dry.calls_edges_missing, 0);
    assert_eq!(dry.unresolved_stable_ids, 0);
    assert_eq!(dry.edges_deleted, 0);
    assert_eq!(dry.edges_inserted, 0);

    // Dry-run left the corruption in place.
    {
        let conn = Connection::open(&db_path).unwrap();
        let b_call = call_node_ids_in_file(&conn, &path_b)[0];
        let a_boot = symbol_entity_id(&conn, &path_a, "boot");
        assert_eq!(
            edge_pairs(&conn, "CALLER", b_call),
            vec![(a_boot, b_call)],
            "dry-run must not modify edges"
        );
    }

    // Apply: rewires to the same-file symbols.
    let applied = repair_call_edges(&db_path, true).unwrap();
    assert!(applied.applied);
    assert_eq!(applied.caller_edges_miswired, 1);
    assert_eq!(applied.calls_edges_miswired, 1);
    assert_eq!(applied.edges_deleted, 2);
    assert_eq!(applied.edges_inserted, 2);

    let conn = Connection::open(&db_path).unwrap();
    let b_call = call_node_ids_in_file(&conn, &path_b)[0];
    let b_boot = symbol_entity_id(&conn, &path_b, "boot");
    let b_same_name = symbol_entity_id(&conn, &path_b, "same_name");
    assert_eq!(edge_pairs(&conn, "CALLER", b_call), vec![(b_boot, b_call)]);
    assert_eq!(
        edge_pairs(&conn, "CALLS", b_call),
        vec![(b_call, b_same_name)]
    );

    // Repair is idempotent: a second pass finds nothing to do.
    let clean = repair_call_edges(&db_path, true).unwrap();
    assert_eq!(clean.caller_edges_miswired, 0);
    assert_eq!(clean.calls_edges_miswired, 0);
    assert_eq!(clean.edges_deleted, 0);
    assert_eq!(clean.edges_inserted, 0);
}

#[test]
#[allow(deprecated)]
fn test_repair_call_edges_recreates_missing_edges() {
    // A Call node whose CALLER edge was never written (or was lost) gets the
    // edge recreated from the persisted stable ID.
    use magellan::{repair_call_edges, CodeGraph};
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path_a = temp_dir.path().join("a.rs").to_string_lossy().to_string();

    graph
        .index_file(&path_a, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();
    drop(graph);

    {
        let conn = Connection::open(&db_path).unwrap();
        let a_call = call_node_ids_in_file(&conn, &path_a)[0];
        conn.execute(
            "DELETE FROM graph_edges WHERE to_id = ?1 AND edge_type = 'CALLER'",
            rusqlite::params![a_call],
        )
        .unwrap();
    }

    let dry = repair_call_edges(&db_path, false).unwrap();
    assert_eq!(dry.caller_edges_missing, 1);
    assert_eq!(dry.caller_edges_miswired, 0);

    let applied = repair_call_edges(&db_path, true).unwrap();
    assert_eq!(applied.caller_edges_missing, 1);
    assert_eq!(applied.edges_inserted, 1);

    let conn = Connection::open(&db_path).unwrap();
    let a_call = call_node_ids_in_file(&conn, &path_a)[0];
    let a_boot = symbol_entity_id(&conn, &path_a, "boot");
    assert_eq!(edge_pairs(&conn, "CALLER", a_call), vec![(a_boot, a_call)]);
}

#[test]
#[allow(deprecated)]
fn test_same_file_caller_wins_over_db_wide_name_fallback() {
    // Even at the public query API level, each file's `same_name` must report
    // exactly its own file's caller (the bug let the wrong instance win the
    // `.first()` lottery).
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let path_a = temp_dir.path().join("a.rs").to_string_lossy().to_string();
    let path_b = temp_dir.path().join("b.rs").to_string_lossy().to_string();

    graph
        .index_file(&path_a, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();
    graph
        .index_file(&path_b, SAME_NAME_FIXTURE_A.as_bytes())
        .unwrap();

    for file in [&path_a, &path_b] {
        let callers = graph.callers_of_symbol(file, "same_name").unwrap();
        assert_eq!(
            callers.len(),
            1,
            "{}'s same_name should have exactly one caller, got {callers:?}",
            file
        );
        assert_eq!(callers[0].caller, "boot");
        assert_eq!(
            callers[0].file_path,
            std::path::PathBuf::from(file),
            "caller of {}'s same_name must be recorded in {}",
            file,
            file
        );
    }
}

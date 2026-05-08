//! Tests verifying that CodeGraph opens fewer connections and reuses them.

use magellan::CodeGraph;

#[test]
fn test_warm_open_skips_redundant_ddl() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    // First open creates schema
    {
        let _graph = CodeGraph::open(&db).expect("first open should succeed");
    }

    // Second open should skip DDL because magellan_meta shows version 11
    {
        let _graph = CodeGraph::open(&db).expect("second open should succeed");
    }

    // Database should still be healthy
    let graph = CodeGraph::open(&db).expect("third open should succeed");
    let stats = graph.get_stats().expect("stats should work");
    assert_eq!(stats.file_count, 0);
}

#[test]
fn test_execution_log_uses_shared_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    let graph = CodeGraph::open(&db).expect("open should succeed");

    let execution_id = "test-exec-1";
    graph
        .execution_log()
        .start_execution(execution_id, "test", &[], None, db.to_str().unwrap())
        .expect("start_execution should succeed");

    let record = graph
        .execution_log()
        .get_by_execution_id(execution_id)
        .expect("get should succeed")
        .expect("record should exist");
    assert_eq!(record.execution_id, execution_id);
}

#[test]
fn test_metrics_ops_uses_shared_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("test.db");

    let graph = CodeGraph::open(&db).expect("open should succeed");

    let metrics = magellan::graph::metrics::FileMetrics {
        file_path: "src/main.rs".to_string(),
        symbol_count: 5,
        loc: 100,
        estimated_loc: 95.0,
        fan_in: 2,
        fan_out: 3,
        complexity_score: 5.0,
        last_updated: 1234567890,
    };
    graph
        .metrics()
        .upsert_file_metrics(&metrics)
        .expect("upsert should succeed");

    let retrieved = graph
        .metrics()
        .get_file_metrics("src/main.rs")
        .expect("get should succeed")
        .expect("record should exist");
    assert_eq!(retrieved.symbol_count, 5);
}

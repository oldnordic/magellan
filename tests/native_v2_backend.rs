//! Test Native V2 backend performance vs SQLite backend
//!
//! This test requires the `native-v2` feature to be enabled on sqlitegraph.

use std::path::PathBuf;
use std::time::Instant;

#[test]
fn test_native_v2_vs_sqlite_backend() -> anyhow::Result<()> {
    println!("\n=== Native V2 Backend vs SQLite Backend ===\n");

    // Test 1: SQLite Backend
    println!("--- Test 1: SQLite Backend (Current Magellan) ---");
    let sqlite_results = benchmark_sqlite_backend(1000)?;
    println!("  SQLite Backend Results:");
    println!(
        "    1000 entity inserts: {:?}",
        sqlite_results.insert_entities
    );
    println!("    1000 edge inserts: {:?}", sqlite_results.insert_edges);
    println!(
        "    1000 neighbor queries: {:?}",
        sqlite_results.neighbor_queries
    );

    // Test 2: Native V2 Backend (only if feature is available)
    #[cfg(feature = "native-v2")]
    {
        println!("\n--- Test 2: Native V2 Backend (High Performance) ---");
        let native_results = benchmark_native_v2_backend(1000)?;
        println!("  Native V2 Backend Results:");
        println!(
            "    1000 entity inserts: {:?}",
            native_results.insert_entities
        );
        println!("    1000 edge inserts: {:?}", native_results.insert_edges);
        println!(
            "    1000 neighbor queries: {:?}",
            native_results.neighbor_queries
        );

        // Compare results
        println!("\n--- Performance Comparison ---");
        let speedup_entities = sqlite_results.insert_entities.as_nanos() as f64
            / native_results.insert_entities.as_nanos() as f64;
        let speedup_edges = sqlite_results.insert_edges.as_nanos() as f64
            / native_results.insert_edges.as_nanos() as f64;
        let speedup_queries = sqlite_results.neighbor_queries.as_nanos() as f64
            / native_results.neighbor_queries.as_nanos() as f64;

        println!("  Entity inserts: {:.2}x faster", speedup_entities);
        println!("  Edge inserts: {:.2}x faster", speedup_edges);
        println!("  Neighbor queries: {:.2}x faster", speedup_queries);

        println!("\n  ✓ Native V2 backend working\n");
    }

    #[cfg(not(feature = "native-v2"))]
    {
        println!("\n--- Test 2: Native V2 Backend ---");
        println!("  ⚠ Native V2 feature not enabled on sqlitegraph");
        println!("  Add to Cargo.toml: sqlitegraph = {{ version = \"0.2.10\", features = [\"native-v2\"] }}\n");
    }

    Ok(())
}

struct BenchmarkResults {
    insert_entities: std::time::Duration,
    insert_edges: std::time::Duration,
    neighbor_queries: std::time::Duration,
}

fn benchmark_sqlite_backend(count: usize) -> anyhow::Result<BenchmarkResults> {
    use sqlitegraph::{open_graph, GraphConfig};
    use std::fs;

    let test_db = PathBuf::from("/tmp/magellann_sqlite_bench.db");
    if test_db.exists() {
        fs::remove_file(&test_db)?;
    }

    // Use the unified open_graph API with SQLite backend
    let config = GraphConfig::sqlite();
    let graph = open_graph(&test_db, &config)?;

    // Benchmark entity insertion
    let start = Instant::now();
    for i in 0..count {
        let node_spec = sqlitegraph::NodeSpec {
            kind: "Function".to_string(),
            name: format!("function_{}", i),
            file_path: Some(format!("src/file_{}.rs", i / 10)),
            data: serde_json::json!({
                "byte_start": i * 100,
                "byte_end": i * 100 + 50,
            }),
        };
        graph.insert_node(node_spec)?;
    }
    let insert_entities = start.elapsed();

    // Benchmark edge insertion
    let start = Instant::now();
    for i in 0..count {
        let edge_spec = sqlitegraph::EdgeSpec {
            from: (i + 1) as i64,
            to: ((i + 1) % count + 1) as i64,
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(edge_spec)?;
    }
    let insert_edges = start.elapsed();

    // Benchmark neighbor queries
    let start = Instant::now();
    for i in 0..count {
        let _ = graph.neighbors((i + 1) as i64, sqlitegraph::NeighborQuery::default())?;
    }
    let neighbor_queries = start.elapsed();

    Ok(BenchmarkResults {
        insert_entities,
        insert_edges,
        neighbor_queries,
    })
}

#[cfg(feature = "native-v2")]
fn benchmark_native_v2_backend(count: usize) -> anyhow::Result<BenchmarkResults> {
    use sqlitegraph::{open_graph, GraphConfig};
    use std::fs;

    let test_db = PathBuf::from("/tmp/magellan_native_v2_bench.db");
    if test_db.exists() {
        fs::remove_file(&test_db)?;
    }

    // Use the unified open_graph API with Native backend
    let config = GraphConfig::native();
    let graph = open_graph(&test_db, &config)?;

    // Benchmark entity insertion
    let start = Instant::now();
    for i in 0..count {
        let node_spec = sqlitegraph::NodeSpec {
            kind: "Function".to_string(),
            name: format!("function_{}", i),
            file_path: Some(format!("src/file_{}.rs", i / 10)),
            data: serde_json::json!({
                "byte_start": i * 100,
                "byte_end": i * 100 + 50,
            }),
        };
        graph.insert_node(node_spec)?;
    }
    let insert_entities = start.elapsed();

    // Benchmark edge insertion
    let start = Instant::now();
    for i in 0..count {
        let edge_spec = sqlitegraph::EdgeSpec {
            from: (i + 1) as i64,
            to: ((i + 1) % count + 1) as i64,
            edge_type: "CALLS".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(edge_spec)?;
    }
    let insert_edges = start.elapsed();

    // Benchmark neighbor queries
    let start = Instant::now();
    for i in 0..count {
        let _ = graph.neighbors((i + 1) as i64, sqlitegraph::NeighborQuery::default())?;
    }
    let neighbor_queries = start.elapsed();

    Ok(BenchmarkResults {
        insert_entities,
        insert_edges,
        neighbor_queries,
    })
}

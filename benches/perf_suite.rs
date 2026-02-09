//! Performance benchmark suite for Magellan
//!
//! Benchmarks measure three representative workloads:
//! - B1: Neighbor expansion (clustered vs standard adjacency)
//! - B2: Call graph reachability (depth-10 traversal)
//! - B3: Symbol lookup (KV O(1) vs SQL query)
//!
//! Run with: cargo bench --bench perf_suite
//!
//! With native-v2-perf feature (clustered adjacency):
//! cargo bench --bench perf_suite --features native-v2-perf

mod harness;

use criterion::{black_box, Criterion, Throughput, criterion_group, criterion_main};
use harness::{setup_high_fanout_graph, setup_large_graph, setup_test_graph};

/// B1: Neighbor Expansion Benchmark
///
/// Measures neighbor query performance for high-fanout nodes.
/// Compares standard adjacency storage vs clustered adjacency.
fn benchmark_b1_neighbor_expansion(c: &mut Criterion) {
    let mut group = c.benchmark_group("B1_Neighbor_Expansion");

    // Setup: graph with nodes having 100 neighbors each
    let (graph, _temp_dir, node_ids) = setup_high_fanout_graph(100, 100);

    group.throughput(Throughput::Elements(100));

    // Baseline: standard neighbors()
    group.bench_function("baseline", |b| {
        let backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend> = graph.__backend_for_benchmarks();
        let snapshot = sqlitegraph::SnapshotId::current();
        b.iter(|| {
            for &node_id in &node_ids {
                let _neighbors: Result<Vec<i64>, _> = black_box(backend.neighbors(
                    black_box(snapshot),
                    black_box(node_id),
                    sqlitegraph::NeighborQuery {
                        direction: sqlitegraph::BackendDirection::Outgoing,
                        edge_type: None,
                    },
                ));
            }
        })
    });

    // Perf: clustered neighbors (if feature available)
    #[cfg(feature = "v2_experimental")]
    group.bench_function("clustered", |b| {
        let backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend> = graph.__backend_for_benchmarks();
        b.iter(|| {
            for &node_id in &node_ids {
                // Use clustered adjacency if available
                let _neighbors: Result<Vec<i64>, _> = black_box(
                    backend
                        .neighbors_clustered(black_box(node_id), black_box(sqlitegraph::BackendDirection::Outgoing))
                );
            }
        })
    });

    group.finish();
}

/// B2: Reachability Traversal Benchmark
///
/// Measures call graph traversal performance for depth-10 reachability.
/// Compares standard BFS traversal vs clustered traversal.
fn benchmark_b2_reachability(c: &mut Criterion) {
    let mut group = c.benchmark_group("B2_Reachability");

    // Setup: graph with depth-10 call chains
    let (graph, _temp_dir, entry_points) = setup_large_graph();

    let mut total_nodes = 0usize;
    let mut total_edges = 0usize;

    // Baseline: standard traversal using neighbors()
    group.bench_function("baseline", |b| {
        let backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend> = graph.__backend_for_benchmarks();
        let snapshot = sqlitegraph::SnapshotId::current();
        let mut visited = std::collections::HashSet::new();
        let mut stack = Vec::new();

        b.iter(|| {
            visited.clear();
            stack.clear();

            // BFS from entry points to depth 10
            for &entry_id in &entry_points {
                stack.push((entry_id, 0));
            }

            let mut local_nodes = 0usize;
            let mut local_edges = 0usize;

            while let Some((node_id, depth)) = stack.pop() {
                if depth > 10 || visited.contains(&node_id) {
                    continue;
                }

                visited.insert(node_id);
                local_nodes += 1;

                if let Ok(neighbors) = backend.neighbors(
                    black_box(snapshot),
                    black_box(node_id),
                    sqlitegraph::NeighborQuery {
                        direction: sqlitegraph::BackendDirection::Outgoing,
                        edge_type: Some("CALLS".to_string()),
                    },
                ) {
                    local_edges += neighbors.len();
                    for &neighbor_id in &neighbors {
                        if !visited.contains(&neighbor_id) {
                            stack.push((neighbor_id, depth + 1));
                        }
                    }
                }
            }

            total_nodes = local_nodes;
            total_edges = local_edges;
        })
    });

    // Report traversal statistics
    println!("B2 Traversal: {} nodes, {} edges", total_nodes, total_edges);

    // Perf: clustered traversal (if feature available)
    #[cfg(feature = "v2_experimental")]
    group.bench_function("clustered", |b| {
        let backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend> = graph.__backend_for_benchmarks();
        let snapshot = sqlitegraph::SnapshotId::current();
        let mut visited = std::collections::HashSet::new();
        let mut stack = Vec::new();

        b.iter(|| {
            visited.clear();
            stack.clear();

            // BFS from entry points to depth 10
            for &entry_id in &entry_points {
                stack.push((entry_id, 0));
            }

            let mut local_nodes = 0usize;
            let mut local_edges = 0usize;

            while let Some((node_id, depth)) = stack.pop() {
                if depth > 10 || visited.contains(&node_id) {
                    continue;
                }

                visited.insert(node_id);
                local_nodes += 1;

                if let Ok(neighbors) = backend.neighbors_clustered(
                    black_box(node_id),
                    black_box(sqlitegraph::BackendDirection::Outgoing),
                ) {
                    local_edges += neighbors.len();
                    for &neighbor_id in &neighbors {
                        if !visited.contains(&neighbor_id) {
                            stack.push((neighbor_id, depth + 1));
                        }
                    }
                }
            }

            total_nodes = local_nodes;
            total_edges = local_edges;
        })
    });

    group.finish();
}

/// B3: Symbol Lookup Benchmark
///
/// Measures symbol lookup performance comparing:
/// - Baseline: SQL query by symbol name
/// - Perf: KV O(1) lookup by FQN (if native-v2 feature enabled)
fn benchmark_b3_symbol_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("B3_Symbol_Lookup");

    // Setup: small graph for symbol lookup
    let (mut graph, _temp_dir, _symbol_ids) = setup_test_graph();

    // Create a test file with known symbols for lookup
    let test_names = vec![
        "function_1", "function_2", "branch_root", "branch_a", "fan_out_source",
    ];

    group.throughput(Throughput::Elements(test_names.len() as u64));

    // Baseline: SQL query for symbol by name
    group.bench_function("baseline", |b| {
        b.iter(|| {
            for name in &test_names {
                let _result = black_box(graph.symbol_id_by_name(black_box("test.rs"), black_box(name)));
            }
        })
    });

    // Perf: KV O(1) lookup (if native-v2 feature enabled)
    #[cfg(feature = "native-v2")]
    group.bench_function("kv_lookup", |b| {
        use magellan::kv;
        let backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend> = graph.__backend_for_benchmarks();
        b.iter(|| {
            for name in &test_names {
                let fqn = format!("test.rs::{}", name);
                let _result = black_box(kv::lookup_symbol_by_fqn(
                    backend,
                    black_box(&fqn),
                ));
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_b1_neighbor_expansion,
    benchmark_b2_reachability,
    benchmark_b3_symbol_lookup
);
criterion_main!(benches);

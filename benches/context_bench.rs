//! Benchmarks for LLM Context API
//!
//! Measures context query latency on large codebases.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use magellan::CodeGraph;
use std::path::PathBuf;
use tempfile::TempDir;

/// Benchmark context summary query
fn bench_context_summary(c: &mut Criterion) {
    let (graph, _temp_dir) = setup_test_graph();

    c.bench_function("context_summary", |b| {
        b.iter(|| {
            let mut g = black_box(&graph);
            magellan::context::get_project_summary(black_box(&mut g))
        })
    });
}

/// Benchmark symbol list query with pagination
fn bench_context_list(c: &mut Criterion) {
    let (graph, _temp_dir) = setup_test_graph();

    let page_sizes = [10, 50, 100];
    let mut group = c.benchmark_group("context_list");

    for page_size in page_sizes {
        group.bench_with_input(
            BenchmarkId::from_parameter(page_size),
            &page_size,
            |b, &page_size| {
                b.iter(|| {
                    let mut g = black_box(&graph);
                    let query = magellan::context::ListQuery {
                        kind: None,
                        page: Some(1),
                        page_size: Some(page_size),
                        cursor: None,
                        file_pattern: None,
                    };
                    magellan::context::list_symbols(black_box(&mut g), black_box(&query))
                })
            },
        );
    }
    group.finish();
}

/// Benchmark symbol detail query
fn bench_context_symbol(c: &mut Criterion) {
    let (graph, _temp_dir) = setup_test_graph();

    c.bench_function("context_symbol", |b| {
        b.iter(|| {
            let mut g = black_box(&graph);
            magellan::context::get_symbol_detail(
                black_box(&mut g),
                black_box("main"),
                black_box(None),
            )
        })
    });
}

/// Benchmark file context query
fn bench_context_file(c: &mut Criterion) {
    let (graph, _temp_dir) = setup_test_graph();

    c.bench_function("context_file", |b| {
        b.iter(|| {
            let mut g = black_box(&graph);
            magellan::context::get_file_context(black_box(&mut g), black_box("/test/main.rs"))
        })
    });
}

/// Benchmark with large codebase (100k+ symbols)
fn bench_context_large_codebase(c: &mut Criterion) {
    let (graph, _temp_dir) = setup_large_test_graph();

    let mut group = c.benchmark_group("context_large");

    group.bench_function("summary_100k", |b| {
        b.iter(|| {
            let mut g = black_box(&graph);
            magellan::context::get_project_summary(black_box(&mut g))
        })
    });

    group.bench_function("list_100k_page1", |b| {
        b.iter(|| {
            let mut g = black_box(&graph);
            let query = magellan::context::ListQuery {
                kind: Some("fn".to_string()),
                page: Some(1),
                page_size: Some(50),
                cursor: None,
                file_pattern: None,
            };
            magellan::context::list_symbols(black_box(&mut g), black_box(&query))
        })
    });

    group.bench_function("list_100k_page100", |b| {
        b.iter(|| {
            let mut g = black_box(&graph);
            let query = magellan::context::ListQuery {
                kind: Some("fn".to_string()),
                page: Some(100),
                page_size: Some(50),
                cursor: None,
                file_pattern: None,
            };
            magellan::context::list_symbols(black_box(&mut g), black_box(&query))
        })
    });

    group.finish();
}

/// Set up a test graph with sample data
fn setup_test_graph() -> (CodeGraph, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create test files and symbols
    for i in 0..100 {
        let file_path = format!("/test/file_{}.rs", i);
        let _ = graph.delete_file(&file_path);

        for j in 0..10 {
            let symbol_name = format!("function_{}_{}", i, j);
            // Index symbol (simplified - in reality would use index_file)
        }
    }

    (graph, temp_dir)
}

/// Set up a large test graph (100k+ symbols)
fn setup_large_test_graph() -> (CodeGraph, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("large.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create 1000 files with 100 symbols each = 100k symbols
    for i in 0..1000 {
        let file_path = format!("/test/large/file_{}.rs", i);
        let _ = graph.delete_file(&file_path);

        for j in 0..100 {
            let symbol_name = format!("fn_{}_{}", i, j);
            // Index symbol
        }
    }

    (graph, temp_dir)
}

criterion_group!(
    benches,
    bench_context_summary,
    bench_context_list,
    bench_context_symbol,
    bench_context_file,
    bench_context_large_codebase,
);

criterion_main!(benches);

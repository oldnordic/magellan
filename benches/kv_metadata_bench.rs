//! KV metadata storage performance benchmarks.
//!
//! Benchmarks compare KV store performance vs SQLite for metadata operations:
//! - Chunk storage/retrieval
//! - Execution log operations
//! - File/symbol metrics
//!
//! Run with: cargo bench --bench kv_metadata_bench --features native-v2

use criterion::{black_box, BenchmarkId, Criterion, criterion_group, criterion_main};
use magellan::generation::{ChunkStore, CodeChunk};
use magellan::graph::{ExecutionLog, MetricsOps};
use magellan::graph::metrics::FileMetrics;
use magellan::CodeGraph;
use tempfile::TempDir;

fn benchmark_chunk_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_ops");

    // Benchmark: Chunk storage (100 chunks)
    group.bench_function("store_100_chunks_kv", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");

            let graph = CodeGraph::open(&db_path).unwrap();
            let backend = graph.__backend_for_benchmarks();
            
            #[cfg(feature = "native-v2")]
            let chunk_store = ChunkStore::with_kv_backend(std::rc::Rc::clone(backend));
            
            #[cfg(not(feature = "native-v2"))]
            let chunk_store = ChunkStore::new(&db_path);

            for i in 0..100 {
                let chunk = CodeChunk::new(
                    format!("file_{}.rs", i),
                    i * 10,
                    (i + 1) * 10,
                    format!("content {}", i),
                    Some(format!("symbol_{}", i)),
                    Some("Function".to_string()),
                );
                black_box(chunk_store.store_chunk(&chunk)).unwrap();
            }
        })
    });

    // Benchmark: Chunk retrieval (100 chunks)
    group.bench_function("get_100_chunks_kv", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");

            // Store chunks first
            {
                let graph = CodeGraph::open(&db_path).unwrap();
                let backend = graph.__backend_for_benchmarks();
                
                #[cfg(feature = "native-v2")]
                let chunk_store = ChunkStore::with_kv_backend(std::rc::Rc::clone(backend));
                
                #[cfg(not(feature = "native-v2"))]
                let chunk_store = ChunkStore::new(&db_path);

                for i in 0..100 {
                    let chunk = CodeChunk::new(
                        format!("file_{}.rs", i),
                        i * 10,
                        (i + 1) * 10,
                        format!("content {}", i),
                        Some(format!("symbol_{}", i)),
                        Some("Function".to_string()),
                    );
                    chunk_store.store_chunk(&chunk).unwrap();
                }
            }

            // Retrieve chunks
            let graph = CodeGraph::open(&db_path).unwrap();
            let chunk_store = ChunkStore::new(&db_path);
            
            for i in 0..100 {
                black_box(chunk_store.get_chunks_by_file(&format!("file_{}.rs", i))).unwrap();
            }
        })
    });

    group.finish();
}

fn benchmark_execution_log_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution_log_ops");

    // Benchmark: Start/finish 100 execution records
    group.bench_function("log_100_executions_kv", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");

            let graph = CodeGraph::open(&db_path).unwrap();
            let backend = graph.__backend_for_benchmarks();
            
            #[cfg(feature = "native-v2")]
            let execution_log = ExecutionLog::with_kv_backend(std::rc::Rc::clone(backend));
            
            #[cfg(not(feature = "native-v2"))]
            let execution_log = ExecutionLog::new(&db_path);

            let args = vec!["scan".to_string()];
            
            for i in 0..100 {
                let exec_id = format!("exec_{}", i);
                execution_log.start_execution(&exec_id, "2.1.0", &args, None, "/test.db").unwrap();
                execution_log.finish_execution(&exec_id, "success", None, i, i * 2, i * 3).unwrap();
            }
        })
    });

    group.finish();
}

fn benchmark_metrics_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics_ops");

    // Benchmark: Upsert 100 file metrics
    group.bench_function("upsert_100_file_metrics_kv", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");

            let graph = CodeGraph::open(&db_path).unwrap();
            let backend = graph.__backend_for_benchmarks();
            
            #[cfg(feature = "native-v2")]
            let metrics = MetricsOps::with_kv_backend(std::rc::Rc::clone(backend));
            
            #[cfg(not(feature = "native-v2"))]
            let metrics = MetricsOps::new(&db_path);

            for i in 0..100 {
                let file_metrics = FileMetrics {
                    file_path: format!("file_{}.rs", i),
                    symbol_count: i,
                    loc: i * 10,
                    estimated_loc: (i * 10) as f64,
                    fan_in: 0,
                    fan_out: i,
                    complexity_score: i as f64,
                    last_updated: i as i64,
                };
                black_box(metrics.upsert_file_metrics(&file_metrics)).unwrap();
            }
        })
    });

    group.finish();
}

fn benchmark_combined_metadata(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_metadata");

    // Benchmark: Complete indexing workflow with all metadata types
    group.bench_function("index_with_all_metadata", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("test.db");

            let source_code = r#"
pub fn main() {
    println!("Hello");
    helper();
}

pub fn helper() {
    println!("Helper");
}
"#;

            let mut graph = CodeGraph::open(&db_path).unwrap();
            
            // Index file (creates graph entities/edges)
            black_box(graph.index_file("test.rs", source_code.as_bytes())).unwrap();

            let backend = graph.__backend_for_benchmarks();

            // Store chunks
            #[cfg(feature = "native-v2")]
            let chunk_store = ChunkStore::with_kv_backend(std::rc::Rc::clone(backend));
            
            #[cfg(not(feature = "native-v2"))]
            let chunk_store = ChunkStore::new(&db_path);

            let chunk = CodeChunk::new(
                "test.rs".to_string(),
                0,
                50,
                "pub fn main() { ... }".to_string(),
                Some("main".to_string()),
                Some("Function".to_string()),
            );
            chunk_store.store_chunk(&chunk).unwrap();

            // Log execution
            #[cfg(feature = "native-v2")]
            let execution_log = ExecutionLog::with_kv_backend(std::rc::Rc::clone(backend));
            
            #[cfg(not(feature = "native-v2"))]
            let execution_log = ExecutionLog::new(&db_path);

            let args = vec!["scan".to_string()];
            execution_log.start_execution("scan", "2.1.0", &args, None, "/test.db").unwrap();
            execution_log.finish_execution("scan", "success", None, 2, 5, 10).unwrap();

            // Store metrics
            #[cfg(feature = "native-v2")]
            let metrics = MetricsOps::with_kv_backend(std::rc::Rc::clone(backend));
            
            #[cfg(not(feature = "native-v2"))]
            let metrics = MetricsOps::new(&db_path);

            let file_metrics = FileMetrics {
                file_path: "test.rs".to_string(),
                symbol_count: 2,
                loc: 50,
                estimated_loc: 50.0,
                fan_in: 0,
                fan_out: 1,
                complexity_score: 1.0,
                last_updated: 1000,
            };
            metrics.upsert_file_metrics(&file_metrics).unwrap();
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_chunk_operations,
    benchmark_execution_log_ops,
    benchmark_metrics_ops,
    benchmark_combined_metadata
);
criterion_main!(benches);

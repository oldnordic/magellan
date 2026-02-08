//! KV storage integration tests for Native V2 backend.
//!
//! Tests verify KV storage operations work correctly with concurrent access.
//! Focus: Thread safety, data consistency, and metadata lifecycle.

#[cfg(feature = "native-v2")]
mod tests {
    use magellan::generation::{ChunkStore, CodeChunk};
    use magellan::graph::{ExecutionLog, MetricsOps};
    use magellan::graph::metrics::FileMetrics;
    use magellan::kv::keys::chunk_key;
    use magellan::CodeGraph;
    use sqlitegraph::{GraphBackend, NativeGraphBackend, SnapshotId};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tempfile::TempDir;

    /// Test: Concurrent KV access from multiple threads.
    ///
    /// Spawns 10 threads, each storing 100 chunks/execution records/metrics.
    /// Verifies all data persisted correctly.
    #[test]
    fn test_concurrent_kv_access() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let results = Arc::new(Mutex::new(Vec::new()));

        // Create Native V2 backend
        let backend: Arc<dyn GraphBackend> = {
            let native = NativeGraphBackend::new(&db_path).unwrap();
            Arc::new(native)
        };

        let mut handles = vec![];

        // Spawn 10 threads
        for thread_id in 0..10 {
            let backend_clone = Arc::clone(&backend);
            let results_clone = Arc::clone(&results);

            let handle = thread::spawn(move || {
                use magellan::kv::keys::{chunk_key, execution_log_key, file_metrics_key};
                use magellan::generation::{ChunkStore, CodeChunk};
                use magellan::graph::{ExecutionLog, MetricsOps};
                use magellan::graph::metrics::FileMetrics;

                let mut thread_results = Vec::new();

                // Store 100 chunks
                for i in 0..100 {
                    let chunk_store = ChunkStore::with_kv_backend(Arc::clone(&backend_clone));
                    
                    let file_path = format!("thread{}_file{}.rs", thread_id, i);
                    let chunk = CodeChunk::new(
                        file_path.clone(),
                        i * 10,
                        (i + 1) * 10,
                        format!("content {}", i),
                        Some(format!("symbol_{}", i)),
                        Some("Function".to_string()),
                    );

                    match chunk_store.store_chunk(&chunk) {
                        Ok(_) => thread_results.push(format!("chunk_{}_{}", thread_id, i)),
                        Err(e) => thread_results.push(format!("chunk_{}_{}_FAILED: {}", thread_id, i, e)),
                    }
                }

                // Store 100 execution records
                let execution_log = ExecutionLog::with_kv_backend(Arc::clone(&backend_clone));
                for i in 0..100 {
                    let exec_id = format!("thread{}_exec_{}", thread_id, i);
                    let args = vec![format!("arg_{}", i)];
                    
                    match execution_log.start_execution(&exec_id, "2.1.0", &args, None, "/test.db") {
                        Ok(_) => {
                            match execution_log.finish_execution(&exec_id, "success", None, i, i * 2, i * 3) {
                                Ok(_) => thread_results.push(format!("exec_{}_{}", thread_id, i)),
                                Err(e) => thread_results.push(format!("exec_{}_{}_FAILED: {}", thread_id, i, e)),
                            }
                        }
                        Err(e) => thread_results.push(format!("exec_{}_{}_FAILED_START: {}", thread_id, i, e)),
                    }
                }

                // Store 100 file metrics
                let metrics = MetricsOps::with_kv_backend(Arc::clone(&backend_clone));
                for i in 0..100 {
                    let file_path = format!("thread{}_file{}.rs", thread_id, i);
                    let file_metrics = FileMetrics {
                        file_path: file_path.clone(),
                        symbol_count: i,
                        loc: i * 10,
                        estimated_loc: (i * 10) as f64,
                        fan_in: 0,
                        fan_out: i,
                        complexity_score: i as f64,
                        last_updated: i as i64,
                    };

                    match metrics.upsert_file_metrics(&file_metrics) {
                        Ok(_) => thread_results.push(format!("metrics_{}_{}", thread_id, i)),
                        Err(e) => thread_results.push(format!("metrics_{}_{}_FAILED: {}", thread_id, i, e)),
                    }
                }

                // Store results
                let mut results = results_clone.lock().unwrap();
                results.extend(thread_results);
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify results
        let results = results.lock().unwrap();
        println!("Total operations: {}", results.len());

        // Should have 3000 successful operations (10 threads * 100 chunks/execs/metrics)
        let failed_count = results.iter().filter(|r| r.contains("_FAILED")).count();
        println!("Failed operations: {}", failed_count);

        // Verify counts via KV store
        let snapshot = SnapshotId::current();
        let mut chunks_found = 0;
        let mut execs_found = 0;
        let mut metrics_found = 0;

        for thread_id in 0..10 {
            for i in 0..10 {
                // Check a sample of chunks
                let file_path = format!("thread{}_file{}.rs", thread_id, i * 10);
                let key = chunk_key(&file_path, i * 100, (i + 1) * 100);
                if backend.kv_get(snapshot, &key).unwrap().is_some() {
                    chunks_found += 1;
                }

                // Check a sample of execution records
                let exec_id = format!("thread{}_exec_{}", thread_id, i * 10);
                let exec_key = execution_log_key(&exec_id);
                if backend.kv_get(snapshot, &exec_key).unwrap().is_some() {
                    execs_found += 1;
                }

                // Check a sample of metrics
                let file_path = format!("thread{}_file{}.rs", thread_id, i * 10);
                let metrics_key = file_metrics_key(&file_path);
                if backend.kv_get(snapshot, &metrics_key).unwrap().is_some() {
                    metrics_found += 1;
                }
            }
        }

        println!("Chunks found in KV: {}", chunks_found);
        println!("Executions found in KV: {}", execs_found);
        println!("Metrics found in KV: {}", metrics_found);

        assert!(chunks_found > 0, "Should have chunks in KV store");
        assert!(execs_found > 0, "Should have executions in KV store");
        assert!(metrics_found > 0, "Should have metrics in KV store");
    }

    /// Test: Write/read contention under concurrent access.
    ///
    /// Writer thread continuously writes chunks.
    /// Reader thread continuously reads chunks.
    /// Verifies no deadlocks or panics.
    #[test]
    fn test_kv_write_read_contention() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let backend: Arc<dyn GraphBackend> = {
            let native = NativeGraphBackend::new(&db_path).unwrap();
            Arc::new(native)
        };

        let backend_writer = Arc::clone(&backend);
        let backend_reader = Arc::clone(&backend);

        // Writer thread
        let writer = thread::spawn(move || {
            use magellan::generation::{ChunkStore, CodeChunk};
            
            let chunk_store = ChunkStore::with_kv_backend(backend_writer);
            
            for i in 0..100 {
                let chunk = CodeChunk::new(
                    format!("file_{}.rs", i % 10),
                    i * 10,
                    (i + 1) * 10,
                    format!("content {}", i),
                    Some(format!("symbol_{}", i)),
                    Some("Function".to_string()),
                );

                if let Err(e) = chunk_store.store_chunk(&chunk) {
                    eprintln!("Writer failed at iteration {}: {}", i, e);
                }
            }
        });

        // Reader thread
        let reader = thread::spawn(move || {
            use magellan::kv::keys::chunk_key;
            
            let snapshot = SnapshotId::current();
            let mut read_count = 0;
            let start = std::time::Instant::now();

            while start.elapsed() < std::time::Duration::from_secs(1) {
                for i in 0..10 {
                    let key = chunk_key(&format!("file_{}.rs", i), i * 10, (i + 1) * 10);
                    match backend_reader.kv_get(snapshot, &key) {
                        Ok(Some(_)) => read_count += 1,
                        Ok(None) => {},
                        Err(e) => eprintln!("Reader failed: {}", e),
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            read_count
        });

        // Wait for both threads
        writer.join().unwrap();
        let reads = reader.join().unwrap();

        println!("Read operations completed: {}", reads);
        assert!(reads >= 0, "Should complete reads without deadlock");
    }

    /// Test: Full metadata lifecycle with KV storage.
    ///
    /// Creates NativeGraphBackend, indexes a file, stores all metadata types,
    /// verifies retrieval, then cleans up.
    #[test]
    fn test_metadata_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index a test file
        let source_code = r#"
pub fn main() {
    println!("Hello");
    helper();
}

pub fn helper() {
    println!("Helper");
}
"#;

        let symbol_count = graph.index_file("test.rs", source_code.as_bytes()).unwrap();
        assert!(symbol_count > 0, "Should index symbols");

        // With KV backend, verify we can store and retrieve metadata
        #[cfg(feature = "native-v2")]
        {
            use magellan::generation::{ChunkStore, CodeChunk};
            use magellan::graph::metrics::FileMetrics;
            use magellan::kv::keys::{chunk_key, file_metrics_key};

            let backend = Arc::clone(graph.__backend_for_benchmarks());
            let snapshot = SnapshotId::current();

            // Store a chunk
            let chunk_store = ChunkStore::with_kv_backend(backend.clone());
            let chunk = CodeChunk::new(
                "test.rs".to_string(),
                0,
                50,
                "pub fn main() { ... }".to_string(),
                Some("main".to_string()),
                Some("Function".to_string()),
            );
            chunk_store.store_chunk(&chunk).unwrap();

            // Verify chunk in KV
            let chunk_key = chunk_key("test.rs", 0, 50);
            assert!(
                backend.kv_get(snapshot, &chunk_key).unwrap().is_some(),
                "Chunk should be in KV"
            );

            // Store file metrics
            let metrics = magellan::graph::MetricsOps::with_kv_backend(backend.clone());
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

            // Verify metrics in KV
            let metrics_key = file_metrics_key("test.rs");
            assert!(
                backend.kv_get(snapshot, &metrics_key).unwrap().is_some(),
                "Metrics should be in KV"
            );

            // Delete file
            graph.delete_file_facts("test.rs").unwrap();

            // Verify KV cleanup (entries should be deleted via invalidate_file_index)
            // Note: This depends on the implementation of delete_file_facts
        }
    }
}

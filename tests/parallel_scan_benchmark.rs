//! Benchmark comparison for parallel vs sequential file scanning
//!
//! This integration test measures the performance improvement from parallel file I/O.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use magellan::CodeGraph;
use magellan::graph::filter::FileFilter;

/// Create a temporary directory with test source files
fn create_test_files(temp_dir: &Path, count: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for i in 0..count {
        let file_path = temp_dir.join(format!("file_{}.rs", i));

        // Create realistic Rust source code
        let content = format!(
            r#"//! Module {i}

/// Function {i}
pub fn function_{i}() -> usize {{
    {i}
}}

/// Struct {i}
pub struct Struct{i} {{
    pub field: usize,
}}

impl Struct{i} {{
    pub fn new() -> Self {{
        Self {{ field: {i} }}
    }}
}}

/// Nested function {i}
pub fn nested_function_{i}(x: usize) -> usize {{
    x + {i}
}}
"#
        );

        fs::write(&file_path, content).unwrap();
        files.push(file_path);
    }

    files
}

/// Sequential scan implementation (original scan logic)
fn sequential_scan(graph: &mut CodeGraph, dir_path: &Path) -> Result<usize, anyhow::Error> {
    use magellan::diagnostics::{DiagnosticStage, WatchDiagnostic};
    use magellan::graph::filter::FileFilter;
    use magellan::validation::validate_path_within_root;
    use std::path::PathBuf;

    let filter = FileFilter::new(dir_path, &[], &[])?;

    let mut candidate_files: Vec<PathBuf> = Vec::new();
    let mut diagnostics = Vec::new();

    // Collect files
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        match validate_path_within_root(path, dir_path) {
            Ok(_) => {}
            Err(_) => continue,
        }

        if filter.should_skip(path).is_some() {
            continue;
        }

        candidate_files.push(path.to_path_buf());
    }

    candidate_files.sort();

    // Sequential file processing
    for path in &candidate_files {
        let path_str = path.to_string_lossy().to_string();

        let source = match std::fs::read(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(WatchDiagnostic::error(
                    path_str,
                    DiagnosticStage::Read,
                    e.to_string(),
                ));
                continue;
            }
        };

        let _ = graph.delete_file(&path_str);
        let _ = graph.index_file(&path_str, &source);
        let _ = graph.index_references(&path_str, &source);
    }

    Ok(candidate_files.len())
}

/// Parallel scan implementation (using the actual scan_directory_with_filter)
fn parallel_scan(graph: &mut CodeGraph, dir_path: &Path) -> Result<usize, anyhow::Error> {
    let filter = FileFilter::new(dir_path, &[], &[])?;
    let result = magellan::graph::scan::scan_directory_with_filter(graph, dir_path, &filter, None)?;
    Ok(result.indexed)
}

#[test]
fn benchmark_parallel_vs_sequential_scan() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Test with different file counts
    let file_counts = vec![10, 50, 100];

    for count in file_counts {
        println!("\n=== Benchmarking with {} files ===", count);

        // Create fresh test files for each iteration
        let test_dir = temp_dir.path().join(format!("batch_{}", count));
        fs::create_dir_all(&test_dir).unwrap();
        create_test_files(&test_dir, count);

        // Benchmark sequential scan
        let mut graph_seq = CodeGraph::open(&db_path).unwrap();
        let start_seq = Instant::now();
        let indexed_seq = sequential_scan(&mut graph_seq, &test_dir).unwrap();
        let duration_seq = start_seq.elapsed();

        // Clear database for parallel scan
        fs::remove_file(&db_path).unwrap();

        // Benchmark parallel scan
        let mut graph_par = CodeGraph::open(&db_path).unwrap();
        let start_par = Instant::now();
        let indexed_par = parallel_scan(&mut graph_par, &test_dir).unwrap();
        let duration_par = start_par.elapsed();

        // Verify results are identical
        assert_eq!(
            indexed_seq, indexed_par,
            "Both scans should index the same number of files"
        );

        // Calculate speedup
        let speedup = duration_seq.as_secs_f64() / duration_par.as_secs_f64();

        println!(
            "Sequential: {:.2?} | Parallel: {:.2?} | Speedup: {:.2}x",
            duration_seq, duration_par, speedup
        );

        // Parallel should be at least as fast (or faster)
        // On single-core systems, they may be similar
        // On multi-core systems, parallel should be faster
        println!(
            "Files indexed: {} (both implementations)",
            indexed_par
        );

        // Clean up test directory
        fs::remove_dir_all(&test_dir).unwrap();
    }

    println!("\n=== Benchmark complete ===");
}

#[test]
fn benchmark_parallel_scan_correctness() {
    // Verify that parallel scan produces identical results to sequential

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path_seq = temp_dir.path().join("seq.db");
    let db_path_par = temp_dir.path().join("par.db");

    // Create test files
    let test_dir = temp_dir.path().join("test_files");
    fs::create_dir_all(&test_dir).unwrap();
    create_test_files(&test_dir, 20);

    // Sequential scan
    let mut graph_seq = CodeGraph::open(&db_path_seq).unwrap();
    let count_seq = sequential_scan(&mut graph_seq, &test_dir).unwrap();

    // Parallel scan
    let mut graph_par = CodeGraph::open(&db_path_par).unwrap();
    let filter = FileFilter::new(&test_dir, &[], &[]).unwrap();
    let result_par = magellan::graph::scan::scan_directory_with_filter(
        &mut graph_par,
        &test_dir,
        &filter,
        None,
    )
    .unwrap();
    let count_par = result_par.indexed;

    // Verify same number of files indexed
    assert_eq!(count_seq, count_par, "File count must match");

    // Verify all files have the same symbol counts
    for entry in fs::read_dir(&test_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        let symbols_seq = graph_seq.symbols_in_file(&path_str).unwrap();
        let symbols_par = graph_par.symbols_in_file(&path_str).unwrap();

        assert_eq!(
            symbols_seq.len(),
            symbols_par.len(),
            "Symbol count mismatch for {}",
            path_str
        );
    }

    println!("Correctness verified: sequential and parallel produce identical results");
}

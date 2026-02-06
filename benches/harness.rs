//! Benchmark harness for performance testing
//!
//! Provides setup functions for creating test graphs of varying sizes
//! and structures for benchmarking neighbor expansion, reachability,
//! and symbol lookup operations.

use magellan::CodeGraph;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary .rs file for indexing
///
/// # Arguments
/// * `content` - Rust source code content
///
/// # Returns
/// Tuple of (PathBuf, TempDir) where:
/// - PathBuf: Path to the temporary file
/// - TempDir: Temporary directory (keeps file alive)
pub fn make_test_file(content: &str) -> (PathBuf, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.rs");
    fs::write(&test_file, content).unwrap();
    (test_file, temp_dir)
}

/// Setup a small test graph with realistic call patterns
///
/// Creates a temporary database and indexes a small test dataset
/// with 10-20 functions in various call patterns (linear chains,
/// branching calls, fan-out patterns).
///
/// # Returns
/// Tuple of (CodeGraph, TempDir, Vec<i64>) where:
/// - CodeGraph: The indexed graph instance
/// - TempDir: Temporary directory (owns the database file)
/// - Vec<i64>: Symbol IDs for benchmarking
pub fn setup_test_graph() -> (CodeGraph, TempDir, Vec<i64>) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create test file with various call patterns
    let source_content = r#"
// Linear chain
fn function_1() { function_2(); }
fn function_2() { function_3(); }
fn function_3() { function_4(); }
fn function_4() { }

// Branching pattern
fn branch_root() {
    branch_a();
    branch_b();
    branch_c();
}
fn branch_a() { }
fn branch_b() { }
fn branch_c() { }

// Fan-out pattern
fn fan_out_source() {
    leaf_1();
    leaf_2();
    leaf_3();
    leaf_4();
    leaf_5();
}
fn leaf_1() { }
fn leaf_2() { }
fn leaf_3() { }
fn leaf_4() { }
fn leaf_5() { }

// Diamond pattern
fn diamond_top() {
    diamond_left();
    diamond_right();
}
fn diamond_left() { diamond_bottom(); }
fn diamond_right() { diamond_bottom(); }
fn diamond_bottom() { }
"#;

    let test_file = temp_dir.path().join("test.rs");
    fs::write(&test_file, source_content).unwrap();

    let path_str = test_file.to_string_lossy().to_string();
    let source = fs::read(&test_file).unwrap();

    // Index the file
    graph.index_file(&path_str, &source).unwrap();

    // Collect all symbol IDs for benchmarking using public API
    let symbol_nodes = graph.symbol_nodes_in_file(&path_str).unwrap();
    let symbol_ids: Vec<i64> = symbol_nodes.iter().map(|(id, _)| *id).collect();

    (graph, temp_dir, symbol_ids)
}

/// Setup a large graph with synthetic data for stress testing
///
/// Generates a synthetic graph with 1000+ nodes in realistic
/// call graph patterns (5-15 depth, 2-5 branching factor).
///
/// # Returns
/// Tuple of (CodeGraph, TempDir, Vec<i64>) where:
/// - CodeGraph: The indexed graph instance
/// - TempDir: Temporary directory (owns the database file)
/// - Vec<i64>: Entry point symbol IDs for traversal benchmarks
pub fn setup_large_graph() -> (CodeGraph, TempDir, Vec<i64>) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Generate synthetic graph with 1000+ nodes
    let source_content = generate_large_graph(1000);

    let test_file = temp_dir.path().join("large_test.rs");
    fs::write(&test_file, source_content).unwrap();

    let path_str = test_file.to_string_lossy().to_string();
    let source = fs::read(&test_file).unwrap();

    // Index the file
    graph.index_file(&path_str, &source).unwrap();

    // Collect entry point IDs (top-level functions) using public API
    let symbol_nodes = graph.symbol_nodes_in_file(&path_str).unwrap();
    let all_symbol_ids: Vec<i64> = symbol_nodes.iter().map(|(id, _)| *id).collect();

    // Select first 10 symbols as entry points for traversal
    let entry_points: Vec<i64> = all_symbol_ids.iter().take(10).copied().collect();

    (graph, temp_dir, entry_points)
}

/// Setup a high-fanout graph for neighbor expansion benchmarks
///
/// Creates nodes with many outgoing edges to test neighbor query
/// performance with high fan-out.
///
/// # Arguments
/// * `num_nodes` - Number of source nodes to create
/// * `fanout` - Number of outgoing edges per node
///
/// # Returns
/// Tuple of (CodeGraph, TempDir, Vec<i64>) where:
/// - CodeGraph: The indexed graph instance
/// - TempDir: Temporary directory (owns the database file)
/// - Vec<i64>: Source node IDs with high fan-out
pub fn setup_high_fanout_graph(num_nodes: usize, fanout: usize) -> (CodeGraph, TempDir, Vec<i64>) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Generate high-fanout graph
    let source_content = generate_fanout_graph(num_nodes, fanout);

    let test_file = temp_dir.path().join("fanout_test.rs");
    fs::write(&test_file, source_content).unwrap();

    let path_str = test_file.to_string_lossy().to_string();
    let source = fs::read(&test_file).unwrap();

    // Index the file
    graph.index_file(&path_str, &source).unwrap();

    // Collect source node IDs (the functions with high fan-out) using public API
    let symbol_nodes = graph.symbol_nodes_in_file(&path_str).unwrap();
    let all_symbol_ids: Vec<i64> = symbol_nodes.iter().map(|(id, _)| *id).collect();

    // Select first num_nodes symbols as the high-fanout nodes
    let source_nodes: Vec<i64> = all_symbol_ids.iter().take(num_nodes).copied().collect();

    (graph, temp_dir, source_nodes)
}

/// Generate a large synthetic graph
///
/// Creates function definitions with realistic call patterns.
/// Uses deterministic generation to ensure reproducibility.
fn generate_large_graph(num_functions: usize) -> String {
    let mut content = String::new();

    // Create hierarchical call chains
    let depth = 10;
    let branching = 3;

    for i in 0..num_functions {
        let function_name = format!("generated_fn_{}", i);

        // Determine calls based on position
        if i < branching {
            // Top-level functions call into depth chains
            content.push_str(&format!("fn {}() {{ ", function_name));
            for d in 1..=depth {
                content.push_str(&format!("depth_{}_{}(); ", i, d));
            }
            content.push_str("}\n");
        } else if i < branching * 10 {
            // Depth chain functions
            let chain_idx = i / branching;
            let depth_idx = i % branching;
            if depth_idx < depth {
                content.push_str(&format!("fn depth_{}_{}() {{ ", chain_idx, depth_idx));
                if depth_idx < depth - 1 {
                    content.push_str(&format!("depth_{}_{}(); ", chain_idx, depth_idx + 1));
                }
                content.push_str("}\n");
            }
        } else {
            // Leaf functions
            content.push_str(&format!("fn {}() {{ }}\n", function_name));
        }
    }

    content
}

/// Generate a high-fanout graph
///
/// Creates functions that call many other functions.
fn generate_fanout_graph(num_nodes: usize, fanout: usize) -> String {
    let mut content = String::new();

    // Create source nodes with high fan-out
    for i in 0..num_nodes {
        content.push_str(&format!("fn fanout_source_{}() {{ ", i));
        for j in 0..fanout {
            content.push_str(&format!("leaf_{}_{}(); ", i, j));
        }
        content.push_str("}\n");
    }

    // Create leaf nodes
    for i in 0..num_nodes {
        for j in 0..fanout {
            content.push_str(&format!("fn leaf_{}_{}() {{ }}\n", i, j));
        }
    }

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_test_graph() {
        let (_graph, _temp_dir, symbol_ids) = setup_test_graph();
        assert!(
            symbol_ids.len() >= 10,
            "Test graph should have at least 10 symbols"
        );
    }

    #[test]
    fn test_setup_large_graph() {
        let (_graph, _temp_dir, entry_points) = setup_large_graph();
        assert_eq!(
            entry_points.len(),
            10,
            "Large graph should have 10 entry points"
        );
    }

    #[test]
    fn test_setup_high_fanout_graph() {
        let (_graph, _temp_dir, source_nodes) = setup_high_fanout_graph(5, 100);
        assert_eq!(source_nodes.len(), 5, "Should have 5 source nodes");
    }

    #[test]
    fn test_make_test_file() {
        let content = "fn test() {}";
        let (path, _temp_dir) = make_test_file(content);
        assert!(path.exists(), "Test file should exist");
        let read_content = fs::read_to_string(&path).unwrap();
        assert_eq!(read_content, content, "Content should match");
    }
}

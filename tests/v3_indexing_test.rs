//! Test V3 backend indexing via CodeGraph

use std::path::Path;
use magellan::CodeGraph;

#[test]
fn test_v3_codegraph_scan() {
    let db_path = "/tmp/test_v3_codegraph.db";
    let _ = std::fs::remove_file(db_path);
    
    // Open/create the graph
    let mut graph = CodeGraph::open(db_path).unwrap();
    
    // Create a simple test file
    let test_dir = "/tmp/magellan_test_src";
    let _ = std::fs::remove_dir_all(test_dir);
    std::fs::create_dir_all(test_dir).unwrap();
    std::fs::write(format!("{}/test.rs", test_dir), "fn main() { println!(\"hello\"); }").unwrap();
    
    // Scan the directory
    let count = graph.scan_directory(Path::new(test_dir), None).unwrap();
    println!("Scanned {} files", count);
    
    // Check counts
    let files = graph.count_files().unwrap();
    let symbols = graph.count_symbols().unwrap();
    
    println!("Files: {}", files);
    println!("Symbols: {}", symbols);
    
    // List files
    let file_nodes = graph.all_file_nodes().unwrap();
    println!("File nodes count: {}", file_nodes.len());
    
    // Cleanup
    let _ = std::fs::remove_dir_all(test_dir);
    let _ = std::fs::remove_file(db_path);
    
    // Assertions
    assert!(files > 0, "Should have at least 1 file, got {}", files);
}

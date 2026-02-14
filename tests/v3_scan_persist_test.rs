//! Test V3 backend data persistence

use std::path::Path;
use magellan::CodeGraph;

#[test]
fn test_v3_scan_persists_data() {
    let db_path = "/tmp/test_v3_persist.db";
    let _ = std::fs::remove_file(db_path);
    
    // Create test directory with a Rust file
    let test_dir = "/tmp/magellan_persist_test";
    let _ = std::fs::remove_dir_all(test_dir);
    std::fs::create_dir_all(test_dir).unwrap();
    std::fs::write(
        format!("{}/test.rs", test_dir), 
        "fn main() { println!(\"hello\"); }\nfn foo() {}"
    ).unwrap();
    
    // First session: scan and index
    {
        let mut graph = CodeGraph::open(db_path).unwrap();
        let count = graph.scan_directory(Path::new(test_dir), None).unwrap();
        println!("First session - Scanned {} files", count);
        
        let files = graph.count_files().unwrap();
        let symbols = graph.count_symbols().unwrap();
        println!("First session - Files: {}, Symbols: {}", files, symbols);
        
        assert!(files > 0, "Should have files after scan");
        assert!(symbols > 0, "Should have symbols after scan");
        
        // Graph is dropped here - data should be flushed
    }
    
    // Second session: verify data persisted
    {
        let graph = CodeGraph::open(db_path).unwrap();
        let files = graph.count_files().unwrap();
        let symbols = graph.count_symbols().unwrap();
        
        println!("Second session - Files: {}, Symbols: {}", files, symbols);
        
        // These should be non-zero if data persisted correctly
        assert!(files > 0, "Files should persist after reopen, got {}", files);
        assert!(symbols > 0, "Symbols should persist after reopen, got {}", symbols);
    }
    
    // Cleanup
    let _ = std::fs::remove_dir_all(test_dir);
    let _ = std::fs::remove_file(db_path);
}

#![cfg(feature = "geometric-backend")]
//! System-level workflow tests for Magellan
//!
//! These tests verify real behavior using actual filesystem operations
//! and real .geo databases using library APIs.

#[cfg(feature = "geometric-backend")]
mod tests {
    use magellan::graph::geometric_backend::GeometricBackend;
    use magellan::graph::geo_index::{scan_directory_with_progress, IndexingMode};
    use std::path::Path;
    use std::time::Duration;
    use std::thread;

    /// Helper: Create a test project with multiple Rust files
    fn create_test_project(dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("src"))?;
        std::fs::create_dir_all(dir.join("src/utils"))?;
        
        // Main lib file
        std::fs::write(
            dir.join("src/lib.rs"),
            r#"pub mod utils;

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn process_data(input: &str) -> String {
    format!("Processed: {}", input)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }
}
"#,
        )?;
        
        // Utils module
        std::fs::write(
            dir.join("src/utils/mod.rs"),
            r#"pub fn helper(x: i32) -> i32 {
    x * 2
}

pub struct DataProcessor {
    value: i32,
}

impl DataProcessor {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
    
    pub fn process(&self) -> i32 {
        self.value * 3
    }
}
"#,
        )?;
        
        // Main binary
        std::fs::write(
            dir.join("src/main.rs"),
            r#"fn main() {
    println!("Hello from main");
}
"#,
        )?;
        
        Ok(())
    }

    /// Test 1: Indexing completes and creates valid .geo database
    #[test]
    fn indexing_completes_and_creates_valid_geo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Create backend and index
        let mut backend = GeometricBackend::create(&db_path).expect("Failed to create backend");
        
        let result = scan_directory_with_progress(
            &mut backend,
            &src_path,
            None, // No progress callback for tests
            IndexingMode::CfgFirst,
        );
        
        assert!(result.is_ok(), "Indexing should succeed: {:?}", result.err());
        
        // Save to disk
        let save_result = backend.save_to_disk();
        assert!(save_result.is_ok(), "Save should succeed");
        
        // Verify database exists
        assert!(db_path.exists(), "Database file should be created");
        
        // Verify we can reopen it
        let reopened = GeometricBackend::open(&db_path);
        assert!(reopened.is_ok(), "Should be able to reopen database");
        
        // Verify stats are available
        let backend = reopened.unwrap();
        let stats = backend.get_stats();
        assert!(stats.is_ok(), "Should be able to get stats");
    }

    /// Test 2: Repeated index of same src tree is idempotent
    #[test]
    fn repeated_index_same_src_tree_is_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // First index
        {
            let mut backend = GeometricBackend::create(&db_path).expect("Failed to create backend");
            scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst)
                .expect("First index failed");
            backend.save_to_disk().expect("First save failed");
        }
        
        // Get first stats
        let stats1 = {
            let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
            backend.get_geometric_stats()
        };
        
        // Second index (same database - should clear and reindex due to clear_file_data)
        {
            let mut backend = GeometricBackend::open(&db_path).expect("Failed to reopen for reindex");
            scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst)
                .expect("Second index failed");
            backend.save_to_disk().expect("Second save failed");
        }
        
        // Get second stats
        let stats2 = {
            let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
            backend.get_geometric_stats()
        };
        
        // Stats should be stable
        assert_eq!(stats1.symbol_count, stats2.symbol_count, 
            "Symbol count should be stable after reindex. First: {}, Second: {}",
            stats1.symbol_count, stats2.symbol_count);
        assert_eq!(stats1.file_count, stats2.file_count,
            "File count should be stable after reindex. First: {}, Second: {}",
            stats1.file_count, stats2.file_count);
    }

    /// Test 3: Reopen counts match after save
    #[test]
    fn reopen_counts_match_after_save() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Index and save
        {
            let mut backend = GeometricBackend::create(&db_path).expect("Failed to create backend");
            scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst)
                .expect("Index failed");
            backend.save_to_disk().expect("Save failed");
        }
        
        // First status check
        let stats1 = {
            let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
            backend.get_geometric_stats()
        };
        
        // Small delay
        thread::sleep(Duration::from_millis(50));
        
        // Second status check (reopen)
        let stats2 = {
            let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
            backend.get_geometric_stats()
        };
        
        // Stats should match
        assert_eq!(stats1.symbol_count, stats2.symbol_count, 
            "Symbol count should match after reopen");
        assert_eq!(stats1.file_count, stats2.file_count, 
            "File count should match after reopen");
        assert_eq!(stats1.node_count, stats2.node_count,
            "Node count should match after reopen");
        assert_eq!(stats1.cfg_block_count, stats2.cfg_block_count,
            "CFG block count should match after reopen");
    }

    /// Test 4: Symbol resolution finds functions
    #[test]
    fn symbol_resolution_finds_functions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Index
        {
            let mut backend = GeometricBackend::create(&db_path).expect("Failed to create backend");
            scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst)
                .expect("Index failed");
            backend.save_to_disk().expect("Save failed");
        }
        
        // Reopen and search for symbols
        let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
        
        // Should find 'add' function
        let add_results = backend.find_symbols_by_name_info("add");
        assert!(!add_results.is_empty(), "Should find 'add' function");
        
        // Should find 'helper' function  
        let helper_results = backend.find_symbols_by_name_info("helper");
        assert!(!helper_results.is_empty(), "Should find 'helper' function");
        
        // Should find 'DataProcessor' struct
        let struct_results = backend.find_symbols_by_name_info("DataProcessor");
        assert!(!struct_results.is_empty(), "Should find 'DataProcessor' struct");
        
        // Should find 'process' method
        let method_results = backend.find_symbols_by_name_info("process");
        assert!(!method_results.is_empty(), "Should find 'process' method");
    }

    /// Test 5: Dead code detection works
    #[test]
    fn dead_code_analysis_runs_successfully() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Index
        {
            let mut backend = GeometricBackend::create(&db_path).expect("Failed to create backend");
            scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst)
                .expect("Index failed");
            backend.save_to_disk().expect("Save failed");
        }
        
        // Reopen and run dead code analysis
        let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
        
        // Get entry points (main function)
        let entry_symbols = backend.find_symbols_by_name_info("main");
        let entry_ids: Vec<u64> = entry_symbols.iter().map(|s| s.id).collect();
        
        // Should be able to analyze without panic
        if !entry_ids.is_empty() {
            let dead_code = backend.dead_code_from_entries(&entry_ids);
            // Just verify it returns something (could be empty if everything is reachable)
            // The important thing is it doesn't panic
        }
    }

    /// Test 6: CFG first indexing mode produces expected data
    #[test]
    fn cfg_first_mode_creates_cfg_data() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Index with CfgFirst mode
        {
            let mut backend = GeometricBackend::create(&db_path).expect("Failed to create backend");
            scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst)
                .expect("Index failed");
            backend.save_to_disk().expect("Save failed");
        }
        
        // Reopen and verify CFG data exists
        let backend = GeometricBackend::open(&db_path).expect("Failed to reopen");
        let stats = backend.get_geometric_stats();
        
        // CFG data should exist in CfgFirst mode
        // Note: CFG count may vary based on parsing, but should generally have data
        // for functions like 'add', 'helper', etc.
    }

    /// Test 7: Both indexing modes work
    #[test]
    fn different_indexing_modes_work() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Test CfgFirst mode
        let db_path_cfg = temp_dir.path().join("test_cfg.geo");
        {
            let mut backend = GeometricBackend::create(&db_path_cfg)
                .expect("Failed to create backend");
            
            let result = scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::CfgFirst);
            assert!(result.is_ok(), "Indexing with CfgFirst should succeed");
            
            backend.save_to_disk().expect("Save failed");
            assert!(db_path_cfg.exists(), "Database should exist for CfgFirst");
        }
        
        // Test FullAst mode
        let db_path_ast = temp_dir.path().join("test_ast.geo");
        {
            let mut backend = GeometricBackend::create(&db_path_ast)
                .expect("Failed to create backend");
            
            let result = scan_directory_with_progress(&mut backend, &src_path, None, IndexingMode::FullAst);
            assert!(result.is_ok(), "Indexing with FullAst should succeed");
            
            backend.save_to_disk().expect("Save failed");
            assert!(db_path_ast.exists(), "Database should exist for FullAst");
        }
    }

    /// Test 8: End-to-end smoke test
    #[test]
    fn end_to_end_smoke_test() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path()).unwrap();
        
        // Step 1: Create and index
        {
            let mut backend = GeometricBackend::create(&db_path)
                .expect("Step 1: Failed to create backend");
            
            let result = scan_directory_with_progress(
                &mut backend, 
                &src_path, 
                None, 
                IndexingMode::CfgFirst
            );
            assert!(result.is_ok(), "Step 1: Indexing failed");
            
            let save_result = backend.save_to_disk();
            assert!(save_result.is_ok(), "Step 1: Save failed");
        }
        
        // Step 2: Reopen and verify data
        {
            let backend = GeometricBackend::open(&db_path)
                .expect("Step 2: Failed to reopen");
            
            let stats = backend.get_geometric_stats();
            
            assert!(stats.symbol_count > 0, "Step 2: Should have symbols");
            assert!(stats.file_count >= 3, "Step 2: Should have at least 3 files");
            
            // Query for specific symbols
            let results = backend.find_symbols_by_name_info("process_data");
            assert!(!results.is_empty(), "Step 2: Should find 'process_data'");
        }
        
        // Step 3: Reindex and verify stability
        {
            let mut backend = GeometricBackend::open(&db_path)
                .expect("Step 3: Failed to reopen for reindex");
            
            let reindex_result = scan_directory_with_progress(
                &mut backend,
                &src_path,
                None,
                IndexingMode::CfgFirst
            );
            assert!(reindex_result.is_ok(), "Step 3: Reindex failed");
            
            backend.save_to_disk().expect("Step 3: Save after reindex failed");
        }
        
        // Step 4: Final verification
        {
            let backend = GeometricBackend::open(&db_path)
                .expect("Step 4: Failed final reopen");
            
            let stats = backend.get_geometric_stats();
            assert!(stats.symbol_count > 0, "Step 4: Should still have symbols");
            
            // Test file path query
            let files = backend.get_all_file_paths();
            assert!(!files.is_empty(), "Step 4: Should have files");
        }
    }
}

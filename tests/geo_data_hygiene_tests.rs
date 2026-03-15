#![cfg(feature = "geometric-backend")]
//! Geometric Backend Data Hygiene Tests
//!
//! Regression tests for duplicate symbol prevention during re-indexing.
//! These tests verify that repeated indexing of the same source tree
//! does not create duplicate logical symbols.

#[cfg(feature = "geometric-backend")]
mod tests {
    use magellan::graph::geometric_backend::{GeometricBackend, InsertSymbol};
    use magellan::ingest::{Language, SymbolKind};
    use std::collections::HashSet;
    use std::path::Path;

    /// Helper: Create a temporary test database path
    fn temp_db_path() -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        format!("/tmp/geo_hygiene_test_{}.geo", timestamp)
    }

    /// Helper: Create a simple InsertSymbol
    fn make_symbol(
        name: &str,
        fqn: &str,
        file_path: &str,
        line: u64,
    ) -> InsertSymbol {
        InsertSymbol {
            name: name.to_string(),
            fqn: fqn.to_string(),
            kind: SymbolKind::Function,
            file_path: file_path.to_string(),
            byte_start: line * 100,
            byte_end: line * 100 + 50,
            start_line: line,
            start_col: 0,
            end_line: line + 1,
            end_col: 50,
            language: Language::Rust,
        }
    }

    /// Helper: Save and get stats (since get_stats reloads from disk)
    fn get_stats_after_save(backend: &GeometricBackend) -> magellan::graph::geometric_backend::GeometricBackendStats {
        backend.save_to_disk().expect("Should save");
        backend.get_geometric_stats()
    }

    /// Test 1: Re-indexing the same file does not duplicate its symbols
    #[test]
    fn geometric_reindex_same_file_does_not_duplicate_symbols() {
        let db_path = temp_db_path();
        let backend = GeometricBackend::create(Path::new(&db_path)).expect("Should create DB");

        let file_path = "/test/src/lib.rs";

        // First insertion
        let symbols = vec![
            make_symbol("foo", "test::foo", file_path, 10),
            make_symbol("bar", "test::bar", file_path, 20),
        ];
        let ids1 = backend.insert_symbols(symbols).expect("Should insert");
        assert_eq!(ids1.len(), 2, "Should assign 2 IDs");

        let stats_after_first = get_stats_after_save(&backend);
        assert_eq!(stats_after_first.symbol_count, 2, "Should have 2 symbols");

        // Re-index: clear and re-insert (simulating re-index)
        backend.clear_file_data(file_path).expect("Should clear file data");

        // Re-insert same symbols with new IDs (simulating fresh parse)
        let symbols2 = vec![
            make_symbol("foo", "test::foo", file_path, 10),
            make_symbol("bar", "test::bar", file_path, 20),
        ];
        let ids2 = backend.insert_symbols(symbols2).expect("Should re-insert");
        assert_eq!(ids2.len(), 2, "Should assign 2 new IDs");

        let stats_after_reindex = get_stats_after_save(&backend);
        assert_eq!(
            stats_after_reindex.symbol_count, 2,
            "Should still have exactly 2 symbols after re-index"
        );

        // Verify IDs are different (new symbols got new IDs)
        let ids1_set: HashSet<_> = ids1.iter().copied().collect();
        let ids2_set: HashSet<_> = ids2.iter().copied().collect();
        assert!(
            ids1_set.is_disjoint(&ids2_set),
            "New symbols should have different IDs"
        );

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    /// Test 2: Re-indexing same tree does not grow total symbol count
    #[test]
    fn geometric_reindex_same_tree_does_not_grow_symbol_count() {
        let db_path = temp_db_path();
        let backend = GeometricBackend::create(Path::new(&db_path)).expect("Should create DB");

        let files = vec![
            "/test/src/a.rs",
            "/test/src/b.rs",
            "/test/src/c.rs",
        ];

        // First index pass - 1 symbol per file
        for (idx, file) in files.iter().enumerate() {
            let symbols = vec![make_symbol(
                &format!("func_{}", idx),
                &format!("test::func_{}", idx),
                file,
                (idx + 1) as u64 * 10,
            )];
            backend.insert_symbols(symbols).expect("Should insert");
        }

        let stats_after_first = get_stats_after_save(&backend);
        assert_eq!(stats_after_first.symbol_count, 3, "Should have 3 symbols");

        // Simulate re-index of same tree
        for (idx, file) in files.iter().enumerate() {
            backend.clear_file_data(*file).expect("Should clear");
            let symbols = vec![make_symbol(
                &format!("func_{}", idx),
                &format!("test::func_{}", idx),
                file,
                (idx + 1) as u64 * 10,
            )];
            backend.insert_symbols(symbols).expect("Should re-insert");
        }

        let stats_after_reindex = get_stats_after_save(&backend);
        assert_eq!(
            stats_after_reindex.symbol_count, 3,
            "Symbol count should remain stable after re-index"
        );

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    /// Test 3: Canonical path handling prevents relative/absolute duplicates
    #[test]
    fn geometric_canonical_path_prevents_relative_absolute_duplicates() {
        let db_path = temp_db_path();
        let backend = GeometricBackend::create(Path::new(&db_path)).expect("Should create DB");

        // These should be treated as the same file due to path normalization
        // The normalization extracts the /src/ suffix from both paths
        let relative_path = "./src/main.rs";
        let absolute_path = "/home/user/project/src/main.rs";

        // Insert with relative path
        let symbols = vec![make_symbol("main", "test::main", relative_path, 1)];
        backend.insert_symbols(symbols).expect("Should insert");

        // Verify initial count via query (not stats which reloads from disk)
        let count_before = backend.find_symbols_by_name_info("main").len();
        assert_eq!(count_before, 1, "Should have 1 symbol");

        // Clear using absolute path (should find and clear the same file due to normalization)
        backend
            .clear_file_data(absolute_path)
            .expect("Should clear using absolute path");

        // Verify the symbol was cleared
        let count_after_clear = backend.find_symbols_by_name_info("main").len();
        assert_eq!(count_after_clear, 0, "Symbol should be cleared via normalized path matching");

        // Re-insert to verify we can still add symbols
        let symbols2 = vec![make_symbol("main", "test::main", relative_path, 1)];
        backend.insert_symbols(symbols2).expect("Should re-insert");

        let count_after_reinsert = backend.find_symbols_by_name_info("main").len();
        assert_eq!(
            count_after_reinsert, 1,
            "Path normalization should prevent relative/absolute duplicates"
        );

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    /// Test 4: Find returns unique symbol after repeated index
    #[test]
    fn geometric_find_unique_symbol_after_repeat_index() {
        let db_path = temp_db_path();
        let backend = GeometricBackend::create(Path::new(&db_path)).expect("Should create DB");

        let file_path = "/test/src/lib.rs";

        // Simulate multiple index passes
        for _pass in 0..5 {
            // Clear previous data for this file
            backend.clear_file_data(file_path).expect("Should clear");

            // Insert with new IDs each pass
            let symbols = vec![make_symbol(
                "unique_func",
                "test::unique_func",
                file_path,
                10,
            )];
            backend.insert_symbols(symbols).expect("Should insert");
        }

        // Should have exactly one symbol named "unique_func"
        let by_name = backend.find_symbols_by_name_info("unique_func");
        assert_eq!(
            by_name.len(), 1,
            "Should have exactly one symbol after 5 re-index passes, got {}",
            by_name.len()
        );

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    /// Test 5: Symbol metadata indices are correctly rebuilt after bulk removal
    #[test]
    fn geometric_rebuild_indices_after_bulk_removal() {
        let db_path = temp_db_path();
        let backend = GeometricBackend::create(Path::new(&db_path)).expect("Should create DB");

        let file_a = "/test/src/a.rs";
        let file_b = "/test/src/b.rs";

        // Insert symbols in two files with same name (legitimate duplicates)
        let symbols_a = vec![make_symbol("shared_name", "test::a::shared_name", file_a, 10)];
        let symbols_b = vec![make_symbol("shared_name", "test::b::shared_name", file_b, 20)];

        backend.insert_symbols(symbols_a).expect("Should insert");
        backend.insert_symbols(symbols_b).expect("Should insert");

        // Verify both exist (legitimate same-name, different-location symbols)
        let count_before = backend.find_symbols_by_name_info("shared_name").len();
        assert_eq!(
            count_before, 2,
            "Should have 2 symbols named 'shared_name' in different files"
        );

        // Clear only file_a
        backend.clear_file_data(file_a).expect("Should clear file_a");

        // Should have exactly one remaining
        let count_after = backend.find_symbols_by_name_info("shared_name").len();
        assert_eq!(
            count_after, 1,
            "Should have 1 symbol after clearing file_a"
        );

        // Verify the remaining symbol is from file_b
        let remaining = backend.find_symbols_by_name_info("shared_name");
        assert_eq!(remaining[0].file_path, file_b, "Remaining symbol should be from file_b");

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    /// Test 6: Multiple files can be cleared independently
    #[test]
    fn geometric_independent_file_clearing() {
        let db_path = temp_db_path();
        let backend = GeometricBackend::create(Path::new(&db_path)).expect("Should create DB");

        let files = vec![
            "/test/src/a.rs",
            "/test/src/b.rs",
            "/test/src/c.rs",
        ];

        // Insert symbols in all files
        for (idx, file) in files.iter().enumerate() {
            let symbols = vec![make_symbol(
                &format!("func_{}", idx),
                &format!("test::func_{}", idx),
                file,
                (idx + 1) as u64 * 10,
            )];
            backend.insert_symbols(symbols).expect("Should insert");
        }

        let stats_before = get_stats_after_save(&backend);
        assert_eq!(stats_before.symbol_count, 3, "Should have 3 symbols");

        // Clear middle file only
        backend.clear_file_data(files[1]).expect("Should clear file_b");

        let stats_after = get_stats_after_save(&backend);
        assert_eq!(stats_after.symbol_count, 2, "Should have 2 symbols after clearing file_b");

        // Verify correct file was cleared
        let remaining: Vec<_> = backend.find_symbols_by_name_info("func_0")
            .into_iter()
            .chain(backend.find_symbols_by_name_info("func_2"))
            .collect();
        assert_eq!(remaining.len(), 2, "Should have func_0 and func_2 remaining");

        let cleared = backend.find_symbols_by_name_info("func_1");
        assert!(cleared.is_empty(), "func_1 should be cleared");

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }
}

/// Empty test suite placeholder when geometric-backend feature is disabled
#[cfg(not(feature = "geometric-backend"))]
mod tests {
    #[test]
    fn geometric_backend_disabled() {
        // Placeholder test when feature is not enabled
    }
}

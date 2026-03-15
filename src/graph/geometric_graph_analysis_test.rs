//! Tests for geometric backend graph analysis commands
//!
//! These tests verify that graph-analysis commands route correctly
//! to the geometric backend and return expected results.

#[cfg(test)]
mod tests {
    use crate::backend_router::MagellanBackend;
    use std::path::Path;

    /// Test fixture path - must be created before running tests
    const TEST_DB_PATH: &str = "/tmp/verify.geo";

    #[test]
    fn geometric_refs_routes_correctly() {
        // Verifies that refs command routes to geometric backend
        // without the "not a sqlite database" error
        let db_path = Path::new(TEST_DB_PATH);
        if !db_path.exists() {
            eprintln!("Test DB not found at {}, skipping", TEST_DB_PATH);
            return;
        }

        let backend = MagellanBackend::open(db_path).expect("Should open .geo file");
        let stats = backend.get_stats().expect("Should get stats");
        assert!(stats.symbol_count > 0, "Should have symbols");

        // get_callers/get_callees should work without error
        let callers = backend.get_callers(1).expect("Should get callers");
        let callees = backend.get_callees(1).expect("Should get callees");

        // Results may be empty if call edges not extracted, but should not error
        let _: Vec<u64> = callers;
        let _: Vec<u64> = callees;
    }

    #[test]
    fn geometric_reachable_routes_correctly() {
        let db_path = Path::new(TEST_DB_PATH);
        if !db_path.exists() {
            eprintln!("Test DB not found, skipping");
            return;
        }

        let backend = MagellanBackend::open(db_path).expect("Should open .geo file");

        // Get first symbol
        let symbols: Vec<_> = (1..200)
            .filter_map(|id| backend.find_symbol_by_id(id))
            .collect();
        assert!(!symbols.is_empty(), "Should find symbols");

        // reachable_from should work without error
        let first_id = symbols[0].id;
        let reachable = backend.reachable_from(first_id);

        // Should at least contain the start symbol
        assert!(
            reachable.contains(&first_id),
            "Reachable should include start symbol"
        );
    }

    #[test]
    fn geometric_dead_code_routes_correctly() {
        let db_path = Path::new(TEST_DB_PATH);
        if !db_path.exists() {
            eprintln!("Test DB not found, skipping");
            return;
        }

        let backend = MagellanBackend::open(db_path).expect("Should open .geo file");

        let symbols: Vec<_> = (1..200)
            .filter_map(|id| backend.find_symbol_by_id(id))
            .collect();
        if !symbols.is_empty() {
            let first_id = symbols[0].id;
            let dead = backend.dead_code_from_entries(&[first_id]);

            // Result should be deterministic
            let dead2 = backend.dead_code_from_entries(&[first_id]);
            assert_eq!(dead, dead2, "Dead code results should be deterministic");
        }
    }

    #[test]
    fn geometric_cycles_detects_known_cycle_or_reports_none_truthfully() {
        let db_path = Path::new(TEST_DB_PATH);
        if !db_path.exists() {
            eprintln!("Test DB not found, skipping");
            return;
        }

        let backend = MagellanBackend::open(db_path).expect("Should open .geo file");

        // find_call_graph_cycles should work without error
        let cycles = backend.find_call_graph_cycles();

        // Result should be deterministic
        let cycles2 = backend.find_call_graph_cycles();
        assert_eq!(cycles, cycles2, "Cycle detection should be deterministic");

        // If no cycles, result should be empty vec (not error)
        // If cycles exist, they should be valid symbol ID groups
        for cycle in &cycles {
            assert!(!cycle.is_empty(), "Cycle should not be empty");
        }
    }

    #[test]
    fn geometric_slice_routes_correctly() {
        let db_path = Path::new(TEST_DB_PATH);
        if !db_path.exists() {
            eprintln!("Test DB not found, skipping");
            return;
        }

        let backend = MagellanBackend::open(db_path).expect("Should open .geo file");

        let symbols: Vec<_> = (1..200)
            .filter_map(|id| backend.find_symbol_by_id(id))
            .collect();
        if !symbols.is_empty() {
            let first_id = symbols[0].id;

            // Both slice directions should work
            let backward = backend.backward_slice(first_id);
            let forward = backend.forward_slice(first_id);

            // Should at least contain the target symbol
            assert!(
                backward.contains(&first_id),
                "Backward slice should include target"
            );
            assert!(
                forward.contains(&first_id),
                "Forward slice should include target"
            );
        }
    }

    #[test]
    fn geometric_graph_analysis_survives_reopen() {
        let db_path = Path::new(TEST_DB_PATH);
        if !db_path.exists() {
            eprintln!("Test DB not found, skipping");
            return;
        }

        // First open
        let backend1 = MagellanBackend::open(db_path).expect("Should open .geo file");
        let stats1 = backend1.get_stats().expect("Should get stats");
        let reachable1 = backend1.reachable_from(1);

        // Second open (simulates reopen)
        let backend2 = MagellanBackend::open(db_path).expect("Should reopen .geo file");
        let stats2 = backend2.get_stats().expect("Should get stats after reopen");
        let reachable2 = backend2.reachable_from(1);

        assert_eq!(
            stats1.symbol_count, stats2.symbol_count,
            "Stats should match after reopen"
        );

        // Sort both vectors before comparing (order is not guaranteed)
        let mut sorted1 = reachable1.clone();
        let mut sorted2 = reachable2.clone();
        sorted1.sort();
        sorted2.sort();
        assert_eq!(
            sorted1, sorted2,
            "Reachable results should match after reopen"
        );
    }
}

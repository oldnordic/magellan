//! Tests for backend router call graph operations

#[cfg(test)]
mod call_graph_tests {
    use crate::backend_router::MagellanBackend;
    use crate::graph::CodeGraph;
    use std::path::PathBuf;

    /// Helper function to create a test database with sample code
    fn setup_test_db_with_calls() -> (tempfile::TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a new CodeGraph database
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // File with simple function calls
        // Note: We use actual function calls that the parser can detect
        let source = r#"
fn caller_function() {
    callee_a();
    callee_b();
}

fn callee_a() {
    helper_func();
}

fn callee_b() {
    callee_a();
}

fn helper_func() {}

fn main() {
    caller_function();
    callee_a();
}
"#;

        // Index the file (this indexes both symbols and calls)
        let symbol_count = graph.index_file("src/test.rs", source.as_bytes()).unwrap();
        assert!(symbol_count > 0, "Expected some symbols to be indexed");

        (temp_dir, db_path)
    }

    #[test]
    fn test_calls_from_symbol_sqlite() {
        let (_temp_dir, db_path) = setup_test_db_with_calls();

        // Open the backend
        let mut backend = MagellanBackend::open(&db_path).unwrap();

        // Test calls_from_symbol - caller_function should call callee_a and callee_b
        let calls = backend.calls_from_symbol("src/test.rs", "caller_function").unwrap();

        // Should have 2 calls: callee_a() and callee_b()
        assert_eq!(calls.len(), 2, "Expected 2 calls from caller_function");

        // Check that we have the expected callees
        let callee_names: Vec<String> = calls.iter().map(|c| c.callee.clone()).collect();
        assert!(
            callee_names.contains(&"callee_a".to_string()),
            "Expected callee_a in calls"
        );
        assert!(
            callee_names.contains(&"callee_b".to_string()),
            "Expected callee_b in calls"
        );

        // Verify call details
        for call in &calls {
            assert_eq!(call.caller, "caller_function");
            assert_eq!(call.file_path.to_str().unwrap(), "src/test.rs");
            // Byte positions should be valid
            assert!(call.byte_start < call.byte_end);
            // Line numbers should be valid (1-indexed)
            assert!(call.start_line > 0);
        }
    }

    #[test]
    fn test_callers_of_symbol_sqlite() {
        let (_temp_dir, db_path) = setup_test_db_with_calls();

        // Open the backend
        let mut backend = MagellanBackend::open(&db_path).unwrap();

        // Test callers_of_symbol - callee_a should be called by caller_function, callee_b, and main
        let calls = backend.callers_of_symbol("src/test.rs", "callee_a").unwrap();

        // Should have 3 calls: from caller_function, callee_b, and main
        assert_eq!(calls.len(), 3, "Expected 3 calls to callee_a");

        // Check that we have the expected callers
        let caller_names: Vec<String> = calls.iter().map(|c| c.caller.clone()).collect();
        assert!(
            caller_names.contains(&"caller_function".to_string()),
            "Expected caller_function in callers"
        );
        assert!(
            caller_names.contains(&"callee_b".to_string()),
            "Expected callee_b in callers"
        );
        assert!(
            caller_names.contains(&"main".to_string()),
            "Expected main in callers"
        );

        // Verify call details
        for call in &calls {
            assert_eq!(call.callee, "callee_a");
            assert_eq!(call.file_path.to_str().unwrap(), "src/test.rs");
            // Byte positions should be valid
            assert!(call.byte_start < call.byte_end);
        }
    }

    #[test]
    fn test_calls_from_symbol_nonexistent() {
        let (_temp_dir, db_path) = setup_test_db_with_calls();

        let mut backend = MagellanBackend::open(&db_path).unwrap();

        // Test with non-existent symbol
        let calls = backend
            .calls_from_symbol("src/test.rs", "nonexistent_function")
            .unwrap();
        assert!(calls.is_empty(), "Expected empty calls for non-existent symbol");
    }

    #[test]
    fn test_callers_of_symbol_nonexistent() {
        let (_temp_dir, db_path) = setup_test_db_with_calls();

        let mut backend = MagellanBackend::open(&db_path).unwrap();

        // Test with non-existent symbol
        let calls = backend
            .callers_of_symbol("src/test.rs", "nonexistent_function")
            .unwrap();
        assert!(calls.is_empty(), "Expected empty callers for non-existent symbol");
    }

    #[test]
    fn test_calls_from_symbol_empty_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a new CodeGraph database
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // File with no calls
        let source = r#"
fn standalone_function() {
    let _x = 42;
}
"#;

        graph.index_file("src/standalone.rs", source.as_bytes()).unwrap();

        let mut backend = MagellanBackend::open(&db_path).unwrap();

        // Test calls_from_symbol - standalone_function has no calls
        let calls = backend
            .calls_from_symbol("src/standalone.rs", "standalone_function")
            .unwrap();
        assert!(calls.is_empty(), "Expected empty calls for standalone function");
    }

    #[test]
    fn test_callers_of_symbol_no_callers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a new CodeGraph database
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // File with uncalled function
        let source = r#"
fn uncalled_function() {
    let _x = 42;
}

fn main() {
    // main doesn't call uncalled_function
}
"#;

        graph.index_file("src/uncalled.rs", source.as_bytes()).unwrap();

        let mut backend = MagellanBackend::open(&db_path).unwrap();

        // Test callers_of_symbol - uncalled_function has no callers
        let calls = backend
            .callers_of_symbol("src/uncalled.rs", "uncalled_function")
            .unwrap();
        assert!(calls.is_empty(), "Expected empty callers for uncalled function");
    }
}

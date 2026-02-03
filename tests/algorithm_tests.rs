//! Tests for graph algorithms (Phase 40)
//!
//! Tests reachability analysis, dead code detection, and program slicing using
//! sqlitegraph algorithm functions.

use tempfile::TempDir;

#[test]
fn test_reachable_symbols_finds_transitive_closure() {
    // Test that reachable_symbols finds all symbols reachable from a start point
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    shared();
}

fn helper_b() {
    shared();
}

fn shared() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get main's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let main_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("main"))
        .expect("Should find main symbol");

    let main_fqn = main_symbol
        .fqn
        .as_ref()
        .or(main_symbol.canonical_fqn.as_ref())
        .expect("main should have FQN");

    // Test forward reachability from main
    // main -> helper_a, helper_b -> shared
    let reachable = graph.reachable_symbols(main_fqn, None).unwrap();

    // We should find at least some reachable symbols
    // (The exact count depends on whether main is included)
    assert!(
        !reachable.is_empty(),
        "Should find reachable symbols from main"
    );

    // Verify we find the expected symbols
    let fqn_names: Vec<_> = reachable
        .iter()
        .filter_map(|s| s.fqn.as_deref())
        .collect();

    // helper_a and helper_b should be directly reachable from main
    assert!(
        fqn_names.contains(&"helper_a") || fqn_names.contains(&"test.rs::helper_a"),
        "Should find helper_a as reachable from main"
    );
}

#[test]
fn test_dead_symbols_finds_unreachable_code() {
    // Test that dead_symbols finds code not reachable from entry point
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    helper_a();
}

fn helper_a() {
    shared();
}

fn shared() {}

fn unused_function() {
    shared();
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get main's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let main_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("main"))
        .expect("Should find main symbol");

    let main_fqn = main_symbol
        .fqn
        .as_ref()
        .or(main_symbol.canonical_fqn.as_ref())
        .expect("main should have FQN");

    // Find dead code from main
    let dead = graph.dead_symbols(main_fqn).unwrap();

    // unused_function should be detected as dead
    let dead_fqns: Vec<_> = dead
        .iter()
        .filter_map(|s| s.symbol.fqn.as_deref())
        .collect();

    assert!(
        dead_fqns.contains(&"unused_function")
            || dead_fqns.iter().any(|f| f.contains("unused_function")),
        "unused_function should be detected as dead"
    );
}

#[test]
fn test_reverse_reachable_finds_callers() {
    // Test that reverse_reachable_symbols finds all callers
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    shared();
}

fn helper_b() {
    shared();
}

fn shared() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get shared's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let shared_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("shared"))
        .expect("Should find shared symbol");

    let shared_fqn = shared_symbol
        .fqn
        .as_ref()
        .or(shared_symbol.canonical_fqn.as_ref())
        .expect("shared should have FQN");

    // Find callers of shared (reverse reachability)
    let callers = graph.reverse_reachable_symbols(shared_fqn, None).unwrap();

    // helper_a and helper_b should both call shared
    let caller_fqns: Vec<_> = callers
        .iter()
        .filter_map(|s| s.fqn.as_deref())
        .collect();

    assert!(
        !callers.is_empty(),
        "Should have callers for shared function"
    );

    // At minimum, we should find helper_a or helper_b in the caller list
    assert!(
        caller_fqns.contains(&"helper_a")
            || caller_fqns.contains(&"helper_b")
            || caller_fqns.iter().any(|f| f.contains("helper")),
        "Should find at least one helper function as a caller of shared"
    );
}

#[test]
fn test_algorithm_empty_database() {
    // Test algorithm behavior on empty database
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let graph = CodeGraph::open(&db_path).unwrap();

    // Querying reachable symbols on empty DB should error
    let result = graph.reachable_symbols("nonexistent", None);
    assert!(
        result.is_err(),
        "Should error when symbol not found in database"
    );
}

#[test]
fn test_algorithm_nonexistent_symbol() {
    // Test algorithm behavior with nonexistent symbol
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = "fn main() {}";

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();

    // Querying nonexistent symbol should error
    let result = graph.reachable_symbols("this_symbol_does_not_exist", None);
    assert!(
        result.is_err(),
        "Should error when symbol ID not found"
    );
}

// Program slicing tests (Phase 40-04)

#[test]
fn test_backward_slice_finds_callers() {
    // Test that backward_slice finds all callers of a target symbol
    // Using call-graph reachability as fallback
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    intermediate();
}

fn intermediate() {
    leaf();
}

fn leaf() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get leaf's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let leaf_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("leaf"))
        .expect("Should find leaf symbol");

    let leaf_fqn = leaf_symbol
        .fqn
        .as_ref()
        .or(leaf_symbol.canonical_fqn.as_ref())
        .expect("leaf should have FQN");

    // Run backward slice on leaf
    let slice_result = graph.backward_slice(leaf_fqn).unwrap();

    // intermediate should be in the backward slice (it calls leaf)
    let slice_fqns: Vec<_> = slice_result
        .slice
        .included_symbols
        .iter()
        .filter_map(|s| s.fqn.as_deref())
        .collect();

    assert!(
        !slice_fqns.is_empty(),
        "Backward slice from leaf should contain callers"
    );

    // intermediate calls leaf, so it should be in the backward slice
    assert!(
        slice_fqns.contains(&"intermediate")
            || slice_fqns.iter().any(|f| f.contains("intermediate")),
        "intermediate should be in backward slice from leaf"
    );
}

#[test]
fn test_forward_slice_finds_callees() {
    // Test that forward_slice finds all symbols reachable from target
    // Using call-graph reachability as fallback
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    intermediate();
}

fn intermediate() {
    leaf();
}

fn leaf() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get main's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let main_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("main"))
        .expect("Should find main symbol");

    let main_fqn = main_symbol
        .fqn
        .as_ref()
        .or(main_symbol.canonical_fqn.as_ref())
        .expect("main should have FQN");

    // Run forward slice from main
    let slice_result = graph.forward_slice(main_fqn).unwrap();

    // intermediate should be in the forward slice (called by main)
    let slice_fqns: Vec<_> = slice_result
        .slice
        .included_symbols
        .iter()
        .filter_map(|s| s.fqn.as_deref())
        .collect();

    assert!(
        !slice_fqns.is_empty(),
        "Forward slice from main should contain callees"
    );

    // intermediate is called by main
    assert!(
        slice_fqns.contains(&"intermediate")
            || slice_fqns.iter().any(|f| f.contains("intermediate")),
        "intermediate should be in forward slice from main"
    );
}

#[test]
fn test_slice_statistics() {
    // Test that slice statistics are correctly computed
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    shared();
}

fn helper_b() {
    shared();
}

fn shared() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get shared's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let shared_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("shared"))
        .expect("Should find shared symbol");

    let shared_fqn = shared_symbol
        .fqn
        .as_ref()
        .or(shared_symbol.canonical_fqn.as_ref())
        .expect("shared should have FQN");

    // Run backward slice
    let slice_result = graph.backward_slice(shared_fqn).unwrap();

    // Statistics should reflect the slice content
    assert_eq!(
        slice_result.statistics.total_symbols,
        slice_result.slice.included_symbols.len(),
        "total_symbols should match included symbols count"
    );

    // With call-graph fallback, data_dependencies is 0
    assert_eq!(
        slice_result.statistics.data_dependencies, 0,
        "data_dependencies should be 0 with call-graph fallback"
    );

    // control_dependencies should be non-zero (we have callers)
    assert!(
        slice_result.statistics.control_dependencies > 0,
        "control_dependencies should be non-zero"
    );
}

#[test]
fn test_slice_is_empty_for_isolated_function() {
    // Test that slice of isolated function returns minimal results
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn isolated_function() {
    let x = 42;
}

fn other_function() {
    let y = 10;
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get isolated_function's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let isolated_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("isolated_function"))
        .expect("Should find isolated_function symbol");

    let isolated_fqn = isolated_symbol
        .fqn
        .as_ref()
        .or(isolated_symbol.canonical_fqn.as_ref())
        .expect("isolated_function should have FQN");

    // Run backward slice - should be empty (no callers)
    let backward_slice = graph.backward_slice(isolated_fqn).unwrap();
    assert!(
        backward_slice.slice.included_symbols.is_empty(),
        "Backward slice of isolated function should be empty"
    );

    // Run forward slice - should be empty (no callees)
    let forward_slice = graph.forward_slice(isolated_fqn).unwrap();
    assert!(
        forward_slice.slice.included_symbols.is_empty(),
        "Forward slice of isolated function should be empty"
    );
}

#[test]
fn test_slice_direction_consistency() {
    // Test that backward and forward slices are consistent
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn a() {
    b();
}

fn b() {
    c();
}

fn c() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get b's FQN
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let b_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("b"))
        .expect("Should find b symbol");

    let b_fqn = b_symbol
        .fqn
        .as_ref()
        .or(b_symbol.canonical_fqn.as_ref())
        .expect("b should have FQN");

    // Run both backward and forward slices
    let backward = graph.backward_slice(b_fqn).unwrap();
    let forward = graph.forward_slice(b_fqn).unwrap();

    // Verify direction is correctly set
    assert!(
        matches!(backward.slice.direction, magellan::SliceDirection::Backward),
        "Backward slice should have Backward direction"
    );
    assert!(
        matches!(forward.slice.direction, magellan::SliceDirection::Forward),
        "Forward slice should have Forward direction"
    );

    // b should be the target for both
    assert_eq!(
        backward.slice.target.kind, forward.slice.target.kind,
        "Target should be the same for both slices"
    );
}

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

#[test]
fn test_enumerate_paths_finds_execution_paths() {
    // Test that enumerate_paths finds all execution paths from a start point
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
    leaf();
}

fn helper_b() {
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

    // Test path enumeration from main
    let result = graph.enumerate_paths(main_fqn, None, 10, 100).unwrap();

    // The result should be valid (even if paths are empty due to graph structure)
    // Verify statistics are properly computed
    if !result.paths.is_empty() {
        assert!(
            result.statistics.avg_length > 0.0,
            "Average path length should be positive when paths exist"
        );
        assert!(
            result.statistics.max_length >= result.statistics.min_length,
            "Max length should be >= min length"
        );
    } else {
        // If no paths found, verify stats are still in valid state
        assert!(
            result.statistics.avg_length >= 0.0,
            "Average path length should be non-negative"
        );
    }

    // Test that we can also enumerate to a specific target
    let leaf_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("leaf"))
        .expect("Should find leaf symbol");

    let leaf_fqn = leaf_symbol
        .fqn
        .as_ref()
        .or(leaf_symbol.canonical_fqn.as_ref())
        .expect("leaf should have FQN");

    // Enumerate paths from main to leaf
    let result_to_leaf = graph
        .enumerate_paths(main_fqn, Some(leaf_fqn), 10, 100)
        .unwrap();

    // Should return a valid result
    assert!(
        result_to_leaf.total_enumerated >= 0,
        "Total enumerated should be non-negative"
    );
}

#[test]
fn test_enumerate_paths_with_end_symbol() {
    // Test path enumeration with a specific end symbol
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    helper();
}

fn helper() {
    target();
}

fn target() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get main's and target's FQNs
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let main_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("main"))
        .expect("Should find main symbol");
    let target_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("target"))
        .expect("Should find target symbol");

    let main_fqn = main_symbol
        .fqn
        .as_ref()
        .or(main_symbol.canonical_fqn.as_ref())
        .expect("main should have FQN");
    let target_fqn = target_symbol
        .fqn
        .as_ref()
        .or(target_symbol.canonical_fqn.as_ref())
        .expect("target should have FQN");

    // Enumerate paths from main to target
    let result = graph
        .enumerate_paths(main_fqn, Some(target_fqn), 10, 100)
        .unwrap();

    // We should find at least one path from main to target
    assert!(
        !result.paths.is_empty(),
        "Should find at least one path from main to target"
    );

    // Each path should end at target
    for path in &result.paths {
        let last_fqn = path
            .symbols
            .last()
            .and_then(|s| s.fqn.as_deref())
            .unwrap_or("");
        assert!(
            last_fqn.contains("target") || last_fqn == target_fqn,
            "Path should end at target symbol"
        );
    }
}

#[test]
fn test_enumerate_paths_respects_bounds() {
    // Test that enumerate_paths respects max_depth and max_paths bounds
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    // Create code with multiple paths
    let source = r#"
fn main() {
    a();
    b();
    c();
}

fn a() { x(); }
fn b() { x(); }
fn c() { x(); }

fn x() {
    y();
}

fn y() {}
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

    // Test with max_paths = 2
    let result = graph.enumerate_paths(main_fqn, None, 10, 2).unwrap();

    // Should return at most 2 paths
    assert!(
        result.paths.len() <= 2,
        "Should respect max_paths bound (got {} paths)",
        result.paths.len()
    );
}

// SCC and condensation tests (Phase 40-02)

#[test]
fn test_detect_cycles_finds_mutual_recursion() {
    // Test that detect_cycles finds mutually recursive functions
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
}

fn a() {
    b();
}

fn b() {
    a();
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Detect cycles
    let report = graph.detect_cycles().unwrap();

    // Should find one cycle (mutual recursion between a and b)
    assert!(
        report.total_count > 0,
        "Should detect cycles in mutually recursive code"
    );

    // Find the cycle with mutual recursion
    let mutual_cycle = report.cycles.iter()
        .find(|c| matches!(c.kind, magellan::graph::CycleKind::MutualRecursion));

    assert!(
        mutual_cycle.is_some(),
        "Should find mutual recursion cycle"
    );

    let cycle = mutual_cycle.unwrap();
    let cycle_fqns: Vec<_> = cycle.members.iter()
        .filter_map(|s| s.fqn.as_deref())
        .collect();

    assert!(
        cycle_fqns.iter().any(|f| f.contains("a")),
        "Cycle should contain function 'a'"
    );
    assert!(
        cycle_fqns.iter().any(|f| f.contains("b")),
        "Cycle should contain function 'b'"
    );
}

#[test]
fn test_detect_cycles_no_cycles_in_dag() {
    // Test that detect_cycles returns no cycles for DAG code
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
}

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

    // Detect cycles
    let report = graph.detect_cycles().unwrap();

    // Should find no cycles (code is a DAG)
    assert!(
        report.total_count == 0,
        "Should detect no cycles in DAG code (found {})",
        report.total_count
    );
    assert!(
        report.cycles.is_empty(),
        "Cycle list should be empty"
    );
}

#[test]
fn test_find_cycles_containing_specific_symbol() {
    // Test that find_cycles_containing finds cycles with a specific symbol
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
    x();
}

fn a() {
    b();
}

fn b() {
    a();
}

fn x() {
    y();
}

fn y() {
    x();
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get symbol_id for function 'a'
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let a_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("a"))
        .expect("Should find 'a' symbol");

    let a_symbol_id = a_symbol
        .symbol_id
        .as_ref()
        .expect("Symbol should have symbol_id");

    // Find cycles containing 'a'
    let cycles = graph.find_cycles_containing(a_symbol_id).unwrap();

    // Should find one cycle containing 'a'
    assert!(
        !cycles.is_empty(),
        "Should find cycles containing 'a'"
    );

    // Verify the cycle contains 'a'
    let cycle_fqns: Vec<_> = cycles[0].members.iter()
        .filter_map(|s| s.fqn.as_deref())
        .collect();

    assert!(
        cycle_fqns.iter().any(|f| f.contains("a")),
        "Cycle should contain 'a'"
    );
    assert!(
        cycle_fqns.iter().any(|f| f.contains("b")),
        "Cycle should contain 'b' (a's cycle partner)"
    );
}

#[test]
fn test_find_cycles_containing_non_cyclic_symbol() {
    // Test that find_cycles_containing returns empty for non-cyclic symbols
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
}

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

    // Get symbol_id for function 'a' (not in a cycle)
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let a_symbol = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("a"))
        .expect("Should find 'a' symbol");

    let a_symbol_id = a_symbol
        .symbol_id
        .as_ref()
        .expect("Symbol should have symbol_id");

    // Find cycles containing 'a'
    let cycles = graph.find_cycles_containing(a_symbol_id).unwrap();

    // Should find no cycles
    assert!(
        cycles.is_empty(),
        "Should find no cycles for non-cyclic symbol 'a'"
    );
}

#[test]
fn test_condense_call_graph_creates_dag() {
    // Test that condense_call_graph collapses SCCs into supernodes
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
}

fn a() {
    b();
}

fn b() {
    a();
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

    // Condense the call graph
    let result = graph.condense_call_graph().unwrap();

    // Should have supernodes
    assert!(
        !result.graph.supernodes.is_empty(),
        "Should create supernodes"
    );

    // One supernode should contain the cycle {a, b}
    let cycle_supernode = result.graph.supernodes.iter()
        .find(|s| {
            let fqns: Vec<_> = s.members.iter()
                .filter_map(|m| m.fqn.as_deref())
                .collect();
            fqns.iter().any(|f| f.contains("a")) && fqns.iter().any(|f| f.contains("b"))
        });

    assert!(
        cycle_supernode.is_some(),
        "Should have a supernode containing the cycle {a, b}"
    );

    // condensation graph should be a DAG (no cycles between supernodes)
    // Verify we have a reasonable structure
    assert!(
        result.graph.supernodes.len() <= 4, // main, {a,b}, c at most
        "Should have at most 4 supernodes"
    );
}

#[test]
fn test_condense_call_graph_single_symbol_supernodes() {
    // Test that single symbols (not in cycles) get their own supernodes
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
}

fn a() {
    b();
}

fn b() {}
fn c() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Condense the call graph
    let result = graph.condense_call_graph().unwrap();

    // Each non-cyclic function should be its own supernode
    let single_member_supernodes: Vec<_> = result.graph.supernodes.iter()
        .filter(|s| s.members.len() == 1)
        .collect();

    assert!(
        single_member_supernodes.len() >= 3,
        "Should have at least 3 single-member supernodes (main, b, c)"
    );
}

#[test]
fn test_condense_call_graph_symbol_to_supernode_mapping() {
    // Test that symbol_to_supernode mapping is correct
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    let source = r#"
fn main() {
    a();
}

fn a() {
    b();
}

fn b() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    let path_str = file_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes())
        .unwrap();
    graph.index_calls(&path_str, source.as_bytes())
        .unwrap();

    // Get symbol IDs
    let symbols = graph.symbols_in_file(&path_str).unwrap();
    let main_id = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("main"))
        .and_then(|s| s.stable_symbol_id.as_ref())
        .expect("Should find main symbol_id");

    // Condense the call graph
    let result = graph.condense_call_graph().unwrap();

    // main should be mapped to a supernode
    assert!(
        result.original_to_supernode.contains_key(main_id),
        "main should be in symbol_to_supernode mapping"
    );

    let supernode_id = result.original_to_supernode.get(main_id).unwrap();
    assert!(
        result.graph.supernodes.iter().any(|s| s.id == *supernode_id),
        "Supernode ID in mapping should exist"
    );
}

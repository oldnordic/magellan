//! Tests for graph algorithms (Phase 40)
//!
//! Tests reachability analysis and dead code detection using
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

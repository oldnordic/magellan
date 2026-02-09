//! Performance benchmarks for graph algorithms (Phase 40)
//!
//! These benchmarks measure algorithm performance on graphs of various sizes.
//! Run with: cargo test --test algo_benchmarks -- --ignored --nocapture --test-threads=1
//!
//! Benchmarks are marked #[ignore] by default to avoid slowing down normal test runs.
//! To run benchmarks:
//!     cargo test --test algo_benchmarks -- --ignored --nocapture --test-threads=1

use std::time::Instant;
use tempfile::TempDir;

#[test]
#[ignore]
fn benchmark_reachability_100_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Build source code with a chain of functions
    let n = 100;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    // Get main's FQN for querying
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

    let start = Instant::now();
    let result = graph.reachable_symbols(main_fqn, None).unwrap();
    let elapsed = start.elapsed();

    println!("Reachability (100 symbols): {:?}", elapsed);
    println!("  Found {} reachable symbols", result.len());

    assert!(elapsed.as_millis() < 100, "Reachability on 100 symbols should complete in <100ms");
}

#[test]
#[ignore]
fn benchmark_reachability_1000_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 1000;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.reachable_symbols("main", None).unwrap();
    let elapsed = start.elapsed();

    println!("Reachability (1000 symbols): {:?}", elapsed);
    println!("  Found {} reachable symbols", result.len());

    assert!(elapsed.as_secs() < 1, "Reachability on 1000 symbols should complete in <1s");
}

#[test]
#[ignore]
fn benchmark_scc_detection_100_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 100;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.detect_cycles().unwrap();
    let elapsed = start.elapsed();

    println!("SCC detection (100 symbols): {:?}", elapsed);
    println!("  Found {} cycles", result.cycles.len());

    assert!(elapsed.as_millis() < 100, "SCC detection on 100 symbols should complete in <100ms");
}

#[test]
#[ignore]
fn benchmark_scc_detection_1000_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 1000;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.detect_cycles().unwrap();
    let elapsed = start.elapsed();

    println!("SCC detection (1000 symbols): {:?}", elapsed);
    println!("  Found {} cycles", result.cycles.len());

    assert!(elapsed.as_secs() < 1, "SCC detection on 1000 symbols should complete in <1s");
}

#[test]
#[ignore]
fn benchmark_path_enumeration_with_bounds() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 100;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.enumerate_paths("main", None, 10, 100).unwrap();
    let elapsed = start.elapsed();

    println!("Path enumeration with bounds (100 symbols, max_depth=10, max_paths=100): {:?}", elapsed);
    println!("  Found {} paths, enumerated {}", result.paths.len(), result.total_enumerated);

    assert!(elapsed.as_secs() < 5, "Path enumeration with bounds should complete in <5s");
}

#[test]
#[ignore]
fn benchmark_backward_slice_100_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 100;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.backward_slice("fn50").unwrap();
    let elapsed = start.elapsed();

    println!("Backward slice (100 symbols, from middle): {:?}", elapsed);
    println!("  Slice size: {}", result.slice.symbol_count);

    assert!(elapsed.as_millis() < 100, "Backward slice on 100 symbols should complete in <100ms");
}

#[test]
#[ignore]
fn benchmark_forward_slice_100_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 100;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.forward_slice("fn50").unwrap();
    let elapsed = start.elapsed();

    println!("Forward slice (100 symbols, from middle): {:?}", elapsed);
    println!("  Slice size: {}", result.slice.symbol_count);

    assert!(elapsed.as_millis() < 100, "Forward slice on 100 symbols should complete in <100ms");
}

#[test]
#[ignore]
fn benchmark_dead_code_detection_1000_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 1000;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.dead_symbols("main").unwrap();
    let elapsed = start.elapsed();

    println!("Dead code detection (1000 symbols): {:?}", elapsed);
    println!("  Found {} dead symbols", result.len());

    assert!(elapsed.as_secs() < 1, "Dead code detection on 1000 symbols should complete in <1s");
}

#[test]
#[ignore]
fn benchmark_condensation_1000_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let n = 1000;
    let mut source = String::from("fn main() {\n    fn1();\n}\n\n");
    for i in 1..=n {
        if i < n {
            source.push_str(&format!("fn fn{}() {{ fn{}(); }}\n\n", i, i + 1));
        } else {
            source.push_str(&format!("fn fn{}() {{}}\n", i));
        }
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.condense_call_graph().unwrap();
    let elapsed = start.elapsed();

    println!("Condensation (1000 symbols): {:?}", elapsed);
    println!("  Created {} supernodes", result.graph.supernodes.len());

    assert!(elapsed.as_secs() < 1, "Condensation on 1000 symbols should complete in <1s");
}

#[test]
#[ignore]
fn benchmark_branching_graph_reachability() {
    // Create a graph with 3-way branching, depth 5 = ~363 symbols
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let branching_factor: u32 = 3;
    let depth: u32 = 5;

    let mut source = String::from("fn main() {\n");
    for i in 0..branching_factor {
        source.push_str(&format!("    branch_0_{}();\n", i));
    }
    source.push_str("}\n\n");

    let mut count = 0;
    for level in 0..depth {
        for _parent in 0..branching_factor.pow(level) {
            for child in 0..branching_factor {
                let name = format!("branch_{}_{}", level, count);
                if level < depth - 1 {
                    source.push_str(&format!("fn {}() {{ branch_{}_{}(); }}\n", name, level + 1, child * branching_factor + child));
                } else {
                    source.push_str(&format!("fn {}() {{}}\n", name));
                }
                count += 1;
            }
        }
        source.push('\n');
    }

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();
    let path_str = db_path.to_string_lossy().to_string();

    graph.index_file(&path_str, source.as_bytes()).unwrap();
    graph.index_calls(&path_str, source.as_bytes()).unwrap();

    let start = Instant::now();
    let result = graph.reachable_symbols("main", None).unwrap();
    let elapsed = start.elapsed();

    println!("Reachability on branching graph (3 branches, depth 5): {:?}", elapsed);
    println!("  Found {} reachable symbols", result.len());

    assert!(elapsed.as_millis() < 100, "Reachability on branching graph should complete in <100ms");
}

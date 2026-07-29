//! Integration test for the MIR CFG wiring in the indexer (ops.rs Step 5.6a).
//!
//! When a nightly toolchain is available, indexing a self-contained .rs file
//! must produce MIR-derived CFG blocks (SwitchInt terminators, Try::branch
//! statements) instead of the tree-sitter approximation. Gated on nightly
//! availability so stable-only environments skip cleanly.

use magellan::CodeGraph;
use tempfile::TempDir;

/// Self-contained: `?` and `match` produce MIR constructs tree-sitter cannot
/// see (`Try::branch`, `switchInt` on the discriminant).
const SELF_CONTAINED: &str = r#"fn parse_and_classify(s: &str) -> Result<i32, std::num::ParseIntError> {
    let n: i32 = s.trim().parse()?;
    match n {
        0 => Ok(0),
        x if x > 0 => Ok(x * 2),
        _ => Ok(-1),
    }
}
"#;

#[test]
fn test_mir_cfg_lands_for_self_contained_file_when_nightly_available() {
    if !magellan::graph::external_tools::rust::is_rustc_nightly_available() {
        eprintln!("nightly rustc not available; skipping MIR wiring test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let rs_path = temp_dir.path().join("demo.rs");
    std::fs::write(&rs_path, SELF_CONTAINED).unwrap();
    let path_str = rs_path.to_str().unwrap();

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph
        .index_file(path_str, SELF_CONTAINED.as_bytes())
        .unwrap();

    let all_cfg = graph.cfg_ops.get_cfg_for_file(path_str).unwrap();
    assert!(!all_cfg.is_empty(), "CFG blocks should exist for demo.rs");

    let terminators: Vec<String> = all_cfg
        .iter()
        .flat_map(|(_, blocks)| blocks.iter().map(|b| b.terminator.clone()))
        .collect();

    assert!(
        terminators.iter().any(|t| t == "SwitchInt"),
        "expected MIR-derived SwitchInt terminator, got: {terminators:?}"
    );
    assert!(
        terminators.iter().any(|t| t == "Call"),
        "expected MIR call terminators (assignment form), got: {terminators:?}"
    );
    assert!(
        !terminators.iter().any(|t| t == "Unknown"),
        "no unclassified MIR terminators expected, got: {terminators:?}"
    );
}

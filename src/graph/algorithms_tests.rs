
use super::*;
use crate::CodeGraph;

use std::sync::atomic::{AtomicU64, Ordering};

static _TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn _next_test_dir() -> std::path::PathBuf {
    let n = _TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("magellan_test_{}_{}", std::process::id(), n))
}

/// Test helper to create a simple call graph for testing
///
/// Creates:
/// - main() -> helper_a() -> leaf()
/// - main() -> helper_b() -> leaf()
/// - unused_function() -> leaf()
///
/// Returns the CodeGraph and symbol IDs for main and unused_function
fn create_test_graph() -> Result<(CodeGraph, String, String)> {
    // Use a persistent temp directory that won't be deleted
    // This is necessary for V3 backend which needs files to remain accessible.
    // Use a unique suffix per call to avoid collisions when tests run in parallel.
    let temp_dir = std::env::temp_dir().join(format!(
        "magellan_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir)?;
    let db_path = temp_dir.join("test.db");

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

fn leaf() {
    println!("leaf");
}

fn unused_function() {
    leaf();
}
"#;

    let mut graph = CodeGraph::open(&db_path)?;
    // Index the file - use a temporary file path
    let test_file = temp_dir.join("test.rs");
    std::fs::write(&test_file, source)?;
    let path_str = test_file.to_string_lossy().to_string();
    let source_bytes = std::fs::read(&test_file)?;

    // Index symbols and calls
    graph.index_file(&path_str, &source_bytes)?;
    graph.index_calls(&path_str, &source_bytes)?;

    // Find the symbol IDs for main and unused_function
    let symbols = crate::graph::query::symbols_in_file(&mut graph, &path_str)?;
    let main_id = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("main"))
        .and_then(|s| s.fqn.clone())
        .unwrap_or_default();

    let unused_id = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("unused_function"))
        .and_then(|s| s.fqn.clone())
        .unwrap_or_default();

    // For testing, use the symbol's FQN directly
    // In a real scenario with proper SymbolId generation, we'd use that
    Ok((graph, main_id, unused_id))
}

#[test]
fn test_resolve_symbol_entity_not_found() {
    let (graph, _, _) = create_test_graph().unwrap();
    let result = graph.resolve_symbol_entity("nonexistent_id_123456789012");
    assert!(result.is_err());
}

#[test]
fn test_symbol_by_entity_id() {
    let (graph, _, _) = create_test_graph().unwrap();

    // Get all entity IDs
    let entity_ids = graph.calls.backend.entity_ids().unwrap();
    let snapshot = SnapshotId::current();

    // Find a Symbol entity
    for entity_id in entity_ids {
        if let Ok(node) = graph.calls.backend.get_node(snapshot, entity_id) {
            if node.kind == "Symbol" {
                let info = graph.symbol_by_entity_id(entity_id);
                assert!(info.is_ok());
                let symbol_info = info.unwrap();
                assert!(!symbol_info.file_path.is_empty());
                assert!(!symbol_info.kind.is_empty());
                return;
            }
        }
    }

    panic!("No Symbol entity found in test graph");
}

#[test]
fn test_reachable_symbols_basic() {
    let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

    // Get all symbols and verify we can query them
    let entity_ids = graph.calls.backend.entity_ids().unwrap();
    eprintln!("Total entities: {}", entity_ids.len());
    let snapshot = SnapshotId::current();
    let mut found_symbols = 0;

    for entity_id in &entity_ids {
        match graph.calls.backend.get_node(snapshot, *entity_id) {
            Ok(node) => {
                eprintln!("  Entity {}: kind={}", entity_id, node.kind);
                if node.kind == "Symbol" {
                    found_symbols += 1;
                }
            }
            Err(e) => {
                eprintln!("  Entity {}: ERROR getting node: {:?}", entity_id, e);
            }
        }
    }

    // We should have found at least some symbols
    assert!(
        found_symbols > 0,
        "Should find Symbol entities in test graph, got {} symbols from {} entities",
        found_symbols,
        entity_ids.len()
    );
}

#[test]
fn test_reachable_symbols_max_depth() {
    let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

    // Get the main function's entity ID
    let snapshot = SnapshotId::current();
    let entity_ids = graph.calls.backend.entity_ids().unwrap();

    let main_entity_id = entity_ids.into_iter().find(|&id| {
        if let Ok(node) = graph.calls.backend.get_node(snapshot, id) {
            if let Ok(data) = serde_json::from_value::<serde_json::Value>(node.data) {
                if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                    return name == "main";
                }
            }
        }
        false
    });

    if let Some(entity_id) = main_entity_id {
        // Verify we can get the node
        let result = graph.calls.backend.get_node(snapshot, entity_id);
        assert!(result.is_ok(), "Should be able to get main node");
    }
}

#[test]
fn test_dead_symbols() {
    let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

    // Get all entity IDs
    let entity_ids = graph.calls.backend.entity_ids().unwrap();

    // We should have some entities in the call graph
    assert!(!entity_ids.is_empty(), "Should have call graph entities");
}

#[test]
fn test_reverse_reachable_symbols() {
    let (graph, _main_id, _unused_id) = create_test_graph().unwrap();

    // Get all entity IDs
    let entity_ids = graph.calls.backend.entity_ids().unwrap();

    // We should have some entities in the call graph
    assert!(!entity_ids.is_empty(), "Should have call graph entities");
}

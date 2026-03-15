#![cfg(feature = "geometric-backend")]
//! Tests for geometric backend CFG extraction and persistence
//!
//! These tests verify that CFG blocks are properly extracted from Rust source
//! and persisted/reloaded in the .geo format.

use std::path::PathBuf;
use tempfile::tempdir;

use magellan::graph::geometric_backend::{GeometricBackend, InsertSymbol};
use magellan::ingest::{Language, SymbolKind};

/// Test helper: create a temp .geo database path
fn temp_geo_path() -> (tempfile::TempDir, PathBuf) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.geo");
    (temp_dir, db_path)
}

/// Simple Rust source with control flow
const SIMPLE_RUST: &str = r#"
pub fn simple_function(x: i32) -> i32 {
    if x > 0 {
        x + 1
    } else {
        x - 1
    }
}
"#;

/// Rust source with loop
const LOOP_RUST: &str = r#"
pub fn loop_function(n: usize) -> usize {
    let mut sum = 0;
    for i in 0..n {
        sum += i;
    }
    sum
}
"#;

#[test]
fn geometric_cfg_blocks_nonzero_after_index() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create backend and index Rust code with CFG
    let backend = GeometricBackend::create(&db_path).unwrap();

    // Manually extract symbols with CFG
    let (symbols, cfg_blocks, _edges) =
        magellan::graph::geometric_backend::extract_symbols_and_cfg_from_file(
            &PathBuf::from("/test.rs"),
            SIMPLE_RUST,
            Language::Rust,
        )
        .unwrap();

    assert!(!symbols.is_empty(), "Should extract symbols");
    assert!(
        !cfg_blocks.is_empty(),
        "Should extract CFG blocks for function with if/else"
    );

    // Insert into backend
    backend.insert_symbols(symbols).unwrap();
    for block in cfg_blocks {
        backend.insert_cfg_block(block).unwrap();
    }
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and check CFG count in stats
    let backend = GeometricBackend::open(&db_path).unwrap();
    let stats = backend.get_stats().unwrap();

    assert!(
        stats.cfg_block_count > 0,
        "cfg_block_count should be non-zero after indexing with CFG"
    );
}

#[test]
fn geometric_cfg_persists_after_reopen() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create backend with CFG
    let backend = GeometricBackend::create(&db_path).unwrap();
    let (symbols, cfg_blocks, _edges) =
        magellan::graph::geometric_backend::extract_symbols_and_cfg_from_file(
            &PathBuf::from("/test.rs"),
            LOOP_RUST,
            Language::Rust,
        )
        .unwrap();

    let cfg_count_before = cfg_blocks.len();
    assert!(cfg_count_before > 0, "Should have extracted CFG blocks");

    backend.insert_symbols(symbols).unwrap();
    for block in cfg_blocks {
        backend.insert_cfg_block(block).unwrap();
    }
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and verify CFG blocks are restored
    let backend = GeometricBackend::open(&db_path).unwrap();
    let stats = backend.get_stats().unwrap();

    assert_eq!(
        stats.cfg_block_count, cfg_count_before,
        "CFG block count should match after reopen"
    );
}

#[test]
fn geometric_status_reports_real_cfg_count() {
    let (_temp_dir, db_path) = temp_geo_path();

    let backend = GeometricBackend::create(&db_path).unwrap();

    // Index with CFG
    let (symbols, cfg_blocks, _edges) =
        magellan::graph::geometric_backend::extract_symbols_and_cfg_from_file(
            &PathBuf::from("/test.rs"),
            SIMPLE_RUST,
            Language::Rust,
        )
        .unwrap();

    let expected_cfg_count = cfg_blocks.len();
    backend.insert_symbols(symbols).unwrap();
    for block in cfg_blocks {
        backend.insert_cfg_block(block).unwrap();
    }
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and check status
    let backend = GeometricBackend::open(&db_path).unwrap();
    let stats = backend.get_stats().unwrap();

    assert_eq!(
        stats.cfg_block_count, expected_cfg_count,
        "Status should report exact CFG block count"
    );
    assert!(stats.symbol_count > 0, "Should have symbols");
}

#[test]
fn geometric_cfg_associates_with_real_functions() {
    let (_temp_dir, db_path) = temp_geo_path();

    let backend = GeometricBackend::create(&db_path).unwrap();

    // Extract both symbols and CFG
    let (symbols, cfg_blocks, _edges) =
        magellan::graph::geometric_backend::extract_symbols_and_cfg_from_file(
            &PathBuf::from("/test.rs"),
            SIMPLE_RUST,
            Language::Rust,
        )
        .unwrap();

    // Verify we have both
    assert!(!symbols.is_empty(), "Should have symbols");
    assert!(!cfg_blocks.is_empty(), "Should have CFG blocks");

    // Each CFG block should reference a function by index
    for block in &cfg_blocks {
        assert!(
            block.function_id >= 0 && (block.function_id as usize) < symbols.len(),
            "CFG block should reference valid function index"
        );
    }

    // Insert symbols first (to get real IDs)
    let symbol_ids = backend.insert_symbols(symbols).unwrap();

    // Then insert CFG blocks (they reference symbol indices)
    for block in cfg_blocks {
        let _ = backend.insert_cfg_block(block).unwrap();
    }

    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and verify symbols exist
    let backend = GeometricBackend::open(&db_path).unwrap();
    for id in symbol_ids {
        let info = backend.find_symbol_by_id_info(id);
        assert!(info.is_some(), "Symbol {} should exist after reopen", id);
    }
}

#[test]
fn geometric_query_and_cfg_coexist_after_reopen() {
    let (_temp_dir, db_path) = temp_geo_path();

    let backend = GeometricBackend::create(&db_path).unwrap();

    // Extract both symbols and CFG
    let (symbols, cfg_blocks, _edges) =
        magellan::graph::geometric_backend::extract_symbols_and_cfg_from_file(
            &PathBuf::from("/test.rs"),
            SIMPLE_RUST,
            Language::Rust,
        )
        .unwrap();

    backend.insert_symbols(symbols).unwrap();
    for block in cfg_blocks {
        backend.insert_cfg_block(block).unwrap();
    }
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen
    let backend = GeometricBackend::open(&db_path).unwrap();

    // Query should work
    let all_symbols = backend.get_all_symbols().unwrap();
    assert!(!all_symbols.is_empty(), "Should have symbols after reopen");

    // CFG stats should work
    let stats = backend.get_stats().unwrap();
    assert!(
        stats.cfg_block_count > 0,
        "Should have CFG blocks after reopen"
    );

    // Both queries and CFG should coexist
    assert!(stats.symbol_count > 0);
    assert!(stats.cfg_block_count > 0);
}

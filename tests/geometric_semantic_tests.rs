#![cfg(feature = "geometric-backend")]
//! Tests for geometric backend semantic metadata persistence
//!
//! These tests verify that symbol metadata (name, FQN, file path) is properly
//! persisted and reloadable in the .geo format.

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

/// Test helper: create a sample symbol
fn sample_symbol(id: u64) -> InsertSymbol {
    InsertSymbol {
        name: format!("test_func_{}", id),
        fqn: format!("crate::module::test_func_{}", id),
        kind: SymbolKind::Function,
        file_path: format!("/home/project/src/file_{}.rs", id % 3),
        byte_start: id * 100,
        byte_end: id * 100 + 50,
        start_line: id * 10,
        start_col: 0,
        end_line: id * 10 + 5,
        end_col: 1,
        language: Language::Rust,
    }
}

#[test]
fn geometric_persists_symbol_name_metadata() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1)];
    let ids = backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and verify name is preserved
    let backend = GeometricBackend::open(&db_path).unwrap();
    let info = backend.find_symbol_by_id_info(ids[0]).unwrap();

    assert_eq!(info.name, "test_func_1", "Symbol name should be persisted");
}

#[test]
fn geometric_persists_fqn_metadata() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1)];
    let ids = backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and verify FQN is preserved
    let backend = GeometricBackend::open(&db_path).unwrap();
    let info = backend.find_symbol_by_id_info(ids[0]).unwrap();

    assert_eq!(
        info.fqn, "crate::module::test_func_1",
        "FQN should be persisted"
    );
}

#[test]
fn geometric_persists_file_path_metadata() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1)];
    let ids = backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and verify file path is preserved
    let backend = GeometricBackend::open(&db_path).unwrap();
    let info = backend.find_symbol_by_id_info(ids[0]).unwrap();

    assert_eq!(
        info.file_path, "/home/project/src/file_1.rs",
        "File path should be persisted"
    );
}

#[test]
fn geometric_file_count_is_real() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols across multiple files
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![
        sample_symbol(0), // file_0.rs
        sample_symbol(1), // file_1.rs
        sample_symbol(2), // file_2.rs
        sample_symbol(3), // file_0.rs (duplicate file)
    ];
    backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and verify file count
    let backend = GeometricBackend::open(&db_path).unwrap();
    let stats = backend.get_stats().unwrap();

    assert_eq!(stats.file_count, 3, "Should have 3 unique files");
}

#[test]
fn geometric_find_by_name_works_after_reopen() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1), sample_symbol(2)];
    backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and find by name
    let backend = GeometricBackend::open(&db_path).unwrap();
    let results = backend.find_symbols_by_name_info("test_func_1");

    assert_eq!(results.len(), 1, "Should find exactly one symbol");
    assert_eq!(results[0].name, "test_func_1");
    assert_eq!(results[0].fqn, "crate::module::test_func_1");
}

#[test]
fn geometric_find_by_fqn_works_after_reopen() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1), sample_symbol(2)];
    backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and find by FQN
    let backend = GeometricBackend::open(&db_path).unwrap();
    let info = backend
        .find_symbol_by_fqn_info("crate::module::test_func_2")
        .unwrap();

    assert_eq!(info.name, "test_func_2");
    assert_eq!(info.fqn, "crate::module::test_func_2");
}

#[test]
fn geometric_symbols_in_file_works_after_reopen() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols across files
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![
        sample_symbol(0), // file_0.rs
        sample_symbol(3), // file_0.rs
        sample_symbol(1), // file_1.rs
    ];
    backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and get symbols in file
    let backend = GeometricBackend::open(&db_path).unwrap();
    let results = backend
        .symbols_in_file("/home/project/src/file_0.rs")
        .unwrap();

    assert_eq!(results.len(), 2, "Should find 2 symbols in file_0.rs");

    let names: Vec<_> = results.iter().map(|s| s.name.clone()).collect();
    assert!(names.contains(&"test_func_0".to_string()));
    assert!(names.contains(&"test_func_3".to_string()));
}

#[test]
fn geometric_export_uses_real_metadata_not_placeholders() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1)];
    backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and export
    let backend = GeometricBackend::open(&db_path).unwrap();
    let json = backend.export_json().unwrap();

    // Verify no placeholders in export
    assert!(
        !json.contains("symbol_1"),
        "Export should not contain placeholder names"
    );
    assert!(
        json.contains("test_func_1"),
        "Export should contain real name"
    );
    assert!(
        json.contains("crate::module::test_func_1"),
        "Export should contain real FQN"
    );
    assert!(
        json.contains("/home/project/src/file_1.rs"),
        "Export should contain real file path"
    );
}

#[test]
fn geometric_query_returns_real_symbols_not_empty_placeholders() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create and insert symbols
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![sample_symbol(1), sample_symbol(2)];
    backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and get all symbols
    let backend = GeometricBackend::open(&db_path).unwrap();
    let all = backend.get_all_symbols().unwrap();

    assert_eq!(all.len(), 2, "Should return 2 symbols");

    for info in &all {
        assert!(
            !info.name.starts_with("symbol_"),
            "Should not have placeholder names"
        );
        assert!(!info.file_path.is_empty(), "Should have real file paths");
        assert!(!info.fqn.is_empty(), "Should have real FQNs");
    }
}

#[test]
fn geometric_find_by_name_and_path_works() {
    let (_temp_dir, db_path) = temp_geo_path();

    // Create symbols with same name in different files
    let backend = GeometricBackend::create(&db_path).unwrap();
    let symbols = vec![
        InsertSymbol {
            name: "common_name".to_string(),
            fqn: "crate::A::common_name".to_string(),
            kind: SymbolKind::Function,
            file_path: "/src/a.rs".to_string(),
            byte_start: 0,
            byte_end: 10,
            start_line: 0,
            start_col: 0,
            end_line: 1,
            end_col: 0,
            language: Language::Rust,
        },
        InsertSymbol {
            name: "common_name".to_string(),
            fqn: "crate::B::common_name".to_string(),
            kind: SymbolKind::Function,
            file_path: "/src/b.rs".to_string(),
            byte_start: 0,
            byte_end: 10,
            start_line: 0,
            start_col: 0,
            end_line: 1,
            end_col: 0,
            language: Language::Rust,
        },
    ];
    let ids = backend.insert_symbols(symbols).unwrap();
    backend.save_to_disk().unwrap();
    drop(backend);

    // Reopen and find specific symbol by name + path
    let backend = GeometricBackend::open(&db_path).unwrap();

    let id_a = backend.find_symbol_id_by_name_and_path("common_name", "/src/a.rs");
    assert_eq!(id_a, Some(ids[0]), "Should find symbol in a.rs");

    let id_b = backend.find_symbol_id_by_name_and_path("common_name", "/src/b.rs");
    assert_eq!(id_b, Some(ids[1]), "Should find symbol in b.rs");
}

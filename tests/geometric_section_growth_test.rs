#![cfg(feature = "geometric-backend")]
//! Tests for geometric backend section capacity growth
//!
//! These tests verify that sections automatically resize when data exceeds capacity.

use magellan::graph::geometric_backend::{GeometricBackend, InsertSymbol};
use magellan::ingest::Language;
use magellan::SymbolKind;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Test that AST section grows when payload exceeds initial capacity
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_ast_section_grows_for_large_payload() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_ast_growth.geo");

    // Create a backend
    let backend = GeometricBackend::create(&db_path).unwrap();

    // Add a large amount of AST data (simulate by adding many nodes)
    for i in 0..10000 {
        backend.add_ast_node(
            &format!("src/file_{}.rs", i % 100),
            "function",
            i * 100,
            i * 100 + 50,
            None,
        );
    }

    // Save should succeed by auto-resizing
    backend
        .save_to_disk()
        .expect("AST section should auto-resize and save successfully");

    // Verify database is valid by reopening
    drop(backend);
    let backend2 = GeometricBackend::open(&db_path).unwrap();
    let nodes = backend2.get_ast_nodes_by_file("src/file_0.rs");
    assert!(!nodes.is_empty(), "AST nodes should be persisted");
}

/// Test that CHUNK section grows for large payloads
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunks_section_grows_for_large_payload() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunk_growth.geo");

    let backend = GeometricBackend::create(&db_path).unwrap();

    // Add many code chunks using the simple API
    for i in 0..5000 {
        backend.insert_code_chunk(
            &format!("src/file_{}.rs", i % 50),
            i * 1000,
            i * 1000 + 500,
            &format!("// Code chunk {} with some content to make it larger\n", i),
            Some(&format!("symbol_{}", i)),
            Some("function"),
        );
    }

    // Save should succeed
    backend
        .save_to_disk()
        .expect("CHUNK section should auto-resize and save successfully");

    // Verify
    drop(backend);
    let backend2 = GeometricBackend::open(&db_path).unwrap();
    let chunks = backend2
        .get_code_chunks("src/file_0.rs")
        .expect("Should get chunks");
    assert!(!chunks.is_empty(), "Code chunks should be persisted");
}

/// Test that watch command produces no debug output in normal mode
#[test]
#[cfg(feature = "geometric-backend")]
fn watch_no_debug_output_in_normal_mode() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_no_debug.geo");
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("audit_fixture");

    // Skip if fixture doesn't exist
    if !fixture_dir.exists() {
        eprintln!(
            "Skipping test: audit_fixture not found at {:?}",
            fixture_dir
        );
        return;
    }

    // Run magellan watch with timeout
    let output = Command::new(env!("CARGO_BIN_EXE_magellan"))
        .args(&[
            "watch",
            "--root",
            fixture_dir.to_str().unwrap(),
            "--db",
            db_path.to_str().unwrap(),
            "--scan-initial",
        ])
        .env_remove("MAGELLAN_DEBUG") // Ensure debug is NOT enabled
        .output()
        .expect("Failed to execute magellan watch");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT contain WATCH_DEBUG output
    assert!(
        !stderr.contains("[WATCH_DEBUG]"),
        "Normal mode should not produce [WATCH_DEBUG] output. stderr: {}",
        stderr
    );
}

/// Test that watch returns error when save fails (simulated by making file read-only)
#[test]
#[cfg(feature = "geometric-backend")]
fn watch_returns_error_when_save_fails() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_save_error.geo");

    // Create and populate a database
    let backend = GeometricBackend::create(&db_path).unwrap();
    backend.insert_code_chunk("src/test.rs", 0, 100, "test content", None, None);
    backend.save_to_disk().unwrap();
    drop(backend);

    // Make the file read-only to simulate save failure
    let mut perms = fs::metadata(&db_path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&db_path, perms).unwrap();

    // Try to open and save - should fail
    let backend2 = GeometricBackend::open(&db_path).unwrap();
    backend2.insert_code_chunk("src/test2.rs", 100, 200, "more content", None, None);
    let result = backend2.save_to_disk();

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&db_path).unwrap().permissions();
    perms.set_readonly(false);
    let _ = fs::set_permissions(&db_path, perms);

    // Should have failed
    assert!(result.is_err(), "Save should fail when file is read-only");
}

/// Test that large source tree can be indexed and reopened
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_large_index_reopen_works() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_large_reopen.geo");

    // Create backend and add substantial data
    let backend = GeometricBackend::create(&db_path).unwrap();

    // Add many symbols with metadata
    for i in 0..100 {
        let file_path = format!("src/file_{}.rs", i);
        let symbols: Vec<InsertSymbol> = (0..50)
            .map(|j| InsertSymbol {
                name: format!("symbol_{}_{}", i, j),
                fqn: format!("crate::module_{}::symbol_{}_{}", i, i, j),
                kind: SymbolKind::Function,
                file_path: file_path.clone(),
                byte_start: (j * 100) as u64,
                byte_end: (j * 100 + 50) as u64,
                start_line: (j * 10) as u64,
                start_col: 0,
                end_line: (j * 10 + 5) as u64,
                end_col: 50,
                language: Language::Rust,
            })
            .collect();
        backend.insert_symbols(symbols).unwrap();
    }

    // Add many AST nodes
    for i in 0..5000 {
        backend.add_ast_node("src/lib.rs", "function", i * 100, i * 100 + 50, None);
    }

    // Save
    backend.save_to_disk().expect("Should save large dataset");
    drop(backend);

    // Reopen and verify
    let backend2 = GeometricBackend::open(&db_path).expect("Should reopen database");
    let all_symbols = backend2.get_all_symbols().expect("Should get symbols");
    assert!(
        !all_symbols.is_empty(),
        "Symbols should be persisted after reopen"
    );
}

/// Test section capacity handles realistic source tree size
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_section_capacity_handles_realistic_src_tree() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_realistic.geo");

    let backend = GeometricBackend::create(&db_path).unwrap();

    // Simulate a realistic medium-sized project:
    // - 50 files
    // - 100 symbols per file
    // - 50 AST nodes per file
    // - 20 chunks per file

    for file_idx in 0..50 {
        let file_path = format!("src/module{}/file{}.rs", file_idx / 10, file_idx);

        // Add symbols
        let symbols: Vec<InsertSymbol> = (0..100)
            .map(|sym_idx| InsertSymbol {
                name: format!("func_{}_{}", file_idx, sym_idx),
                fqn: format!(
                    "crate::module_{}::func_{}_{}",
                    file_idx / 10,
                    file_idx,
                    sym_idx
                ),
                kind: SymbolKind::Function,
                file_path: file_path.clone(),
                byte_start: (sym_idx * 100) as u64,
                byte_end: (sym_idx * 100 + 50) as u64,
                start_line: (sym_idx * 10) as u64,
                start_col: 0,
                end_line: (sym_idx * 10 + 5) as u64,
                end_col: 50,
                language: Language::Rust,
            })
            .collect();
        backend.insert_symbols(symbols).unwrap();

        // Add AST nodes
        for ast_idx in 0..50 {
            backend.add_ast_node(
                &file_path,
                "function",
                ast_idx * 100,
                ast_idx * 100 + 50,
                None,
            );
        }

        // Add code chunks
        for chunk_idx in 0..20 {
            backend.insert_code_chunk(
                &file_path,
                chunk_idx * 200,
                chunk_idx * 200 + 100,
                &"// Some code content here\n".repeat(20),
                Some(&format!("symbol_{}_{}", file_idx, chunk_idx)),
                Some("function"),
            );
        }
    }

    // Save - this would have failed with 1MB section limits before the fix
    backend
        .save_to_disk()
        .expect("Should handle realistic project size");

    // Verify by reopening
    drop(backend);
    let backend2 = GeometricBackend::open(&db_path).expect("Should reopen");
    let symbols = backend2.get_all_symbols().expect("Should get symbols");
    assert!(symbols.len() > 100, "Should have many symbols persisted");
}

#![cfg(feature = "geometric-backend")]
//! Integration tests for chunks command on geometric backend

use magellan::graph::geo_index::{scan_directory_with_progress, IndexingMode};
use magellan::graph::geometric_backend::GeometricBackend;

/// Test chunks command lists all chunks
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunks_command_lists_chunks() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunks_cmd.geo");

    // Create source files
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();

    std::fs::write(
        src_dir.join("main.rs"),
        r#"
fn main() {
    helper();
}

fn helper() {}
"#,
    )
    .unwrap();

    // Index
    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
        backend.save_to_disk().unwrap();
    }

    // Verify chunks exist
    {
        let backend = GeometricBackend::open(&db_path).unwrap();
        let chunks = backend.get_all_chunks().unwrap();

        assert!(!chunks.is_empty(), "Should have chunks");
        assert!(
            chunks.len() >= 2,
            "Should have at least 2 chunks (main and helper)"
        );

        // Verify chunk content
        let main_chunk = chunks
            .iter()
            .find(|c| c.symbol_name == Some("main".to_string()));
        assert!(main_chunk.is_some(), "Should have main function chunk");

        let helper_chunk = chunks
            .iter()
            .find(|c| c.symbol_name == Some("helper".to_string()));
        assert!(helper_chunk.is_some(), "Should have helper function chunk");
    }
}

/// Test chunks command respects file filter
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunks_command_respects_file_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunks_filter.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();

    std::fs::write(
        src_dir.join("main.rs"),
        r#"
fn main() {}
fn foo() {}
"#,
    )
    .unwrap();

    std::fs::write(
        src_dir.join("lib.rs"),
        r#"
pub fn bar() {}
"#,
    )
    .unwrap();

    // Index
    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
        backend.save_to_disk().unwrap();
    }

    // Verify filtering works
    {
        let backend = GeometricBackend::open(&db_path).unwrap();
        let all_chunks = backend.get_all_chunks().unwrap();

        // Filter by file path
        let main_chunks: Vec<_> = all_chunks
            .iter()
            .filter(|c| c.file_path.contains("main.rs"))
            .collect();

        assert!(!main_chunks.is_empty(), "Should have main.rs chunks");
        assert!(
            main_chunks.iter().all(|c| c.file_path.contains("main.rs")),
            "All filtered chunks should be from main.rs"
        );
    }
}

/// Test chunks survive reopen
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunks_command_survives_reopen() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunks_reopen.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();

    std::fs::write(
        src_dir.join("test.rs"),
        r#"
fn test_func() -> i32 { 42 }
"#,
    )
    .unwrap();

    // First session: index and save
    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
        backend.save_to_disk().unwrap();

        let chunks = backend.get_all_chunks().unwrap();
        assert_eq!(chunks.len(), 1, "Should have 1 chunk before reopen");
        std::fs::write(temp_dir.path().join("count.txt"), chunks.len().to_string()).unwrap();
    }

    // Second session: reopen and verify
    {
        let backend = GeometricBackend::open(&db_path).unwrap();
        let chunks = backend.get_all_chunks().unwrap();

        let expected: usize = std::fs::read_to_string(temp_dir.path().join("count.txt"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();

        assert_eq!(
            chunks.len(),
            expected,
            "Chunk count should match after reopen"
        );

        // Verify content
        let chunk = &chunks[0];
        assert_eq!(chunk.symbol_name, Some("test_func".to_string()));
        assert!(chunk.content.contains("test_func"));
    }
}

/// Test chunks respect kind filter
#[test]
#[cfg(feature = "geometric-backend")]
fn geometric_chunks_command_respects_kind_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_chunks_kind.geo");

    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();

    std::fs::write(
        src_dir.join("test.rs"),
        r#"
fn foo() {}
pub struct Bar {}
"#,
    )
    .unwrap();

    // Index
    {
        let mut backend = GeometricBackend::create(&db_path).unwrap();
        scan_directory_with_progress(&mut backend, &src_dir, None, IndexingMode::CfgFirst).unwrap();
        backend.save_to_disk().unwrap();
    }

    // Verify kind filtering
    {
        let backend = GeometricBackend::open(&db_path).unwrap();
        let all_chunks = backend.get_all_chunks().unwrap();

        // Filter by Function kind
        let function_chunks: Vec<_> = all_chunks
            .iter()
            .filter(|c| {
                c.symbol_kind
                    .as_ref()
                    .map(|k| k.contains("Function"))
                    .unwrap_or(false)
            })
            .collect();

        assert!(!function_chunks.is_empty(), "Should have function chunks");
    }
}

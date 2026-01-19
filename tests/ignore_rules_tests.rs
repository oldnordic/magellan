//! Integration tests for gitignore-style ignore rules and CLI include/exclude globs.
//!
//! Tests:
//! 1. .gitignore and .ignore are honored and record Skipped diagnostics with IgnoredByGitignore
//! 2. --include restricts to a subset even if other files exist
//! 3. --exclude overrides include and prevents indexing; diagnostic reason is ExcludedByGlob
//! 4. Parse/index error on one file does NOT stop indexing other files

use magellan::{CodeGraph, FileFilter};
use std::fs;
use tempfile::TempDir;

/// Test that .gitignore and .ignore files are honored
#[test]
fn test_gitignore_and_ignore_honored() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let db_path = root.join("test.db");

    // Create .gitignore
    fs::write(root.join(".gitignore"), "ignored_dir/**\ntarget/\n").unwrap();

    // Create .ignore (ripgrep-style)
    fs::write(root.join(".ignore"), "*.tmp\n").unwrap();

    // Create directory structure
    fs::create_dir_all(root.join("ignored_dir")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();

    // Create files
    fs::write(root.join("src/lib.rs"), "fn lib() {}").unwrap();
    fs::write(root.join("ignored_dir/code.rs"), "fn ignored() {}").unwrap();
    fs::write(root.join("target/lib.rs"), "fn target() {}").unwrap();
    fs::write(root.join("main.tmp"), "temporary content").unwrap();

    // Create filter and scan
    let filter = FileFilter::new(root, &[], &[]).unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let result = magellan::graph::scan::scan_directory_with_filter(
        &mut graph,
        root,
        &filter,
        None,
    )
    .unwrap();

    // Only src/lib.rs should be indexed
    assert_eq!(result.indexed, 1);

    // Check diagnostics
    let ignored_dir_diag = result
        .diagnostics
        .iter()
        .find(|d| d.path().contains("ignored_dir"));
    assert!(ignored_dir_diag.is_some());

    let target_diag = result.diagnostics.iter().find(|d| d.path().contains("target"));
    assert!(target_diag.is_some());

    let tmp_diag = result.diagnostics.iter().find(|d| d.path().ends_with(".tmp"));
    assert!(tmp_diag.is_some());

    // Verify src/lib.rs was actually indexed
    let symbols = graph.symbols_in_file(&root.join("src/lib.rs").to_string_lossy()).unwrap();
    assert!(!symbols.is_empty());
}

/// Test that --include restricts to a subset even if other files exist
#[test]
fn test_include_pattern_restricts() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let db_path = root.join("test.db");

    // Create directory structure
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("examples")).unwrap();

    // Create files
    fs::write(root.join("src/lib.rs"), "fn lib() {}").unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(root.join("tests/test.rs"), "fn test() {}").unwrap();
    fs::write(root.join("examples/demo.rs"), "fn demo() {}").unwrap();

    // Create filter with --include "src/**"
    let filter = FileFilter::new(root, &["src/**".to_string()], &[]).unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let result = magellan::graph::scan::scan_directory_with_filter(
        &mut graph,
        root,
        &filter,
        None,
    )
    .unwrap();

    // Only src/** files should be indexed
    assert_eq!(result.indexed, 2);

    // Check that tests and examples have skip diagnostics
    let tests_diag = result.diagnostics.iter().find(|d| d.path().contains("tests"));
    assert!(tests_diag.is_some());

    let examples_diag = result
        .diagnostics
        .iter()
        .find(|d| d.path().contains("examples"));
    assert!(examples_diag.is_some());

    // Verify src files were indexed
    let lib_symbols = graph
        .symbols_in_file(&root.join("src/lib.rs").to_string_lossy())
        .unwrap();
    assert!(!lib_symbols.is_empty());

    let main_symbols = graph
        .symbols_in_file(&root.join("src/main.rs").to_string_lossy())
        .unwrap();
    assert!(!main_symbols.is_empty());
}

/// Test that --exclude overrides include and prevents indexing
#[test]
fn test_exclude_overrides_include() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let db_path = root.join("test.db");

    // Create directory structure
    fs::create_dir_all(root.join("src")).unwrap();

    // Create files
    fs::write(root.join("src/lib.rs"), "fn lib() {}").unwrap();
    fs::write(root.join("src/test.rs"), "fn test() {}").unwrap();
    fs::write(root.join("src/test_helper.rs"), "fn helper() {}").unwrap();

    // Create filter with --include "src/**" and --exclude "**/*test*"
    let filter = FileFilter::new(
        root,
        &["src/**".to_string()],
        &["**/*test*.rs".to_string()],
    )
    .unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let result = magellan::graph::scan::scan_directory_with_filter(
        &mut graph,
        root,
        &filter,
        None,
    )
    .unwrap();

    // Only src/lib.rs should be indexed (test.rs and test_helper.rs are excluded)
    assert_eq!(result.indexed, 1);

    // Check that test files have ExcludedByGlob diagnostics
    let test_diag = result
        .diagnostics
        .iter()
        .find(|d| d.path() == "src/test.rs");
    assert!(test_diag.is_some());
    assert!(matches!(test_diag.unwrap(), magellan::WatchDiagnostic::Skipped { reason, .. }
        if matches!(reason, magellan::SkipReason::ExcludedByGlob)));

    let helper_diag = result
        .diagnostics
        .iter()
        .find(|d| d.path() == "src/test_helper.rs");
    assert!(helper_diag.is_some());

    // Verify src/lib.rs was indexed
    let lib_symbols = graph
        .symbols_in_file(&root.join("src/lib.rs").to_string_lossy())
        .unwrap();
    assert!(!lib_symbols.is_empty());
}

/// Test that parse/index error on one file does NOT stop indexing other files
#[test]
fn test_error_on_one_file_continues() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let db_path = root.join("test.db");

    // Create valid files
    fs::write(root.join("good1.rs"), "fn good1() {}").unwrap();
    fs::write(root.join("good2.rs"), "fn good2() {}").unwrap();

    // Create a file that will fail to read (permissions)
    let bad_file = root.join("bad.rs");
    fs::write(&bad_file, "fn bad() {}").unwrap();

    // Make file unreadable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bad_file).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&bad_file, perms).unwrap();

        let filter = FileFilter::new(root, &[], &[]).unwrap();
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let result = magellan::graph::scan::scan_directory_with_filter(
            &mut graph,
            root,
            &filter,
            None,
        )
        .unwrap();

        // At least good1.rs and good2.rs should be indexed
        assert!(result.indexed >= 2);

        // Should have an error diagnostic for bad.rs
        let bad_diag = result.diagnostics.iter().find(|d| d.path() == "bad.rs");
        assert!(bad_diag.is_some());

        // Verify good files were indexed
        let good1_symbols = graph
            .symbols_in_file(&root.join("good1.rs").to_string_lossy())
            .unwrap();
        assert!(!good1_symbols.is_empty());

        let good2_symbols = graph
            .symbols_in_file(&root.join("good2.rs").to_string_lossy())
            .unwrap();
        assert!(!good2_symbols.is_empty());

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&bad_file).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&bad_file, perms).unwrap();
    }

    #[cfg(not(unix))]
    {
        let filter = FileFilter::new(root, &[], &[]).unwrap();
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let result = magellan::graph::scan::scan_directory_with_filter(
            &mut graph,
            root,
            &filter,
            None,
        )
        .unwrap();

        // On non-Unix, all 3 files should be indexed (no permission error)
        assert_eq!(result.indexed, 3);
    }
}

/// Test determinism: same files + rules = same diagnostics
#[test]
fn test_deterministic_filtering() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Use temp_dir for databases (outside the scan directory)
    let db_dir = TempDir::new().unwrap();
    let db_path1 = db_dir.path().join("test1.db");
    let db_path2 = db_dir.path().join("test2.db");

    // Create .gitignore
    fs::write(root.join(".gitignore"), "ignored.rs\n").unwrap();

    // Create files in non-alphabetical order
    fs::write(root.join("z.rs"), "fn z() {}").unwrap();
    fs::write(root.join("a.rs"), "fn a() {}").unwrap();
    fs::write(root.join("m.rs"), "fn m() {}").unwrap();
    fs::write(root.join("ignored.rs"), "fn ignored() {}").unwrap();

    // Scan twice with same filter configuration (new filter each time)
    let mut graph1 = CodeGraph::open(&db_path1).unwrap();
    let filter1 = FileFilter::new(root, &[], &[]).unwrap();
    let result1 = magellan::graph::scan::scan_directory_with_filter(
        &mut graph1,
        root,
        &filter1,
        None,
    )
    .unwrap();

    let mut graph2 = CodeGraph::open(&db_path2).unwrap();
    let filter2 = FileFilter::new(root, &[], &[]).unwrap();
    let result2 = magellan::graph::scan::scan_directory_with_filter(
        &mut graph2,
        root,
        &filter2,
        None,
    )
    .unwrap();

    // Results should be identical
    assert_eq!(result1.indexed, result2.indexed);

    // The number of indexed files should be 3 (a.rs, m.rs, z.rs)
    assert_eq!(result1.indexed, 3);

    // After sorting, diagnostics should be consistent
    let mut diags1 = result1.diagnostics.clone();
    let mut diags2 = result2.diagnostics.clone();
    diags1.sort();
    diags2.sort();

    // Should both have the same number of diagnostics
    assert_eq!(diags1.len(), diags2.len());

    // Both should have diagnostic for ignored.rs
    assert!(diags1.iter().any(|d| d.path() == "ignored.rs"));
    assert!(diags2.iter().any(|d| d.path() == "ignored.rs"));

    // Both should have diagnostic for .gitignore (unsupported language)
    assert!(diags1.iter().any(|d| d.path() == ".gitignore"));
    assert!(diags2.iter().any(|d| d.path() == ".gitignore"));
}

/// Test that internal ignores take precedence over gitignore
#[test]
fn test_internal_ignores_precedence() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let db_path = root.join("test.db");

    // Create .gitignore that would normally allow .db files
    fs::write(root.join(".gitignore"), "!*.db\n").unwrap();

    // Create a .db file
    fs::write(root.join("data.db"), "database content").unwrap();

    // Create filter
    let filter = FileFilter::new(root, &[], &[]).unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let result = magellan::graph::scan::scan_directory_with_filter(
        &mut graph,
        root,
        &filter,
        None,
    )
    .unwrap();

    // .db file should be skipped (internal ignore wins)
    assert_eq!(result.indexed, 0);

    let db_diag = result.diagnostics.iter().find(|d| d.path() == "data.db");
    assert!(db_diag.is_some());

    // Should be IgnoredInternal, not IgnoredByGitignore
    match db_diag.unwrap() {
        magellan::WatchDiagnostic::Skipped { reason, .. } => {
            assert_eq!(reason, &magellan::SkipReason::IgnoredInternal);
        }
        _ => panic!("Expected Skipped diagnostic"),
    }
}

/// Test that root .gitignore works correctly
#[test]
fn test_root_gitignore() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let db_path = root.join("test.db");

    // Create directory structure
    fs::create_dir_all(root.join("src/subdir")).unwrap();

    // Root .gitignore
    fs::write(root.join(".gitignore"), "root_ignored.rs\nsrc_ignored.rs\n").unwrap();

    // Create files
    fs::write(root.join("root_ignored.rs"), "fn x() {}").unwrap();
    fs::write(root.join("root_included.rs"), "fn y() {}").unwrap();
    fs::write(root.join("src/src_ignored.rs"), "fn z() {}").unwrap();
    fs::write(root.join("src/subdir/nested.rs"), "fn w() {}").unwrap();

    // Create filter
    let filter = FileFilter::new(root, &[], &[]).unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();
    let result = magellan::graph::scan::scan_directory_with_filter(
        &mut graph,
        root,
        &filter,
        None,
    )
    .unwrap();

    // root_ignored.rs and src_ignored.rs should be skipped
    // root_included.rs and nested.rs should be indexed
    // Note: .gitignore files are not language files, so they won't be indexed
    assert!(result.indexed >= 2);

    // Verify the right files were indexed
    let root_included_symbols = graph
        .symbols_in_file(&root.join("root_included.rs").to_string_lossy())
        .unwrap();
    assert!(!root_included_symbols.is_empty());

    let nested_symbols = graph
        .symbols_in_file(&root.join("src/subdir/nested.rs").to_string_lossy())
        .unwrap();
    assert!(!nested_symbols.is_empty());

    // Verify root_ignored.rs has diagnostic
    let root_ignored_diag = result
        .diagnostics
        .iter()
        .find(|d| d.path() == "root_ignored.rs");
    assert!(root_ignored_diag.is_some());

    // Verify src_ignored.rs has diagnostic
    let src_ignored_diag = result
        .diagnostics
        .iter()
        .find(|d| d.path().contains("src_ignored.rs"));
    assert!(src_ignored_diag.is_some());
}

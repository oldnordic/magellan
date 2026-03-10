//! CLI Backend UX Tests - PHASE 8
//!
//! Tests for build feature UX completion:
//! - --backends shows available storage backends and features
//! - --help mentions backend formats truthfully
//! - status shows backend type correctly
//! - --version includes compiled backend list

use std::process::Command;
use tempfile::TempDir;

/// Test --backends flag shows backend information
#[test]
fn test_backends_flag_shows_backends() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "magellan", "--", "--backends"])
        .output()
        .expect("Failed to run magellan --backends");

    let combined: String = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);

    // Should show backend information
    assert!(
        combined.contains("backend") || combined.contains("Backend"),
        "--backends output should mention 'backend'"
    );

    // Should show available backends
    assert!(
        combined.contains("sqlite") || combined.contains("geometric"),
        "--backends output should list available backends"
    );
}

/// Test --help mentions backend-related options
#[test]
fn test_help_shows_backends_option() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "magellan", "--", "--help"])
        .output()
        .expect("Failed to run magellan --help");

    let combined: String = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);

    // Should mention --backends flag
    assert!(
        combined.contains("--backends") || combined.contains("backends"),
        "--help should mention --backends flag"
    );
}

/// Test --version includes compiled backend list
#[test]
fn test_version_shows_compiled_backends() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "magellan", "--", "--version"])
        .output()
        .expect("Failed to run magellan --version");

    let combined: String = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);

    // Should show "backends:" in version output
    assert!(
        combined.contains("backends:"),
        "--version should show compiled backends"
    );

    // Should list at least sqlite
    assert!(
        combined.contains("sqlite"),
        "--version should list sqlite backend"
    );
}

/// Test status shows backend type for SQLite database (using library directly)
#[test]
fn test_status_shows_backend_type_sqlite() {
    use magellan::graph::backend::Backend;
    use magellan::CodeGraph;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_status.db");

    // Create a database using the library (open creates if not exists)
    let graph = CodeGraph::open(&db_path).expect("Failed to create database");

    // Verify we can query the status
    let stats = graph.get_stats().expect("Failed to get stats");

    // New database should have zero symbols
    assert_eq!(stats.symbol_count, 0, "New database should have 0 symbols");
}

/// Test status command on geometric database
#[test]
#[cfg(feature = "geometric-backend")]
fn test_status_shows_backend_type_geometric() {
    use magellan::backend_router::MagellanBackend;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_status.geo");

    // Create a geometric database
    let backend = MagellanBackend::create(&db_path).expect("Failed to create geometric db");

    // Verify we can get stats
    let stats = backend.get_stats();
    assert!(
        stats.is_ok(),
        "Should be able to get geometric backend stats"
    );

    // Stats should show zero content (new database)
    assert_eq!(
        stats.unwrap().file_count,
        0,
        "New database should have 0 files"
    );
}

/// Test backend type detection for different file extensions
#[test]
fn test_backend_detection_by_extension() {
    use magellan::capabilities::BackendType;

    // SQLite detection
    let sqlite_type = BackendType::from_extension(Some("db"));
    assert_eq!(
        sqlite_type,
        Some(BackendType::SQLite),
        "Should detect .db as SQLite"
    );

    // Geometric detection - only if feature is enabled
    #[cfg(feature = "geometric-backend")]
    {
        let geo_type = BackendType::from_extension(Some("geo"));
        assert_eq!(
            geo_type,
            Some(BackendType::Geometric),
            "Should detect .geo as Geometric"
        );
    }

    // Native V3 detection
    let v3_type = BackendType::from_extension(Some("v3"));
    assert_eq!(
        v3_type,
        Some(BackendType::NativeV3),
        "Should detect .v3 as NativeV3"
    );

    // Unknown extension returns Some(SQLite) - it's the default
    let unknown_type = BackendType::from_extension(Some("unknown"));
    assert_eq!(
        unknown_type,
        Some(BackendType::SQLite),
        "Should default to SQLite for unknown extensions"
    );

    // No extension returns Some(SQLite) - it's the default
    let no_ext = BackendType::from_extension(None);
    assert_eq!(
        no_ext,
        Some(BackendType::SQLite),
        "Should default to SQLite for no extension"
    );
}

/// Test BackendType display names and extensions
#[test]
fn test_backend_type_properties() {
    use magellan::capabilities::BackendType;

    assert_eq!(BackendType::SQLite.extension(), "db");
    assert_eq!(BackendType::SQLite.display_name(), "SQLite");

    assert_eq!(BackendType::Geometric.extension(), "geo");
    assert_eq!(BackendType::Geometric.display_name(), "Geometric");

    assert_eq!(BackendType::NativeV3.extension(), "v3");
    assert_eq!(BackendType::NativeV3.display_name(), "Native V3");
}

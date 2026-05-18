//! Watch-mode integration tests for .magellan.toml config support.
//!
//! Verifies that project config correctly filters paths during scan,
//! and that backward compatibility is maintained when no config exists.

use std::fs;

use magellan::project_config::ProjectConfig;
use magellan::CodeGraph;

fn create_temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let tests = dir.path().join("tests");
    let generated = dir.path().join("src").join("generated");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&tests).unwrap();
    fs::create_dir_all(&generated).unwrap();

    fs::write(src.join("main.rs"), "fn main() {}").unwrap();
    fs::write(src.join("lib.rs"), "pub fn hello() {}").unwrap();
    fs::write(
        generated.join("auto.rs"),
        "// generated\nfn generated_fn() {}",
    )
    .unwrap();
    fs::write(tests.join("test_main.rs"), "#[test] fn test_main() {}").unwrap();

    dir
}

/// Test: .magellan.toml with include/exclude correctly filters files.
#[test]
fn watch_with_magellan_toml_filters_paths() {
    let dir = create_temp_project();

    let config_toml = r#"
[index]
include = ["src/"]
exclude = ["src/generated/**"]
"#;
    fs::write(dir.path().join(".magellan.toml"), config_toml).unwrap();

    let config = ProjectConfig::load(dir.path()).unwrap();
    assert!(config.index.include.contains(&"src/".to_string()));
    assert!(config
        .index
        .exclude
        .contains(&"src/generated/**".to_string()));

    let filter = config.to_file_filter(dir.path()).unwrap();

    // src/main.rs should NOT be skipped
    let main_rs = dir.path().join("src").join("main.rs");
    assert!(
        filter.should_skip(&main_rs).is_none(),
        "src/main.rs should be included"
    );

    // src/generated/auto.rs should BE skipped
    let auto_rs = dir.path().join("src").join("generated").join("auto.rs");
    assert!(
        filter.should_skip(&auto_rs).is_some(),
        "src/generated/auto.rs should be excluded"
    );

    // tests/test_main.rs should BE skipped (not in include list)
    let test_rs = dir.path().join("tests").join("test_main.rs");
    assert!(
        filter.should_skip(&test_rs).is_some(),
        "tests/test_main.rs should be skipped (not in include patterns)"
    );
}

/// Test: Without .magellan.toml, behavior matches pre-v4 (backward compat).
#[test]
fn watch_without_config_uses_defaults() {
    let dir = create_temp_project();

    let config = ProjectConfig::load(dir.path()).unwrap();
    assert!(config.project.name.is_none());
    assert!(config.index.include.is_empty()); // empty = include all
    assert!(config.index.exclude.is_empty());

    let filter = config.to_file_filter(dir.path()).unwrap();

    let main_rs = dir.path().join("src").join("main.rs");
    assert!(
        filter.should_skip(&main_rs).is_none(),
        "src/main.rs should be included by default (empty include = all)"
    );

    let test_rs = dir.path().join("tests").join("test_main.rs");
    assert!(
        filter.should_skip(&test_rs).is_none(),
        "tests/test_main.rs should be included (empty include = all)"
    );
}

/// Test: scan_directory_with_filter respects config-driven filter.
#[test]
fn scan_with_config_filter_indexes_correct_files() {
    let dir = create_temp_project();

    let config = ProjectConfig {
        index: magellan::project_config::IndexSection {
            include: vec!["src/".into()],
            exclude: vec!["src/generated/**".into()],
        },
        ..Default::default()
    };
    let filter = config.to_file_filter(dir.path()).unwrap();

    let db_path = dir.path().join(".magellan").join("test.db");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let result = graph
        .scan_directory_with_filter(dir.path(), &filter, None)
        .unwrap();

    assert!(
        result.indexed >= 2,
        "Should index at least 2 files (main.rs, lib.rs), got {}",
        result.indexed
    );

    // Verify src/generated/auto.rs is NOT in the database
    let files = graph.all_file_nodes().unwrap();
    let has_generated = files.keys().any(|p| p.contains("generated"));
    assert!(
        !has_generated,
        "src/generated/ should be excluded, but found in: {:?}",
        files.keys().collect::<Vec<_>>()
    );
}

/// Test: Config with tests/ in include list indexes test files.
#[test]
fn scan_with_tests_included_indexes_test_files() {
    let dir = create_temp_project();

    let config = ProjectConfig {
        index: magellan::project_config::IndexSection {
            include: vec!["src/".into(), "tests/".into()],
            exclude: vec![],
        },
        ..Default::default()
    };
    let filter = config.to_file_filter(dir.path()).unwrap();

    let db_path = dir.path().join(".magellan").join("test.db");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let mut graph = CodeGraph::open(&db_path).unwrap();

    graph
        .scan_directory_with_filter(dir.path(), &filter, None)
        .unwrap();

    let files = graph.all_file_nodes().unwrap();
    let has_test = files.keys().any(|p| p.contains("test_main"));
    assert!(
        has_test,
        "tests/test_main.rs should be indexed, files: {:?}",
        files.keys().collect::<Vec<_>>()
    );
}

/// Test: Cargo.toml manifest is stored in magellan_meta during watch.
#[test]
fn cargo_manifest_stored_in_magellan_meta() {
    let dir = create_temp_project();

    let cargo_toml = r#"
[package]
name = "test-crate"
version = "0.1.0"
edition = "2021"

[features]
default = ["sqlite-backend"]
sqlite-backend = []

[dependencies]
anyhow = "1.0"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let db_path = dir.path().join(".magellan").join("test.db");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let _graph = CodeGraph::open(&db_path).unwrap();

    // Simulate what watch pipeline does: parse manifest and store in DB
    let manifest = magellan::project_config::CargoManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.package_name, Some("test-crate".to_string()));

    // Store in magellan_meta via direct SQL (watch pipeline does this)
    let metadata_json = serde_json::to_string(&manifest).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE magellan_meta SET project_name = ?1, project_metadata = ?2 WHERE id = 1",
        rusqlite::params![manifest.package_name, metadata_json],
    )
    .unwrap();

    // Verify round-trip
    let (name, meta_json): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT project_name, project_metadata FROM magellan_meta WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(name, Some("test-crate".to_string()));
    let loaded: magellan::project_config::CargoManifest =
        serde_json::from_str(&meta_json.unwrap()).unwrap();
    assert_eq!(loaded.package_name, Some("test-crate".to_string()));
    assert!(loaded.dependencies.contains(&"anyhow".to_string()));
}

/// Test: ProjectConfig round-trips through TOML serialization.
#[test]
fn config_round_trip() {
    let dir = tempfile::tempdir().unwrap();

    let original = ProjectConfig {
        project: magellan::project_config::ProjectSection {
            name: Some("test-project".into()),
        },
        index: magellan::project_config::IndexSection {
            include: vec!["src/".into(), "tests/".into()],
            exclude: vec!["src/generated/**".into()],
        },
        watch: magellan::project_config::WatchSection {
            debounce_ms: 1000,
            gitignore_aware: false,
            scan_initial: true,
        },
    };

    let toml_str = toml::to_string_pretty(&original).unwrap();
    fs::write(dir.path().join(".magellan.toml"), &toml_str).unwrap();

    let loaded = ProjectConfig::load(dir.path()).unwrap();
    assert_eq!(loaded.project.name, Some("test-project".into()));
    assert_eq!(loaded.index.include, vec!["src/", "tests/"]);
    assert_eq!(loaded.index.exclude, vec!["src/generated/**"]);
    assert_eq!(loaded.watch.debounce_ms, 1000);
    assert!(!loaded.watch.gitignore_aware);
    assert!(loaded.watch.scan_initial);
}

//! Tests for JSON export functionality
//!
//! TDD Phase 5.5: JSON Export

use magellan::graph::export::{
    export_graph, export_jsonl, stream_json, stream_json_minified, stream_ndjson, ExportConfig,
    ExportFormat,
};
use magellan::CodeGraph;
use tempfile::TempDir;

// ============================================================================
// Migration Tests
// ============================================================================

#[test]
fn test_migration_creates_backup() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create an old database (version 3)
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
         VALUES (1, 3, 1, 1000000000)",
        [],
    ).unwrap();
    drop(conn);

    // Run migration
    let result = magellan::migrate_cmd::run_migrate(db_path.clone(), false, false).unwrap();

    assert!(result.success);
    assert_eq!(result.old_version, 3);
    assert_eq!(result.new_version, magellan::migrate_cmd::MAGELLAN_SCHEMA_VERSION);
    assert!(result.backup_path.is_some());

    // Verify backup exists
    let backup_path = result.backup_path.unwrap();
    assert!(backup_path.exists());
    assert!(backup_path.to_string_lossy().contains(".bak"));
}

#[test]
fn test_migration_dry_run() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create an old database (version 3)
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
         VALUES (1, 3, 1, 1000000000)",
        [],
    ).unwrap();
    drop(conn);

    // Run dry-run migration
    let result = magellan::migrate_cmd::run_migrate(db_path.clone(), true, false).unwrap();

    assert!(result.success);
    assert_eq!(result.old_version, 3);
    assert!(result.message.contains("dry run"));
    assert!(result.backup_path.is_none());

    // Verify version didn't change
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: i64 = conn.query_row(
        "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(version, 3);
}

#[test]
fn test_migration_already_current() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a current database
    let graph = magellan::CodeGraph::open(&db_path).unwrap();
    drop(graph);

    // Run migration
    let result = magellan::migrate_cmd::run_migrate(db_path, false, false).unwrap();

    assert!(result.success);
    assert!(result.message.contains("already at current version"));
    assert!(result.backup_path.is_none());
}

#[test]
fn test_migration_nonexistent_database() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("nonexistent.db");

    // Run migration on non-existent database
    let result = magellan::migrate_cmd::run_migrate(db_path, false, false).unwrap();

    assert!(!result.success);
    assert!(result.message.contains("not found"));
}

#[test]
fn test_migration_no_backup_flag() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create an old database (version 3)
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
         VALUES (1, 3, 1, 1000000000)",
        [],
    ).unwrap();
    drop(conn);

    // Run migration with --no-backup
    let result = magellan::migrate_cmd::run_migrate(db_path.clone(), false, true).unwrap();

    assert!(result.success);
    assert!(result.backup_path.is_none());

    // Verify no backup file created
    let backups = std::fs::read_dir(temp_dir.path()).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "bak").unwrap_or(false))
        .count();
    assert_eq!(backups, 0);
}

#[test]
fn test_code_graph_exports_to_json() {
    // Create temporary database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create test source code
    let source = r#"
fn main() {
    println!("Hello");
    helper();
}

fn helper() {
    println!("Helper");
}
"#;

    // Index the file
    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();
    graph
        .index_references("test.rs", source.as_bytes())
        .unwrap();

    // Export to JSON
    let json = graph.export_json().unwrap();

    // Verify JSON is valid
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Check structure
    assert!(parsed.is_object(), "Root should be an object");
    assert!(parsed.get("files").is_some(), "Should have 'files' field");
    assert!(
        parsed.get("symbols").is_some(),
        "Should have 'symbols' field"
    );
    assert!(
        parsed.get("references").is_some(),
        "Should have 'references' field"
    );
}

#[test]
fn test_export_json_includes_file_details() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"fn test() {}"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    let json = graph.export_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let files = &parsed["files"];
    assert!(files.is_array(), "files should be an array");

    if files.as_array().unwrap().len() > 0 {
        let file = &files[0];
        assert!(file.get("path").is_some(), "File should have 'path'");
        assert!(file.get("hash").is_some(), "File should have 'hash'");
    }
}

#[test]
fn test_export_json_includes_symbol_details() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    println!("Hello");
}

struct MyStruct {
    field: i32,
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    let json = graph.export_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let symbols = &parsed["symbols"];
    assert!(symbols.is_array(), "symbols should be an array");

    // Should have at least main and MyStruct
    let symbol_count = symbols.as_array().unwrap().len();
    assert!(
        symbol_count >= 2,
        "Should have at least 2 symbols (main, MyStruct), got {}",
        symbol_count
    );

    // Check symbol structure
    for symbol in symbols.as_array().unwrap() {
        assert!(symbol.get("name").is_some(), "Symbol should have 'name'");
        assert!(symbol.get("kind").is_some(), "Symbol should have 'kind'");
        assert!(symbol.get("file").is_some(), "Symbol should have 'file'");
        assert!(
            symbol.get("start_line").is_some(),
            "Symbol should have 'start_line'"
        );
        assert!(
            symbol.get("end_line").is_some(),
            "Symbol should have 'end_line'"
        );
    }
}

#[test]
fn test_export_json_includes_call_details() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    helper();
}

fn helper() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    let json = graph.export_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let calls = &parsed["calls"];
    assert!(calls.is_array(), "calls should be an array");

    // Should have main -> helper call
    let call_count = calls.as_array().unwrap().len();
    assert!(
        call_count >= 1,
        "Should have at least 1 call (main -> helper), got {}",
        call_count
    );

    // Check call structure
    for call in calls.as_array().unwrap() {
        assert!(call.get("caller").is_some(), "Call should have 'caller'");
        assert!(call.get("callee").is_some(), "Call should have 'callee'");
        assert!(call.get("file").is_some(), "Call should have 'file'");
        assert!(
            call.get("start_line").is_some(),
            "Call should have 'start_line'"
        );
    }
}

/// Test that stream_json produces identical output to export_json
#[test]
fn test_stream_json_matches_export_json() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    println!("Hello");
    helper();
}

fn helper() {
    println!("Helper");
}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();
    graph
        .index_references("test.rs", source.as_bytes())
        .unwrap();

    // Get in-memory export
    let json = graph.export_json().unwrap();

    // Get streaming export
    let mut buffer = Vec::new();
    let config = ExportConfig::new(ExportFormat::Json);
    stream_json(&mut graph, &config, &mut buffer).unwrap();
    let streamed_json = String::from_utf8(buffer).unwrap();

    // Outputs should be identical (same deterministic sorting)
    assert_eq!(
        json, streamed_json,
        "stream_json should produce identical output to export_json"
    );
}

/// Test that stream_json_minified produces compact JSON
#[test]
fn test_stream_json_minified_compact() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"fn test() {}"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    // Get minified streaming export
    let mut buffer = Vec::new();
    let config = ExportConfig::new(ExportFormat::Json);
    stream_json_minified(&mut graph, &config, &mut buffer).unwrap();
    let minified = String::from_utf8(buffer).unwrap();

    // Minified JSON should have minimal whitespace (fewer newlines than pretty)
    let newline_count = minified.chars().filter(|&c| c == '\n').count();
    assert!(
        newline_count <= 1,
        "Minified JSON should have at most 1 newline, got {}",
        newline_count
    );

    // Should still be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&minified).unwrap();
    assert!(
        parsed.is_object(),
        "Minified output should be valid JSON object"
    );
}

/// Test that stream_ndjson produces valid JSONL output
#[test]
fn test_stream_ndjson_valid_format() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    helper();
}

fn helper() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    // Get streaming NDJSON export
    let mut buffer = Vec::new();
    let config = ExportConfig::new(ExportFormat::JsonL);
    stream_ndjson(&mut graph, &config, &mut buffer).unwrap();
    let ndjson = String::from_utf8(buffer).unwrap();

    // NDJSON should have one JSON object per line
    let lines: Vec<&str> = ndjson.lines().collect();
    assert!(lines.len() > 0, "NDJSON should have at least one line");

    // Each line should be valid JSON
    for (i, line) in lines.iter().enumerate() {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!(
                "Line {} should be valid JSON: {}\nLine content: '{}'",
                i, e, line
            );
        });
        assert!(parsed.is_object(), "Each line should be a JSON object");
    }
}

/// Test that stream_json respects include_symbols filter
#[test]
fn test_stream_json_respects_symbols_filter() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"fn test() {}"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    // Export without symbols
    let mut buffer = Vec::new();
    let config = ExportConfig {
        format: ExportFormat::Json,
        include_symbols: false,
        include_references: true,
        include_calls: true,
        minify: true,
        filters: Default::default(),
        include_collisions: false,
        collisions_field: magellan::graph::query::CollisionField::Fqn,
    };
    stream_json_minified(&mut graph, &config, &mut buffer).unwrap();
    let json = String::from_utf8(buffer).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let symbols = &parsed["symbols"];
    assert!(
        symbols.as_array().unwrap().is_empty(),
        "Symbols should be empty when include_symbols=false"
    );
}

/// Test that stream_json respects include_calls filter
#[test]
fn test_stream_json_respects_calls_filter() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let source = r#"
fn main() {
    helper();
}

fn helper() {}
"#;

    let mut graph = CodeGraph::open(&db_path).unwrap();
    graph.index_file("test.rs", source.as_bytes()).unwrap();

    // Export without calls
    let mut buffer = Vec::new();
    let config = ExportConfig {
        format: ExportFormat::Json,
        include_symbols: true,
        include_references: true,
        include_calls: false,
        minify: true,
        filters: Default::default(),
        include_collisions: false,
        collisions_field: magellan::graph::query::CollisionField::Fqn,
    };
    stream_json_minified(&mut graph, &config, &mut buffer).unwrap();
    let json = String::from_utf8(buffer).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let calls = &parsed["calls"];
    assert!(
        calls.as_array().unwrap().is_empty(),
        "Calls should be empty when include_calls=false"
    );
}

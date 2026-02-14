//! Cross-backend migration integration tests
//!
//! Tests the migration pipeline between SQLite databases:
//! - Creating sample SQLite databases with graph data
//! - Running migration via run_migrate_backend()
//! - Verifying data preservation (entity/edge counts, side tables)
//!
//! This is a TDD test: initially fails because migration implementation
//! is incomplete, then passes after plans 47-01 through 47-04 complete.

use anyhow::Result;
use magellan::migrate_backend_cmd::{run_migrate_backend, BackendFormat, BackendMigrationResult};
use magellan::CodeGraph;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

/// Test helper: Get entity and edge counts from a database
///
/// Opens the database and queries graph_entities and graph_edges tables.
/// Returns tuple of (entity_count, edge_count).
fn get_graph_counts(db_path: &Path) -> Result<(i64, i64)> {
    let conn = rusqlite::Connection::open(db_path)?;

    let entity_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM graph_entities", [], |row| row.get(0))
        .unwrap_or(0);

    let edge_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM graph_edges", [], |row| row.get(0))
        .unwrap_or(0);

    Ok((entity_count, edge_count))
}

/// Test helper: Get row counts for all Magellan side tables
///
/// Returns HashMap with table name -> row count for:
/// - code_chunks
/// - file_metrics
/// - symbol_metrics
/// - execution_log
/// - ast_nodes
/// - cfg_blocks
fn get_side_table_counts(db_path: &Path) -> Result<HashMap<String, i64>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let mut counts = HashMap::new();

    let side_tables = [
        "code_chunks",
        "file_metrics",
        "symbol_metrics",
        "execution_log",
        "ast_nodes",
        "cfg_blocks",
    ];

    for table in side_tables {
        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", table),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        counts.insert(table.to_string(), count);
    }

    Ok(counts)
}

/// Test helper: Ensure metrics schema exists
///
/// Creates file_metrics and symbol_metrics tables if they don't exist.
/// Mirrors magellan::graph::db_compat::ensure_metrics_schema.
fn ensure_metrics_schema(conn: &rusqlite::Connection) -> Result<()> {
    // File-level metrics table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_metrics (
            file_path TEXT PRIMARY KEY,
            symbol_count INTEGER NOT NULL,
            loc INTEGER NOT NULL,
            estimated_loc REAL NOT NULL,
            fan_in INTEGER NOT NULL DEFAULT 0,
            fan_out INTEGER NOT NULL DEFAULT 0,
            complexity_score REAL NOT NULL DEFAULT 0.0,
            last_updated INTEGER NOT NULL
        )",
        [],
    )?;

    // Symbol-level metrics table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbol_metrics (
            symbol_id INTEGER PRIMARY KEY,
            symbol_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            file_path TEXT NOT NULL,
            loc INTEGER NOT NULL,
            estimated_loc REAL NOT NULL,
            fan_in INTEGER NOT NULL DEFAULT 0,
            fan_out INTEGER NOT NULL DEFAULT 0,
            cyclomatic_complexity INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            FOREIGN KEY (symbol_id) REFERENCES graph_entities(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // Indexes
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_metrics_complexity ON file_metrics(complexity_score DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_in ON symbol_metrics(fan_in DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_fan_out ON symbol_metrics(fan_out DESC)",
        [],
    )?;

    Ok(())
}

/// Test helper: Ensure AST schema exists
///
/// Creates ast_nodes table if it doesn't exist.
/// Mirrors magellan::graph::db_compat::ensure_ast_schema.
fn ensure_ast_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ast_nodes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent_id INTEGER,
            kind TEXT NOT NULL,
            byte_start INTEGER NOT NULL,
            byte_end INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent ON ast_nodes(parent_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span ON ast_nodes(byte_start, byte_end)",
        [],
    )?;

    Ok(())
}

/// Test helper: Ensure CFG schema exists
///
/// Creates cfg_blocks table if it doesn't exist.
/// Mirrors magellan::graph::db_compat::ensure_cfg_schema.
fn ensure_cfg_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cfg_blocks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            function_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            terminator TEXT NOT NULL,
            byte_start INTEGER NOT NULL,
            byte_end INTEGER NOT NULL,
            start_line INTEGER NOT NULL,
            start_col INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            end_col INTEGER NOT NULL,
            FOREIGN KEY (function_id) REFERENCES graph_entities(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function ON cfg_blocks(function_id)",
        [],
    )?;

    Ok(())
}

#[test]
fn test_round_trip_migration_preserves_data() {
    // Create temp directory for test databases
    let temp_dir = TempDir::new().unwrap();
    let source_db = temp_dir.path().join("source.db");
    let target_db = temp_dir.path().join("target.db");

    // Step 1: Create source SQLite database with sample data
    let source_code_1 = r#"
pub fn main() {
    println!("Hello, world");
    helper();
}

pub fn helper() {
    println!("Helper function");
}
"#;

    let source_code_2 = r#"
pub struct MyStruct {
    pub value: i32,
}

impl MyStruct {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
}
"#;

    // Create source graph and index files
    {
        let mut graph = CodeGraph::open(&source_db).unwrap();

        // Index two files to create File nodes, Symbol nodes, and DEFINES edges
        let file1_path = "src/main.rs";
        let file2_path = "src/lib.rs";

        graph.index_file(file1_path, source_code_1.as_bytes()).unwrap();
        graph.index_file(file2_path, source_code_2.as_bytes()).unwrap();

        // Verify we indexed some symbols
        let symbol_count = graph.count_symbols().unwrap();
        assert!(symbol_count > 0, "Should have indexed at least one symbol");
    }

    // Step 2: Add side table data directly via rusqlite
    {
        let conn = rusqlite::Connection::open(&source_db).unwrap();

        // Ensure schemas exist
        ensure_metrics_schema(&conn).unwrap();
        ensure_ast_schema(&conn).unwrap();
        ensure_cfg_schema(&conn).unwrap();

        // Note: code_chunks schema is automatically created by CodeGraph::open
        // via ChunkStore. We don't need to create it here.

        // Insert code_chunks (1 row) - using actual schema
        conn.execute(
            "INSERT INTO code_chunks (file_path, byte_start, byte_end, content, content_hash, symbol_name, symbol_kind, created_at)
             VALUES ('src/main.rs', 0, 100, 'pub fn main() { ... }', 'abc123', 'main', 'Function', 1000)",
            [],
        ).unwrap();

        // Insert file_metrics (2 rows) - use INSERT OR REPLACE in case backfill created entries
        conn.execute(
            "INSERT OR REPLACE INTO file_metrics (file_path, symbol_count, loc, estimated_loc, fan_in, fan_out, complexity_score, last_updated)
             VALUES ('src/main.rs', 3, 20, 20.0, 0, 1, 1.0, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO file_metrics (file_path, symbol_count, loc, estimated_loc, fan_in, fan_out, complexity_score, last_updated)
             VALUES ('src/lib.rs', 2, 15, 15.0, 1, 0, 0.5, 1000)",
            [],
        ).unwrap();

        // Insert symbol_metrics (3 rows) - need actual symbol IDs from graph
        // First, get symbol IDs from the graph
        let symbol_ids: Vec<i64> = conn
            .query_row(
                "SELECT GROUP_CONCAT(id) FROM graph_entities WHERE kind='Symbol'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| {
                let ids: Vec<i64> = s.split(',').filter_map(|x| x.parse().ok()).collect();
                if ids.len() >= 3 { Some(ids) } else { None }
            })
            .unwrap_or_else(|| vec![100, 101, 102]); // Fallback IDs

        for (i, symbol_id) in symbol_ids.iter().take(3).enumerate() {
            let name = match i {
                0 => "main",
                1 => "helper",
                2 => "MyStruct",
                _ => "unknown",
            };
            // Use &[&dyn ToSql] for dynamic parameter types
            use rusqlite::params;
            conn.execute(
                "INSERT OR REPLACE INTO symbol_metrics (symbol_id, symbol_name, kind, file_path, loc, estimated_loc, fan_in, fan_out, cyclomatic_complexity, last_updated)
                 VALUES (?1, ?2, 'Function', 'src/main.rs', 10, 10.0, 0, 1, 1, 1000)",
                params![symbol_id, name],
            ).unwrap();
        }

        // Insert execution_log (1 row) - using actual schema
        conn.execute(
            "INSERT INTO execution_log (execution_id, tool_version, args, db_path, started_at, finished_at, duration_ms, outcome, files_indexed, symbols_indexed, references_indexed)
             VALUES ('test-exec-1', '2.1.0', '[\"scan\"]', '/test.db', 1000, 1100, 100, 'success', 2, 5, 8)",
            [],
        ).unwrap();

        // Insert ast_nodes (4 rows)
        conn.execute(
            "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end) VALUES (NULL, 'SourceFile', 0, 200)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end) VALUES (1, 'FunctionDeclaration', 0, 50)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end) VALUES (1, 'FunctionDeclaration', 52, 100)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end) VALUES (2, 'BlockExpression', 12, 48)",
            [],
        ).unwrap();

        // Insert cfg_blocks (2 rows) - need function IDs
        let func_id = symbol_ids.first().copied().unwrap_or(100);
        conn.execute(
            "INSERT INTO cfg_blocks (function_id, kind, terminator, byte_start, byte_end, start_line, start_col, end_line, end_col)
             VALUES (?1, 'Entry', 'None', 0, 10, 1, 0, 1, 10)",
            [func_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO cfg_blocks (function_id, kind, terminator, byte_start, byte_end, start_line, start_col, end_line, end_col)
             VALUES (?1, 'Return', 'Return', 11, 20, 2, 4, 2, 20)",
            [func_id],
        ).unwrap();
    }

    // Step 3: Get baseline counts from source database
    let (source_entities, source_edges) = get_graph_counts(&source_db).unwrap();
    let source_side_counts = get_side_table_counts(&source_db).unwrap();

    println!("Source database:");
    println!("  Entities: {}", source_entities);
    println!("  Edges: {}", source_edges);
    println!("  Side tables:");
    for (table, count) in &source_side_counts {
        println!("    {}: {}", table, count);
    }

    // Step 4: Run migration
    let result: Result<BackendMigrationResult> =
        run_migrate_backend(source_db.clone(), target_db.clone(), None, false);

    // Step 5: Verify migration succeeded
    // Note: This test is designed to fail initially (RED phase of TDD)
    // It will pass after plans 47-01 through 47-04 complete
    match result {
        Ok(migration_result) => {
            assert!(migration_result.success, "Migration should succeed");

            // Step 6: Get migrated counts
            let (target_entities, target_edges) = get_graph_counts(&target_db).unwrap();
            let target_side_counts = get_side_table_counts(&target_db).unwrap();

            println!("\nTarget database (migrated):");
            println!("  Entities: {}", target_entities);
            println!("  Edges: {}", target_edges);
            println!("  Side tables:");
            for (table, count) in &target_side_counts {
                println!("    {}: {}", table, count);
            }

            // Verify entity/edge counts match
            assert_eq!(
                source_entities, target_entities,
                "Entity count should match after migration"
            );
            assert_eq!(
                source_edges, target_edges,
                "Edge count should match after migration"
            );

            // Verify side table counts match
            for table in ["code_chunks", "file_metrics", "symbol_metrics", "execution_log", "ast_nodes", "cfg_blocks"] {
                let source_count = *source_side_counts.get(table).unwrap_or(&0);
                let target_count = *target_side_counts.get(table).unwrap_or(&0);
                assert_eq!(
                    source_count, target_count,
                    "{} count should match after migration (source={}, target={})",
                    table, source_count, target_count
                );
            }

            // Verify target format is SQLite
            assert_eq!(
                BackendFormat::Sqlite,
                migration_result.target_format,
                "Target format should be SQLite"
            );

            println!("\nMigration successful! All data preserved.");
        }
        Err(e) => {
            // During RED phase (before 47-01 through 47-04 complete),
            // we expect migration to fail
            panic!("Migration failed (expected during RED phase): {}", e);
        }
    }
}

#[test]
fn test_migration_detects_sqlite_format() {
    use magellan::migrate_backend_cmd::detect_backend_format;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a SQLite database
    {
        let _graph = CodeGraph::open(&db_path).unwrap();
    }

    // Detect format
    let format = detect_backend_format(&db_path).unwrap();
    assert_eq!(format, BackendFormat::Sqlite);
}

#[test]
fn test_migration_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let source_db = temp_dir.path().join("source.db");
    let target_db = temp_dir.path().join("target.db");

    // Create source database
    {
        let _graph = CodeGraph::open(&source_db).unwrap();
    }

    // Run dry-run migration (clone target_db since run_migrate_backend takes ownership)
    let result = run_migrate_backend(source_db, target_db.clone(), None, true).unwrap();

    assert!(result.success);
    assert!(!target_db.exists(), "Target should not be created in dry-run mode");
    assert_eq!(result.entities_migrated, 0);
    assert_eq!(result.edges_migrated, 0);
    assert!(!result.side_tables_migrated);
}

/// Test: Migration preserves multi-byte UTF-8 chunk content byte-identically.
#[test]
fn test_migration_preserves_chunk_content() {
    use magellan::generation::{ChunkStore, CodeChunk};
    

    let temp_dir = TempDir::new().unwrap();
    let source_db = temp_dir.path().join("source.db");
    let native_db = temp_dir.path().join("native.db");

    // Create SQLite database with UTF-8 content
    let utf8_content = "pub fn 日本語 function() { let emoji = \"😀🚀\"; }";

    {
        let chunk_store = ChunkStore::new(&source_db);
        chunk_store.ensure_schema().unwrap();

        let chunk = CodeChunk::new(
            "test.rs".to_string(),
            0,
            utf8_content.len(),
            utf8_content.to_string(),
            Some("test".to_string()),
            Some("Function".to_string()),
        );
        chunk_store.store_chunk(&chunk).unwrap();
    }

    // Get original content from SQLite
    let _original_content: String = {
        let conn = rusqlite::Connection::open(&source_db).unwrap();
        let mut stmt = conn.prepare("SELECT content FROM code_chunks WHERE file_path = 'test.rs'").unwrap();
        stmt.query_row([], |row| row.get(0)).unwrap()
    };

    // Run migration
    run_migrate_backend(source_db, native_db.clone(), None, false).unwrap();

    // Verify SQLite side table copy worked
    let conn = rusqlite::Connection::open(&native_db).unwrap();
    let mut stmt = conn.prepare("SELECT content FROM code_chunks WHERE file_path = 'test.rs'").unwrap();
    let content: String = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(content, utf8_content, "UTF-8 content should be preserved");
}



/// Test: Cross-file reference indexing (XREF-01 requirement)
///
/// Verifies that references across file boundaries are indexed correctly:
/// 1. Index multiple files with cross-file symbol references
/// 2. Query references_to_symbol for a symbol
/// 3. Verify references exist from multiple files
#[test]
fn test_cross_file_reference_indexing() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create three Rust files with cross-file references
    // helper.rs: defines helper() function
    // lib.rs: defines and calls helper()
    // main.rs: calls helper() from lib.rs
    let helper_rs = r#"
pub fn helper() -> i32 {
    42
}
"#;

    let lib_rs = r#"
pub use crate::helper::helper;

pub fn lib_function() {
    let x = helper();
}
"#;

    let main_rs = r#"
use mycrate::helper;

fn main() {
    let y = helper();
}
"#;

    // Index all files with symbols and references
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index helper.rs first (defines the symbol)
        let helper_path = temp_dir.path().join("src/helper.rs");
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        std::fs::write(&helper_path, helper_rs).unwrap();
        let helper_path_str = helper_path.to_string_lossy().to_string();
        let helper_source = std::fs::read(&helper_path).unwrap();
        graph.index_file(&helper_path_str, &helper_source).unwrap();

        // Index lib.rs (defines and calls helper)
        let lib_path = temp_dir.path().join("src/lib.rs");
        std::fs::write(&lib_path, lib_rs).unwrap();
        let lib_path_str = lib_path.to_string_lossy().to_string();
        let lib_source = std::fs::read(&lib_path).unwrap();
        graph.index_file(&lib_path_str, &lib_source).unwrap();
        graph.index_references(&lib_path_str, &lib_source).unwrap();

        // Index main.rs (calls helper)
        let main_path = temp_dir.path().join("src/main.rs");
        std::fs::write(&main_path, main_rs).unwrap();
        let main_path_str = main_path.to_string_lossy().to_string();
        let main_source = std::fs::read(&main_path).unwrap();
        graph.index_file(&main_path_str, &main_source).unwrap();
        graph.index_references(&main_path_str, &main_source).unwrap();
    }

    // Now query references to helper() symbol
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Find the helper symbol node ID
        let helper_path_str = temp_dir.path().join("src/helper.rs").to_string_lossy().to_string();
        let symbols = magellan::graph::query::symbols_in_file(&mut graph, &helper_path_str).unwrap();

        let _helper_symbol = symbols
            .iter()
            .find(|s| s.name.as_deref() == Some("helper"))
            .expect("helper symbol should exist");

        // Get the node ID for helper symbol
        let symbol_id = magellan::graph::query::symbol_id_by_name(
            &mut graph,
            &helper_path_str,
            "helper"
        ).unwrap().expect("helper symbol should have node ID");

        // Query all references to helper
        let references = magellan::graph::query::references_to_symbol(&mut graph, symbol_id).unwrap();

        // Verify we have references from multiple files
        // lib.rs should reference helper
        let lib_refs: Vec<_> = references
            .iter()
            .filter(|r| r.file_path.ends_with("src/lib.rs"))
            .collect();

        // main.rs should reference helper
        let main_refs: Vec<_> = references
            .iter()
            .filter(|r| r.file_path.ends_with("src/main.rs"))
            .collect();

        // Assert we found cross-file references
        assert!(
            !lib_refs.is_empty() || !main_refs.is_empty(),
            "Expected at least one cross-file reference to helper(), found {}",
            references.len()
        );

        println!("Cross-file references to helper():");
        for ref_fact in &references {
            println!(
                "  {}:{}:{} -> {}",
                ref_fact.file_path.display(),
                ref_fact.start_line,
                ref_fact.start_col,
                ref_fact.referenced_symbol
            );
        }
    }
}

/// Test: Multi-file refs command (XREF-01 requirement)
///
/// Verifies that refs command returns references from multiple files:
/// 1. Create three files with cross-file references
/// 2. Index all files
/// 3. Run refs command on a symbol
/// 4. Verify refs shows references from multiple files with correct file_path
#[test]
fn test_refs_command_multi_file() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create three files with cross-file references
    let helper_rs = r#"
pub fn helper() -> i32 {
    42
}
"#;

    let lib_rs = r#"
pub fn call_helper() {
    let x = helper();
}

pub fn helper() -> i32 {
    43
}
"#;

    let other_rs = r#"
pub fn other_function() {
    let y = helper();
}
"#;

    // Index all files
    let lib_path_str;
    let other_path_str;
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index helper.rs
        let helper_path = temp_dir.path().join("src/helper.rs");
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        std::fs::write(&helper_path, helper_rs).unwrap();
        let helper_path_str = helper_path.to_string_lossy().to_string();
        let helper_source = std::fs::read(&helper_path).unwrap();
        graph.index_file(&helper_path_str, &helper_source).unwrap();

        // Index lib.rs (defines and calls helper)
        let lib_path = temp_dir.path().join("src/lib.rs");
        std::fs::write(&lib_path, lib_rs).unwrap();
        lib_path_str = lib_path.to_string_lossy().to_string();
        let lib_source = std::fs::read(&lib_path).unwrap();
        graph.index_file(&lib_path_str, &lib_source).unwrap();
        graph.index_references(&lib_path_str, &lib_source).unwrap();

        // Index other.rs (calls helper)
        let other_path = temp_dir.path().join("src/other.rs");
        std::fs::write(&other_path, other_rs).unwrap();
        other_path_str = other_path.to_string_lossy().to_string();
        let other_source = std::fs::read(&other_path).unwrap();
        graph.index_file(&other_path_str, &other_source).unwrap();
        graph.index_references(&other_path_str, &other_source).unwrap();
    }

    // Query references to helper() from lib.rs (the one defined there)
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Get the helper symbol from lib.rs
        let symbol_id = magellan::graph::query::symbol_id_by_name(
            &mut graph,
            &lib_path_str,
            "helper"
        ).unwrap().expect("helper symbol should exist in lib.rs");

        // Query all references to this helper symbol
        let references = magellan::graph::query::references_to_symbol(&mut graph, symbol_id).unwrap();

        // Verify we have references from multiple files
        let other_refs: Vec<_> = references
            .iter()
            .filter(|r| r.file_path.ends_with("src/other.rs"))
            .collect();

        // other.rs should reference helper from lib.rs
        assert!(
            !other_refs.is_empty(),
            "Expected other.rs to reference helper(), found {} references",
            references.len()
        );

        println!("Multi-file refs to helper() from lib.rs:");
        for ref_fact in &references {
            println!(
                "  {}:{}:{} -> {}",
                ref_fact.file_path.display(),
                ref_fact.start_line,
                ref_fact.start_col,
                ref_fact.referenced_symbol
            );
        }

        // Verify each reference has correct file_path
        for ref_fact in &references {
            assert!(
                ref_fact.file_path.ends_with("src/lib.rs") ||
                ref_fact.file_path.ends_with("src/other.rs") ||
                ref_fact.file_path.ends_with("src/helper.rs"),
                "Reference file_path should be one of the indexed files, got: {}",
                ref_fact.file_path.display()
            );
        }
    }
}

/// Test: Multi-file find command
///
/// Verifies that find command handles symbols from multiple files:
/// 1. Create two files with symbols having the same name
/// 2. Index both files
/// 3. Query for the symbol name
/// 4. Verify results include symbols from both files with correct file_path
#[test]
fn test_find_command_multi_file() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create two files with symbols having the same name
    let file1_content = r#"
pub fn common_function() -> i32 {
    100
}

pub fn unique_to_file1() -> i32 {
    1
}
"#;

    let file2_content = r#"
pub fn common_function() -> String {
    "file2".to_string()
}

pub fn unique_to_file2() -> i32 {
    2
}
"#;

    // Index both files
    let file1_path_str;
    let file2_path_str;
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Index file1.rs
        let file1_path = temp_dir.path().join("src/file1.rs");
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        std::fs::write(&file1_path, file1_content).unwrap();
        file1_path_str = file1_path.to_string_lossy().to_string();
        let file1_source = std::fs::read(&file1_path).unwrap();
        graph.index_file(&file1_path_str, &file1_source).unwrap();

        // Index file2.rs
        let file2_path = temp_dir.path().join("src/file2.rs");
        std::fs::write(&file2_path, file2_content).unwrap();
        file2_path_str = file2_path.to_string_lossy().to_string();
        let file2_source = std::fs::read(&file2_path).unwrap();
        graph.index_file(&file2_path_str, &file2_source).unwrap();
    }

    // Query for symbols with name "common_function"
    {
        let mut graph = CodeGraph::open(&db_path).unwrap();

        // Get symbols from file1
        let symbols_file1 = magellan::graph::query::symbols_in_file(
            &mut graph,
            &file1_path_str
        ).unwrap();

        // Get symbols from file2
        let symbols_file2 = magellan::graph::query::symbols_in_file(
            &mut graph,
            &file2_path_str
        ).unwrap();

        // Verify both files have common_function
        let common_in_file1 = symbols_file1
            .iter()
            .filter(|s| s.name.as_deref() == Some("common_function"))
            .count();

        let common_in_file2 = symbols_file2
            .iter()
            .filter(|s| s.name.as_deref() == Some("common_function"))
            .count();

        assert_eq!(common_in_file1, 1, "file1.rs should have one common_function");
        assert_eq!(common_in_file2, 1, "file2.rs should have one common_function");

        // Verify each symbol has correct file_path
        for symbol in &symbols_file1 {
            assert!(
                symbol.file_path.ends_with("src/file1.rs"),
                "Symbol from file1 should have correct file_path, got: {}",
                symbol.file_path.display()
            );
        }

        for symbol in &symbols_file2 {
            assert!(
                symbol.file_path.ends_with("src/file2.rs"),
                "Symbol from file2 should have correct file_path, got: {}",
                symbol.file_path.display()
            );
        }

        println!("Multi-file find results for 'common_function':");
        println!("  file1.rs: {} symbols", symbols_file1.len());
        println!("  file2.rs: {} symbols", symbols_file2.len());
    }
}



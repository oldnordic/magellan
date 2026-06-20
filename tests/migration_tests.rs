//! Database migration and schema version tests

use tempfile::TempDir;

#[test]
fn test_new_database_has_v17_schema() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Open a new database (should create with v17)
    let _graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Verify schema version
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: i64 = conn
        .query_row(
            "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(version, 17, "New databases should have schema version 17");

    // Verify ast_nodes table exists
    let has_ast_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ast_nodes'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_ast_table,
        "ast_nodes table should exist in new databases"
    );

    // Verify cfg_blocks table exists (v7 addition)
    let has_cfg_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cfg_blocks'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_cfg_table,
        "cfg_blocks table should exist in new databases (v7)"
    );

    // Verify cfg_hash column exists (v8 addition)
    let has_cfg_hash: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='cfg_hash'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_cfg_hash,
        "cfg_hash column should exist in cfg_blocks (v8)"
    );

    // Verify statements column exists (v9 addition)
    let has_statements: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='statements'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_statements,
        "statements column should exist in cfg_blocks (v9)"
    );

    // v10 is reserved; legacy 4D CFG columns should not be created for new databases

    // v11 geo_index_meta removed — table no longer created, existing tables harmless

    // Verify symbol_fts table exists (v12 addition)
    let has_symbol_fts: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='symbol_fts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_symbol_fts,
        "symbol_fts table should exist in new databases (v12)"
    );

    // Verify source_documents table exists (v13 addition)
    let has_source_inventory: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_documents'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_source_inventory,
        "source_documents table should exist in new databases (v13)"
    );

    // Verify candidate_facts table exists (v14 addition)
    let has_candidate_facts: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='candidate_facts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_candidate_facts,
        "candidate_facts table should exist in new databases (v14)"
    );

    // Verify project_metadata column exists (v15 addition)
    let has_project_metadata: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('magellan_meta') WHERE name='project_metadata'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_project_metadata,
        "project_metadata column should exist in magellan_meta (v15)"
    );

    // Verify project_name column exists (v15 addition)
    let has_project_name: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('magellan_meta') WHERE name='project_name'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_project_name,
        "project_name column should exist in magellan_meta (v15)"
    );

    // Verify cfg_condition column exists (v16 addition)
    let has_cfg_condition: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='cfg_condition'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_cfg_condition,
        "cfg_condition column should exist in cfg_blocks (v16)"
    );

    for legacy_column in ["coord_x", "coord_y", "coord_z", "coord_t"] {
        let has_legacy_column: bool = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name=?1",
                [legacy_column],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(
            !has_legacy_column,
            "legacy column {legacy_column} should not exist in cfg_blocks"
        );
    }
}

#[test]
fn test_fresh_database_creation_v17() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("fresh.db");

    // Create a fresh database
    {
        let _graph = magellan::CodeGraph::open(&db_path).unwrap();
    }

    // Verify everything was created correctly
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Check magellan_meta
    let version: i64 = conn
        .query_row(
            "SELECT magellan_schema_version FROM magellan_meta",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 17);

    // Check ast_nodes table
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM ast_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0); // Empty but exists

    // Check cfg_blocks table (v7)
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cfg_blocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0); // Empty but exists

    // v11 geo_index_meta removed — no longer created

    // Check symbol_fts table (v12)
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbol_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0); // Empty but exists

    // Check source_documents table (v13)
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM source_documents", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0); // Empty but exists

    // Check candidate_facts table (v14)
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM candidate_facts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0); // Empty but exists

    // Check indexes
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_ast_%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert!(indexes.contains(&"idx_ast_nodes_parent".to_string()));
    assert!(indexes.contains(&"idx_ast_nodes_span".to_string()));
}

/// Test that v4->v17 migration creates the required tables

#[test]
fn test_migration_v4_to_v17_creates_required_tables() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v4_to_v15.db");

    // Create a v4 database (without ast_nodes table)
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
         VALUES (1, 4, 5, 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    // Run migration using run_migrate
    let result = magellan::migrate_cmd::run_migrate(db_path.clone(), false, true).unwrap();
    assert!(result.success);
    assert_eq!(result.old_version, 4);
    assert_eq!(result.new_version, 17);

    // Verify ast_nodes table exists
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let has_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ast_nodes'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_table, "ast_nodes table should be created");

    // Verify cfg_blocks table exists (v7)
    let has_cfg_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cfg_blocks'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_cfg_table, "cfg_blocks table should be created (v7)");

    // Verify cfg_hash column exists (v8)
    let has_cfg_hash: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='cfg_hash'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_cfg_hash, "cfg_hash column should be created (v8)");

    // Verify statements column exists (v9)
    let has_statements: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='statements'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_statements, "statements column should be created (v9)");

    // v10 is reserved; legacy 4D CFG columns are not created for new databases
    for legacy_column in ["coord_x", "coord_y", "coord_z", "coord_t"] {
        let has_legacy_column: bool = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name=?1",
                [legacy_column],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(
            !has_legacy_column,
            "legacy column {legacy_column} should not be created"
        );
    }

    // v11 geo_index_meta removed — no longer created

    // Verify indexes exist
    let has_parent_index: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_ast_nodes_parent'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_parent_index, "idx_ast_nodes_parent should be created");

    let has_span_index: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_ast_nodes_span'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_span_index, "idx_ast_nodes_span should be created");

    // Verify source_documents table exists (v13)
    let has_source_inventory: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_documents'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_source_inventory,
        "source_documents table should be created (v13)"
    );

    // Verify candidate_facts table exists (v14)
    let has_candidate_facts: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='candidate_facts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(
        has_candidate_facts,
        "candidate_facts table should be created (v14)"
    );

    // Verify schema version was updated
    let version: i64 = conn
        .query_row(
            "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 17, "schema version should be 17 after migration");
}

/// Test that opening a v4 database auto-upgrades to v17
#[test]
fn test_opening_v4_database_auto_upgrades_to_v17() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_v4_auto.db");

    // Create a v4 database using sqlitegraph (simulating old database)
    let _sqlite_graph = sqlitegraph::SqliteGraph::open(&db_path).unwrap();

    // Set magellan_meta to v4
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magellan_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            magellan_schema_version INTEGER NOT NULL,
            sqlitegraph_schema_version INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO magellan_meta (id, magellan_schema_version, sqlitegraph_schema_version, created_at)
         VALUES (1, 4, 5, 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    // Open the database with CodeGraph (should auto-upgrade to v13)
    let _graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Verify schema version is now 13
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: i64 = conn
        .query_row(
            "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(
        version, 17,
        "Opening v4 database should auto-upgrade to v17"
    );

    // Verify ast_nodes table exists
    let has_ast_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ast_nodes'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_ast_table,
        "ast_nodes table should be created during auto-upgrade"
    );

    // Verify cfg_hash column exists (v8)
    let has_cfg_hash: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='cfg_hash'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_cfg_hash,
        "cfg_hash column should be created during auto-upgrade (v8)"
    );

    // Verify statements column exists (v9)
    let has_statements: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='statements'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_statements,
        "statements column should be created during auto-upgrade (v9)"
    );

    // v10 is reserved; legacy 4D CFG columns are not created during auto-upgrade
    for legacy_column in ["coord_x", "coord_y", "coord_z", "coord_t"] {
        let has_legacy_column: bool = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name=?1",
                [legacy_column],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(
            !has_legacy_column,
            "legacy column {legacy_column} should not exist after auto-upgrade"
        );
    }

    // v11 geo_index_meta removed — no longer created during migration

    // Verify source_documents table exists (v13)
    let has_source_inventory: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_documents'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_source_inventory,
        "source_documents table should be created during auto-upgrade (v13)"
    );

    // Verify candidate_facts table exists (v14)
    let has_candidate_facts: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='candidate_facts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_candidate_facts,
        "candidate_facts table should be created during auto-upgrade (v14)"
    );
}

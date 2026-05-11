//! Database migration and schema version tests

use tempfile::TempDir;

#[test]
fn test_new_database_has_v16_schema() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Open a new database (should create with v15)
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

    assert_eq!(version, 16, "New databases should have schema version 16");

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

    // Verify 4D coordinate columns exist (v10 addition)
    let has_coord_x: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_x'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_coord_x,
        "coord_x column should exist in cfg_blocks (v10)"
    );

    // Verify geo_index_meta table exists (v11 addition)
    let has_geo_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='geo_index_meta'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_geo_meta,
        "geo_index_meta table should exist in new databases (v11)"
    );

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
}

#[test]
fn test_fresh_database_creation_v16() {
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
    assert_eq!(version, 16);

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

    // Check geo_index_meta table (v11)
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM geo_index_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0); // Empty but exists

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

/// Test that v4->v16 migration creates the required tables
#[test]
fn test_migration_v4_to_v16_creates_required_tables() {
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
         VALUES (1, 4, 4, 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    // Run migration using run_migrate
    let result = magellan::migrate_cmd::run_migrate(db_path.clone(), false, true).unwrap();
    assert!(result.success);
    assert_eq!(result.old_version, 4);
    assert_eq!(result.new_version, 16);

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

    // Verify 4D coordinate columns exist (v10)
    let has_coord_x: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_x'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_coord_x, "coord_x column should be created (v10)");

    // Verify geo_index_meta table exists (v11)
    let has_geo_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='geo_index_meta'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    assert!(has_geo_meta, "geo_index_meta table should be created (v11)");

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
    assert_eq!(version, 16, "schema version should be 16 after migration");
}

/// Test that opening a v4 database auto-upgrades to v16
#[test]
fn test_opening_v4_database_auto_upgrades_to_v16() {
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
         VALUES (1, 4, 4, 1000)",
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
        version, 16,
        "Opening v4 database should auto-upgrade to v16"
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

    // Verify 4D coordinate columns exist (v10)
    let has_coord_x: bool = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='coord_x'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_coord_x,
        "coord_x column should be created during auto-upgrade (v10)"
    );

    // Verify geo_index_meta table exists (v11)
    let has_geo_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='geo_index_meta'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    assert!(
        has_geo_meta,
        "geo_index_meta table should be created during auto-upgrade (v11)"
    );

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

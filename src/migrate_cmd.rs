//! Database migration command implementation
//!
//! Handles migrations from schema v1-v5 to v6:
//! - v2: magellan_meta table
//! - v4: BLAKE3-based SymbolId (additive fields)
//! - v5: AST nodes table for hierarchy storage
//! - v6: AST nodes file_id column for per-file tracking

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Transaction};
use std::fs;
use std::path::{Path, PathBuf};

/// Current Magellan schema version
/// v4: BLAKE3-based SymbolId, canonical_fqn, display_fqn
/// v5: AST nodes table for hierarchy storage
/// v6: AST nodes file_id column for per-file tracking
/// v7: CFG blocks table for control flow graph storage
/// v8: cfg_blocks.cfg_hash column for cache invalidation
/// v9: cfg_blocks.statements column for AST snippets
/// v10: reserved migration slot (legacy 4D CFG columns removed from new schema)
/// v11: (removed — geo_index_meta table no longer created)
/// v12: FTS5 full-text search index
/// v12: symbol_fts FTS5 virtual table for fast symbol search
/// v13: source_documents table for graph memory source inventory
/// v14: candidate_facts table for graph memory candidate staging
/// v15: magellan_meta.project_name and project_metadata columns
/// v16: cfg_blocks.cfg_condition column for feature-gated blocks
/// v17: telemetry_events table for performance telemetry
/// v18: repository snapshot tables for temporal tracking
/// v19: symbol scoring tables for candidate ranking
pub const MAGELLAN_SCHEMA_VERSION: i64 = 19;

/// Migration result summary
#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub success: bool,
    pub backup_path: Option<PathBuf>,
    pub old_version: i64,
    pub new_version: i64,
    pub message: String,
}

/// Run database migration
///
/// Creates backup, uses transaction for atomicity, supports rollback on error.
///
/// # Arguments
/// * `db_path` - Path to database file
/// * `dry_run` - If true, check version only without migrating
/// * `no_backup` - If true, skip backup creation
///
/// # Returns
/// Migration result with version info and backup path
pub fn run_migrate(db_path: PathBuf, dry_run: bool, no_backup: bool) -> Result<MigrationResult> {
    // Check database exists
    if !db_path.exists() {
        return Ok(MigrationResult {
            success: false,
            backup_path: None,
            old_version: 0,
            new_version: MAGELLAN_SCHEMA_VERSION,
            message: format!("Database not found: {}", db_path.display()),
        });
    }

    // Open database and check current version
    let conn = rusqlite::Connection::open(&db_path)?;

    // Check if magellan_meta table exists
    let has_meta_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='magellan_meta' LIMIT 1",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    let current_version: Option<i64> = if has_meta_table {
        conn.query_row(
            "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?
    } else {
        None
    };

    let old_version = current_version.unwrap_or(1);

    if old_version == MAGELLAN_SCHEMA_VERSION {
        return Ok(MigrationResult {
            success: true,
            backup_path: None,
            old_version,
            new_version: MAGELLAN_SCHEMA_VERSION,
            message: "Database already at current version".to_string(),
        });
    }

    if old_version > MAGELLAN_SCHEMA_VERSION {
        return Ok(MigrationResult {
            success: false,
            backup_path: None,
            old_version,
            new_version: MAGELLAN_SCHEMA_VERSION,
            message: format!(
                "Database version {} is newer than current {}",
                old_version, MAGELLAN_SCHEMA_VERSION
            ),
        });
    }

    if dry_run {
        return Ok(MigrationResult {
            success: true,
            backup_path: None,
            old_version,
            new_version: MAGELLAN_SCHEMA_VERSION,
            message: format!(
                "Would migrate from version {} to {} (dry run)",
                old_version, MAGELLAN_SCHEMA_VERSION
            ),
        });
    }

    // Create backup
    let backup_path = if !no_backup {
        Some(create_backup(&db_path)?)
    } else {
        None
    };

    // Run migration in transaction
    let tx = conn.unchecked_transaction()?;

    // Execute version-specific migrations
    migrate_from_version(&tx, old_version)?;

    // Update magellan_meta version
    tx.execute(
        "UPDATE magellan_meta SET magellan_schema_version=?1 WHERE id=1",
        params![MAGELLAN_SCHEMA_VERSION],
    )?;

    tx.commit()?;

    Ok(MigrationResult {
        success: true,
        backup_path,
        old_version,
        new_version: MAGELLAN_SCHEMA_VERSION,
        message: format!(
            "Migrated from version {} to {}",
            old_version, MAGELLAN_SCHEMA_VERSION
        ),
    })
}

/// Create backup of database file
fn create_backup(db_path: &Path) -> Result<PathBuf> {
    let backup_path = db_path.with_extension(format!(
        "v{}.bak",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ));

    fs::copy(db_path, &backup_path)?;

    Ok(backup_path)
}

/// Execute migration steps from old version
fn migrate_from_version(tx: &Transaction, old_version: i64) -> Result<()> {
    // For schema version 3 -> 4 migration:
    // The schema change is BLAKE3-based SymbolId and canonical_fqn/display_fqn fields.
    // These fields are added with #[serde(default)] so old data deserializes correctly.
    // The migration primarily needs to ensure the magellan_meta table exists and
    // that new SymbolNode fields can be added.

    if old_version < 2 {
        // Create magellan_meta table if it doesn't exist (v1 -> v2)
        tx.execute(
            "CREATE TABLE IF NOT EXISTS magellan_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                magellan_schema_version INTEGER NOT NULL,
                sqlitegraph_schema_version INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;
    }

    if old_version < 4 {
        // v3 -> v4: BLAKE3 SymbolId migration
        // SymbolNode schema changes are additive (Option fields with defaults)
        // No explicit schema migration needed - just version bump
        // Existing symbols will have symbol_id=None, new symbols get BLAKE3 IDs
    }

    if old_version < 5 {
        // v4 -> v5: AST nodes table
        // Create ast_nodes table for storing AST hierarchy
        tx.execute(
            "CREATE TABLE IF NOT EXISTS ast_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id INTEGER,
                kind TEXT NOT NULL,
                byte_start INTEGER NOT NULL,
                byte_end INTEGER NOT NULL
            )",
            [],
        )?;

        // Create indexes for efficient queries
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_parent
             ON ast_nodes(parent_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_span
             ON ast_nodes(byte_start, byte_end)",
            [],
        )?;
    }

    if old_version < 6 {
        // v5 -> v6: Add file_id to ast_nodes table
        // Add file_id column for per-file AST node tracking
        tx.execute("ALTER TABLE ast_nodes ADD COLUMN file_id INTEGER", [])?;

        // Create index for efficient per-file queries
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_ast_nodes_file_id
             ON ast_nodes(file_id)",
            [],
        )?;
    }

    if old_version < 7 {
        // v6 -> v7: Add cfg_blocks table for control flow graph storage
        tx.execute(
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

        // Index for function-based queries
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_function
             ON cfg_blocks(function_id)",
            [],
        )?;

        // Index for span-based position queries
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_span
             ON cfg_blocks(byte_start, byte_end)",
            [],
        )?;
    }

    if old_version < 8 {
        // v7 -> v8: Add cfg_hash column for cache invalidation
        // This allows tools like Mirage to detect when CFG structure changes
        tx.execute("ALTER TABLE cfg_blocks ADD COLUMN cfg_hash TEXT", [])?;

        // Index for hash-based cache lookups
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_cfg_blocks_hash
             ON cfg_blocks(cfg_hash)",
            [],
        )?;
    }

    if old_version < 9 {
        // v8 -> v9: Add statements column to cfg_blocks for AST snippets
        tx.execute("ALTER TABLE cfg_blocks ADD COLUMN statements TEXT", [])?;
    }

    if old_version < 10 {
        // v9 -> v10: reserved migration slot; legacy 4D CFG columns are no longer created
    }

    if old_version < 11 {
        // v10 -> v11: geo_index_meta removed — no-op
        // Previously created geo_index_meta table; existing databases with this table are harmless
    }

    if old_version < 12 {
        // v11 -> v12: Add FTS5 virtual table for fast symbol search
        // FTS5 indexes the 'name' column from graph_entities for prefix/full-text search
        tx.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS symbol_fts USING fts5(\n                name,\n                content='graph_entities',\n                content_rowid='id'\n            )",
            [],
        )?;
    }

    if old_version < 13 {
        // v12 -> v13: Add source_documents table for graph memory source inventory
        tx.execute(
            "CREATE TABLE IF NOT EXISTS source_documents (\n                id INTEGER PRIMARY KEY AUTOINCREMENT,\n                path_or_uri TEXT NOT NULL UNIQUE,\n                source_kind TEXT NOT NULL,\n                content_hash TEXT NOT NULL,\n                observed_at INTEGER NOT NULL,\n                source_timestamp INTEGER,\n                title TEXT,\n                author TEXT,\n                tags TEXT,\n                wikilinks TEXT,\n                frontmatter TEXT\n            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_source_docs_path ON source_documents(path_or_uri)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_source_docs_hash ON source_documents(content_hash)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_source_docs_kind ON source_documents(source_kind)",
            [],
        )?;
    }

    if old_version < 14 {
        // v13 -> v14: Add candidate_facts table for graph memory candidate staging
        tx.execute(
            "CREATE TABLE IF NOT EXISTS candidate_facts (\n                id INTEGER PRIMARY KEY AUTOINCREMENT,\n                candidate_id TEXT NOT NULL UNIQUE,\n                source_document_id INTEGER NOT NULL,\n                subject_type TEXT NOT NULL,\n                subject_key TEXT NOT NULL,\n                predicate TEXT NOT NULL,\n                object_type TEXT,\n                object_key TEXT,\n                properties_json TEXT NOT NULL,\n                status TEXT NOT NULL DEFAULT 'pending',\n                rejection_reason TEXT,\n                created_at INTEGER NOT NULL,\n                reviewed_at INTEGER,\n                FOREIGN KEY (source_document_id) REFERENCES source_documents(id)\n            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_candidate_facts_status ON candidate_facts(status)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_candidate_facts_source ON candidate_facts(source_document_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_candidate_facts_predicate ON candidate_facts(predicate)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_candidate_facts_status_created ON candidate_facts(status, created_at)",
            [],
        )?;
    }

    if old_version < 15 {
        // v14 -> v15: Add project_name and project_metadata to magellan_meta
        // Check if columns exist first (they may have been added via db_compat)
        let has_project_name: bool = tx
            .query_row(
                "SELECT 1 FROM pragma_table_info('magellan_meta') WHERE name='project_name' LIMIT 1",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !has_project_name {
            tx.execute("ALTER TABLE magellan_meta ADD COLUMN project_name TEXT", [])?;
        }

        let has_project_metadata: bool = tx
            .query_row(
                "SELECT 1 FROM pragma_table_info('magellan_meta') WHERE name='project_metadata' LIMIT 1",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !has_project_metadata {
            tx.execute(
                "ALTER TABLE magellan_meta ADD COLUMN project_metadata TEXT",
                [],
            )?;
        }
    }

    if old_version < 16 {
        // v15 -> v16: Add cfg_condition column to cfg_blocks for feature-gated blocks
        let has_cfg_condition: bool = tx
            .query_row(
                "SELECT 1 FROM pragma_table_info('cfg_blocks') WHERE name='cfg_condition' LIMIT 1",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !has_cfg_condition {
            tx.execute("ALTER TABLE cfg_blocks ADD COLUMN cfg_condition TEXT", [])?;
        }
    }

    if old_version < 17 {
        // v16 -> v17: Add telemetry_events table for performance telemetry
        tx.execute(
            "CREATE TABLE IF NOT EXISTS telemetry_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                execution_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_name TEXT NOT NULL,
                timestamp_ns INTEGER NOT NULL,
                duration_ns INTEGER,
                value REAL,
                unit TEXT,
                metadata TEXT
            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_telemetry_events_execution
             ON telemetry_events(execution_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_telemetry_events_type_name
             ON telemetry_events(event_type, event_name)",
            [],
        )?;
    }

    if old_version < 18 {
        // v17 -> v18: Add repository snapshot tables for temporal tracking
        tx.execute(
            "CREATE TABLE IF NOT EXISTS repo_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_root TEXT NOT NULL,
                commit_oid TEXT NOT NULL UNIQUE,
                tree_oid TEXT NOT NULL,
                author_time INTEGER NOT NULL,
                commit_time INTEGER NOT NULL,
                commit_message TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_repo_snapshots_commit
             ON repo_snapshots(commit_oid)",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS repo_snapshot_parents (
                snapshot_id INTEGER NOT NULL,
                parent_oid TEXT NOT NULL,
                FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE,
                PRIMARY KEY (snapshot_id, parent_oid)
            )",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS file_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_file_versions_snapshot
             ON file_versions(snapshot_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_file_versions_path
             ON file_versions(file_path)",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS symbol_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id INTEGER NOT NULL,
                stable_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_path TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                body_hash TEXT,
                FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbol_versions_snapshot
             ON symbol_versions(snapshot_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbol_versions_stable
             ON symbol_versions(stable_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbol_versions_name
             ON symbol_versions(name)",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS edge_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id INTEGER NOT NULL,
                source_stable_id TEXT NOT NULL,
                target_stable_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                FOREIGN KEY (snapshot_id) REFERENCES repo_snapshots(id) ON DELETE CASCADE
            )",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_edge_versions_snapshot
             ON edge_versions(snapshot_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_edge_versions_source
             ON edge_versions(source_stable_id)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_edge_versions_target
             ON edge_versions(target_stable_id)",
            [],
        )?;
    }

    Ok(())
}

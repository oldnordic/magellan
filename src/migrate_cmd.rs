//! Database migration command implementation
//!
//! Handles migrations from schema v1-v4 to v5:
//! - v2: magellan_meta table
//! - v4: BLAKE3-based SymbolId (additive fields)
//! - v5: AST nodes table for hierarchy storage

use anyhow::Result;
use rusqlite::{params, Transaction, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};

/// Current Magellan schema version
/// v4: BLAKE3-based SymbolId, canonical_fqn, display_fqn
/// v5: AST nodes table for hierarchy storage
pub const MAGELLAN_SCHEMA_VERSION: i64 = 5;

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
pub fn run_migrate(
    db_path: PathBuf,
    dry_run: bool,
    no_backup: bool,
) -> Result<MigrationResult> {
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
        conn
            .query_row(
                "SELECT magellan_schema_version FROM magellan_meta WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()?
    } else {
        None
    };

    let old_version = match current_version {
        Some(v) => v,
        None => {
            // Old database without magellan_meta table, assume version 1
            1
        }
    };

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

    Ok(())
}

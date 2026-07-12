use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::generation::ChunkStore;

use super::{db_compat, execution_log, metrics, side_tables, telemetry};

pub(crate) struct SqliteRuntimeComponents {
    pub(crate) side_tables: Arc<dyn side_tables::SideTables>,
    pub(crate) chunks: ChunkStore,
    pub(crate) execution_log: execution_log::ExecutionLog,
    pub(crate) metrics: metrics::MetricsOps,
    pub(crate) telemetry: telemetry::TelemetryOps,
    pub(crate) needs_backfill: bool,
    pub(crate) side_conn: Arc<parking_lot::Mutex<rusqlite::Connection>>,
}

pub(crate) fn is_memory_db(path: &Path) -> bool {
    path.as_os_str() == ":memory:"
}

#[cfg(feature = "sqlite-backend")]
pub(crate) fn configure_sqlite_pragmas(db_path: &Path) -> Result<()> {
    let pragma_conn = rusqlite::Connection::open(db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open connection for PRAGMA config: {}", e))?;

    let journal_mode = pragma_conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| {
            let mode: String = row.get(0)?;
            Ok(mode)
        })
        .map_err(|e| anyhow::anyhow!("Failed to set WAL mode: {}", e))?;
    if !is_memory_db(db_path) {
        debug_assert_eq!(journal_mode, "wal", "WAL mode should be enabled");
    }

    pragma_conn
        .execute("PRAGMA synchronous = NORMAL", [])
        .map_err(|e| anyhow::anyhow!("Failed to set synchronous: {}", e))?;
    pragma_conn
        .execute("PRAGMA cache_size = -64000", [])
        .map_err(|e| anyhow::anyhow!("Failed to set cache_size: {}", e))?;
    pragma_conn
        .execute("PRAGMA temp_store = MEMORY", [])
        .map_err(|e| anyhow::anyhow!("Failed to set temp_store: {}", e))?;

    Ok(())
}

#[cfg(feature = "sqlite-backend")]
pub(crate) fn initialize_sqlite_runtime(db_path: &Path) -> Result<SqliteRuntimeComponents> {
    let side_conn = rusqlite::Connection::open(db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open shared side-table connection: {}", e))?;
    side_conn.pragma_update(None, "busy_timeout", 5000)?;
    let side_conn_arc = Arc::new(parking_lot::Mutex::new(side_conn));

    let needs_ddl = db_compat::needs_schema_upgrade(&side_conn_arc.lock(), db_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    db_compat::ensure_magellan_meta(&side_conn_arc.lock(), db_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let side_tables: Arc<dyn side_tables::SideTables> = Arc::new(
        side_tables::SqliteSideTables::with_shared(Arc::clone(&side_conn_arc))?,
    );

    let shared_conn = rusqlite::Connection::open(db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open shared connection for ChunkStore: {}", e))?;
    shared_conn.pragma_update(None, "busy_timeout", 5000)?;

    let chunks = ChunkStore::with_connection(shared_conn);
    chunks.ensure_schema()?;

    let execution_log = execution_log::ExecutionLog::with_connection(Arc::clone(&side_conn_arc));
    let metrics = metrics::MetricsOps::with_connection(Arc::clone(&side_conn_arc), db_path);
    let telemetry = telemetry::TelemetryOps::with_connection(Arc::clone(&side_conn_arc));

    if needs_ddl {
        db_compat::ensure_ast_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        db_compat::ensure_cfg_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        db_compat::ensure_metrics_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        db_compat::ensure_source_inventory_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        db_compat::ensure_candidate_fact_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        db_compat::ensure_telemetry_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        db_compat::ensure_temporal_schema(&side_conn_arc.lock(), db_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    db_compat::ensure_coverage_schema(&side_conn_arc.lock(), db_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let needs_backfill = {
        let metric_count: i64 = side_conn_arc
            .lock()
            .query_row("SELECT COUNT(*) FROM file_metrics", [], |row| row.get(0))
            .unwrap_or(0);

        let symbol_count: i64 = side_conn_arc
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM graph_entities WHERE kind = 'Symbol'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        metric_count == 0 && symbol_count > 0
    };

    Ok(SqliteRuntimeComponents {
        side_tables,
        chunks,
        execution_log,
        metrics,
        telemetry,
        needs_backfill,
        side_conn: side_conn_arc,
    })
}

//! Graph persistence layer using sqlitegraph
//!
//! # Position Conventions
//!
//! Symbol and reference spans in the graph use tree-sitter position conventions:
//! - **Line positions**: 1-indexed (line 1 is the first line)
//! - **Column positions**: 0-indexed (column 0 is the first character in a line)
//! - **Byte offsets**: 0-indexed from file start (byte 0 is the first byte)
//!
//! See [MANUAL.md](../../MANUAL.md#3-position-conventions) for detailed documentation.
//!
//! # :memory: Database Limitation
//!
//! CodeGraph uses SQLite Shared connections via `sqlitegraph` and opens a separate
//! shared connection for `ChunkStore`. These Shared connections don't work with
//! `:memory:` databases because each thread would get its own separate in-memory
//! database instance.
//!
//! **Workaround:** Use file-based databases for CodeGraph operations.
//! See [MANUAL.md](../../MANUAL.md#known-limitations) for details.
pub mod algorithms;
pub mod ambiguity;
pub mod backend;
pub mod candidate_fact;
pub mod ontology;
pub mod source_inventory;
// pub mod memory_graph;

// Re-export MemoryGraph types for public API
// Note: GraphStats is not re-exported here due to name collision with CodeGraph's GraphStats
// Access via graph::memory_graph::GraphStats if needed
// pub use memory_graph::{GraphSymbol, MemoryGraph};
mod ast_extractor;

mod ast_node;
mod ast_ops;

mod cache;
mod call_ops;
mod calls;
pub mod canonical_fqn;
pub mod cfg_edges_extract;
mod cfg_extractor;
mod cfg_ops;
mod count;
pub mod crate_name;
pub mod db_compat;
pub mod embed;
pub mod execution_log;
pub mod export;
mod files;
pub mod filter;
mod freshness;
mod imports; // Private module for import operations
pub mod metrics;
mod module_resolver;
pub mod multi_db;
pub mod navigator;
mod ops;
pub mod query;
mod references;
pub mod scan;
pub mod schema;
pub mod search;
pub mod side_tables;
mod symbol_index;
mod symbol_lookup;
mod symbols;
pub mod telemetry;
pub mod validation;
pub mod wal;

// Re-export small public types from ops.
pub use ops::{index_file, DeleteResult, ReconcileOutcome};

// Re-export metrics types
pub use metrics::BackfillResult;

// Re-export test helpers for integration tests.
// The test_helpers module is public in ops.rs for use by delete_transaction_tests.rs
pub use ops::test_helpers;

// Re-export symbol ID generation function
pub use symbols::generate_symbol_id;
#[cfg(test)]
mod ast_tests;
#[cfg(test)]
mod tests;

use anyhow::Result;
use sqlitegraph::GraphBackend;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::graph::scan::ScanResult;

use crate::generation::{ChunkStore, CodeChunk};
use crate::references::{CallFact, ReferenceFact};

// Re-export public types
pub use algorithms::{
    CondensationGraph, CondensationResult, Cycle, CycleKind, CycleReport, DeadSymbol,
    ExecutionPath, PathEnumerationResult, PathStatistics, ProgramSlice, SliceDirection,
    SliceResult, SliceStatistics, Supernode, SymbolInfo,
};
pub use ast_extractor::{extract_ast_nodes, language_from_path, normalize_node_kind};
pub use ast_node::{is_structural_kind, AstNode, AstNodeWithText};
// Re-export CFG types for public API
#[deprecated(since = "10.0.0", note = "Use cfg_edges_extract instead")]
pub use cfg_extractor::{BlockKind, CfgExtractor, TerminatorKind};
pub use cfg_ops::CfgOps;
pub use multi_db::MultiDbContext;

pub use cache::CacheStats;
pub use db_compat::MAGELLAN_SCHEMA_VERSION;
pub use db_compat::{
    ensure_ast_schema, ensure_candidate_fact_schema, ensure_cfg_schema, ensure_coverage_schema,
    ensure_source_inventory_schema, CFG_EDGE,
};
pub use execution_log::ExecutionLog;
pub use export::{ExportConfig, ExportFormat};
pub use freshness::{check_freshness, FreshnessStatus, STALE_THRESHOLD_SECS};
pub use metrics::MetricsOps;
pub use schema::{CallNode, CfgBlock, CfgEdge, CrossFileRef, FileNode, ReferenceNode, SymbolNode};

/// Statistics for a CodeGraph database
///
/// Contains counts of various entity types in the graph.
#[derive(Debug, Clone)]
pub struct GraphStats {
    /// Number of symbols in the graph
    pub symbol_count: usize,
    /// Number of files in the graph
    pub file_count: usize,
    /// Number of CFG blocks (0 for SQLite backend without CFG)
    pub cfg_block_count: usize,
}

/// Progress callback for scan_directory
///
/// Receives (current_count, total_count, current_file_path) as scanning progresses
pub type ScanProgress = dyn Fn(usize, usize, &str) + Send + Sync;

/// Check if a database path is an in-memory database.
///
/// # Arguments
/// * `path` - Database path to check
///
/// # Returns
/// true if the path is :memory:, false otherwise
///
/// # Note
/// In-memory databases have no file path and cannot be used with operations
/// that require file-based access (e.g., exports, some ChunkStore operations).
fn is_memory_db(path: &Path) -> bool {
    path.as_os_str() == ":memory:"
}

/// Graph database wrapper for Magellan
///
/// Provides deterministic, idempotent operations for persisting code facts.
pub struct CodeGraph {
    /// File operations module
    files: files::FileOps,

    /// Symbol operations module
    symbols: symbols::SymbolOps,

    /// Reference operations module
    references: references::ReferenceOps,

    /// Call operations module
    calls: call_ops::CallOps,

    /// Import operations module
    imports: imports::ImportOps,

    /// Module resolver for import path resolution
    module_resolver: module_resolver::ModuleResolver,

    /// Code chunk storage module
    chunks: ChunkStore,

    /// Execution log module for tracking Magellan runs
    execution_log: execution_log::ExecutionLog,

    /// Metrics module for pre-computed file and symbol metrics
    metrics: metrics::MetricsOps,

    /// Telemetry module for performance metrics
    telemetry: telemetry::TelemetryOps,

    /// File node cache for frequently accessed files
    file_node_cache: cache::FileNodeCache,

    /// CFG block operations module
    pub cfg_ops: cfg_ops::CfgOps,

    /// Side tables for backend-agnostic storage (chunks, AST, metrics, etc.)
    side_tables: Arc<dyn side_tables::SideTables>,

    /// Shared SQLite connection for Magellan side-table operations.
    /// Eliminates redundant connections opened by schema checks and diagnostics.
    pub(crate) side_conn: Arc<std::sync::Mutex<rusqlite::Connection>>,

    /// Whether to use batch SQLite transactions for indexing.
    ///
    /// When `true` (default), `index_file` uses `bulk_insert_entities`/`bulk_insert_edges`
    /// wrapped in `TransactionGuard` for ~27x throughput improvement on bulk indexing.
    ///
    /// When `false`, falls back to individual per-insert auto-commit mode. This is
    /// required for watch mode where `BEGIN IMMEDIATE` transactions on the single
    /// pooled connection deadlock with the flush cycle.
    pub(crate) batch_mode: bool,

    embeddings_enabled: bool,
    embedder: Box<dyn crate::graph::embed::TextEmbedder>,

    /// Database file path for re-opening connections
    db_path: PathBuf,
}

impl CodeGraph {
    /// Get the database file path
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn navigator(&self) -> navigator::SymbolNavigator<'_> {
        navigator::SymbolNavigator::new(self)
    }

    pub fn embeddings_enabled(&self) -> bool {
        self.embeddings_enabled
    }

    pub fn configure_embeddings(
        &mut self,
        provider: &crate::config::EmbedProvider,
        enabled: bool,
        base_url: &str,
        model: &str,
        api_key: &str,
        num_ctx: usize,
    ) {
        self.embeddings_enabled = enabled;
        self.embedder =
            crate::graph::embed::create_embedder(provider, enabled, base_url, model, api_key, num_ctx);
    }

    #[cfg(test)]
    pub fn enable_embeddings_for_test(&mut self) {
        self.embeddings_enabled = true;
        self.embedder = Box::new(crate::graph::embed::HashEmbedder::new(128));
    }

    pub fn hopgraph_search(
        &self,
        query: &str,
        k: usize,
        hops: u32,
    ) -> anyhow::Result<Vec<crate::graph::search::HopgraphHit>> {
        if !self.embeddings_enabled {
            return Ok(Vec::new());
        }
        let sg = self.symbols.sqlite_graph()?;
        // When hops > 0, request more vector candidates so graph expansion has
        // more seeds to work from, and allow the result set to grow beyond k.
        let vector_k = if hops > 0 { k * 2 } else { k };
        let raw_hits =
            crate::graph::search::search_symbols(sg, self.embedder.as_ref(), query, vector_k)?;

        // Build initial hit set with vector scores
        let mut hit_scores: std::collections::HashMap<i64, (f32, u32)> =
            std::collections::HashMap::new();
        for &(entity_id, score) in &raw_hits {
            hit_scores.insert(entity_id, (score, 0));
        }

        // Graph expansion: BFS from each initial hit via REFERENCES edges
        if hops > 0 && !raw_hits.is_empty() {
            let nav = navigator::SymbolNavigator::new(self);
            let initial_ids: Vec<i64> = raw_hits.iter().map(|(id, _)| *id).collect();
            let alpha = 0.7_f32;
            for start_id in &initial_ids {
                // k_hop_references returns all nodes reachable within `hops` hops
                if let Ok(expanded) = nav.k_hop_references(*start_id, hops) {
                    for info in expanded {
                        // Skip if already in results (vector hit or earlier expansion)
                        if hit_scores.contains_key(&info.id) {
                            continue;
                        }
                        // Use estimated depth = 1 (can't distinguish exact hop from
                        // k_hop_references output which returns all reachable nodes)
                        let estimated_depth = 1u32;
                        let graph_proximity = 1.0_f32 / (1.0 + estimated_depth as f32);
                        let seed_score = hit_scores[start_id].0;
                        // Plan formula: alpha * vector_score + (1-alpha) * (1 - graph_proximity)
                        // (1 - proximity) penalizes distant nodes, keeping vector hits on top
                        let blended = alpha * seed_score + (1.0 - alpha) * (1.0 - graph_proximity);
                        hit_scores.insert(info.id, (blended, estimated_depth));
                    }
                }
            }
        }

        // Resolve all entity_ids to symbol metadata
        let all_ids: Vec<i64> = hit_scores.keys().copied().collect();
        let resolved = {
            let conn = self.side_conn.lock().unwrap_or_else(|e| e.into_inner());
            navigator::SymbolNavigator::resolve_entities_with_conn(&conn, &all_ids)?
        };

        let mut resolved_map = std::collections::HashMap::new();
        for info in resolved {
            resolved_map.insert(info.id, info);
        }

        // Build final hits
        let mut hits: Vec<crate::graph::search::HopgraphHit> = hit_scores
            .into_iter()
            .map(|(entity_id, (score, hop_distance))| {
                let info = resolved_map.get(&entity_id);
                crate::graph::search::HopgraphHit {
                    entity_id,
                    score,
                    name: info
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    kind: info
                        .map(|i| i.kind.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    file_path: info.and_then(|i| i.file_path.clone()),
                    start_line: info.map(|i| i.start_line).unwrap_or(0),
                    hop_distance,
                }
            })
            .collect();

        // Sort by score ascending (lower cosine distance = better match)
        hits.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // When hops > 0, allow results beyond k to surface graph-expanded hits.
        // Cap at k + per-hop budget so output doesn't explode.
        let cap = if hops > 0 {
            k + (hops as usize * k / 2).max(5)
        } else {
            k
        };
        hits.truncate(cap);
        Ok(hits)
    }

    /// Open a graph database at the given path
    ///
    /// # Arguments
    /// * `db_path` - Path to the database file (created if not exists)
    ///
    /// # Returns
    /// A new CodeGraph instance
    pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        // Convert to PathBuf for reuse
        let db_path_buf = db_path.as_ref().to_path_buf();

        // Phase 1: read-only compatibility preflight for existing DB files.
        // This MUST run before any sqlitegraph or Magellan side-table writes occur.
        {
            db_compat::preflight_sqlitegraph_compat(&db_path_buf)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }

        // Phase 2: Backend opening
        #[cfg(feature = "sqlite-backend")]
        #[allow(
            clippy::arc_with_non_send_sync,
            reason = "sqlitegraph backend is used single-threaded"
        )]
        let (backend, sqlite_backend): (
            Arc<dyn GraphBackend>,
            Option<Arc<sqlitegraph::SqliteGraphBackend>>,
        ) = {
            use sqlitegraph::{SqliteGraph, SqliteGraphBackend};
            let cfg = sqlitegraph::SqliteConfig::new().with_pool_size(1);
            let sqlite_graph = SqliteGraph::open_with_config(&db_path_buf, &cfg)?;
            eprintln!("Using SQLite backend: {:?}", db_path_buf);
            let sqlite_backend = Arc::new(SqliteGraphBackend::from_graph(sqlite_graph));
            let backend: Arc<dyn GraphBackend> = { (sqlite_backend.clone()) as _ };
            (backend, Some(sqlite_backend))
        };

        #[cfg(not(feature = "sqlite-backend"))]
        compile_error!("'sqlite-backend' feature must be enabled");

        // Phase 2b: Configure SQLite performance PRAGMAs
        #[cfg(feature = "sqlite-backend")]
        {
            // Note: sqlitegraph 1.0.0 already configures these in from_connection(),
            // but we set them explicitly here to ensure they're applied even if
            // sqlitegraph changes its defaults in future versions.
            //
            // These PRAGMA settings are configured on a separate connection but affect
            // the entire database file (PRAGMA is file-level, not connection-level).
            //
            // Scoped block ensures connection closes even if PRAGMA operations fail.
            // Without this scope, early returns via ? would leak the connection.
            let pragma_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
                anyhow::anyhow!("Failed to open connection for PRAGMA config: {}", e)
            })?;

            // WAL mode for better concurrency (allows reads during writes)
            // query() returns the new mode value, execute() would error
            // Note: :memory: databases don't support WAL mode (returns "memory")
            let journal_mode = pragma_conn
                .query_row("PRAGMA journal_mode = WAL", [], |row| {
                    let mode: String = row.get(0)?;
                    Ok(mode)
                })
                .map_err(|e| anyhow::anyhow!("Failed to set WAL mode: {}", e))?;
            // Only assert WAL mode for file-based databases (not :memory:)
            if !is_memory_db(&db_path_buf) {
                debug_assert_eq!(journal_mode, "wal", "WAL mode should be enabled");
            }

            // Faster writes (safe with WAL mode - durability still guaranteed)
            pragma_conn
                .execute("PRAGMA synchronous = NORMAL", [])
                .map_err(|e| anyhow::anyhow!("Failed to set synchronous: {}", e))?;

            // Increase cache (negative value = KB, -64000 = 64MB)
            // Note: sqlitegraph also sets this to -64000, ensuring 64MB cache
            pragma_conn
                .execute("PRAGMA cache_size = -64000", [])
                .map_err(|e| anyhow::anyhow!("Failed to set cache_size: {}", e))?;

            // Temp tables in memory (faster than disk)
            pragma_conn
                .execute("PRAGMA temp_store = MEMORY", [])
                .map_err(|e| anyhow::anyhow!("Failed to set temp_store: {}", e))?;
            // pragma_conn drops automatically here at block end
        }

        // Build initial file_index from database (eager initialization)
        let file_index = HashMap::new();
        let mut files = files::FileOps {
            backend: Arc::clone(&backend),
            file_index,
        };

        // Populate file_index with existing File nodes from database
        files.rebuild_file_index()?;

        // Phase 3: SQLite-specific side-table initialization
        let (side_tables, chunks, execution_log, metrics, telemetry, needs_backfill, side_conn) = {
            // Open ONE shared connection for all Magellan side-table operations.
            // Previously each subsystem opened its own connection (~10 total).
            let side_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
                anyhow::anyhow!("Failed to open shared side-table connection: {}", e)
            })?;
            side_conn.pragma_update(None, "busy_timeout", 5000)?;
            let side_conn_arc = Arc::new(std::sync::Mutex::new(side_conn));

            // Check whether DDL needs to run at all.
            let needs_ddl = db_compat::needs_schema_upgrade(
                &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            // Phase 3a: Magellan-owned DB compatibility metadata.
            // MUST run after sqlitegraph open and before any other Magellan side-table writes.
            db_compat::ensure_magellan_meta(
                &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                &db_path_buf,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            // Create SQLite side tables reusing the shared connection
            let side_tables: Arc<dyn side_tables::SideTables> = Arc::new(
                side_tables::sqlite_impl::SqliteSideTables::with_mutex(Arc::clone(&side_conn_arc))?,
            );

            // Open a shared connection for ChunkStore to enable transactional operations
            // This allows chunk operations to participate in transactions with graph operations
            let shared_conn = rusqlite::Connection::open(&db_path_buf).map_err(|e| {
                anyhow::anyhow!("Failed to open shared connection for ChunkStore: {}", e)
            })?;
            shared_conn.pragma_update(None, "busy_timeout", 5000)?;

            // Initialize ChunkStore with shared connection and ensure schema exists
            let chunks = ChunkStore::with_connection(shared_conn);
            chunks.ensure_schema()?;

            // Initialize ExecutionLog reusing the shared connection
            let execution_log =
                execution_log::ExecutionLog::with_connection(Arc::clone(&side_conn_arc));

            // Initialize MetricsOps reusing the shared connection
            let metrics = metrics::MetricsOps::with_connection(Arc::clone(&side_conn_arc));

            // Initialize TelemetryOps reusing the shared connection
            let telemetry = telemetry::TelemetryOps::with_connection(Arc::clone(&side_conn_arc));

            // Only run AST / CFG / coverage DDL when the schema is new or was upgraded.
            // On warm opens this skips ~6 redundant CREATE TABLE IF NOT EXISTS calls.
            if needs_ddl {
                db_compat::ensure_ast_schema(
                    &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                db_compat::ensure_cfg_schema(
                    &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                db_compat::ensure_metrics_schema(
                    &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                db_compat::ensure_source_inventory_schema(
                    &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                db_compat::ensure_candidate_fact_schema(
                    &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }

            // Coverage schema is not versioned in magellan_meta; always ensure it.
            db_compat::ensure_coverage_schema(
                &side_conn_arc.lock().unwrap_or_else(|e| e.into_inner()),
                &db_path_buf,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            // Detect if this is an upgrade (metrics tables exist but are empty)
            let needs_backfill = {
                // Check if metrics tables are empty
                let metric_count: i64 = side_conn_arc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .query_row("SELECT COUNT(*) FROM file_metrics", [], |row| row.get(0))
                    .unwrap_or(0);

                // Also check if we have symbols (indicating existing database)
                let symbol_count: i64 = side_conn_arc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .query_row(
                        "SELECT COUNT(*) FROM graph_entities WHERE kind = 'Symbol'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                // Backfill needed if: no metrics but we have symbols
                metric_count == 0 && symbol_count > 0
            };

            (
                side_tables,
                chunks,
                execution_log,
                metrics,
                telemetry,
                needs_backfill,
                side_conn_arc,
            )
        };

        // Initialize file node cache with capacity of 128 entries
        let file_node_cache = cache::FileNodeCache::new(128);

        // Initialize module resolver
        let project_root = db_path_buf
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let module_resolver =
            module_resolver::ModuleResolver::new(Arc::clone(&backend), project_root);

        let mut graph = Self {
            files,
            symbols: symbols::SymbolOps {
                backend: Arc::clone(&backend),
                lookup: symbol_lookup::SymbolLookup::new(),
                sqlite_backend: sqlite_backend.clone(),
                batch_mode: true,
            },
            references: references::ReferenceOps {
                backend: Arc::clone(&backend),
                sqlite_backend: sqlite_backend.clone(),
                batch_mode: true,
            },
            calls: call_ops::CallOps {
                backend: Arc::clone(&backend),
                sqlite_backend: sqlite_backend.clone(),
                batch_mode: true,
            },
            imports: imports::ImportOps {
                backend: Arc::clone(&backend),
            },
            module_resolver,
            chunks: chunks.clone(),
            execution_log,
            metrics,
            telemetry,
            file_node_cache,
            cfg_ops: cfg_ops::CfgOps::new(chunks),
            side_tables,
            side_conn,
            batch_mode: true,
            embeddings_enabled: false,
            embedder: crate::graph::embed::create_embedder(
                &crate::config::EmbedProvider::Hash,
                false,
                "",
                "",
                "",
                0,
            ),
            db_path: db_path_buf,
        };

        // Build module index for path resolution
        // This enables import resolution during indexing
        let _ = graph.module_resolver.build_module_index();

        // Build symbol lookup index for O(1) resolution
        // This is a one-time cost (~50-100ms for 10k symbols) that enables fast lookups
        if let Err(e) = graph.symbols.lookup.rebuild_from_backend(&*backend) {
            eprintln!("Warning: Failed to build symbol lookup index: {}", e);
        }

        // Trigger backfill if we have existing symbols but no metrics
        if needs_backfill {
            let _ = graph.backfill_metrics(None);
        }

        if let Ok(cfg) = crate::config::load() {
            if cfg.embeddings.enabled {
                graph.configure_embeddings(
                    &cfg.embeddings.provider,
                    cfg.embeddings.enabled,
                    &cfg.embeddings.base_url,
                    &cfg.embeddings.model,
                    &cfg.embeddings.api_key,
                    cfg.embeddings.num_ctx,
                );
            }
        }

        Ok(graph)
    }

    /// Checkpoint the SQLite WAL to prevent unbounded growth.
    pub fn checkpoint_wal(&self) -> Result<()> {
        let conn = self
            .side_conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        wal::checkpoint_conn(&conn).map_err(|e| anyhow::anyhow!("WAL checkpoint failed: {}", e))
    }

    /// Rebuild the FTS5 search index using the existing side connection.
    ///
    /// This is the preferred method for rebuilding FTS5 during watch/indexing
    /// because it reuses the secondary connection instead of opening a new one,
    /// preventing uncoordinated WAL access that can corrupt the database on
    /// process termination.
    pub fn rebuild_fts5(&self) -> Result<()> {
        let conn = self
            .side_conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        conn.execute("INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild')", [])
            .map_err(|e| anyhow::anyhow!("FTS5 rebuild failed: {}", e))?;
        Ok(())
    }

    /// Embed symbols from DB without re-parsing source files.
    ///
    /// Reads entity metadata from the database, finds symbols missing HNSW vectors,
    /// reads each source file once, extracts bodies via byte offsets, and embeds
    /// in batches. Returns (embedded_count, skipped_count, failed_count).
    ///
    /// If `force` is true, re-embeds all symbols regardless of existing vectors.
    /// `progress_callback` is called per file group with (file_path, symbols_in_file, file_index, total_files).
    /// `num_parallel` controls how many concurrent HTTP embedding requests are fired (default 4).
    pub fn embed_from_db(
        &mut self,
        force: bool,
        batch_size: usize,
        num_parallel: usize,
        mut progress_callback: impl FnMut(&str, usize, usize, usize),
    ) -> Result<(usize, usize, usize)> {
        use std::collections::{HashMap, HashSet};

        if !self.embeddings_enabled {
            anyhow::bail!("Embeddings not enabled");
        }

        // Step 1: Query all Symbol entities from side_conn
        let entities: Vec<(i64, String, String, String)> = {
            let conn = self.side_conn.lock().unwrap();
            let mut stmt = conn.prepare_cached(
                "SELECT id, name, file_path, data FROM graph_entities WHERE kind = 'Symbol' ORDER BY file_path, id"
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };

        let total = entities.len();

        // Step 2: If force, clear existing HNSW vectors for the 'symbols' index.
        // Otherwise, find which entity IDs already have vectors and skip them.
        let skip_ids: HashSet<i64> = if force {
            {
                let sg = self.symbols.sqlite_graph()?;
                search::clear_search_index(sg)?;
            }
            // Reset AUTOINCREMENT counters so new inserts start from 1.
            // Without this, IDs resume from the old high-water mark, causing
            // a mismatch between global vector IDs and local layer IDs.
            let conn = self.side_conn.lock().unwrap();
            conn.execute_batch(
                "DELETE FROM sqlite_sequence WHERE name IN ('hnsw_vectors', 'hnsw_layers')"
            )?;
            HashSet::new()
        } else {
            let conn = self.side_conn.lock().unwrap();
            let index_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM hnsw_indexes WHERE name = 'symbols'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !index_exists {
                HashSet::new()
            } else {
                let mut stmt = conn.prepare_cached(
                    "SELECT v.metadata FROM hnsw_vectors v JOIN hnsw_indexes i ON v.index_id = i.id WHERE i.name = 'symbols'"
                )?;
                let meta_strings: Vec<String> = stmt
                    .query_map([], |row| row.get::<_, String>(0))?
                    .filter_map(|r| r.ok())
                    .collect();
                meta_strings
                    .into_iter()
                    .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .filter_map(|v| v.get("entity_id")?.as_i64())
                    .collect()
            }
        };

        // Step 3: Filter to entities that need embedding
        let to_embed: Vec<(i64, String, String, String)> = entities
            .into_iter()
            .filter(|(id, _, _, _)| !skip_ids.contains(id))
            .collect();

        let skipped = total - to_embed.len();

        if to_embed.is_empty() {
            return Ok((0, skipped, 0));
        }

        // Step 4: Group by file_path
        let mut by_file: HashMap<String, Vec<(i64, String, String, String)>> = HashMap::new();
        for ent in to_embed {
            by_file.entry(ent.2.clone()).or_default().push(ent);
        }
        let mut file_groups: Vec<_> = by_file.into_iter().collect();
        file_groups.sort_by(|a, b| a.0.cmp(&b.0));
        let total_files = file_groups.len();

        // Step 5: Embed per file
        let mut embedded_count = 0usize;
        let mut failed_count = 0usize;

        // Resolve project root from db_path
        let root = self
            .db_path
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."));

        let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

        // Create the thread pool once for the entire embed run (not per file).
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_parallel)
            .build()
            .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

        for (file_idx, (file_path, file_entities)) in file_groups.iter().enumerate() {
            let is_absolute = file_path.starts_with('/');
            let full_path = if is_absolute {
                PathBuf::from(file_path)
            } else {
                root.join(file_path)
            };

            let full_path_canonical = match full_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    failed_count += file_entities.len();
                    progress_callback(file_path, file_entities.len(), file_idx, total_files);
                    continue;
                }
            };

            // Absolute paths from the DB are trusted (stored during indexing).
            // Only apply boundary check to relative paths we constructed from root.
            if !is_absolute && !full_path_canonical.starts_with(&root_canonical) {
                failed_count += file_entities.len();
                progress_callback(file_path, file_entities.len(), file_idx, total_files);
                continue;
            }

            let source = match std::fs::read(&full_path_canonical) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("embed: read failed {:?}: {}", full_path_canonical, e);
                    failed_count += file_entities.len();
                    progress_callback(file_path, file_entities.len(), file_idx, total_files);
                    continue;
                }
            };

            let source_bytes = &source;

            // Build embed texts using symbol_fact_embed_text with source body
            let mut texts = Vec::with_capacity(file_entities.len());
            let mut ids = Vec::with_capacity(file_entities.len());

            for (id, name, _, data_str) in file_entities {
                let data: serde_json::Value =
                    serde_json::from_str(data_str).unwrap_or_else(|_| serde_json::json!({}));
                let byte_start =
                    data.get("byte_start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let byte_end = data.get("byte_end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let kind_normalized = data
                    .get("kind_normalized")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let body = if byte_end > byte_start && byte_end <= source_bytes.len() {
                    let body_raw = crate::common::extract_symbol_content_safe(
                        source_bytes,
                        byte_start,
                        byte_end,
                    );
                    match body_raw {
                        Some(b) if !b.trim().is_empty() => Some(b),
                        _ => None,
                    }
                } else {
                    None
                };

                let name_opt = Some(name.clone());
                let text = embed::symbol_fact_embed_text(
                    &name_opt,
                    file_path,
                    kind_normalized,
                    body.as_deref(),
                );
                texts.push(text);
                ids.push(*id);
            }

            // Split into chunks, embed all chunks in parallel, write results serially.
            // TextEmbedder: Sync, so &dyn TextEmbedder is safe to share across rayon threads.
            let chunks: Vec<(&[String], &[i64])> = (0..texts.len())
                .step_by(batch_size)
                .map(|s| {
                    let e = (s + batch_size).min(texts.len());
                    (&texts[s..e], &ids[s..e])
                })
                .collect();

            type ChunkResult = Result<Vec<(i64, Vec<f32>)>>;

            let embedder_ref: &dyn embed::TextEmbedder = self.embedder.as_ref();

            let chunk_results: Vec<ChunkResult> = pool.install(|| {
                use rayon::prelude::*;
                chunks
                    .par_iter()
                    .map(|(chunk_texts, chunk_ids)| {
                        let text_refs: Vec<&str> = chunk_texts.iter().map(|s| s.as_str()).collect();
                        let vectors = embedder_ref.embed_batch(&text_refs)?;
                        Ok(chunk_ids
                            .iter()
                            .zip(vectors)
                            .map(|(id, vec)| (*id, vec))
                            .collect())
                    })
                    .collect()
            });

            for result in chunk_results {
                match result {
                    Ok(entries) => {
                        let sg = self.symbols.sqlite_graph()?;
                        match search::bulk_add_to_search_index(sg, &entries) {
                            Ok(n) => embedded_count += n,
                            Err(e) => {
                                tracing::warn!("embed: bulk insert failed: {}", e);
                                failed_count += entries.len();
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("embed: embed_batch failed: {}", e);
                        failed_count += batch_size;
                    }
                }
            }

            progress_callback(file_path, file_entities.len(), file_idx, total_files);
        }

        Ok((embedded_count, skipped, failed_count))
    }

    /// Index a file into the graph (idempotent)
    ///
    /// # Behavior
    /// 1. Compute SHA-256 hash of file contents
    /// 2. Upsert File node with path and hash
    /// 3. DELETE all existing Symbol nodes and DEFINES edges for this file
    /// 4. Parse symbols from source code
    /// 5. Insert new Symbol nodes
    /// 6. Create DEFINES edges from File to each Symbol
    /// 7. Index calls (CALLS edges)
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of symbols indexed
    pub fn index_file(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        ops::index_file(self, path, source)
    }

    /// Delete a file and all derived data from the graph
    ///
    /// This delegates to `delete_file_facts` which removes *all* file-derived facts
    /// (symbols, references, calls, chunks, file node).
    ///
    /// # Returns
    /// DeleteResult with counts of deleted entities
    pub fn delete_file(&mut self, path: &str) -> Result<DeleteResult> {
        ops::delete_file(self, path)
    }

    /// Delete ALL facts derived from a file path.
    ///
    /// This is the authoritative deletion path used by reconcile.
    ///
    /// # Returns
    /// DeleteResult with counts of deleted entities
    pub fn delete_file_facts(&mut self, path: &str) -> Result<DeleteResult> {
        ops::delete_file_facts(self, path)
    }

    /// Query all symbols defined in a file
    ///
    /// # Arguments
    /// * `path` - File path
    ///
    /// # Returns
    /// Vector of SymbolFact for all symbols in the file
    pub fn symbols_in_file(&mut self, path: &str) -> Result<Vec<crate::ingest::SymbolFact>> {
        query::symbols_in_file(self, path)
    }

    /// Query symbols defined in a file, optionally filtered by kind
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `kind` - Optional symbol kind filter (None returns all symbols)
    ///
    /// # Returns
    /// Vector of SymbolFact matching the kind filter
    pub fn symbols_in_file_with_kind(
        &mut self,
        path: &str,
        kind: Option<crate::ingest::SymbolKind>,
    ) -> Result<Vec<crate::ingest::SymbolFact>> {
        query::symbols_in_file_with_kind(self, path, kind)
    }

    /// Query symbol facts along with their node IDs for deterministic ordering/output.
    pub fn symbol_nodes_in_file(
        &mut self,
        path: &str,
    ) -> Result<Vec<(i64, crate::ingest::SymbolFact)>> {
        query::symbol_nodes_in_file(self, path)
    }

    /// Query the node ID of a specific symbol by file path and symbol name
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `name` - Symbol name
    ///
    /// # Returns
    /// `Option<i64>` - Some(node_id) if found, None if not found
    ///
    /// # Note
    /// This is a minimal query helper for testing. It reuses existing graph queries
    /// and maintains determinism. No new indexes or caching.
    pub fn symbol_id_by_name(&mut self, path: &str, name: &str) -> Result<Option<i64>> {
        query::symbol_id_by_name(self, path, name)
    }

    /// Index references for a file into the graph
    ///
    /// # Behavior
    /// 1. Parse symbols from source
    /// 2. Extract references to those symbols
    /// 3. Insert Reference nodes
    /// 4. Create REFERENCES edges from Reference to Symbol
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of references indexed
    pub fn index_references(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        query::index_references(self, path, source)
    }

    /// Query all references to a specific symbol
    ///
    /// # Arguments
    /// * `symbol_id` - Node ID of the target symbol
    ///
    /// # Returns
    /// Vector of ReferenceFact for all references to the symbol
    pub fn references_to_symbol(&mut self, symbol_id: i64) -> Result<Vec<ReferenceFact>> {
        query::references_to_symbol(self, symbol_id)
    }

    /// Lookup symbol extent (byte + line span) for a specific symbol name in a file.
    pub fn symbol_extents(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<(i64, crate::ingest::SymbolFact)>> {
        query::symbol_extents(self, path, name)
    }

    /// Index calls for a file into the graph
    ///
    /// # Behavior
    /// 1. Get file node ID
    /// 2. Get all symbols for this file
    /// 3. Extract calls from source
    /// 4. Insert Call nodes and CALLS edges
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `source` - File contents as bytes
    ///
    /// # Returns
    /// Number of calls indexed
    pub fn index_calls(&mut self, path: &str, source: &[u8]) -> Result<usize> {
        calls::index_calls(self, path, source)
    }

    /// Query all calls FROM a specific symbol (forward call graph)
    ///
    /// # Arguments
    /// * `path` - File path containing the symbol
    /// * `name` - Symbol name
    ///
    /// # Returns
    /// Vector of CallFact for all calls from this symbol
    pub fn calls_from_symbol(&mut self, path: &str, name: &str) -> Result<Vec<CallFact>> {
        calls::calls_from_symbol(self, path, name)
    }

    /// Query all calls TO a specific symbol (reverse call graph)
    ///
    /// # Arguments
    /// * `path` - File path containing the symbol
    /// * `name` - Symbol name
    ///
    /// # Returns
    /// Vector of CallFact for all calls to this symbol
    pub fn callers_of_symbol(&mut self, path: &str, name: &str) -> Result<Vec<CallFact>> {
        calls::callers_of_symbol(self, path, name)
    }

    /// Count total number of files in the graph
    pub fn count_files(&self) -> Result<usize> {
        count::count_files(self)
    }

    /// Count total number of symbols in the graph
    pub fn count_symbols(&self) -> Result<usize> {
        count::count_symbols(self)
    }

    /// Count total number of references in the graph
    pub fn count_references(&self) -> Result<usize> {
        count::count_references(self)
    }

    /// Count total number of calls in the graph
    pub fn count_calls(&self) -> Result<usize> {
        count::count_calls(self)
    }

    /// Count total number of CFG blocks in the graph
    ///
    /// Note: Returns 0 for SQLite backend.
    pub fn count_cfg_blocks(&self) -> Result<usize> {
        Ok(0)
    }

    /// Check if coverage schema tables exist in the database.
    ///
    /// Returns true if all three coverage tables are present.
    pub fn check_coverage_schema(&self) -> Result<bool> {
        let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| {
            anyhow::anyhow!("Failed to open connection for coverage schema check: {}", e)
        })?;
        let tables = [
            "cfg_block_coverage",
            "cfg_edge_coverage",
            "cfg_coverage_meta",
        ];
        for table in tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if count == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Get combined statistics for the graph
    ///
    /// Returns symbol count, file count, and cfg block count
    pub fn get_stats(&self) -> Result<GraphStats> {
        Ok(GraphStats {
            symbol_count: self.count_symbols()?,
            file_count: self.count_files()?,
            cfg_block_count: 0, // CFG blocks not tracked in SQLite backend
        })
    }

    /// Reconcile a file path against filesystem + content hash.
    ///
    /// This is the deterministic primitive used by scan and watcher updates.
    pub fn reconcile_file_path(&mut self, path: &Path, path_key: &str) -> Result<ReconcileOutcome> {
        ops::reconcile_file_path(self, path, path_key)
    }

    /// Reconcile a file path using pre-read source bytes.
    ///
    /// Same as `reconcile_file_path` but avoids re-reading from disk.
    pub fn reconcile_file_path_with_source(
        &mut self,
        path: &Path,
        path_key: &str,
        source: &[u8],
    ) -> Result<ReconcileOutcome> {
        ops::reconcile_file_path_with_source(self, path, path_key, source)
    }

    /// Scan a directory and index all Rust files found
    ///
    /// # Behavior
    /// 1. Walk directory recursively
    /// 2. Find all .rs files
    /// 3. Index each file (symbols + references)
    /// 4. Report progress via callback
    ///
    /// # Arguments
    /// * `dir_path` - Directory to scan
    /// * `progress` - Optional callback for progress reporting (current, total)
    ///
    /// # Returns
    /// Number of files indexed
    ///
    /// # Guarantees
    /// - Only .rs files are processed
    /// - Files are indexed in sorted order for determinism
    /// - Non-.rs files are silently skipped
    pub fn scan_directory(
        &mut self,
        dir_path: &Path,
        progress: Option<&ScanProgress>,
    ) -> Result<usize> {
        scan::scan_directory(self, dir_path, progress)
    }

    /// Scan a directory with a pre-built `FileFilter`.
    ///
    /// Use when include/exclude patterns come from project config.
    pub fn scan_directory_with_filter(
        &mut self,
        dir_path: &Path,
        filter: &filter::FileFilter,
        progress: Option<&ScanProgress>,
    ) -> Result<ScanResult> {
        scan::scan_directory_with_filter(self, dir_path, filter, progress)
    }

    /// Async version of scan_directory with parallel file reading
    ///
    /// Uses tokio for async file I/O, improving performance on slow filesystems.
    /// Graph operations remain synchronous (CodeGraph is not Send).
    ///
    /// # Arguments
    /// * `dir_path` - Directory to scan recursively
    /// * `progress` - Optional callback for progress updates (current, total)
    ///
    /// # Returns
    /// Number of files indexed
    pub async fn scan_directory_async(
        &mut self,
        dir_path: &Path,
        progress: Option<&ScanProgress>,
    ) -> Result<usize> {
        let filter = filter::FileFilter::new(dir_path, &[], &[])?;
        let result = scan::scan_directory_async(self, dir_path, &filter, progress).await?;
        Ok(result.indexed)
    }

    /// Backfill metrics for all existing files in the database
    ///
    /// This is called automatically after database migration to schema version 5.
    /// Can also be called manually to recompute metrics.
    ///
    /// # Arguments
    /// * `progress` - Optional callback for progress updates (current, total)
    ///
    /// # Returns
    /// BackfillResult with total files processed and any errors
    pub fn backfill_metrics(
        &mut self,
        progress: Option<&ScanProgress>,
    ) -> Result<metrics::BackfillResult> {
        self.metrics.backfill_all_metrics(progress)
    }

    /// Export all graph data to JSON format
    ///
    /// # Returns
    /// JSON string containing all files, symbols, references, and calls
    pub fn export_json(&mut self) -> Result<String> {
        export::export_json(self)
    }

    /// Get the FileNode for a given file path
    ///
    /// # Arguments
    /// * `path` - File path to query
    ///
    /// # Returns
    /// `Option<FileNode>` with file metadata including timestamps, or None if not found
    pub fn get_file_node(&mut self, path: &str) -> Result<Option<FileNode>> {
        // Check cache first
        if let Some(node) = self.file_node_cache.get(&path.to_string()) {
            return Ok(Some(node.clone()));
        }

        // Cache miss - query database
        let result = self.files.get_file_node(path)?;

        // Store in cache if found
        if let Some(ref node) = result {
            self.file_node_cache.put(path.to_string(), node.clone());
        }

        Ok(result)
    }

    /// Get all FileNodes from the database
    ///
    /// # Returns
    /// HashMap of file path -> FileNode for all files in the database
    pub fn all_file_nodes(&mut self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.files.all_file_nodes()
    }

    /// Get all FileNodes from the database (read-only, doesn't require mutation).
    ///
    /// # Returns
    /// HashMap of file path -> FileNode for all files in the database
    pub fn all_file_nodes_readonly(&self) -> Result<std::collections::HashMap<String, FileNode>> {
        self.files.all_file_nodes_readonly()
    }

    /// Get code chunks for a specific file.
    ///
    /// # Arguments
    /// * `file_path` - File path to query
    ///
    /// # Returns
    /// Vector of CodeChunk for all chunks in the file
    pub fn get_code_chunks(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
        self.chunks.get_chunks_for_file(file_path)
    }

    /// Get code chunks for a specific symbol in a file.
    ///
    /// # Arguments
    /// * `file_path` - File path containing the symbol
    /// * `symbol_name` - Symbol name to query
    ///
    /// # Returns
    /// Vector of CodeChunk for the symbol (may be multiple for overloads)
    pub fn get_code_chunks_for_symbol(
        &self,
        file_path: &str,
        symbol_name: &str,
    ) -> Result<Vec<CodeChunk>> {
        self.chunks.get_chunks_for_symbol(file_path, symbol_name)
    }

    /// Get a code chunk by exact byte span.
    ///
    /// # Arguments
    /// * `file_path` - File path containing the chunk
    /// * `byte_start` - Starting byte offset
    /// * `byte_end` - Ending byte offset
    ///
    /// # Returns
    /// `Option<CodeChunk>` if found, None otherwise
    pub fn get_code_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>> {
        self.chunks
            .get_chunk_by_span(file_path, byte_start, byte_end)
    }

    /// Store code chunks for a file.
    ///
    /// # Arguments
    /// * `chunks` - Code chunks to store
    ///
    /// # Returns
    /// Vector of inserted chunk IDs
    pub fn store_code_chunks(&self, chunks: &[CodeChunk]) -> Result<Vec<i64>> {
        self.chunks.store_chunks(chunks)
    }

    /// Count total code chunks stored.
    pub fn count_chunks(&self) -> Result<usize> {
        self.chunks.count_chunks()
    }

    /// Get the execution log for recording command execution
    pub fn execution_log(&self) -> &execution_log::ExecutionLog {
        &self.execution_log
    }

    /// Get the metrics operations module
    pub fn metrics(&self) -> &metrics::MetricsOps {
        &self.metrics
    }

    /// Get the telemetry operations module
    pub fn telemetry(&self) -> &telemetry::TelemetryOps {
        &self.telemetry
    }

    /// Validate graph invariants post-run
    ///
    /// Checks for orphan references, orphan calls, and other structural issues.
    /// This is a convenience method that calls validation::validate_graph().
    ///
    /// # Returns
    /// ValidationReport with validation results (errors, warnings, passed status)
    pub fn validate_graph(&mut self) -> validation::ValidationReport {
        validation::validate_graph(self).unwrap_or_else(|e| validation::ValidationReport {
            passed: false,
            errors: vec![validation::ValidationError::new(
                "VALIDATION_ERROR".to_string(),
                format!("Validation failed with error: {}", e),
            )],
            warnings: Vec::new(),
        })
    }

    /// Get cache statistics for monitoring cache effectiveness
    ///
    /// # Returns
    /// CacheStats with hits, misses, size, and hit rate
    pub fn cache_stats(&self) -> CacheStats {
        self.file_node_cache.stats()
    }

    /// Invalidate cache entry for a specific file path
    ///
    /// This should be called when a file is modified or deleted to ensure
    /// cache doesn't return stale data.
    ///
    /// # Arguments
    /// * `path` - File path to invalidate
    pub fn invalidate_cache(&mut self, path: &str) {
        self.file_node_cache.invalidate(&path.to_string());
    }

    /// Clear all cache entries
    ///
    /// This resets the cache to empty state, useful for testing or after bulk operations.
    pub fn clear_cache(&mut self) {
        self.file_node_cache.clear();
    }

    /// Get backend for testing/benchmarking
    ///
    /// This method provides access to the underlying graph backend for
    /// performance testing and internal operations.
    ///
    /// # WARNING
    /// This is for benchmarking only. Direct backend access bypasses CodeGraph's
    /// transactional and caching layers.
    #[doc(hidden)]
    pub fn __backend_for_benchmarks(&self) -> &std::sync::Arc<dyn sqlitegraph::GraphBackend> {
        &self.files.backend
    }

    /// Rebuild FTS5 symbol search index
    ///
    /// Rebuilds the FTS5 virtual table (symbol_fts) that indexes symbol names
    /// from graph_entities for fast prefix and full-text search.
    ///
    /// This should be called after batch indexing operations to ensure the
    /// FTS5 index is synchronized with graph_entities.
    ///
    /// # Performance
    /// Typically ~500ms for 1,000 files. Call after batch completion, not per-file.
    ///
    /// # Safety
    /// This function opens its own SQLite connection. Do not call during watch mode
    /// where other connections may be writing to the WAL. Use `CodeGraph::rebuild_fts5()`
    /// for watch-mode safe FTS5 rebuild via the shared side_conn.
    ///
    /// # Arguments
    /// * `db_path` - Path to the database file (needed for direct SQLite connection)
    pub fn rebuild_fts5_index(db_path: &Path) -> Result<()> {
        use rusqlite::Connection;

        // Open direct SQLite connection for FTS5 rebuild
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "busy_timeout", 5000)?;

        // Rebuild FTS5 index - this scans all rows in graph_entities and rebuilds
        conn.execute("INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild')", [])?;

        Ok(())
    }

    /// Get backend reference.
    ///
    /// This method provides access to the underlying graph backend.
    ///
    /// # WARNING
    /// This is internal API. Direct backend access bypasses CodeGraph's
    /// transactional and caching layers.
    #[doc(hidden)]
    pub fn __backend_for_watcher(&self) -> &std::sync::Arc<dyn sqlitegraph::GraphBackend> {
        &self.files.backend
    }

    /// Get backend reference for backend router operations.
    ///
    /// This method provides access to the underlying graph backend for
    /// backend-agnostic operations across SQLite and V3 backends.
    ///
    /// # WARNING
    /// This is internal API. Direct backend access bypasses CodeGraph's
    /// transactional and caching layers.
    #[doc(hidden)]
    pub fn backend(&self) -> &std::sync::Arc<dyn sqlitegraph::GraphBackend> {
        &self.files.backend
    }

    // ===== V3-Exclusive KV Operations =====
    // These methods are ONLY available with the V3 backend.
    // They will return None or error when using SQLite backend.

    /// Get symbol node by entity ID
    ///
    /// Returns the full SymbolNode data for a given entity ID.
    /// Works with both SQLite and V3 backends.
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID to look up
    ///
    /// # Returns
    /// Some(SymbolNode) if found and is a Symbol, None otherwise
    pub fn get_symbol_by_entity_id(&self, entity_id: i64) -> Option<SymbolNode> {
        use sqlitegraph::SnapshotId;

        let snapshot = SnapshotId::current();
        match self.files.backend.get_node(snapshot, entity_id) {
            Ok(node) => {
                if node.kind != "Symbol" {
                    return None;
                }
                serde_json::from_value(node.data).ok()
            }
            Err(_) => None,
        }
    }

    // ===== Label Operations =====
    // Note: add_label uses side_tables, other label ops delegate to query.rs

    /// Add a label to an entity (uses side_tables)
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID to label
    /// * `label` - The label to add
    pub fn add_label(&self, entity_id: i64, label: &str) -> Result<()> {
        self.side_tables.add_label(entity_id, label)
    }

    /// Get all labels for an entity (uses side_tables)
    ///
    /// # Arguments
    /// * `entity_id` - The entity ID
    pub fn get_labels_for_entity(&self, entity_id: i64) -> Result<Vec<String>> {
        self.side_tables.get_labels_for_entity(entity_id)
    }
}

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
mod algorithms_traversal;
mod algorithms_types;
pub mod ambiguity;
pub mod backend;
pub mod candidate_fact;
pub mod ontology;
pub mod source_inventory;

#[cfg(feature = "mir-frontend")]
pub mod mir_frontend;
// pub mod memory_graph;

// Re-export MemoryGraph types for public API
// Note: GraphStats is not re-exported here due to name collision with CodeGraph's GraphStats
// Access via graph::memory_graph::GraphStats if needed
// pub use memory_graph::{GraphSymbol, MemoryGraph};
mod ast_extractor;
pub mod external_tools;

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
mod facts_api;
mod files;
pub mod filter;
mod freshness;
mod imports; // Private module for import operations
mod maintenance;
pub mod metrics;
mod module_resolver;
pub mod multi_db;
pub mod navigator;
mod ops;
pub mod query;
mod references;
mod runtime;
pub mod scan;
pub mod schema;
pub mod scorer;
pub mod search;
pub mod side_tables;
mod symbol_lookup;
pub(crate) mod symbols;
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

use crate::graph::runtime::{is_memory_db, SqliteRuntimeComponents};
use crate::graph::scan::ScanResult;

use crate::generation::ChunkStore;
use crate::references::CallFact;

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

pub use cache::{CacheStats, EntityCacheKey, ExpandCacheKey, NameCacheKey, ThreadSafeCache};
pub use db_compat::MAGELLAN_SCHEMA_VERSION;
pub use db_compat::{
    ensure_ast_schema, ensure_candidate_fact_schema, ensure_cfg_schema, ensure_coverage_schema,
    ensure_source_inventory_schema, ensure_telemetry_schema, ensure_temporal_schema, CFG_EDGE,
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

/// A stitched interprocedural edge from a caller CFG block to a callee entry block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectCallIcfgEdge {
    /// Persisted call-site fact.
    pub call: CallFact,
    /// Caller function symbol ID.
    pub caller_symbol_id: i64,
    /// Callee function symbol ID.
    pub callee_symbol_id: i64,
    /// Index of the caller CFG block containing the call site.
    pub caller_block_idx: usize,
    /// Index of the callee CFG entry block.
    pub callee_entry_block_idx: usize,
    /// Index of the caller CFG block that resumes after the call, if any.
    pub caller_resume_block_idx: Option<usize>,
    /// Indices of callee CFG blocks that return control to the caller.
    pub callee_return_block_indices: Vec<usize>,
}

/// Progress callback for scan_directory
///
/// Receives (current_count, total_count, current_file_path) as scanning progresses
pub type ScanProgress = dyn Fn(usize, usize, &str) + Send + Sync;

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

    /// Navigator query caches (thread-safe for `&self` access)
    entity_cache: cache::ThreadSafeCache<cache::EntityCacheKey, navigator::SymbolInfo>,
    name_cache: cache::ThreadSafeCache<cache::NameCacheKey, Vec<navigator::SymbolInfo>>,
    expand_cache: cache::ThreadSafeCache<cache::ExpandCacheKey, Vec<navigator::TypedEdgeHop>>,

    /// CFG block operations module
    pub cfg_ops: cfg_ops::CfgOps,

    /// Side tables for backend-agnostic storage (chunks, AST, metrics, etc.)
    side_tables: Arc<dyn side_tables::SideTables>,

    /// Shared SQLite connection for Magellan side-table operations.
    /// Eliminates redundant connections opened by schema checks and diagnostics.
    /// Uses `parking_lot::Mutex` for fast uncontended locking without poison overhead.
    pub(crate) side_conn: Arc<parking_lot::Mutex<rusqlite::Connection>>,

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

    /// Cached compile_commands.json for per-file C/C++ compilation flags.
    /// Set via `set_compile_commands`. Used during LLVM IR CFG extraction.
    pub(crate) compile_commands:
        Option<std::sync::Arc<external_tools::compile_commands::CompileCommandsDb>>,

    /// Database file path for re-opening connections
    db_path: PathBuf,
}

impl CodeGraph {
    /// Get the database file path
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Load and cache compile_commands.json for C/C++ LLVM IR extraction.
    ///
    /// Once set, per-file flags are looked up during indexing and passed to clang.
    /// Call once at watcher/scan startup when the file exists.
    pub fn set_compile_commands(&mut self, path: &Path) -> anyhow::Result<()> {
        let db = external_tools::compile_commands::CompileCommandsDb::load(path)?;
        self.compile_commands = Some(std::sync::Arc::new(db));
        Ok(())
    }

    pub(crate) fn side_connection(&self) -> &Arc<parking_lot::Mutex<rusqlite::Connection>> {
        &self.side_conn
    }

    /// Execute a closure against the shared side-table SQLite connection.
    ///
    /// This keeps command-layer metadata and side-table access on the same
    /// graph-owned connection lifecycle instead of reopening ad hoc handles.
    pub fn with_side_tables_conn<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<T>,
    ) -> Result<T> {
        let conn = self.side_conn.lock();
        f(&conn)
    }

    pub(crate) fn compute_content_hash(&self, source: &[u8]) -> String {
        self.files.compute_hash(source)
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
        self.embedder = crate::graph::embed::create_embedder(
            provider, enabled, base_url, model, api_key, num_ctx,
        );
    }

    #[cfg(test)]
    pub fn enable_embeddings_for_test(&mut self) {
        self.embeddings_enabled = true;
        self.embedder = Box::new(crate::graph::embed::HashEmbedder::new(128));
    }

    /// Check whether embeddings are stale relative to the graph index.
    ///
    /// Compares the most recent file index time (`file_metrics.last_updated`)
    /// against the most recent embedding time (`hnsw_vectors.updated_at`).
    /// Returns a human-readable warning when the graph has been updated more
    /// recently than the embeddings.
    pub fn check_embedding_staleness(&self) -> anyhow::Result<Option<String>> {
        let conn = self.side_conn.lock();

        let max_embed_time: Option<i64> = conn
            .query_row("SELECT MAX(updated_at) FROM hnsw_vectors", [], |row| {
                row.get(0)
            })
            .ok();

        let max_index_time: Option<i64> = conn
            .query_row("SELECT MAX(last_updated) FROM file_metrics", [], |row| {
                row.get(0)
            })
            .ok();

        match (max_embed_time, max_index_time) {
            (Some(embed), Some(index)) if index > embed + 60 => {
                let stale_secs = index - embed;
                let mins = stale_secs / 60;
                let msg = if mins > 0 {
                    format!(
                        "⚠️  Embeddings are ~{} minutes stale. Run `magellan embed --db {}` to refresh.",
                        mins,
                        self.db_path.display()
                    )
                } else {
                    format!(
                        "⚠️  Embeddings are ~{} seconds stale. Run `magellan embed --db {}` to refresh.",
                        stale_secs,
                        self.db_path.display()
                    )
                };
                Ok(Some(msg))
            }
            (None, Some(_)) => {
                // No embeddings at all
                Ok(Some(format!(
                    "⚠️  No embeddings found. Run `magellan embed --db {}` to enable HopGraph.",
                    self.db_path.display()
                )))
            }
            _ => Ok(None),
        }
    }

    pub fn hopgraph_search(
        &self,
        query: &str,
        k: usize,
        hops: u32,
    ) -> anyhow::Result<Vec<crate::graph::search::HopgraphHit>> {
        let sg = self.symbols.sqlite_graph()?;

        // Phase 1: FTS5 seed search — always current, zero maintenance cost.
        let seed_k = if hops > 0 { k * 2 } else { k };
        let raw_hits = {
            let conn = self.side_conn.lock();
            crate::graph::search::fts_search_symbols(&conn, query, seed_k)?
        };

        // (entity_id → (score, hop_distance))
        let mut hit_scores: std::collections::HashMap<i64, (f32, u32)> =
            std::collections::HashMap::new();
        for &(entity_id, score) in &raw_hits {
            hit_scores.insert(entity_id, (score, 0));
        }

        // Phase 2: call-graph BFS from each FTS5 seed via CALLER+CALLS edges.
        // k_hop_filtered has no Both direction, so run two passes and union.
        if hops > 0 && !raw_hits.is_empty() {
            use sqlitegraph::backend::BackendDirection;
            let initial_ids: Vec<i64> = raw_hits.iter().map(|(id, _)| *id).collect();
            for start_id in &initial_ids {
                let seed_score = hit_scores[start_id].0;
                let callers = sg.query().k_hop_filtered(
                    *start_id,
                    hops,
                    BackendDirection::Incoming,
                    &["CALLER"],
                );
                let callees = sg.query().k_hop_filtered(
                    *start_id,
                    hops,
                    BackendDirection::Outgoing,
                    &["CALLS"],
                );
                let neighbors: Vec<i64> = callers
                    .unwrap_or_default()
                    .into_iter()
                    .chain(callees.unwrap_or_default())
                    .collect();
                for (depth, neighbor_id) in neighbors.iter().enumerate() {
                    if hit_scores.contains_key(neighbor_id) {
                        continue;
                    }
                    let d = (depth as u32).min(hops);
                    let score = seed_score * (0.7_f32.powi(d as i32 + 1));
                    hit_scores.insert(*neighbor_id, (score, d + 1));
                }
            }
        }

        // Resolve entity_ids → symbol metadata.
        let all_ids: Vec<i64> = hit_scores.keys().copied().collect();
        let resolved = {
            let conn = self.side_conn.lock();
            navigator::SymbolNavigator::resolve_entities_with_conn(&conn, &all_ids)?
        };
        let mut resolved_map = std::collections::HashMap::new();
        for info in resolved {
            resolved_map.insert(info.id, info);
        }

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

        // Higher score = better match.
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

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
            let cfg = sqlitegraph::SqliteConfig::new().with_pool_size(5);
            let sqlite_graph = if is_memory_db(&db_path_buf) {
                SqliteGraph::open_in_memory_with_config(&cfg)?
            } else {
                SqliteGraph::open_with_config(&db_path_buf, &cfg)?
            };
            eprintln!("Using SQLite backend: {:?}", db_path_buf);
            let sqlite_backend = Arc::new(SqliteGraphBackend::from_graph(sqlite_graph));
            let backend: Arc<dyn GraphBackend> = { (sqlite_backend.clone()) as _ };
            (backend, Some(sqlite_backend))
        };

        #[cfg(not(feature = "sqlite-backend"))]
        compile_error!("'sqlite-backend' feature must be enabled");

        // Phase 2b: Configure SQLite performance PRAGMAs
        #[cfg(feature = "sqlite-backend")]
        runtime::configure_sqlite_pragmas(&db_path_buf)?;

        // Build initial file_index from database (eager initialization)
        let file_index = HashMap::new();
        let mut files = files::FileOps {
            backend: Arc::clone(&backend),
            file_index,
        };

        // Populate file_index with existing File nodes from database
        files.rebuild_file_index()?;

        // Phase 3: SQLite-specific side-table initialization
        let SqliteRuntimeComponents {
            side_tables,
            chunks,
            execution_log,
            metrics,
            telemetry,
            needs_backfill,
            side_conn,
        } = runtime::initialize_sqlite_runtime(&db_path_buf)?;

        // Initialize file node cache with capacity of 128 entries
        let file_node_cache = cache::FileNodeCache::new(128);

        // Initialize navigator query caches (thread-safe)
        let entity_cache = cache::ThreadSafeCache::new(256);
        let name_cache = cache::ThreadSafeCache::new(256);
        let expand_cache = cache::ThreadSafeCache::new(256);

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
            entity_cache,
            name_cache,
            expand_cache,
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
            compile_commands: None,
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
        let conn = self.side_conn.lock();
        wal::checkpoint_conn_with_retry(&conn, 5)
            .map_err(|e| anyhow::anyhow!("WAL checkpoint failed: {}", e))
    }

    /// Rebuild the FTS5 search index using the existing side connection.
    ///
    /// This is the preferred method for rebuilding FTS5 during watch/indexing
    /// because it reuses the secondary connection instead of opening a new one,
    /// preventing uncoordinated WAL access that can corrupt the database on
    /// process termination.
    pub fn rebuild_fts5(&self) -> Result<()> {
        let conn = self.side_conn.lock();
        conn.execute("INSERT INTO symbol_fts(symbol_fts) VALUES('rebuild')", [])
            .map_err(|e| anyhow::anyhow!("FTS5 rebuild failed: {}", e))?;
        Ok(())
    }

    /// Rebuild the code_chunks_fts index after bulk code_chunk changes.
    ///
    /// This mirrors `rebuild_fts5` but for the content search index.
    /// Called from the watch pipeline after batch re-indexing.
    pub fn rebuild_code_chunks_fts(&self) -> Result<()> {
        let conn = self.side_conn.lock();
        conn.execute(
            "INSERT INTO code_chunks_fts(code_chunks_fts) VALUES('rebuild')",
            [],
        )
        .map_err(|e| anyhow::anyhow!("code_chunks_fts rebuild failed: {}", e))?;
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
            let conn = self.side_conn.lock();
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
            let conn = self.side_conn.lock();
            conn.execute_batch(
                "DELETE FROM sqlite_sequence WHERE name IN ('hnsw_vectors', 'hnsw_layers')",
            )?;
            HashSet::new()
        } else {
            let conn = self.side_conn.lock();
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
        // If the requested thread count fails, fall back to rayon's default pool;
        // if that also fails, propagate the error rather than panicking.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_parallel)
            .build()
            .or_else(|_| rayon::ThreadPoolBuilder::new().build())
            .map_err(|e| anyhow::anyhow!("Failed to build rayon thread pool: {}", e))?;

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

            let t_embed_start = std::time::Instant::now();
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
            let t_embed = t_embed_start.elapsed();

            let t_insert_start = std::time::Instant::now();
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
            let t_insert = t_insert_start.elapsed();
            eprintln!(
                "[embed timing] file={} chunks={} embed={:?} insert={:?}",
                file_path,
                chunks.len(),
                t_embed,
                t_insert
            );

            progress_callback(file_path, file_entities.len(), file_idx, total_files);
        }

        Ok((embedded_count, skipped, failed_count))
    }
}

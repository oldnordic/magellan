//! Status command implementation for Magellan
//!
//! Provides status query functionality and execution tracking.

use anyhow::Result;
use magellan::backend_router::{BackendType, MagellanBackend};
use magellan::capabilities::{capabilities_for_path, BackendCapabilities};
use magellan::output::{generate_execution_id, output_json, CoverageInfo, JsonResponse, StatusResponse};
use magellan::{CodeGraph, OutputFormat};
use std::path::PathBuf;

/// Tracks execution metadata for logging and debugging
pub struct ExecutionTracker {
    exec_id: String,
    tool_version: String,
    args: Vec<String>,
    root: Option<String>,
    db_path: String,
    outcome: String,
    error_message: Option<String>,
    files_indexed: usize,
    symbols_indexed: usize,
    references_indexed: usize,
}

impl ExecutionTracker {
    pub fn new(args: Vec<String>, root: Option<String>, db_path: String) -> Self {
        Self {
            exec_id: generate_execution_id(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            args,
            root,
            db_path,
            outcome: "success".to_string(),
            error_message: None,
            files_indexed: 0,
            symbols_indexed: 0,
            references_indexed: 0,
        }
    }

    pub fn start(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().start_execution(
            &self.exec_id,
            &self.tool_version,
            &self.args,
            self.root.as_deref(),
            &self.db_path,
        )?;
        Ok(())
    }

    pub fn finish(&self, graph: &CodeGraph) -> Result<()> {
        graph.execution_log().finish_execution(
            &self.exec_id,
            &self.outcome,
            self.error_message.as_deref(),
            self.files_indexed,
            self.symbols_indexed,
            self.references_indexed,
        )
    }

    /// Set execution outcome to error with message
    #[expect(dead_code)]
    pub fn set_error(&mut self, msg: String) {
        self.outcome = "error".to_string();
        self.error_message = Some(msg);
    }

    /// Set indexing counts for execution tracking
    #[expect(dead_code)]
    pub fn set_counts(&mut self, files: usize, symbols: usize, references: usize) {
        self.files_indexed = files;
        self.symbols_indexed = symbols;
        self.references_indexed = references;
    }

    pub fn exec_id(&self) -> &str {
        &self.exec_id
    }
}

/// Run status query command
pub fn run_status(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let backend_caps = capabilities_for_path(&db_path);

    if MagellanBackend::detect_type(&db_path) == BackendType::Geometric {
        return run_status_geometric(db_path, output_format, backend_caps);
    }

    let graph = CodeGraph::open(&db_path)?;
    let tracker = ExecutionTracker::new(
        vec!["status".to_string()],
        None,
        db_path.to_string_lossy().to_string(),
    );
    tracker.start(&graph)?;

    let file_count = graph.count_files()?;
    let symbol_count = graph.count_symbols()?;
    let reference_count = graph.count_references()?;
    let call_count = graph.count_calls()?;
    let chunk_count = graph.count_chunks()?;

    // Query coverage data directly from side tables
    let (coverage_blocks, coverage_edges, coverage_meta) =
        match rusqlite::Connection::open(&db_path) {
            Ok(conn) => {
                let blocks = query_coverage_count(&conn, "cfg_block_coverage");
                let edges = query_coverage_count(&conn, "cfg_edge_coverage");
                let meta = conn
                    .query_row(
                        "SELECT source_kind, source_revision, ingested_at FROM cfg_coverage_meta LIMIT 1",
                        [],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )
                    .ok();
                drop(conn);
                (blocks, edges, meta)
            }
            Err(e) => {
                eprintln!("Warning: could not query coverage data: {}", e);
                (0, 0, None)
            }
        };

    let has_coverage = coverage_blocks > 0 || coverage_meta.is_some();
    let coverage = if has_coverage {
        let (source, revision, ingested_at) = match coverage_meta {
            Some((kind, rev, ts)) => (
                Some(kind),
                Some(rev),
                Some(
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|d| d.to_rfc3339())
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
            ),
            None => (None, None, None),
        };
        CoverageInfo {
            available: true,
            covered_blocks: coverage_blocks,
            covered_edges: coverage_edges,
            source,
            revision,
            ingested_at,
        }
    } else {
        CoverageInfo {
            available: false,
            covered_blocks: 0,
            covered_edges: 0,
            source: None,
            revision: None,
            ingested_at: None,
        }
    };

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = StatusResponse {
                files: file_count,
                symbols: symbol_count,
                references: reference_count,
                calls: call_count,
                code_chunks: chunk_count,
                coverage,
            };
            let exec_id = tracker.exec_id().to_string();
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            println!(
                "Backend: {} ({})",
                backend_caps.backend_type.display_name(),
                backend_caps.database_extension_hint
            );
            println!("Format: {}", backend_caps.format_hint);

            if backend_caps.supports_vacuum_maintenance {
                println!("Maintenance: vacuum available");
            } else {
                println!("Maintenance: not available");
            }

            println!();
            println!("Database contents:");
            println!("  files: {}", file_count);
            println!("  symbols: {}", symbol_count);
            println!("  references: {}", reference_count);
            println!("  calls: {}", call_count);
            println!("  code_chunks: {}", chunk_count);

            println!();
            if coverage.available {
                println!("Coverage data:");
                println!("  covered blocks: {}", coverage.covered_blocks);
                println!("  covered edges: {}", coverage.covered_edges);
                if let Some(ref kind) = coverage.source {
                    println!(
                        "  source: {} (rev {}, {})",
                        kind,
                        coverage.revision.as_deref().unwrap_or("unknown"),
                        coverage.ingested_at.as_deref().unwrap_or("unknown")
                    );
                }
            } else {
                println!("Coverage data: none (run 'magellan ingest-coverage --lcov <file>')");
            }
        }
    }

    tracker.finish(&graph)?;
    Ok(())
}

/// Run status for geometric backend databases
fn run_status_geometric(
    db_path: PathBuf,
    output_format: OutputFormat,
    backend_caps: BackendCapabilities,
) -> Result<()> {
    use magellan::backend_router::MagellanBackend;

    let backend = MagellanBackend::open(&db_path)?;
    let stats = backend.get_stats()?;

    let coverage = CoverageInfo {
        available: false,
        covered_blocks: 0,
        covered_edges: 0,
        source: None,
        revision: None,
        ingested_at: None,
    };

    match output_format {
        OutputFormat::Json | OutputFormat::Pretty => {
            let response = StatusResponse {
                files: stats.file_count,
                symbols: stats.symbol_count,
                references: 0,
                calls: 0,
                code_chunks: stats.cfg_block_count,
                coverage,
            };
            let exec_id = generate_execution_id();
            let json_response = JsonResponse::new(response, &exec_id);
            output_json(&json_response, output_format)?;
        }
        OutputFormat::Human => {
            println!(
                "Backend: {} ({})",
                backend_caps.backend_type.display_name(),
                backend_caps.database_extension_hint
            );
            println!("Format: {}", backend_caps.format_hint);

            if backend_caps.supports_vacuum_maintenance {
                println!("Maintenance: CFG vacuum available");
            } else {
                println!("Maintenance: not available");
            }

            println!();
            println!("Database contents:");
            println!("  files: {}", stats.file_count);
            println!("  symbols: {}", stats.symbol_count);
            println!("  cfg_blocks: {}", stats.cfg_block_count);
            println!("  (Call graph tracked separately in memory)");
        }
    }

    Ok(())
}

/// Query a coverage count from a known table.
fn query_coverage_count(conn: &rusqlite::Connection, table: &str) -> usize {
    const VALID_TABLES: &[&str] = &["cfg_block_coverage", "cfg_edge_coverage"];
    if !VALID_TABLES.contains(&table) {
        eprintln!("Warning: invalid coverage table name: {}", table);
        return 0;
    }
    let sql = format!("SELECT COUNT(*) FROM {} WHERE hit_count > 0", table);
    match conn.query_row(&sql, [], |row| row.get::<_, i64>(0)) {
        Ok(n) => n as usize,
        Err(rusqlite::Error::SqliteFailure(_code, Some(msg)))
            if msg.contains("no such table") =>
        {
            0
        }
        Err(e) => {
            eprintln!("Warning: coverage query failed for {}: {}", table, e);
            0
        }
    }
}

//! Ingest coverage data from LCOV tracefiles.
//!
//! Parses LCOV output (e.g. from `cargo llvm-cov --lcov`) and maps
//! line-level hit counts to CFG blocks and edges.

use anyhow::{Context, Result};
use rusqlite::params;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::CodeGraph;

/// Coverage data extracted from a single LCOV file.
#[derive(Debug, Default)]
struct LcovData {
    /// (file_path, line_number) -> hit_count
    line_hits: HashMap<(String, u32), u64>,
    /// (file_path, line_number) -> max branch taken count
    branch_hits: HashMap<(String, u32), u64>,
}

/// Run the ingest-coverage command.
///
/// # Arguments
/// * `db_path` - Path to the sqlitegraph database
/// * `lcov_path` - Path to the LCOV tracefile
pub fn run_ingest_coverage(db_path: PathBuf, lcov_path: PathBuf) -> Result<()> {
    let _graph = CodeGraph::open(&db_path)?;

    // Determine project root for path normalization
    let project_root = db_path
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."));

    // Parse LCOV file
    let lcov_data = parse_lcov_file(&lcov_path)
        .with_context(|| format!("Failed to parse LCOV file: {:?}", lcov_path))?;

    // Normalize LCOV paths to project-relative
    let line_hits = normalize_paths(&lcov_data.line_hits, project_root);
    let branch_hits = normalize_paths(&lcov_data.branch_hits, project_root);

    // Get git revision for provenance
    let source_revision = get_git_revision(&db_path).unwrap_or_else(|| {
        eprintln!("Warning: could not determine git revision for coverage provenance");
        "unknown".to_string()
    });
    let ingested_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Open a dedicated database connection for bulk insert
    let mut conn = rusqlite::Connection::open(&db_path)?;

    // Run all inserts in a single transaction for atomicity and performance
    let tx = conn.transaction()?;

    // Insert block coverage
    let (total_blocks, unmapped_lines) = insert_block_coverage(
        &tx,
        &line_hits,
        &source_revision,
        ingested_at,
    )?;

    // Insert edge coverage
    let total_edges = insert_edge_coverage(
        &tx,
        &branch_hits,
        &source_revision,
        ingested_at,
    )?;

    // Update metadata, but don't overwrite a real revision with "unknown"
    let revision_to_store: String = if source_revision == "unknown" {
        // Check if existing revision is real; if so, keep it
        let existing: Option<String> = tx
            .query_row(
                "SELECT source_revision FROM cfg_coverage_meta WHERE source_kind = 'lcov'",
                [],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        match existing {
            Some(r) if r != "unknown" && !r.is_empty() => r,
            _ => source_revision,
        }
    } else {
        source_revision
    };

    tx.execute(
        "INSERT INTO cfg_coverage_meta (source_kind, source_revision, ingested_at, total_blocks, total_edges)
         VALUES ('lcov', ?1, ?2, ?3, ?4)
         ON CONFLICT(source_kind) DO UPDATE SET
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at,
             total_blocks = excluded.total_blocks,
             total_edges = excluded.total_edges",
        params![revision_to_store, ingested_at, total_blocks, total_edges],
    )?;

    tx.commit()?;

    if unmapped_lines > 0 {
        eprintln!(
            "Warning: {} line hits could not be mapped to CFG blocks (file/line mismatch)",
            unmapped_lines
        );
    }

    println!(
        "Ingested coverage: {} blocks, {} edges from lcov (rev {}, {})",
        total_blocks,
        total_edges,
        revision_to_store,
        chrono::DateTime::from_timestamp(ingested_at, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "invalid timestamp".to_string()),
    );

    Ok(())
}

/// Canonicalize paths to match the absolute format stored in graph_entities.file_path.
fn normalize_paths(
    hits: &HashMap<(String, u32), u64>,
    project_root: &Path,
) -> HashMap<(String, u32), u64> {
    let mut normalized = HashMap::with_capacity(hits.len());
    for ((path, line), count) in hits {
        let path_buf = Path::new(path);
        let abs = if path_buf.is_absolute() {
            path_buf.to_path_buf()
        } else {
            project_root.join(path_buf)
        };
        // The indexer stores canonicalized paths; match that format.
        let canonical = std::fs::canonicalize(&abs)
            .unwrap_or(abs)
            .to_string_lossy()
            .to_string();
        normalized.insert((canonical, *line), *count);
    }
    normalized
}

/// Parse an LCOV tracefile into line and branch hit maps.
fn parse_lcov_file(path: &Path) -> Result<LcovData> {
    use lcov::{Reader, Record};

    let mut data = LcovData::default();
    let reader = Reader::open_file(path)
        .with_context(|| format!("Failed to open LCOV file: {:?}", path))?;

    let mut current_file = String::new();

    for record in reader {
        let record = record.with_context(|| "Failed to read LCOV record")?;
        match record {
            Record::SourceFile { path, .. } => {
                current_file = path.to_string_lossy().to_string();
            }
            Record::LineData { line, count, .. } if !current_file.is_empty() => {
                let key = (current_file.clone(), line);
                data.line_hits.insert(key, count);
            }
            Record::BranchData { line, taken, .. } if !current_file.is_empty() => {
                let key = (current_file.clone(), line);
                let taken_val = taken.unwrap_or(0);
                data.branch_hits
                    .entry(key)
                    .and_modify(|v| *v = (*v).max(taken_val))
                    .or_insert(taken_val);
            }
            _ => {}
        }
    }

    Ok(data)
}

/// Insert block coverage by mapping LCOV line hits to cfg_blocks.
///
/// Returns the number of successfully mapped blocks and the number of unmapped lines.
fn insert_block_coverage(
    tx: &rusqlite::Transaction,
    line_hits: &HashMap<(String, u32), u64>,
    source_revision: &str,
    ingested_at: i64,
) -> Result<(i64, i64)> {
    let mut stmt = tx.prepare(
        "INSERT INTO cfg_block_coverage (block_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (?1, ?2, 'lcov', ?3, ?4)
         ON CONFLICT(block_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
    )?;

    let mut block_stmt = tx.prepare(
        "SELECT b.id, b.start_line, b.start_col
         FROM cfg_blocks b
         JOIN graph_entities e ON b.function_id = e.id
         WHERE e.file_path = ?1
           AND b.start_line <= ?2
           AND b.end_line >= ?2
         ORDER BY (b.start_line = ?2) DESC, b.start_col ASC, b.id DESC",
    )?;

    let mut count = 0i64;
    let mut unmapped = 0i64;

    for ((file_path, line), hits) in line_hits {
        let blocks: Vec<(i64, i64, i64)> = block_stmt
            .query_map(params![file_path, line], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if let Some((block_id, _, _)) = blocks.first() {
            stmt.execute(params![block_id, *hits as i64, source_revision, ingested_at])?;
            count += 1;
        } else {
            unmapped += 1;
        }
    }

    Ok((count, unmapped))
}

/// Insert edge coverage by mapping LCOV branch hits to cfg_edges via source blocks.
///
/// Returns 0 if `cfg_edges` table does not exist (SQLite backend without CFG edge storage).
fn insert_edge_coverage(
    tx: &rusqlite::Transaction,
    branch_hits: &HashMap<(String, u32), u64>,
    source_revision: &str,
    ingested_at: i64,
) -> Result<i64> {
    // Check if cfg_edges table exists (geometric backend only)
    let edge_table_exists = match tx.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cfg_edges'",
        [],
        |_| Ok(true),
    ) {
        Ok(_) => true,
        Err(rusqlite::Error::QueryReturnedNoRows) => false,
        Err(e) => return Err(e.into()),
    };

    if !edge_table_exists {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT INTO cfg_edge_coverage (edge_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (?1, ?2, 'lcov', ?3, ?4)
         ON CONFLICT(edge_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
    )?;

    let mut edge_stmt = tx.prepare(
        "SELECT e.id
         FROM cfg_edges e
         JOIN cfg_blocks src ON e.source_idx = src.id
         JOIN graph_entities ent ON src.function_id = ent.id
         WHERE ent.file_path = ?1
           AND src.start_line <= ?2
           AND src.end_line >= ?2",
    )?;

    let mut count = 0i64;

    for ((file_path, line), hits) in branch_hits {
        let edges: Vec<i64> = edge_stmt
            .query_map(params![file_path, line], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        for edge_id in edges {
            stmt.execute(params![edge_id, *hits as i64, source_revision, ingested_at])?;
            count += 1;
        }
    }

    Ok(count)
}

/// Get the current git revision for the project containing the database.
fn get_git_revision(db_path: &Path) -> Option<String> {
    let db_dir = db_path.parent()?;
    let repo = git2::Repository::discover(db_dir).ok()?;
    let head = repo.head().ok()?;
    let oid = head.target()?;
    Some(oid.to_string())
}

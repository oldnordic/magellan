//! Ingest coverage data from LCOV tracefiles.
//!
//! Parses LCOV output (e.g. from `cargo llvm-cov --lcov`) and maps
//! line-level hit counts to CFG blocks and edges.

use anyhow::{Context, Result};
use rusqlite::params;
use std::collections::HashMap;
use std::path::PathBuf;

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

    // Parse LCOV file
    let lcov_data = parse_lcov_file(&lcov_path)
        .with_context(|| format!("Failed to parse LCOV file: {:?}", lcov_path))?;

    // Get git revision for provenance
    let source_revision = get_git_revision(&db_path).unwrap_or_default();
    let ingested_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Open a dedicated database connection for bulk insert
    let conn = rusqlite::Connection::open(&db_path)?;

    // Insert block coverage
    let total_blocks = insert_block_coverage(
        &conn,
        &lcov_data.line_hits,
        &source_revision,
        ingested_at,
    )?;

    // Insert edge coverage
    let total_edges = insert_edge_coverage(
        &conn,
        &lcov_data.branch_hits,
        &source_revision,
        ingested_at,
    )?;

    // Update metadata
    conn.execute(
        "INSERT INTO cfg_coverage_meta (source_kind, source_revision, ingested_at, total_blocks, total_edges)
         VALUES ('lcov', ?1, ?2, ?3, ?4)
         ON CONFLICT(source_kind) DO UPDATE SET
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at,
             total_blocks = excluded.total_blocks,
             total_edges = excluded.total_edges",
        params![source_revision, ingested_at, total_blocks, total_edges],
    )?;

    println!(
        "Ingested coverage: {} blocks, {} edges from lcov (rev {}, {})",
        total_blocks,
        total_edges,
        source_revision,
        chrono::DateTime::from_timestamp(ingested_at, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
    );

    Ok(())
}

/// Parse an LCOV tracefile into line and branch hit maps.
fn parse_lcov_file(path: &PathBuf) -> Result<LcovData> {
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
            Record::LineData { line, count, .. } => {
                if !current_file.is_empty() {
                    let key = (current_file.clone(), line);
                    data.line_hits.insert(key, count);
                }
            }
            Record::BranchData { line, taken, .. } => {
                if !current_file.is_empty() {
                    let key = (current_file.clone(), line);
                    let taken_val = taken.unwrap_or(0);
                    data.branch_hits
                        .entry(key)
                        .and_modify(|v| *v = (*v).max(taken_val))
                        .or_insert(taken_val);
                }
            }
            _ => {}
        }
    }

    Ok(data)
}

/// Insert block coverage by mapping LCOV line hits to cfg_blocks.
fn insert_block_coverage(
    conn: &rusqlite::Connection,
    line_hits: &HashMap<(String, u32), u64>,
    source_revision: &str,
    ingested_at: i64,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        "INSERT INTO cfg_block_coverage (block_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (?1, ?2, 'lcov', ?3, ?4)
         ON CONFLICT(block_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
    )?;

    let mut count = 0i64;

    for ((file_path, line), hits) in line_hits {
        // Find all blocks whose span includes this line
        let mut block_stmt = conn.prepare(
            "SELECT id, start_line, start_col
             FROM cfg_blocks
             WHERE file_path = ?1
               AND start_line <= ?2
               AND end_line >= ?2
             ORDER BY (start_line = ?2) DESC, start_col ASC, id DESC",
        )?;

        let blocks: Vec<(i64, i64, i64)> = block_stmt
            .query_map(params![file_path, line], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Assign to the first (best-matching) block per line
        if let Some((block_id, _, _)) = blocks.first() {
            stmt.execute(params![block_id, *hits as i64, source_revision, ingested_at])?;
            count += 1;
        }
    }

    Ok(count)
}

/// Insert edge coverage by mapping LCOV branch hits to cfg_edges via source blocks.
fn insert_edge_coverage(
    conn: &rusqlite::Connection,
    branch_hits: &HashMap<(String, u32), u64>,
    source_revision: &str,
    ingested_at: i64,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        "INSERT INTO cfg_edge_coverage (edge_id, hit_count, source_kind, source_revision, ingested_at)
         VALUES (?1, ?2, 'lcov', ?3, ?4)
         ON CONFLICT(edge_id) DO UPDATE SET
             hit_count = excluded.hit_count,
             source_kind = excluded.source_kind,
             source_revision = excluded.source_revision,
             ingested_at = excluded.ingested_at",
    )?;

    let mut count = 0i64;

    for ((file_path, line), hits) in branch_hits {
        // Find edges whose source block spans this line
        let mut edge_stmt = conn.prepare(
            "SELECT e.id
             FROM cfg_edges e
             JOIN cfg_blocks src ON e.source_idx = src.id
             WHERE src.file_path = ?1
               AND src.start_line <= ?2
               AND src.end_line >= ?2",
        )?;

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
fn get_git_revision(db_path: &PathBuf) -> Option<String> {
    let db_dir = db_path.parent()?;
    let repo = git2::Repository::discover(db_dir).ok()?;
    let head = repo.head().ok()?;
    let oid = head.target()?;
    Some(oid.to_string())
}

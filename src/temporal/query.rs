use crate::temporal::persistence::{build_scc_lineages, TemporalSccBarcodeReport};
use crate::temporal::scc::compute_snapshot_sccs;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TemporalStatus {
    pub snapshot_count: usize,
    pub file_version_count: usize,
    pub symbol_version_count: usize,
    pub edge_version_count: usize,
    pub latest_commit_oid: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolBarcodePoint {
    pub snapshot_id: i64,
    pub commit_oid: String,
    pub file_path: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolBarcode {
    pub stable_id: String,
    pub snapshot_count: usize,
    pub first_commit_oid: Option<String>,
    pub last_commit_oid: Option<String>,
    pub points: Vec<SymbolBarcodePoint>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EdgeBarcodePoint {
    pub snapshot_id: i64,
    pub commit_oid: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EdgeBarcode {
    pub source_stable_id: String,
    pub target_stable_id: String,
    pub kind: String,
    pub snapshot_count: usize,
    pub first_commit_oid: Option<String>,
    pub last_commit_oid: Option<String>,
    pub points: Vec<EdgeBarcodePoint>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AsOfSymbolMatch {
    pub stable_id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub start_col: i64,
    pub end_line: i64,
    pub end_col: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AsOfSymbolLookup {
    pub commit_oid: String,
    pub snapshot_id: i64,
    pub count: usize,
    pub matches: Vec<AsOfSymbolMatch>,
}

fn open_temporal_db(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path)
        .with_context(|| format!("Failed to open temporal database {}", db_path.display()))
}

fn count_rows(conn: &Connection, table_name: &str) -> Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {}", table_name);
    let mut stmt = conn.prepare(&sql)?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count as usize)
}

pub fn load_temporal_status(db_path: &Path) -> Result<TemporalStatus> {
    let conn = open_temporal_db(db_path)?;
    let latest_commit_oid = conn
        .query_row(
            "SELECT commit_oid FROM repo_snapshots ORDER BY commit_time DESC, id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;

    Ok(TemporalStatus {
        snapshot_count: count_rows(&conn, "repo_snapshots")?,
        file_version_count: count_rows(&conn, "file_versions")?,
        symbol_version_count: count_rows(&conn, "symbol_versions")?,
        edge_version_count: count_rows(&conn, "edge_versions")?,
        latest_commit_oid,
    })
}

pub fn load_symbol_barcode(db_path: &Path, stable_id: &str) -> Result<SymbolBarcode> {
    let conn = open_temporal_db(db_path)?;

    let resolved: String = {
        let exists: Option<String> = conn
            .query_row(
                "SELECT stable_id FROM symbol_versions WHERE stable_id = ?1 LIMIT 1",
                params![stable_id],
                |row| row.get(0),
            )
            .optional()?;
        match exists {
            Some(_) => stable_id.to_string(),
            None => conn
                .query_row(
                    "SELECT stable_id FROM symbol_versions WHERE name = ?1 LIMIT 1",
                    params![stable_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("No symbol history found for '{}'", stable_id))?,
        }
    };

    let mut stmt = conn.prepare(
        "SELECT rs.id, rs.commit_oid, sv.file_path, sv.name, sv.kind
         FROM symbol_versions sv
         JOIN repo_snapshots rs ON rs.id = sv.snapshot_id
         WHERE sv.stable_id = ?1
         ORDER BY rs.commit_time, rs.id",
    )?;
    let rows = stmt.query_map(params![resolved], |row| {
        Ok(SymbolBarcodePoint {
            snapshot_id: row.get(0)?,
            commit_oid: row.get(1)?,
            file_path: row.get(2)?,
            name: row.get(3)?,
            kind: row.get(4)?,
        })
    })?;

    let mut points = Vec::new();
    for row in rows {
        points.push(row?);
    }

    if points.is_empty() {
        anyhow::bail!("No symbol history found for '{}'", stable_id);
    }

    Ok(SymbolBarcode {
        stable_id: resolved,
        snapshot_count: points.len(),
        first_commit_oid: points.first().map(|point| point.commit_oid.clone()),
        last_commit_oid: points.last().map(|point| point.commit_oid.clone()),
        points,
    })
}

pub fn load_edge_barcode(
    db_path: &Path,
    source_stable_id: &str,
    target_stable_id: &str,
    kind: &str,
) -> Result<EdgeBarcode> {
    let conn = open_temporal_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT rs.id, rs.commit_oid
         FROM edge_versions ev
         JOIN repo_snapshots rs ON rs.id = ev.snapshot_id
         WHERE ev.source_stable_id = ?1
           AND ev.target_stable_id = ?2
           AND ev.kind = ?3
         ORDER BY rs.commit_time, rs.id",
    )?;
    let rows = stmt.query_map(params![source_stable_id, target_stable_id, kind], |row| {
        Ok(EdgeBarcodePoint {
            snapshot_id: row.get(0)?,
            commit_oid: row.get(1)?,
        })
    })?;

    let mut points = Vec::new();
    for row in rows {
        points.push(row?);
    }

    if points.is_empty() {
        anyhow::bail!(
            "No edge history found for {} -> {} ({})",
            source_stable_id,
            target_stable_id,
            kind
        );
    }

    Ok(EdgeBarcode {
        source_stable_id: source_stable_id.to_string(),
        target_stable_id: target_stable_id.to_string(),
        kind: kind.to_string(),
        snapshot_count: points.len(),
        first_commit_oid: points.first().map(|point| point.commit_oid.clone()),
        last_commit_oid: points.last().map(|point| point.commit_oid.clone()),
        points,
    })
}

pub fn lookup_symbol_as_of(
    db_path: &Path,
    commit_oid: &str,
    symbol_name: &str,
) -> Result<AsOfSymbolLookup> {
    let conn = open_temporal_db(db_path)?;
    let snapshot_id: i64 = conn
        .query_row(
            "SELECT id FROM repo_snapshots WHERE commit_oid = ?1",
            params![commit_oid],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("No snapshot found for commit {}", commit_oid))?;

    let mut stmt = conn.prepare(
        "SELECT stable_id, name, kind, file_path, start_line, start_col, end_line, end_col
         FROM symbol_versions
         WHERE snapshot_id = ?1 AND name = ?2
         ORDER BY file_path, start_line, start_col",
    )?;
    let rows = stmt.query_map(params![snapshot_id, symbol_name], |row| {
        Ok(AsOfSymbolMatch {
            stable_id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            file_path: row.get(3)?,
            start_line: row.get(4)?,
            start_col: row.get(5)?,
            end_line: row.get(6)?,
            end_col: row.get(7)?,
        })
    })?;

    let mut matches = Vec::new();
    for row in rows {
        matches.push(row?);
    }

    Ok(AsOfSymbolLookup {
        commit_oid: commit_oid.to_string(),
        snapshot_id,
        count: matches.len(),
        matches,
    })
}

pub fn load_scc_barcodes(db_path: &Path) -> Result<TemporalSccBarcodeReport> {
    let conn = open_temporal_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, commit_oid
         FROM repo_snapshots
         ORDER BY commit_time, id",
    )?;
    let snapshot_rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut all_sccs = Vec::new();
    for row in snapshot_rows {
        let (snapshot_id, commit_oid) = row?;
        all_sccs.extend(compute_snapshot_sccs(&conn, snapshot_id, &commit_oid)?);
    }

    Ok(build_scc_lineages(&all_sccs))
}

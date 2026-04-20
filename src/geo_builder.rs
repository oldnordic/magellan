//! GeoBuilder — Translates SQLite graph data into geometric spatial format
//!
//! This module provides lazy construction of .geo indexes from the primary
//! SQLite database. It is invoked on-demand when a geometric query runs.
//!
//! ## What gets translated
//! - **Symbols**: `graph_entities` rows with `kind = 'Symbol'` -> geometric SYMBOL section
//! - **Call edges**: Call nodes with CALLER/CALLS edges -> direct symbol-to-symbol edges
//!
//! ## What is NOT translated
//! - CFG blocks: stored in `cfg_blocks` but CFG edges are not persisted in SQLite
//! - AST nodes: stored in `ast_nodes` but not needed for call graph analysis
//! - Code chunks: stored in side tables but not needed for call graph analysis

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::graph::schema::GeoIndexMeta;

#[cfg(feature = "geometric-backend")]
use crate::graph::geometric_backend::{GeometricBackend, InsertSymbol, SymbolCallEdge};

#[cfg(feature = "geometric-backend")]
use crate::ingest::{Language, SymbolKind};

/// Statistics from a geo index build
#[derive(Debug, Clone)]
pub struct BuildStats {
    pub symbol_count: usize,
    pub call_count: usize,
    pub geo_path: PathBuf,
}

/// Build a .geo file from a SQLite database
///
/// # Arguments
/// * `db_path` — Path to the SQLite database (source of truth)
/// * `geo_path` — Path where the .geo file should be written
///
/// # Returns
/// Statistics about what was built
#[cfg(feature = "geometric-backend")]
pub fn build_geo_index(db_path: &Path, geo_path: &Path) -> Result<BuildStats> {
    // Remove stale .geo if it exists
    if geo_path.exists() {
        std::fs::remove_file(geo_path)
            .with_context(|| format!("Failed to remove stale .geo file at {:?}", geo_path))?;
    }

    // Create the geometric backend
    let backend = GeometricBackend::create(geo_path)
        .with_context(|| format!("Failed to create geometric database at {:?}", geo_path))?;

    // Open a direct SQLite connection for raw queries
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite database at {:?}", db_path))?;

    // Build symbols and collect ID mapping
    let (symbol_count, id_mapping) = build_symbols(&backend, &conn)
        .context("Failed to build symbols")?;

    // Build call edges using the ID mapping
    let call_count = build_call_edges(&backend, &conn, &id_mapping)
        .context("Failed to build call edges")?;

    // Persist the geometric backend to disk
    backend
        .save_to_disk()
        .context("Failed to save geometric backend to disk")?;

    // Record metadata in SQLite
    let checksum = compute_checksum(geo_path)?;
    GeoIndexMeta::record_geo_index_built(
        &conn,
        geo_path.to_str().unwrap_or(""),
        symbol_count as i64,
        call_count as i64,
        0, // cfg_block_count — not built from SQLite
        &checksum,
    )?;

    Ok(BuildStats {
        symbol_count,
        call_count,
        geo_path: geo_path.to_path_buf(),
    })
}

/// Stub version when geometric-backend feature is disabled
#[cfg(not(feature = "geometric-backend"))]
pub fn build_geo_index(_db_path: &Path, _geo_path: &Path) -> Result<BuildStats> {
    anyhow::bail!(
        "Building .geo indexes requires the 'geometric-backend' feature. \
         Install with: cargo install magellan --features geometric-backend"
    )
}

#[cfg(feature = "geometric-backend")]
fn build_symbols(
    backend: &GeometricBackend,
    conn: &rusqlite::Connection,
) -> Result<(usize, HashMap<i64, u64>)> {
    let mut stmt = conn.prepare(
        "SELECT id, name, data FROM graph_entities WHERE kind = 'Symbol'"
    )?;

    let mut symbols = Vec::new();
    let mut id_mapping = HashMap::new();

    let rows = stmt.query_map([], |row| {
        let entity_id: i64 = row.get(0)?;
        let name: String = row.get(1)?;
        let data_json: String = row.get(2)?;
        Ok((entity_id, name, data_json))
    })?;

    for row in rows {
        let (entity_id, _name, data_json) = row?;

        // Deserialize the SymbolNode payload
        let symbol_node: crate::graph::schema::SymbolNode =
            match serde_json::from_str(&data_json) {
                Ok(node) => node,
                Err(_) => continue, // Skip malformed entries
            };

        let kind = parse_symbol_kind(&symbol_node.kind);
        let language = Language::Rust; // Default; file extension not stored in SymbolNode

        // file_path may come from the graph_entities row or the SymbolNode
        let file_path = symbol_node.fqn.as_deref().unwrap_or("").to_string();

        symbols.push(InsertSymbol {
            name: symbol_node.name.unwrap_or_default(),
            fqn: symbol_node.fqn.unwrap_or_default(),
            kind,
            file_path,
            byte_start: symbol_node.byte_start as u64,
            byte_end: symbol_node.byte_end as u64,
            start_line: symbol_node.start_line as u64,
            start_col: symbol_node.start_col as u64,
            end_line: symbol_node.end_line as u64,
            end_col: symbol_node.end_col as u64,
            language,
        });

        // Remember the mapping: we will fill in geometric IDs after insert
        id_mapping.insert(entity_id, 0u64);
    }

    let count = symbols.len();
    if count == 0 {
        return Ok((0, id_mapping));
    }

    let geo_ids = backend
        .insert_symbols(symbols)
        .context("Failed to insert symbols into geometric backend")?;

    // Update mapping with actual geometric IDs
    let entity_ids: Vec<i64> = id_mapping.keys().copied().collect();
    for (idx, entity_id) in entity_ids.iter().enumerate() {
        if let Some(v) = id_mapping.get_mut(entity_id) {
            *v = geo_ids[idx];
        }
    }

    Ok((count, id_mapping))
}

#[cfg(feature = "geometric-backend")]
fn build_call_edges(
    backend: &GeometricBackend,
    conn: &rusqlite::Connection,
    id_mapping: &HashMap<i64, u64>,
) -> Result<usize> {
    // Query all Call nodes and their edge relationships
    // In sqlitegraph:
    //   Symbol -(CALLER edge)-> Call node -(CALLS edge)-> Symbol
    //
    // We need to join graph_entities (Call nodes) with graph_edges to find
    // the caller Symbol (via CALLER edge) and callee Symbol (via CALLS edge).

    let mut stmt = conn.prepare(
        "SELECT
            c.id AS call_node_id,
            c.data AS call_data,
            caller.from_id AS caller_symbol_id,
            callee.to_id AS callee_symbol_id
         FROM graph_entities c
         LEFT JOIN graph_edges caller ON caller.to_id = c.id AND caller.edge_type = 'CALLER'
         LEFT JOIN graph_edges callee ON callee.from_id = c.id AND callee.edge_type = 'CALLS'
         WHERE c.kind = 'Call'"
    )?;

    let rows = stmt.query_map([], |row| {
        let call_data: String = row.get(1)?;
        let caller_entity_id: Option<i64> = row.get(2)?;
        let callee_entity_id: Option<i64> = row.get(3)?;
        Ok((call_data, caller_entity_id, callee_entity_id))
    })?;

    let mut edges = Vec::new();

    for row in rows {
        let (call_data, caller_entity_id, callee_entity_id) = row?;

        let caller_geo_id = caller_entity_id.and_then(|id| id_mapping.get(&id).copied());
        let callee_geo_id = callee_entity_id.and_then(|id| id_mapping.get(&id).copied());

        let (caller_geo_id, callee_geo_id) = match (caller_geo_id, callee_geo_id) {
            (Some(caller), Some(callee)) => (caller, callee),
            _ => continue, // Skip if we can't resolve both ends
        };

        // Parse CallNode to get position info
        let call_node: crate::graph::schema::CallNode =
            match serde_json::from_str(&call_data) {
                Ok(node) => node,
                Err(_) => continue,
            };

        edges.push(SymbolCallEdge {
            src_symbol_id: caller_geo_id,
            dst_symbol_id: callee_geo_id,
            file_path: call_node.file,
            byte_start: call_node.byte_start,
            byte_end: call_node.byte_end,
            start_line: call_node.start_line,
            start_col: call_node.start_col,
        });
    }

    let count = edges.len();
    backend.insert_call_edges(edges);

    Ok(count)
}

#[cfg(feature = "geometric-backend")]
fn parse_symbol_kind(kind_str: &str) -> SymbolKind {
    match kind_str {
        "Function" => SymbolKind::Function,
        "Method" => SymbolKind::Method,
        "Struct" | "Class" => SymbolKind::Class,
        "Enum" => SymbolKind::Enum,
        "Trait" | "Interface" => SymbolKind::Interface,
        "Impl" => SymbolKind::Class,
        "Module" => SymbolKind::Module,
        "Variable" => SymbolKind::Unknown,
        "Union" => SymbolKind::Union,
        "Namespace" => SymbolKind::Namespace,
        "TypeAlias" => SymbolKind::TypeAlias,
        _ => SymbolKind::Unknown,
    }
}

fn compute_checksum(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open file for checksum: {:?}", path))?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    let hash = Sha256::digest(&contents);
    Ok(format!("{:x}", hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_geo_index_without_feature() {
        // When geometric-backend is disabled, build_geo_index should fail
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let geo_path = temp_dir.path().join("test.geo");

        let result = build_geo_index(&db_path, &geo_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("geometric-backend"));
    }
}

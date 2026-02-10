//! Metrics computation for CodeGraph
//!
//! Computes fan-in, fan-out, LOC, and complexity metrics during file indexing.
//! Provides backend-agnostic implementations using GraphBackend trait API
//! for both SQLite and Native V2 backends.

use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

#[cfg(not(feature = "native-v2"))]
use rusqlite::OptionalExtension;

#[cfg(feature = "native-v2")]
use sqlitegraph::SnapshotId;

use super::schema::{FileMetrics, SymbolMetrics};
use super::MetricsOps;

impl MetricsOps {
    /// Compute and store metrics for a file and all its symbols
    ///
    /// # Arguments
    /// * `file_path` - Path to the file
    /// * `source` - File contents as bytes
    /// * `symbol_facts` - Vector of SymbolNode data for all symbols in the file
    pub fn compute_for_file(
        &self,
        file_path: &str,
        source: &[u8],
        symbol_facts: &[crate::graph::schema::SymbolNode],
    ) -> Result<()> {
        // Count symbols in this file
        let symbol_count = symbol_facts.len() as i64;

        // Compute actual LOC (newline count + 1)
        let loc = source.iter().filter(|&&b| b == b'\n').count() as i64 + 1;

        // Compute estimated LOC (bytes / 40 heuristic)
        let estimated_loc = source.len() as f64 / 40.0;

        // Compute fan-in (incoming edges from other files)
        let fan_in = self.compute_file_fan_in(file_path)?;

        // Compute fan-out (outgoing edges to other files)
        let fan_out = self.compute_file_fan_out(file_path)?;

        // Compute complexity score (weighted)
        let complexity_score = calculate_complexity(loc, fan_in, fan_out);

        // Store file metrics
        let file_metrics = FileMetrics {
            file_path: file_path.to_string(),
            symbol_count,
            loc,
            estimated_loc,
            fan_in,
            fan_out,
            complexity_score,
            last_updated: Self::now_timestamp(),
        };
        self.upsert_file_metrics(&file_metrics)?;

        // Compute per-symbol metrics
        for symbol in symbol_facts {
            if let Err(e) = self.compute_and_store_symbol_metrics(symbol, file_path) {
                // Log error but don't fail entire file metrics
                let symbol_name = symbol.name.as_deref().unwrap_or("<unknown>");
                warn!(error = %e, symbol_name = %symbol_name, "Failed to compute metrics");
            }
        }

        Ok(())
    }

    /// Compute file-level fan-in (incoming references/calls from other files)
    ///
    /// For SQLite: Uses direct SQL queries on graph_entities and graph_edges tables
    /// For Native V2: Returns 0 (full implementation deferred - requires DEFINES edge traversal)
    fn compute_file_fan_in(&self, file_path: &str) -> Result<i64> {
        #[cfg(feature = "native-v2")]
        {
            if self.kv_backend.is_some() {
                // Native V2: Return 0 for now - full implementation requires
                // traversing DEFINES edges to find file associations
                return Ok(0);
            }
        }

        // SQLite fallback
        #[cfg(not(feature = "native-v2"))]
        {
            return self.compute_file_fan_in_sqlite(file_path);
        }

        // Native V2 with no backend (shouldn't happen in practice)
        Ok(0)
    }

    /// Compute file-level fan-out (outgoing references/calls to other files)
    ///
    /// For SQLite: Uses direct SQL queries on graph_entities and graph_edges tables
    /// For Native V2: Returns 0 (full implementation deferred - requires DEFINES edge traversal)
    fn compute_file_fan_out(&self, file_path: &str) -> Result<i64> {
        #[cfg(feature = "native-v2")]
        {
            if self.kv_backend.is_some() {
                // Native V2: Return 0 for now - full implementation requires
                // traversing DEFINES edges to find file associations
                return Ok(0);
            }
        }

        // SQLite fallback
        #[cfg(not(feature = "native-v2"))]
        {
            return self.compute_file_fan_out_sqlite(file_path);
        }

        // Native V2 with no backend (shouldn't happen in practice)
        Ok(0)
    }

    /// Compute and store metrics for a single symbol
    fn compute_and_store_symbol_metrics(
        &self,
        symbol: &crate::graph::schema::SymbolNode,
        file_path: &str,
    ) -> Result<()> {
        // Get the FQN for lookup
        let fqn = symbol.fqn.as_deref().unwrap_or("");
        if fqn.is_empty() {
            return Ok(()); // Skip symbols without FQN
        }

        // Get symbol_id from graph for this symbol
        let symbol_id = self.find_symbol_id(fqn)?;

        if symbol_id.is_none() {
            // Symbol not in database yet (might be during initial indexing)
            return Ok(());
        }

        let symbol_id = symbol_id.unwrap();

        // Compute LOC from span (end_line - start_line + 1)
        let loc = if symbol.end_line > 0 && symbol.end_line >= symbol.start_line {
            (symbol.end_line - symbol.start_line + 1) as i64
        } else {
            1
        };

        // Compute estimated LOC from byte span
        let byte_span = if symbol.byte_end > symbol.byte_start {
            symbol.byte_end - symbol.byte_start
        } else {
            1
        };
        let estimated_loc = byte_span as f64 / 40.0;

        // Compute symbol-level fan-in and fan-out
        let fan_in = self.compute_symbol_fan_in(symbol_id)?;
        let fan_out = self.compute_symbol_fan_out(symbol_id)?;

        // Create symbol metrics
        let symbol_name = symbol.name.as_deref().unwrap_or("").to_string();
        let metrics = SymbolMetrics {
            symbol_id,
            symbol_name,
            kind: symbol.kind.clone(),
            file_path: file_path.to_string(),
            loc,
            estimated_loc,
            fan_in,
            fan_out,
            cyclomatic_complexity: 1, // Placeholder for Phase 35
            last_updated: Self::now_timestamp(),
        };

        self.upsert_symbol_metrics(&metrics)?;
        Ok(())
    }

    /// Find symbol_id by FQN
    ///
    /// For SQLite: Uses SQL query on graph_entities table
    /// For Native V2: Iterates all entities and filters by kind=Symbol and fqn match
    fn find_symbol_id(&self, fqn: &str) -> Result<Option<i64>> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                return self.find_symbol_id_native(backend, fqn);
            }
        }

        // SQLite fallback
        #[cfg(not(feature = "native-v2"))]
        {
            return self.find_symbol_id_sqlite(fqn);
        }

        // Native V2 with no backend (shouldn't happen in practice)
        Ok(None)
    }

    /// Compute symbol-level fan-in (incoming edges)
    ///
    /// For SQLite: Counts edges in graph_edges where target_id = symbol_id
    /// For Native V2: Uses node_degree() to get incoming edge count
    fn compute_symbol_fan_in(&self, symbol_id: i64) -> Result<i64> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                return self.compute_symbol_fan_in_native(backend, symbol_id);
            }
        }

        // SQLite fallback
        #[cfg(not(feature = "native-v2"))]
        {
            return self.compute_symbol_fan_in_sqlite(symbol_id);
        }

        // Native V2 with no backend (shouldn't happen in practice)
        Ok(0)
    }

    /// Compute symbol-level fan-out (outgoing edges)
    ///
    /// For SQLite: Counts edges in graph_edges where source_id = symbol_id
    /// For Native V2: Uses node_degree() to get outgoing edge count
    fn compute_symbol_fan_out(&self, symbol_id: i64) -> Result<i64> {
        #[cfg(feature = "native-v2")]
        {
            if let Some(ref backend) = self.kv_backend {
                return self.compute_symbol_fan_out_native(backend, symbol_id);
            }
        }

        // SQLite fallback
        #[cfg(not(feature = "native-v2"))]
        {
            return self.compute_symbol_fan_out_sqlite(symbol_id);
        }

        // Native V2 with no backend (shouldn't happen in practice)
        Ok(0)
    }

    /// Get current Unix timestamp in seconds
    fn now_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    // =========================================================================
    // SQLite-specific implementations (direct SQL queries)
    // =========================================================================

    #[cfg(not(feature = "native-v2"))]
    fn compute_file_fan_in_sqlite(&self, file_path: &str) -> Result<i64> {
        use rusqlite::params;

        let conn = self.connect()?;

        // Count incoming references from other files
        let ref_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_entities ge
                 JOIN graph_edges edge ON edge.target_id = ge.id
                 JOIN graph_entities source ON source.id = edge.source_id
                 WHERE source.kind = 'Symbol'
                 AND json_extract(source.data, '$.file_path') != ?1
                 AND json_extract(ge.data, '$.file_path') = ?1",
                params![file_path, file_path],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Also count incoming calls from other files
        let call_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_entities ge
                 JOIN graph_edges edge ON edge.target_id = ge.id
                 JOIN graph_entities call ON call.id = edge.source_id
                 WHERE call.kind = 'Call'
                 AND json_extract(call.data, '$.file') != ?1
                 AND json_extract(ge.data, '$.file_path') = ?1",
                params![file_path, file_path],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(ref_count + call_count)
    }

    #[cfg(not(feature = "native-v2"))]
    fn compute_file_fan_out_sqlite(&self, file_path: &str) -> Result<i64> {
        use rusqlite::params;

        let conn = self.connect()?;

        // Count outgoing references to symbols in other files
        let ref_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_entities ge
                 JOIN graph_edges edge ON edge.source_id = ge.id
                 JOIN graph_entities target ON target.id = edge.target_id
                 WHERE ge.kind = 'Symbol'
                 AND json_extract(ge.data, '$.file_path') = ?1
                 AND json_extract(target.data, '$.file_path') != ?1",
                params![file_path, file_path],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Also count outgoing calls to symbols in other files
        let call_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_entities ge
                 JOIN graph_entities call ON call.id = ge.id
                 JOIN graph_edges edge ON edge.source_id = call.id
                 JOIN graph_entities target ON target.id = edge.target_id
                 WHERE call.kind = 'Call'
                 AND json_extract(call.data, '$.file') = ?1
                 AND json_extract(target.data, '$.file_path') != ?1",
                params![file_path, file_path],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(ref_count + call_count)
    }

    #[cfg(not(feature = "native-v2"))]
    fn find_symbol_id_sqlite(&self, fqn: &str) -> Result<Option<i64>> {
        use rusqlite::params;

        let conn = self.connect()?;
        let result = conn
            .query_row(
                "SELECT id FROM graph_entities WHERE kind = 'Symbol' AND json_extract(data, '$.fqn') = ?1",
                params![fqn],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| anyhow::anyhow!("Failed to find symbol_id: {}", e))?;

        Ok(result)
    }

    #[cfg(not(feature = "native-v2"))]
    fn compute_symbol_fan_in_sqlite(&self, symbol_id: i64) -> Result<i64> {
        use rusqlite::params;

        let conn = self.connect()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE target_id = ?1",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(count)
    }

    #[cfg(not(feature = "native-v2"))]
    fn compute_symbol_fan_out_sqlite(&self, symbol_id: i64) -> Result<i64> {
        use rusqlite::params;

        let conn = self.connect()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE source_id = ?1",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(count)
    }

    // =========================================================================
    // Native V2-specific implementations (GraphBackend trait API)
    // =========================================================================

    #[cfg(feature = "native-v2")]
    fn find_symbol_id_native(
        &self,
        backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend>,
        fqn: &str,
    ) -> Result<Option<i64>> {
        use crate::graph::schema::SymbolNode;

        let snapshot = SnapshotId::current();
        let entity_ids = backend.entity_ids()?;

        for entity_id in entity_ids {
            let node = match backend.get_node(snapshot, entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind == "Symbol" {
                if let Ok(symbol) = serde_json::from_value::<SymbolNode>(node.data) {
                    if symbol.fqn.as_deref().unwrap_or("") == fqn {
                        return Ok(Some(entity_id));
                    }
                }
            }
        }

        Ok(None)
    }

    #[cfg(feature = "native-v2")]
    fn compute_symbol_fan_in_native(
        &self,
        backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend>,
        symbol_id: i64,
    ) -> Result<i64> {
        let snapshot = SnapshotId::current();

        // Use node_degree to get incoming edge count
        match backend.node_degree(snapshot, symbol_id) {
            Ok((incoming, _outgoing)) => Ok(incoming as i64),
            Err(_) => Ok(0),
        }
    }

    #[cfg(feature = "native-v2")]
    fn compute_symbol_fan_out_native(
        &self,
        backend: &std::rc::Rc<dyn sqlitegraph::GraphBackend>,
        symbol_id: i64,
    ) -> Result<i64> {
        let snapshot = SnapshotId::current();

        // Use node_degree to get outgoing edge count
        match backend.node_degree(snapshot, symbol_id) {
            Ok((_incoming, outgoing)) => Ok(outgoing as i64),
            Err(_) => Ok(0),
        }
    }
}

/// Calculate weighted complexity score
///
/// Formula: loc*0.1 + fan_in*0.5 + fan_out*0.3
///
/// Weights:
/// - LOC: 0.1 (larger files are slightly more complex)
/// - Fan-in: 0.5 (highly used files are more critical)
/// - Fan-out: 0.3 (files with many dependencies are more complex)
fn calculate_complexity(loc: i64, fan_in: i64, fan_out: i64) -> f64 {
    let loc_weight = 0.1;
    let fan_in_weight = 0.5;
    let fan_out_weight = 0.3;

    (loc as f64 * loc_weight)
        + (fan_in as f64 * fan_in_weight)
        + (fan_out as f64 * fan_out_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_complexity() {
        // Low complexity
        let score1 = calculate_complexity(10, 0, 0);
        assert_eq!(score1, 1.0);

        // High LOC, low dependencies
        let score2 = calculate_complexity(1000, 1, 1);
        assert_eq!(score2, 100.0 + 0.5 + 0.3);

        // High fan-in (widely used)
        let score3 = calculate_complexity(10, 100, 1);
        assert_eq!(score3, 1.0 + 50.0 + 0.3);

        // High fan-out (many dependencies)
        let score4 = calculate_complexity(10, 1, 100);
        assert_eq!(score4, 1.0 + 0.5 + 30.0);
    }
}

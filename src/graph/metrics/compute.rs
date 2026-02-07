//! Metrics computation for CodeGraph
//!
//! Computes fan-in, fan-out, LOC, and complexity metrics during file indexing.

use anyhow::Result;
use rusqlite::OptionalExtension;
use std::time::{SystemTime, UNIX_EPOCH};

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
        // Native V2: metrics are not supported (no SQLite tables)
        // Return early if db_path is empty (indicates disabled state)
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(());
        }

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
                eprintln!(
                    "Warning: Failed to compute metrics for symbol '{}': {}",
                    symbol_name, e
                );
            }
        }

        Ok(())
    }

    /// Compute file-level fan-in (incoming references/calls from other files)
    fn compute_file_fan_in(&self, file_path: &str) -> Result<i64> {
        // Native V2: metrics tables don't exist
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(0);
        }

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

    /// Compute file-level fan-out (outgoing references/calls to other files)
    fn compute_file_fan_out(&self, file_path: &str) -> Result<i64> {
        // Native V2: metrics tables don't exist
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(0);
        }

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

    /// Compute and store metrics for a single symbol
    fn compute_and_store_symbol_metrics(
        &self,
        symbol: &crate::graph::schema::SymbolNode,
        file_path: &str,
    ) -> Result<()> {
        // Native V2: metrics tables don't exist
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(());
        }

        // Get the FQN for lookup
        let fqn = symbol.fqn.as_deref().unwrap_or("");
        if fqn.is_empty() {
            return Ok(()); // Skip symbols without FQN
        }

        // Get symbol_id from graph_entities for this symbol
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
    fn find_symbol_id(&self, fqn: &str) -> Result<Option<i64>> {
        // Native V2: metrics tables don't exist
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(None);
        }

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

    /// Compute symbol-level fan-in (incoming edges)
    fn compute_symbol_fan_in(&self, symbol_id: i64) -> Result<i64> {
        // Native V2: metrics tables don't exist
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(0);
        }

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

    /// Compute symbol-level fan-out (outgoing edges)
    fn compute_symbol_fan_out(&self, symbol_id: i64) -> Result<i64> {
        // Native V2: metrics tables don't exist
        #[cfg(feature = "native-v2")]
        if self.db_path.as_os_str().is_empty() {
            return Ok(0);
        }

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

    /// Get current Unix timestamp in seconds
    fn now_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
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

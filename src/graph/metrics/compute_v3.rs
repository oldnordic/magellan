//! V3-specific metrics computation using graph backend
//!
//! This module provides metrics computation for the V3 backend using
//! graph traversal APIs instead of SQL queries.

use anyhow::Result;
use sqlitegraph::{GraphBackend, SnapshotId, GraphEntity};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::schema::{FileMetrics, SymbolMetrics};
use crate::graph::schema::SymbolNode;

/// V3 metrics computation using graph backend
pub struct V3MetricsCompute {
    backend: Arc<dyn GraphBackend>,
}

impl V3MetricsCompute {
    /// Create a new V3 metrics computation instance
    pub fn new(backend: Arc<dyn GraphBackend>) -> Self {
        Self { backend }
    }

    /// Compute and store metrics for a file and all its symbols
    ///
    /// # Arguments
    /// * `file_path` - Path to the file
    /// * `source` - File contents as bytes
    /// * `symbol_facts` - Vector of SymbolNode data for all symbols in the file
    /// * `store_fn` - Callback to store file metrics
    /// * `store_symbol_fn` - Callback to store symbol metrics
    pub fn compute_for_file<F, S>(
        &self,
        file_path: &str,
        source: &[u8],
        symbol_facts: &[SymbolNode],
        store_fn: F,
        store_symbol_fn: S,
    ) -> Result<()>
    where
        F: Fn(&FileMetrics) -> Result<()>,
        S: Fn(&SymbolMetrics) -> Result<()>,
    {
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
        store_fn(&file_metrics)?;

        // Compute per-symbol metrics
        for symbol in symbol_facts {
            if let Err(e) = self.compute_and_store_symbol_metrics(
                symbol,
                file_path,
                &store_symbol_fn,
            ) {
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
        let snapshot = SnapshotId::current();
        let entity_ids = self.backend.entity_ids()?;
        let mut count = 0;

        for entity_id in entity_ids {
            // Get node to check if it's in this file
            let node = match self.backend.get_node(snapshot, entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            // Skip if not in target file
            if !self.is_node_in_file(&node, file_path) {
                continue;
            }

            // Count incoming edges from other files
            let incoming = self.backend.neighbors(
                snapshot,
                entity_id,
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Incoming,
                    edge_type: None,
                },
            )?;

            for source_id in incoming {
                let source_node = match self.backend.get_node(snapshot, source_id) {
                    Ok(n) => n,
                    Err(_) => continue,
                };

                // Count if from a different file
                if !self.is_node_in_file(&source_node, file_path) {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Compute file-level fan-out (outgoing references/calls to other files)
    fn compute_file_fan_out(&self, file_path: &str) -> Result<i64> {
        let snapshot = SnapshotId::current();
        let entity_ids = self.backend.entity_ids()?;
        let mut count = 0;

        for entity_id in entity_ids {
            // Get node to check if it's in this file
            let node = match self.backend.get_node(snapshot, entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            // Skip if not in target file
            if !self.is_node_in_file(&node, file_path) {
                continue;
            }

            // Count outgoing edges to other files
            let outgoing = self.backend.neighbors(
                snapshot,
                entity_id,
                sqlitegraph::NeighborQuery {
                    direction: sqlitegraph::BackendDirection::Outgoing,
                    edge_type: None,
                },
            )?;

            for target_id in outgoing {
                let target_node = match self.backend.get_node(snapshot, target_id) {
                    Ok(n) => n,
                    Err(_) => continue,
                };

                // Count if to a different file
                if !self.is_node_in_file(&target_node, file_path) {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Check if a node belongs to the given file
    fn is_node_in_file(&self, node: &GraphEntity, file_path: &str) -> bool {
        match serde_json::from_value::<serde_json::Value>(node.data.clone()) {
            Ok(data) => {
                // Check file_path field first (for Symbol nodes)
                if let Some(fp) = data.get("file_path").and_then(|v| v.as_str()) {
                    return fp == file_path;
                }
                // Check file field (for Call nodes)
                if let Some(f) = data.get("file").and_then(|v| v.as_str()) {
                    return f == file_path;
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Compute and store metrics for a single symbol
    fn compute_and_store_symbol_metrics<S>(
        &self,
        symbol: &SymbolNode,
        file_path: &str,
        store_fn: &S,
    ) -> Result<()>
    where
        S: Fn(&SymbolMetrics) -> Result<()>,
    {
        // Get the FQN for lookup
        let fqn = symbol.fqn.as_deref().unwrap_or("");
        if fqn.is_empty() {
            return Ok(()); // Skip symbols without FQN
        }

        // Get symbol_id from graph_entities for this symbol
        let symbol_id = match self.find_symbol_id(fqn)? {
            Some(id) => id,
            None => return Ok(()), // Symbol not in database yet
        };

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

        store_fn(&metrics)?;
        Ok(())
    }

    /// Find symbol_id by FQN
    fn find_symbol_id(&self, fqn: &str) -> Result<Option<i64>> {
        let snapshot = SnapshotId::current();
        let entity_ids = self.backend.entity_ids()?;

        for entity_id in entity_ids {
            let node = match self.backend.get_node(snapshot, entity_id) {
                Ok(n) => n,
                Err(_) => continue,
            };

            if node.kind != "Symbol" {
                continue;
            }

            match serde_json::from_value::<serde_json::Value>(node.data) {
                Ok(data) => {
                    if let Some(node_fqn) = data.get("fqn").and_then(|v| v.as_str()) {
                        if node_fqn == fqn {
                            return Ok(Some(entity_id));
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(None)
    }

    /// Compute symbol-level fan-in (incoming edges)
    fn compute_symbol_fan_in(&self, symbol_id: i64) -> Result<i64> {
        let snapshot = SnapshotId::current();
        let count = self.backend.neighbors(
            snapshot,
            symbol_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Incoming,
                edge_type: None,
            },
        )?;
        Ok(count.len() as i64)
    }

    /// Compute symbol-level fan-out (outgoing edges)
    fn compute_symbol_fan_out(&self, symbol_id: i64) -> Result<i64> {
        let snapshot = SnapshotId::current();
        let count = self.backend.neighbors(
            snapshot,
            symbol_id,
            sqlitegraph::NeighborQuery {
                direction: sqlitegraph::BackendDirection::Outgoing,
                edge_type: None,
            },
        )?;
        Ok(count.len() as i64)
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
fn calculate_complexity(loc: i64, fan_in: i64, fan_out: i64) -> f64 {
    let loc_weight = 0.1;
    let fan_in_weight = 0.5;
    let fan_out_weight = 0.3;

    (loc as f64 * loc_weight)
        + (fan_in as f64 * fan_in_weight)
        + (fan_out as f64 * fan_out_weight)
}

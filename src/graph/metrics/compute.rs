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

        // Compute cyclomatic complexity from CFG blocks
        let cyclomatic_complexity = self.compute_cyclomatic_complexity(symbol_id)?;

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
            cyclomatic_complexity,
            last_updated: Self::now_timestamp(),
        };

        self.upsert_symbol_metrics(&metrics)?;
        Ok(())
    }

    /// Find symbol_id by FQN
    fn find_symbol_id(&self, fqn: &str) -> Result<Option<i64>> {
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
        use rusqlite::params;

        let conn = self.connect()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE to_id = ?1",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(count)
    }

    /// Compute symbol-level fan-out (outgoing edges)
    fn compute_symbol_fan_out(&self, symbol_id: i64) -> Result<i64> {
        use rusqlite::params;

        let conn = self.connect()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE from_id = ?1",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(count)
    }

    /// Compute cyclomatic complexity from CFG blocks
    ///
    /// Cyclomatic Complexity = Number of decision points + 1
    /// 
    /// Decision points are CFG blocks with terminators that indicate branching:
    /// - return, continue, break (not fallthrough)
    ///
    /// # Arguments
    /// * `symbol_id` - The entity ID of the symbol (function)
    ///
    /// # Returns
    /// Cyclomatic complexity as i64 (minimum value is 1)
    fn compute_cyclomatic_complexity(&self, symbol_id: i64) -> Result<i64> {
        use rusqlite::params;

        let conn = self.connect()?;
        
        // Count CFG blocks for this function that have non-fallthrough terminators
        // These represent decision points (branches)
        let decision_points: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cfg_blocks 
                 WHERE function_id = ?1 
                 AND terminator != 'fallthrough'",
                params![symbol_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Cyclomatic complexity = decision_points + 1
        // Minimum complexity is 1 (a function with no branches)
        Ok(decision_points.max(0) + 1)
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

    /// Test that symbol fan-in/fan-out metrics are computed correctly
    /// This test verifies the fix for the bug where column names were wrong
    /// (target_id/to_id and source_id/from_id confusion)
    #[test]
    fn test_symbol_fan_in_fan_out_computation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create a test file with two functions where one calls the other
        let test_source = r#"
fn caller_function() {
    callee_function();
}

fn callee_function() {
    println!("called");
}
"#;

        // Index the file
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, test_source).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, test_source.as_bytes()).unwrap();

        // Get the symbol IDs using symbol_id_by_name
        let caller_id = graph.symbol_id_by_name(&path_str, "caller_function").unwrap();
        let callee_id = graph.symbol_id_by_name(&path_str, "callee_function").unwrap();
        
        assert!(caller_id.is_some(), "caller_function should be indexed");
        assert!(callee_id.is_some(), "callee_function should be indexed");
        
        let caller_id = caller_id.unwrap();
        let callee_id = callee_id.unwrap();

        // Get symbol metrics from the metrics field
        let caller_metrics = graph.metrics.get_symbol_metrics(caller_id).unwrap()
            .expect("caller_function metrics should exist");
        let callee_metrics = graph.metrics.get_symbol_metrics(callee_id).unwrap()
            .expect("callee_function metrics should exist");
        
        // callee_function should have fan_in >= 1 (being called by caller_function)
        assert!(
            callee_metrics.fan_in >= 1,
            "callee_function should have fan_in >= 1 (called by caller_function), got {}",
            callee_metrics.fan_in
        );
        
        // caller_function should have fan_out >= 1 (calling callee_function)
        assert!(
            caller_metrics.fan_out >= 1,
            "caller_function should have fan_out >= 1 (calls callee_function), got {}",
            caller_metrics.fan_out
        );
    }

    /// Test that cyclomatic complexity is computed from CFG data
    /// Verifies the fix for placeholder complexity=1
    #[test]
    fn test_cyclomatic_complexity_from_cfg() {
        use crate::graph::schema::SymbolNode;
        
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create a test file with a function that has multiple branches
        let test_source = r#"
fn simple_function() {
    println!("simple");
}

fn complex_function(x: i32) {
    if x > 0 {
        println!("positive");
    } else if x < 0 {
        println!("negative");
    } else {
        println!("zero");
    }
    
    for i in 0..x {
        if i % 2 == 0 {
            continue;
        }
        println!("{}", i);
    }
}
"#;

        // Index the file
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, test_source).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, test_source.as_bytes()).unwrap();

        // Get the symbol IDs
        let simple_id = graph.symbol_id_by_name(&path_str, "simple_function").unwrap();
        let complex_id = graph.symbol_id_by_name(&path_str, "complex_function").unwrap();
        
        assert!(simple_id.is_some(), "simple_function should be indexed");
        assert!(complex_id.is_some(), "complex_function should be indexed");
        
        let simple_id = simple_id.unwrap();
        let complex_id = complex_id.unwrap();

        // Get symbol metrics
        let simple_metrics = graph.metrics.get_symbol_metrics(simple_id).unwrap()
            .expect("simple_function metrics should exist");
        let complex_metrics = graph.metrics.get_symbol_metrics(complex_id).unwrap()
            .expect("complex_function metrics should exist");
        
        // simple_function has no branches, complexity should be 1
        assert_eq!(
            simple_metrics.cyclomatic_complexity, 1,
            "simple_function should have complexity 1, got {}",
            simple_metrics.cyclomatic_complexity
        );
        
        // complex_function has:
        // - if/else if/else (3 branches)
        // - for loop (1 branch)
        // - if inside loop (1 branch)
        // - continue (1 branch)
        // Total complexity should be > 1
        assert!(
            complex_metrics.cyclomatic_complexity > 1,
            "complex_function should have complexity > 1 (has if/else, for, if, continue), got {}",
            complex_metrics.cyclomatic_complexity
        );
    }

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

//! Metrics backfill for existing databases
//!
//! When upgrading from schema version 4 to 5, metrics tables exist but are empty.
//! This module provides backfill functionality to compute metrics for all existing files.

use anyhow::Result;
use rusqlite::params;

use super::MetricsOps;

/// Result of a backfill operation
#[derive(Debug, Clone)]
pub struct BackfillResult {
    /// Total number of files to process
    pub total: usize,
    /// Number of files successfully processed
    pub processed: usize,
    /// Errors encountered: (file_path, error_message)
    pub errors: Vec<(String, String)>,
}

impl MetricsOps {
    /// Backfill metrics for all existing files in the database
    ///
    /// This is called automatically after database migration to schema version 5.
    /// Can also be called manually to recompute metrics.
    ///
    /// # Arguments
    /// * `progress` - Optional callback for progress updates (current, total)
    ///
    /// # Returns
    /// BackfillResult with total files processed and any errors
    pub fn backfill_all_metrics(
        &self,
        progress: Option<&dyn Fn(usize, usize)>,
    ) -> Result<BackfillResult> {
        let conn = self.connect()?;

        // Get all unique file paths from graph_entities (Symbol nodes)
        let mut stmt = conn.prepare(
            "SELECT DISTINCT json_extract(data, '$.file_path') as file_path
             FROM graph_entities
             WHERE kind = 'Symbol'
             AND json_extract(data, '$.file_path') IS NOT NULL
             ORDER BY file_path",
        )?;

        let files: Vec<String> = stmt
            .query_map([], |row| {
                let file_path: String = row.get(0)?;
                Ok(file_path)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        drop(stmt);
        drop(conn); // Release lock before long operation

        let total = files.len();
        let mut processed = 0;
        let mut errors = Vec::new();

        for file_path in files {
            // Read file from disk
            let source = match std::fs::read(&file_path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push((file_path.clone(), format!("Read error: {}", e)));
                    continue;
                }
            };

            // Get symbols for this file from graph_entities
            let symbols = match self.get_file_symbols(&file_path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push((file_path.clone(), format!("Symbol query error: {}", e)));
                    continue;
                }
            };

            // Compute metrics (same logic as index_file)
            if let Err(e) = self.compute_for_file(&file_path, &source, &symbols) {
                errors.push((file_path.clone(), format!("Compute error: {}", e)));
            }

            processed += 1;

            // Report progress
            if let Some(cb) = progress {
                cb(processed, total);
            }
        }

        Ok(BackfillResult {
            total,
            processed,
            errors,
        })
    }

    /// Get all symbols for a specific file from graph_entities
    ///
    /// Queries the database for Symbol nodes with matching file_path
    /// and returns them as SymbolNode structs.
    ///
    /// # Arguments
    /// * `file_path` - File path to query symbols for
    ///
    /// # Returns
    /// Vector of SymbolNode structs for all symbols in the file
    fn get_file_symbols(&self, file_path: &str) -> Result<Vec<crate::graph::schema::SymbolNode>> {
        let conn = self.connect()?;

        let mut stmt = conn.prepare(
            "SELECT data
             FROM graph_entities
             WHERE kind = 'Symbol'
             AND json_extract(data, '$.file_path') = ?1",
        )?;

        let symbols = stmt
            .query_map(params![file_path], |row| {
                let data: String = row.get(0)?;
                // Parse JSON to extract SymbolNode
                    let symbol: crate::graph::schema::SymbolNode = serde_json::from_str(&data)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    Ok(symbol)
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(symbols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backfill_result_structure() {
        // Verify BackfillResult has the expected fields
        let result = BackfillResult {
            total: 10,
            processed: 9,
            errors: vec![("test.rs".to_string(), "Read error".to_string())],
        };

        assert_eq!(result.total, 10);
        assert_eq!(result.processed, 9);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].0, "test.rs");
        assert_eq!(result.errors[0].1, "Read error");
    }
}

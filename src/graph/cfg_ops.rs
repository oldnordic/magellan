//! CFG block storage and retrieval operations
//!
//! This module handles persistence of CFG (Control Flow Graph) data
//! extracted from AST nodes.

use anyhow::Result;
use rusqlite::params;

use crate::graph::schema::CfgBlock;
use crate::graph::cfg_extractor::CfgExtractor;
use crate::generation::ChunkStore;

/// CFG block operations
pub struct CfgOps {
    /// ChunkStore for database connection (shares connection with AST nodes)
    chunks: ChunkStore,
}

impl CfgOps {
    /// Create new CfgOps instance
    pub fn new(chunks: ChunkStore) -> Self {
        Self { chunks }
    }

    /// Extract and store CFG blocks for a function
    ///
    /// # Arguments
    /// * `func_node` - tree-sitter node for the function
    /// * `source` - Source code bytes
    /// * `function_id` - Database ID of the function symbol
    ///
    /// # Returns
    /// Number of CFG blocks inserted
    pub fn index_cfg_for_function(
        &self,
        func_node: &tree_sitter::Node,
        source: &[u8],
        function_id: i64,
    ) -> Result<usize> {
        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(func_node, function_id);

        if blocks.is_empty() {
            return Ok(0);
        }

        self.insert_cfg_blocks(&blocks)?;

        Ok(blocks.len())
    }

    /// Insert CFG blocks into database
    ///
    /// Uses bulk insert for efficiency.
    fn insert_cfg_blocks(&self, blocks: &[CfgBlock]) -> Result<usize> {
        let conn = self.chunks.connect()?;

        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO cfg_blocks (
                    function_id, kind, terminator,
                    byte_start, byte_end,
                    start_line, start_col,
                    end_line, end_col
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;

            for block in blocks {
                stmt.execute(params![
                    block.function_id,
                    block.kind,
                    block.terminator,
                    block.byte_start,
                    block.byte_end,
                    block.start_line,
                    block.start_col,
                    block.end_line,
                    block.end_col,
                ])?;
            }
        }
        tx.commit()?;

        Ok(blocks.len())
    }

    /// Delete all CFG blocks for a specific function
    ///
    /// Called when a file is re-indexed.
    ///
    /// # Arguments
    /// * `function_id` - Database ID of the function symbol
    pub fn delete_cfg_for_function(&self, function_id: i64) -> Result<usize> {
        let conn = self.chunks.connect()?;
        let affected = conn.execute(
            "DELETE FROM cfg_blocks WHERE function_id = ?1",
            params![function_id],
        )?;
        Ok(affected)
    }

    /// Delete all CFG blocks for functions in a file
    ///
    /// Called when a file is deleted or re-indexed.
    ///
    /// # Arguments
    /// * `function_ids` - List of function symbol IDs to delete CFG for
    pub fn delete_cfg_for_functions(&self, function_ids: &[i64]) -> Result<usize> {
        if function_ids.is_empty() {
            return Ok(0);
        }

        let conn = self.chunks.connect()?;

        // Build placeholders for IN list
        let placeholders = function_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "DELETE FROM cfg_blocks WHERE function_id IN ({})",
            placeholders
        );

        let mut stmt = conn.prepare(&sql)?;

        // Convert &[i64] to rusqlite parameters
        let params: Vec<_> = function_ids.iter().copied().collect();

        let affected = stmt.execute(rusqlite::params_from_iter(params))?;
        Ok(affected)
    }

    /// Get CFG blocks for a function
    ///
    /// # Arguments
    /// * `function_id` - Database ID of the function symbol
    ///
    /// # Returns
    /// Vector of CfgBlock for the function
    pub fn get_cfg_for_function(&self, function_id: i64) -> Result<Vec<CfgBlock>> {
        let conn = self.chunks.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, function_id, kind, terminator,
                    byte_start, byte_end,
                    start_line, start_col,
                    end_line, end_col
             FROM cfg_blocks
             WHERE function_id = ?1
             ORDER BY byte_start",
        )?;

        let blocks = stmt
            .query_map(params![function_id], |row| {
                Ok(CfgBlock {
                    function_id: row.get(1)?,
                    kind: row.get(2)?,
                    terminator: row.get(3)?,
                    byte_start: row.get(4)?,
                    byte_end: row.get(5)?,
                    start_line: row.get(6)?,
                    start_col: row.get(7)?,
                    end_line: row.get(8)?,
                    end_col: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(blocks)
    }

    /// Get all CFG blocks for a file (by function)
    ///
    /// # Arguments
    /// * `file_path` - Path to the source file
    ///
    /// # Returns
    /// Vector of (function_id, Vec<CfgBlock>) tuples
    pub fn get_cfg_for_file(&self, file_path: &str) -> Result<Vec<(i64, Vec<CfgBlock>)>> {
        let conn = self.chunks.connect()?;

        let mut stmt = conn.prepare(
            "SELECT e.id AS function_id,
                    c.id, c.function_id, c.kind, c.terminator,
                    c.byte_start, c.byte_end,
                    c.start_line, c.start_col,
                    c.end_line, c.end_col
             FROM cfg_blocks c
             JOIN graph_entities e ON c.function_id = e.id
             WHERE e.file_path = ?1
             ORDER BY e.id, c.byte_start",
        )?;

        let mut result = std::collections::HashMap::new();

        let rows = stmt.query_map(params![file_path], |row| {
            let function_id: i64 = row.get(0)?;
            let block = CfgBlock {
                function_id: row.get(2)?,
                kind: row.get(3)?,
                terminator: row.get(4)?,
                byte_start: row.get(5)?,
                byte_end: row.get(6)?,
                start_line: row.get(7)?,
                start_col: row.get(8)?,
                end_line: row.get(9)?,
                end_col: row.get(10)?,
            };
            Ok((function_id, block))
        })?;

        for row in rows {
            let (function_id, block) = row?;
            result.entry(function_id).or_insert_with(Vec::new).push(block);
        }

        Ok(result.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_retrieve_cfg_blocks() {
        // This test requires a CodeGraph instance
        // See integration tests for full end-to-end testing
    }
}
